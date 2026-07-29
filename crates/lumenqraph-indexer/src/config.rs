//! Indexer configuration, loaded from environment (see `.env.example`).

use anyhow::Context;
use crate::keys::{KeyTemplate, parse_durability};

#[derive(Debug, Clone)]
pub struct Config {
    pub database_url: String,
    pub rpc_url: String,
    /// Contract IDs to index. Empty => index all contract events.
    pub contract_ids: Vec<String>,
    pub poll_interval_secs: u64,
    pub page_size: u32,
    /// Ledger to start from on a fresh index. 0 => start near the current tip.
    /// Clamped to the RPC retention window (~7 days, 120k ledgers for SDF public RPC).
    pub start_ledger: i64,
    /// Max ledgers behind the tip we'll fetch in a single live polling cycle.
    /// This is a conservative safety limit, NOT the RPC retention window.
    /// The RPC retention window is ~7 days (120k ledgers on SDF public RPC),
    /// but we catch up more conservatively in live polling to avoid performance
    /// issues or RPC processing limits. Public Soroban RPCs reject `getEvents`
    /// whose `startLedger` is too far behind (`-32001` "processing limit").
    /// If the cursor falls further behind (e.g. after downtime) we skip ahead to
    /// this window and log the unrecoverable gap rather than stalling forever.
    /// Raise it with a retaining/paid RPC. Default 4000 (~5–6h) is conservative
    /// for the SDF public endpoints.
    pub max_catchup_ledgers: i64,
    /// When true, snapshot each active contract's instance storage into
    /// `contract_state` (versioned). Off by default — it costs one extra RPC
    /// call per active contract per cycle. Best paired with `CONTRACT_IDS`.
    pub state_indexing: bool,
    /// When true, snapshot *per-holder* balances into `contract_data`: for each
    /// holder named in a token's events this cycle, fetch its `Balance(Address)`
    /// entry. Off by default — it costs one RPC call per newly-active holder per
    /// cycle, so it should be paired with `CONTRACT_IDS`.
    pub key_indexing: bool,
    /// The symbol naming the balance storage-key variant (`DataKey::Balance` in
    /// the soroban token reference). Configurable for tokens that differ.
    pub balance_key_symbol: String,
    /// Durability of the balance storage entry: "persistent" (default) or
    /// "temporary".
    pub balance_key_durability: String,
    /// Configurable key templates for per-key state indexing (JSON array).
    /// Format: [{"symbol":"Balance","events":["transfer","mint"],"params":[1,2],"durability":"persistent","label":"balance"}]
    /// Each template defines: symbol (storage key variant), events (triggering event names),
    /// params (topic indices to extract), durability (persistent/temporary), label (optional grouping tag).
    pub key_templates: Vec<KeyTemplate>,
    /// Keep only the last N ledgers of history, pruning older rows as the tip
    /// advances. 0 (default) => keep everything. Set this when the database has
    /// a hard size cap (e.g. a 500MB free tier) that an unbounded index would
    /// hit; see `retention`.
    pub retention_ledgers: i64,
    /// When true, check whether a tracked contract's executable has changed and,
    /// if so, re-read its interface and append a `contract_spec_versions` row
    /// with the diff (see `specs`). Costs one RPC call per tracked contract per
    /// cycle, so it defaults to on only when `CONTRACT_IDS` bounds that set;
    /// in index-all mode it must be enabled explicitly, and then only covers
    /// contracts active in the cycle. `STATE_INDEXING` already reads each
    /// instance and detects upgrades for free, so this adds no calls alongside it.
    pub upgrade_watch: bool,
    /// Number of ledgers to re-scan each cycle to handle shallow reorgs where
    /// the RPC returns different events for recently-passed ledgers. Each cycle,
    /// we re-fetch the last N ledgers and upsert events with updated content.
    /// 0 (default) means no trailing re-scan. Small values (10-100) provide shallow
    /// reorg protection with minimal overhead. Requires careful tuning based on
    /// the RPC's reorg exposure.
    pub reorg_overlap_ledgers: i64,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let contract_ids: Vec<String> = std::env::var("CONTRACT_IDS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        Ok(Self {
            database_url: env("DATABASE_URL")?,
            rpc_url: env("RPC_URL")?,
            poll_interval_secs: env_parse("POLL_INTERVAL_SECS", 5)?,
            page_size: env_parse("PAGE_SIZE", 1000)?,
            start_ledger: env_parse("START_LEDGER", 0)?,
            max_catchup_ledgers: env_parse("MAX_CATCHUP_LEDGERS", 4000)?,
            state_indexing: env_bool("STATE_INDEXING", false),
            key_indexing: env_bool("KEY_INDEXING", false),
            upgrade_watch: env_bool("UPGRADE_WATCH", !contract_ids.is_empty()),
            contract_ids,
            balance_key_symbol: std::env::var("BALANCE_KEY_SYMBOL")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "Balance".to_string()),
            balance_key_durability: std::env::var("BALANCE_KEY_DURABILITY")
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "persistent".to_string()),
            key_templates: parse_key_templates()?,
            retention_ledgers: env_parse("RETENTION_LEDGERS", 0)?,
            reorg_overlap_ledgers: env_parse("REORG_OVERLAP_LEDGERS", 0)?,
        })
    }
}

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(default)
}

fn env(key: &str) -> anyhow::Result<String> {
    std::env::var(key).with_context(|| format!("missing required env var {key}"))
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> anyhow::Result<T>
where
    T::Err: std::fmt::Display,
{
    match std::env::var(key) {
        Ok(v) if !v.trim().is_empty() => v
            .trim()
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid {key}: {e}")),
        _ => Ok(default),
    }
}

#[derive(serde::Deserialize)]
struct KeyTemplateJson {
    symbol: String,
    events: Vec<String>,
    params: Vec<usize>,
    #[serde(default = "default_durability")]
    durability: String,
    label: Option<String>,
}

fn default_durability() -> String {
    "persistent".to_string()
}

fn parse_key_templates() -> anyhow::Result<Vec<KeyTemplate>> {
    let json_str = std::env::var("KEY_TEMPLATES").unwrap_or_default();
    if json_str.trim().is_empty() {
        return Ok(Vec::new());
    }

    let templates: Vec<KeyTemplateJson> = serde_json::from_str(&json_str)
        .context("KEY_TEMPLATES must be a JSON array")?;

    templates
        .into_iter()
        .map(|t| {
            Ok(KeyTemplate {
                symbol: t.symbol,
                event_names: t.events,
                param_indices: t.params,
                durability: parse_durability(&t.durability),
                label: t.label,
            })
        })
        .collect()
}
