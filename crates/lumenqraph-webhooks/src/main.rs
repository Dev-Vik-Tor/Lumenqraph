//! Lumenqraph webhooks — a standalone service that pushes indexed events to
//! registered subscriber URLs. Separate from the API so delivery retries and
//! failures never touch the read path.

mod config;
mod dispatcher;
mod metrics;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use config::Config;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer())
        .init();

    let config = Config::from_env()?;
    let pool = PgPoolOptions::new()
        .max_connections(env_parse_u32("DATABASE_MAX_CONNECTIONS", 5))
        .min_connections(env_parse_u32("DATABASE_MIN_CONNECTIONS", 1))
        .acquire_timeout(Duration::from_secs(env_parse_u64(
            "DATABASE_ACQUIRE_TIMEOUT_SECS",
            30,
        )))
        .idle_timeout(Duration::from_secs(env_parse_u64(
            "DATABASE_IDLE_TIMEOUT_SECS",
            600,
        )))
        .connect(&config.database_url)
        .await
        .context("failed to connect to Postgres")?;

    let http = reqwest::Client::builder()
        .connect_timeout(config.connect_timeout())
        .timeout(config.total_timeout())
        .build()?;

    let metrics_bind_addr = std::env::var("WEBHOOKS_METRICS_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:9091".to_string());

    let pool_arc = std::sync::Arc::new(pool.clone());
    metrics::start_metrics_server(pool_arc, &metrics_bind_addr).await?;

    info!(tick_secs = config.tick_secs, "starting lumenqraph webhooks");
    let interval = Duration::from_secs(config.tick_secs.max(1));

    loop {
        if let Err(e) = dispatcher::enqueue(&pool, config.batch_size).await {
            tracing::warn!(error = %e, "enqueue failed");
        }
        if let Err(e) = dispatcher::deliver(&pool, &http, &config).await {
            tracing::warn!(error = %e, "deliver failed");
        }

        tokio::select! {
            _ = tokio::time::sleep(interval) => {}
            _ = shutdown_signal() => {
                info!("shutdown signal received; stopping webhooks");
                return Ok(());
            }
        }
    }
}

fn env_parse_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

fn env_parse_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(default)
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
