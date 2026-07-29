//! `GET /contracts/:contract_id/events` — most-recent events for a contract,
//! newest first, with keyset (cursor) pagination and an optional `event_name`
//! filter. Each row includes both raw base64 XDR and decoded JSON.
//!
//! Supports both offset (deprecated for large result sets) and cursor pagination.
//! For large result sets, cursor pagination is strongly recommended as it has
//! constant-time per-page performance, whereas offset pagination degrades linearly.

use axum::extract::{Path, Query, State};
use axum::Json;
use lumenqraph_core::EventRow;
use serde::{Deserialize, Serialize};

use crate::error::{ApiError, ApiResult};
use crate::pagination;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct EventsQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    #[serde(default)]
    offset: i64,
    /// Opaque cursor from a previous response's `next_cursor`.
    after: Option<String>,
    /// Optional filter, e.g. `?event_name=transfer`.
    event_name: Option<String>,
}

fn default_limit() -> i64 {
    50
}

#[derive(Serialize)]
pub struct EventsResponse {
    /// The event rows in the result set.
    pub data: Vec<EventRow>,
    /// Opaque cursor to fetch the next page. Null if this is the last page.
    pub next_cursor: Option<String>,
}

pub async fn list_events(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
    Query(q): Query<EventsQuery>,
) -> ApiResult<Json<EventsResponse>> {
    if !lumenqraph_core::is_valid_contract_id(&contract_id) {
        return Err(ApiError::bad_request("invalid contract id"));
    }
    let limit = q.limit.clamp(1, 1000);

    // If cursor is provided, use keyset pagination; otherwise fall back to offset.
    let events: Vec<EventRow> = if let Some(ref cursor) = q.after {
        let page_config = pagination::PaginationConfig::new(limit, Some(cursor))
            .map_err(|e| ApiError::bad_request(format!("invalid cursor: {e}")))?;
        sqlx::query_as(
            "SELECT event_id, contract_id, ledger, ledger_closed_at, event_type,
                    topics, decoded_topics, event_name, value, decoded_value,
                    enriched, tx_hash, in_successful_call, paging_token, created_at
             FROM events
             WHERE contract_id = $1
               AND ($2::text IS NULL OR event_name = $2)
               AND ($3::bigint IS NULL OR ledger < $3 OR (ledger = $3 AND event_id < $4))
             ORDER BY ledger DESC, event_id DESC
             LIMIT $5",
        )
        .bind(&contract_id)
        .bind(&q.event_name)
        .bind(page_config.after_ledger)
        .bind(page_config.after_event_id)
        .bind(limit + 1)
        .fetch_all(&state.pool)
        .await?
    } else {
        // Backward compatibility: use offset pagination if no cursor provided
        let offset = q.offset.max(0);
        sqlx::query_as(
            "SELECT event_id, contract_id, ledger, ledger_closed_at, event_type,
                    topics, decoded_topics, event_name, value, decoded_value,
                    enriched, tx_hash, in_successful_call, paging_token, created_at
             FROM events
             WHERE contract_id = $1
               AND ($2::text IS NULL OR event_name = $2)
             ORDER BY ledger DESC, event_id DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(&contract_id)
        .bind(&q.event_name)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    };

    // Determine if there's a next page and slice the sentinel off.
    let (has_next_page, result_events) = if q.after.is_some() && events.len() as i64 > limit {
        let mut trimmed = events;
        trimmed.truncate(limit as usize);
        (true, trimmed)
    } else {
        (false, events)
    };

    let next_cursor = if has_next_page {
        result_events
            .last()
            .map(|e| pagination::encode_cursor(e.ledger, &e.event_id))
    } else {
        None
    };

    Ok(Json(EventsResponse {
        data: result_events,
        next_cursor,
    }))
}
