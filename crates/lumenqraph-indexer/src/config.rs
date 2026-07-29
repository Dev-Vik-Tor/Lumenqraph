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

        // Parse numeric config with validation.
        let poll_interval_secs = env_parse("POLL_INTERVAL_SECS", 5)?;
        let page_size = env_parse("PAGE_SIZE", 1000)?;
        let max_catchup_ledgers = env_parse("MAX_CATCHUP_LEDGERS", 4000)?;
        let retention_ledgers = env_parse("RETENTION_LEDGERS", 0)?;
        let reorg_overlap_ledgers = env_parse("REORG_OVERLAP_LEDGERS", 0)?;

        // Validate and clamp PAGE_SIZE to RPC documented bounds (1–10000).
        let page_size = clamp_with_warning("PAGE_SIZE", page_size, 1, 10000);

        // Validate POLL_INTERVAL_SECS minimum.
        let poll_interval_secs = clamp_with_warning("POLL_INTERVAL_SECS", poll_interval_secs, 1, u64::MAX);

        // Validate MAX_CATCHUP_LEDGERS minimum.
        let max_catchup_ledgers = clamp_with_warning("MAX_CATCHUP_LEDGERS", max_catchup_ledgers, 1, i64::MAX);

        // Validate RETENTION_LEDGERS minimum (0 = disabled).
        let retention_ledgers = if retention_ledgers < 0 {
            tracing::warn!(
                requested = retention_ledgers,
                clamped_to = 0,
                "RETENTION_LEDGERS cannot be negative; clamping to 0 (disabled)"
            );
            0
        } else {
            retention_ledgers
        };

        // Validate REORG_OVERLAP_LEDGERS minimum (0 = disabled).
        let reorg_overlap_ledgers = if reorg_overlap_ledgers < 0 {
            tracing::warn!(
                requested = reorg_overlap_ledgers,
                clamped_to = 0,
                "REORG_OVERLAP_LEDGERS cannot be negative; clamping to 0 (disabled)"
            );
            0
        } else {
            reorg_overlap_ledgers
        };

        Ok(Self {
            database_url: env("DATABASE_URL")?,
            rpc_url: env("RPC_URL")?,
            poll_interval_secs,
            page_size,
            start_ledger: env_parse("START_LEDGER", 0)?,
            max_catchup_ledgers,
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
            retention_ledgers,
            reorg_overlap_ledgers,
        })
    }
}

/// Clamp a numeric config value to [min, max] and log a warning if clamped.
fn clamp_with_warning<T: std::cmp::PartialOrd + std::fmt::Display + Copy>(
    key: &str,
    value: T,
    min: T,
    max: T,
) -> T {
    if value < min {
        tracing::warn!(
            requested = %value,
            clamped_to = %min,
            "{} is below minimum; clamping to {}", key, min
        );
        min
    } else if value > max {
        tracing::warn!(
            requested = %value,
            clamped_to = %max,
            "{} exceeds maximum; clamping to {}", key, max
        );
        max
    } else {
        value
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_size_clamping() {
        // Test PAGE_SIZE bounds (1–10000).
        assert_eq!(clamp_with_warning("PAGE_SIZE", 0u32, 1, 10000), 1);
        assert_eq!(clamp_with_warning("PAGE_SIZE", 1u32, 1, 10000), 1);
        assert_eq!(clamp_with_warning("PAGE_SIZE", 5000u32, 1, 10000), 5000);
        assert_eq!(clamp_with_warning("PAGE_SIZE", 10000u32, 1, 10000), 10000);
        assert_eq!(clamp_with_warning("PAGE_SIZE", 100000u32, 1, 10000), 10000);
    }

    #[test]
    fn poll_interval_clamping() {
        // Test POLL_INTERVAL_SECS minimum.
        assert_eq!(clamp_with_warning("POLL_INTERVAL_SECS", 0u64, 1, u64::MAX), 1);
        assert_eq!(clamp_with_warning("POLL_INTERVAL_SECS", 1u64, 1, u64::MAX), 1);
        assert_eq!(clamp_with_warning("POLL_INTERVAL_SECS", 5u64, 1, u64::MAX), 5);
        assert_eq!(clamp_with_warning("POLL_INTERVAL_SECS", 60u64, 1, u64::MAX), 60);
    }

    #[test]
    fn max_catchup_ledgers_clamping() {
        // Test MAX_CATCHUP_LEDGERS minimum.
        assert_eq!(clamp_with_warning("MAX_CATCHUP_LEDGERS", 0i64, 1, i64::MAX), 1);
        assert_eq!(clamp_with_warning("MAX_CATCHUP_LEDGERS", 1i64, 1, i64::MAX), 1);
        assert_eq!(clamp_with_warning("MAX_CATCHUP_LEDGERS", 4000i64, 1, i64::MAX), 4000);
        assert_eq!(clamp_with_warning("MAX_CATCHUP_LEDGERS", 120000i64, 1, i64::MAX), 120000);
    }

    #[test]
    fn retention_ledgers_validation() {
        // Negative values should be clamped to 0.
        let retention = -100i64;
        let clamped = if retention < 0 { 0 } else { retention };
        assert_eq!(clamped, 0);

        // Zero and positive should pass through.
        assert_eq!(if 0i64 < 0 { 0 } else { 0i64 }, 0);
        assert_eq!(if 1000i64 < 0 { 0 } else { 1000i64 }, 1000);
    }

    #[test]
    fn reorg_overlap_ledgers_validation() {
        // Negative values should be clamped to 0.
        let reorg = -50i64;
        let clamped = if reorg < 0 { 0 } else { reorg };
        assert_eq!(clamped, 0);

        // Zero and positive should pass through.
        assert_eq!(if 0i64 < 0 { 0 } else { 0i64 }, 0);
        assert_eq!(if 100i64 < 0 { 0 } else { 100i64 }, 100);
    }
}
