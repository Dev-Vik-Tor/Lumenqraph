//! `GET /contracts/:contract_id/events` — most-recent events for a contract,
//! newest first, with keyset (cursor) pagination and an optional `event_name`
//! filter. Each row includes both raw base64 XDR and decoded JSON.
//!
//! Supports both offset (deprecated for large result sets) and cursor pagination.
//! For large result sets, cursor pagination is strongly recommended as it has
//! constant-time per-page performance, whereas offset pagination degrades linearly.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::DateTime;
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
    /// Optional ledger range filter: minimum ledger (inclusive).
    from_ledger: Option<i64>,
    /// Optional ledger range filter: maximum ledger (inclusive).
    to_ledger: Option<i64>,
    /// Optional time range filter: minimum timestamp (RFC3339).
    since: Option<String>,
    /// Optional time range filter: maximum timestamp (RFC3339).
    until: Option<String>,
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
    let limit = q.limit.clamp(1, 1000);

    // Validate and parse time range if provided
    let since_datetime: Option<DateTime<chrono::Utc>> = if let Some(ref since) = q.since {
        Some(
            DateTime::parse_from_rfc3339(since)
                .map_err(|_| ApiError::bad_request("Invalid 'since' timestamp format (RFC3339)"))?
                .with_timezone(&chrono::Utc),
        )
    } else {
        None
    };

    let until_datetime: Option<DateTime<chrono::Utc>> = if let Some(ref until) = q.until {
        Some(
            DateTime::parse_from_rfc3339(until)
                .map_err(|_| ApiError::bad_request("Invalid 'until' timestamp format (RFC3339)"))?
                .with_timezone(&chrono::Utc),
        )
    } else {
        None
    };

    // Validate time range consistency
    if let (Some(since), Some(until)) = (since_datetime, until_datetime) {
        if since > until {
            return Err(ApiError::bad_request("'since' must be before 'until'"));
        }
    }

    // Validate ledger range consistency
    if let (Some(from_ledger), Some(to_ledger)) = (q.from_ledger, q.to_ledger) {
        if from_ledger > to_ledger {
            return Err(ApiError::bad_request("'from_ledger' must be <= 'to_ledger'"));
        }
    }

    // If cursor is provided, use keyset pagination; otherwise fall back to offset.
    let events: Vec<EventRow> = if let Some(ref cursor) = q.after {
        let page_config = pagination::PaginationConfig::new(limit, Some(cursor));
        sqlx::query_as(
            "SELECT event_id, contract_id, ledger, ledger_closed_at, event_type,
                    topics, decoded_topics, event_name, value, decoded_value,
                    enriched, tx_hash, in_successful_call, paging_token, created_at
             FROM events
             WHERE contract_id = $1
               AND ($2::text IS NULL OR event_name = $2)
               AND ($3::bigint IS NULL OR ledger >= $3)
               AND ($4::bigint IS NULL OR ledger <= $4)
               AND ($5::timestamp IS NULL OR ledger_closed_at >= $5)
               AND ($6::timestamp IS NULL OR ledger_closed_at <= $6)
               AND ($7::bigint IS NULL OR ledger < $7 OR (ledger = $7 AND event_id < $8))
             ORDER BY ledger DESC, event_id DESC
             LIMIT $9",
        )
        .bind(&contract_id)
        .bind(&q.event_name)
        .bind(q.from_ledger)
        .bind(q.to_ledger)
        .bind(since_datetime)
        .bind(until_datetime)
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
               AND ($3::bigint IS NULL OR ledger >= $3)
               AND ($4::bigint IS NULL OR ledger <= $4)
               AND ($5::timestamp IS NULL OR ledger_closed_at >= $5)
               AND ($6::timestamp IS NULL OR ledger_closed_at <= $6)
             ORDER BY ledger DESC, event_id DESC
             LIMIT $7 OFFSET $8",
        )
        .bind(&contract_id)
        .bind(&q.event_name)
        .bind(q.from_ledger)
        .bind(q.to_ledger)
        .bind(since_datetime)
        .bind(until_datetime)
        .bind(limit)
        .bind(offset)
        .fetch_all(&state.pool)
        .await?
    };

    // Determine if there's a next page and extract the cursor.
    let (has_next_page, result_events) = if q.after.is_some() && events.len() as i64 > limit {
        let mut trimmed = events;
        trimmed.truncate(limit as usize);
        let next_cursor = trimmed
            .last()
            .map(|e| pagination::encode_cursor(e.ledger, &e.event_id));
        (true, trimmed)
    } else {
        let next_cursor = events
            .last()
            .map(|e| pagination::encode_cursor(e.ledger, &e.event_id));
        (false, events)
    };

    Ok(Json(EventsResponse {
        data: result_events,
        next_cursor: if has_next_page {
            result_events
                .last()
                .map(|e| pagination::encode_cursor(e.ledger, &e.event_id))
        } else {
            None
        },
    }))
}
