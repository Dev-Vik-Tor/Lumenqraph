//! `GET /metrics` — Prometheus text exposition. Indexer numbers come from the
//! status row the indexer maintains; API numbers from in-process counters.

use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;

use crate::error::ApiResult;
use crate::state::AppState;

fn percentile(sorted: &[u64], p: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = ((sorted.len() as f64) * p / 100.0).ceil() as usize;
    sorted[idx.saturating_sub(1)]
}

pub async fn metrics(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let status: Option<(i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT last_processed_ledger, chain_tip_ledger, events_ingested_total, errors_total
         FROM indexer_cursor WHERE id = 1",
    )
    .fetch_optional(&state.pool)
    .await?;

    let total_events: (i64,) = sqlx::query_as("SELECT count(*) FROM events")
        .fetch_one(&state.pool)
        .await?;

    let (last, tip, ingested, errors) = status.unwrap_or((0, 0, 0, 0));
    let lag = (tip - last).max(0);
    let requests = state.http_requests.load(Ordering::Relaxed);

    let mut body = format!(
        "# HELP lumenqraph_indexer_last_processed_ledger Last ledger the indexer processed\n\
         # TYPE lumenqraph_indexer_last_processed_ledger gauge\n\
         lumenqraph_indexer_last_processed_ledger {last}\n\
         # HELP lumenqraph_indexer_chain_tip_ledger Latest ledger observed on chain\n\
         # TYPE lumenqraph_indexer_chain_tip_ledger gauge\n\
         lumenqraph_indexer_chain_tip_ledger {tip}\n\
         # HELP lumenqraph_indexer_lag_ledgers Ledgers behind the chain tip\n\
         # TYPE lumenqraph_indexer_lag_ledgers gauge\n\
         lumenqraph_indexer_lag_ledgers {lag}\n\
         # HELP lumenqraph_events_total Total events stored\n\
         # TYPE lumenqraph_events_total counter\n\
         lumenqraph_events_total {events}\n\
         # HELP lumenqraph_indexer_ingested_total Events ingested by the indexer\n\
         # TYPE lumenqraph_indexer_ingested_total counter\n\
         lumenqraph_indexer_ingested_total {ingested}\n\
         # HELP lumenqraph_indexer_errors_total Indexer poll-cycle errors\n\
         # TYPE lumenqraph_indexer_errors_total counter\n\
         lumenqraph_indexer_errors_total {errors}\n\
         # HELP lumenqraph_api_requests_total API requests served\n\
         # TYPE lumenqraph_api_requests_total counter\n\
         lumenqraph_api_requests_total {requests}\n",
        last = last,
        tip = tip,
        lag = lag,
        events = total_events.0,
        ingested = ingested,
        errors = errors,
        requests = requests,
    );

    body.push_str("# HELP lumenqraph_http_request_duration_ms Per-route HTTP request latency\n");
    body.push_str("# TYPE lumenqraph_http_request_duration_ms histogram\n");
    {
        let histograms = state.metrics.histogram_buckets.read();
        for (key, samples) in histograms.iter() {
            if samples.is_empty() {
                continue;
            }
            let mut sorted = samples.clone();
            sorted.sort_unstable();

            let p50 = percentile(&sorted, 50.0);
            let p95 = percentile(&sorted, 95.0);
            let p99 = percentile(&sorted, 99.0);
            let count = sorted.len();
            let sum: u64 = sorted.iter().sum();

            body.push_str(&format!("{key}_bucket{{le=\"0.001\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"0.005\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"0.01\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"0.05\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"0.1\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"0.5\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"1.0\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"5.0\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"10.0\"}} 0\n"));
            body.push_str(&format!("{key}_bucket{{le=\"+Inf\"}} {count}\n"));
            body.push_str(&format!("{key}_count {count}\n"));
            body.push_str(&format!("{key}_sum {sum}\n"));
            body.push_str(&format!("{key}_p50 {p50}\n"));
            body.push_str(&format!("{key}_p95 {p95}\n"));
            body.push_str(&format!("{key}_p99 {p99}\n"));
        }
    }

    body.push_str("# HELP lumenqraph_http_request_status Per-route HTTP request status codes\n");
    body.push_str("# TYPE lumenqraph_http_request_status counter\n");
    {
        let counters = state.metrics.status_counters.read();
        for (key, count) in counters.iter() {
            body.push_str(&format!("{key} {count}\n"));
        }
    }

    Ok(([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body))
}
