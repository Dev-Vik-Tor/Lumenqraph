//! Lumenqraph indexer — an always-on process that tails Soroban RPC and writes
//! decoded events into Postgres. It talks to nothing but the RPC and its own DB.
//!
//! Usage:
//!   lumenqraph-indexer                    # live tail (default)
//!   lumenqraph-indexer backfill [LEDGER]  # one-shot catch-up within RPC window (~7 days) then exit
//!   lumenqraph-indexer deep-backfill [OPTIONS]  # gapless history from a data-lake export (#84)
//!   lumenqraph-indexer inspect <CONTRACT> # print a contract's on-chain interface
//!
//! deep-backfill options:
//!   --from <LEDGER>   Start ledger (required)
//!   --to   <LEDGER>   End ledger   (default: max / run to EOF of input)
//!   --source <TYPE>   Source type: galexie (default: galexie)
//!   --input <PATH>    Input file(s); use '-' for stdin; may be repeated

mod backfill;
mod config;
mod convert;
mod cursor;
mod deep_backfill;
mod keys;
mod poller;
mod retention;
mod rpc_client;
mod specs;
mod state;
mod store;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use config::Config;
use rpc_client::RpcClient;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer())
        .init();

    let config = Config::from_env()?;
    let rpc = RpcClient::new(config.rpc_url.clone());
    let args: Vec<String> = std::env::args().collect();

    // `inspect` needs only RPC — handle it before touching the database.
    if args.get(1).map(String::as_str) == Some("inspect") {
        let contract_id = args
            .get(2)
            .context("usage: lumenqraph-indexer inspect <contract_id>")?;
        return inspect(&rpc, contract_id).await;
    }

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .context("failed to connect to Postgres")?;

    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .context("failed to run migrations")?;

    if args.get(1).map(String::as_str) == Some("backfill") {
        let from = args
            .get(2)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(config.start_ledger);
        info!(from, "running in backfill mode");
        return backfill::run(pool, rpc, config, from).await;
    }

    // deep-backfill: ingest beyond the RPC retention window from a data-lake
    // source. Parse manual args: --from, --to, --source, --input (repeatable).
    if args.get(1).map(String::as_str) == Some("deep-backfill") {
        return run_deep_backfill(args, pool, config).await;
    }

    info!(
        rpc = %config.rpc_url,
        contracts = ?config.contract_ids,
        poll_secs = config.poll_interval_secs,
        "starting lumenqraph indexer (live)"
    );
    poller::run(pool, rpc, config).await
}

/// Fetch a contract's deployed WASM and print its parsed interface as JSON.
async fn inspect(rpc: &RpcClient, contract_id: &str) -> anyhow::Result<()> {
    let Some((wasm_hash, wasm)) = rpc.get_contract_wasm(contract_id).await? else {
        anyhow::bail!(
            "no WASM found for {contract_id} (not a contract, or a Stellar Asset Contract)"
        );
    };
    eprintln!("wasm hash {wasm_hash} ({} bytes)", wasm.len());
    match lumenqraph_core::ContractSpec::from_wasm(&wasm) {
        Some(spec) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&spec.to_interface_json())?
            );
            Ok(())
        }
        None => anyhow::bail!("contract has no contractspecv0 interface section"),
    }
}

/// Parse `deep-backfill` CLI arguments and dispatch to [`deep_backfill::run`].
///
/// Accepted flags (space- or `=`-separated):
///   --from  <ledger>          Start ledger (required)
///   --to    <ledger>          End ledger   (optional; defaults to "until EOF")
///   --source <type>           Data source: `galexie` (default)
///   --input <path>            Input file; use `-` for stdin (may be repeated)
async fn run_deep_backfill(
    args: Vec<String>,
    pool: sqlx::PgPool,
    config: Config,
) -> anyhow::Result<()> {
    use std::path::PathBuf;
    use deep_backfill::{GalexieSource, HistoricalSource};

    let mut from_ledger: Option<i64> = None;
    let mut to_ledger: Option<i64> = None;
    let mut source_type = "galexie".to_string();
    let mut inputs: Vec<PathBuf> = Vec::new();

    let mut i = 2usize; // skip "lumenqraph-indexer" and "deep-backfill"
    while i < args.len() {
        match args[i].as_str() {
            "--from" => {
                i += 1;
                from_ledger = Some(
                    args.get(i)
                        .context("--from requires a ledger number")?
                        .parse::<i64>()
                        .context("--from: invalid ledger number")?,
                );
            }
            "--to" => {
                i += 1;
                to_ledger = Some(
                    args.get(i)
                        .context("--to requires a ledger number")?
                        .parse::<i64>()
                        .context("--to: invalid ledger number")?,
                );
            }
            "--source" => {
                i += 1;
                source_type = args
                    .get(i)
                    .context("--source requires a type (e.g. galexie)")?
                    .clone();
            }
            "--input" => {
                i += 1;
                inputs.push(PathBuf::from(
                    args.get(i).context("--input requires a file path")?,
                ));
            }
            flag if flag.starts_with("--from=") => {
                from_ledger = Some(
                    flag.trim_start_matches("--from=")
                        .parse::<i64>()
                        .context("--from: invalid ledger number")?,
                );
            }
            flag if flag.starts_with("--to=") => {
                to_ledger = Some(
                    flag.trim_start_matches("--to=")
                        .parse::<i64>()
                        .context("--to: invalid ledger number")?,
                );
            }
            flag if flag.starts_with("--source=") => {
                source_type = flag.trim_start_matches("--source=").to_string();
            }
            flag if flag.starts_with("--input=") => {
                inputs.push(PathBuf::from(flag.trim_start_matches("--input=")));
            }
            other => {
                anyhow::bail!("unknown deep-backfill flag: {other}");
            }
        }
        i += 1;
    }

    let from_ledger = from_ledger.context(
        "deep-backfill requires --from <ledger>\n\
         Example: lumenqraph-indexer deep-backfill \
         --from 1000000 --input /data/export.ndjson",
    )?;

    // Default to stdin when no --input is given.
    if inputs.is_empty() {
        inputs.push(PathBuf::from("-"));
    }

    let source: Box<dyn HistoricalSource> = match source_type.as_str() {
        "galexie" => Box::new(GalexieSource::new(inputs)),
        other => anyhow::bail!(
            "unknown source type '{other}'; supported: galexie"
        ),
    };

    deep_backfill::run(pool, config, source, from_ledger, to_ledger).await
}
