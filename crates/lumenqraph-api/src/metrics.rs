//! `GET /metrics` — Prometheus text exposition. Indexer numbers come from the
//! status row the indexer maintains; API numbers from in-process counters.

use std::sync::atomic::Ordering;

use axum::extract::State;
use axum::http::header;
use axum::response::IntoResponse;

use crate::error::ApiResult;
use crate::state::AppState;

pub async fn metrics(State(state): State<AppState>) -> ApiResult<impl IntoResponse> {
    let status: Option<(i64, i64, i64, i64, i64, i64, i64, i64, i64, i64)> = sqlx::query_as(
        "SELECT last_processed_ledger, chain_tip_ledger, events_ingested_total, errors_total,
                events_enriched_total, events_not_enriched_total, spec_fetch_failures_total,
                rpc_calls_total, rpc_errors_total, rpc_errors_32001_total
         FROM indexer_cursor WHERE id = 1",
    )
    .fetch_optional(&state.pool)
    .await?;

    let total_events: (i64,) = sqlx::query_as("SELECT count(*) FROM events")
        .fetch_one(&state.pool)
        .await?;

    let (last, tip, ingested, errors, enriched, not_enriched, spec_fetch_failures,
         rpc_calls, rpc_errors, rpc_errors_32001) = status.unwrap_or((0, 0, 0, 0, 0, 0, 0, 0, 0, 0));
    let lag = (tip - last).max(0);
    let lag_time_secs = lag * 5; // Approximate ~5 seconds per ledger
    let requests = state.http_requests.load(Ordering::Relaxed);

    let body = format!(
        "# HELP lumenqraph_indexer_last_processed_ledger Last ledger the indexer processed\n\
         # TYPE lumenqraph_indexer_last_processed_ledger gauge\n\
         lumenqraph_indexer_last_processed_ledger {last}\n\
         # HELP lumenqraph_indexer_chain_tip_ledger Latest ledger observed on chain\n\
         # TYPE lumenqraph_indexer_chain_tip_ledger gauge\n\
         lumenqraph_indexer_chain_tip_ledger {tip}\n\
         # HELP lumenqraph_indexer_lag_ledgers Ledgers behind the chain tip\n\
         # TYPE lumenqraph_indexer_lag_ledgers gauge\n\
         lumenqraph_indexer_lag_ledgers {lag}\n\
         # HELP lumenqraph_indexer_lag_seconds Estimated time behind the chain tip in seconds\n\
         # TYPE lumenqraph_indexer_lag_seconds gauge\n\
         lumenqraph_indexer_lag_seconds {lag_time_secs}\n\
         # HELP lumenqraph_events_total Total events stored\n\
         # TYPE lumenqraph_events_total counter\n\
         lumenqraph_events_total {events}\n\
         # HELP lumenqraph_indexer_ingested_total Events ingested by the indexer\n\
         # TYPE lumenqraph_indexer_ingested_total counter\n\
         lumenqraph_indexer_ingested_total {ingested}\n\
         # HELP lumenqraph_indexer_errors_total Indexer poll-cycle errors\n\
         # TYPE lumenqraph_indexer_errors_total counter\n\
         lumenqraph_indexer_errors_total {errors}\n\
         # HELP lumenqraph_events_enriched_total Events successfully enriched with spec data\n\
         # TYPE lumenqraph_events_enriched_total counter\n\
         lumenqraph_events_enriched_total {enriched}\n\
         # HELP lumenqraph_events_not_enriched_total Events without matching spec (fallback to decoded)\n\
         # TYPE lumenqraph_events_not_enriched_total counter\n\
         lumenqraph_events_not_enriched_total {not_enriched}\n\
         # HELP lumenqraph_spec_fetch_failures_total Failed attempts to fetch contract specs\n\
         # TYPE lumenqraph_spec_fetch_failures_total counter\n\
         lumenqraph_spec_fetch_failures_total {spec_fetch_failures}\n\
         # HELP lumenqraph_rpc_calls_total Total RPC method calls made\n\
         # TYPE lumenqraph_rpc_calls_total counter\n\
         lumenqraph_rpc_calls_total {rpc_calls}\n\
         # HELP lumenqraph_rpc_errors_total RPC errors encountered\n\
         # TYPE lumenqraph_rpc_errors_total counter\n\
         lumenqraph_rpc_errors_total {rpc_errors}\n\
         # HELP lumenqraph_rpc_errors_32001_total RPC -32001 processing-limit errors\n\
         # TYPE lumenqraph_rpc_errors_32001_total counter\n\
         lumenqraph_rpc_errors_32001_total {rpc_errors_32001}\n\
         # HELP lumenqraph_api_requests_total API requests served\n\
         # TYPE lumenqraph_api_requests_total counter\n\
         lumenqraph_api_requests_total {requests}\n",
        last = last,
        tip = tip,
        lag = lag,
        lag_time_secs = lag_time_secs,
        events = total_events.0,
        ingested = ingested,
        errors = errors,
        enriched = enriched,
        not_enriched = not_enriched,
        spec_fetch_failures = spec_fetch_failures,
        rpc_calls = rpc_calls,
        rpc_errors = rpc_errors,
        rpc_errors_32001 = rpc_errors_32001,
        requests = requests,
    );

    Ok(([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], body))
}
