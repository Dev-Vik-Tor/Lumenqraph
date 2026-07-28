//! Keyset (cursor) pagination utilities for efficient paging through large result sets.
//!
//! Implements Relay-style cursor pagination using (ledger, event_id) tuples.
//! This avoids the O(n) cost of OFFSET pagination on large tables.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;

/// Encode a cursor from (ledger, event_id) position.
pub fn encode_cursor(ledger: i64, event_id: &str) -> String {
    B64.encode(format!("{ledger}|{event_id}"))
}

/// Decode a cursor to (ledger, event_id). Returns None for absent/malformed cursors.
pub fn decode_cursor(cursor: Option<&str>) -> Option<(i64, String)> {
    let raw = cursor?;
    let bytes = B64.decode(raw).ok()?;
    let s = String::from_utf8(bytes).ok()?;
    let (ledger, id) = s.split_once('|')?;
    Some((ledger.parse().ok()?, id.to_string()))
}

/// Configuration for keyset (cursor) pagination requests.
pub struct PaginationConfig {
    pub after_ledger: Option<i64>,
    pub after_event_id: Option<String>,
}

impl PaginationConfig {
    /// Create pagination config from a limit and optional cursor.
    pub fn new(_limit: i64, after: Option<&str>) -> Self {
        let (after_ledger, after_event_id) = match decode_cursor(after) {
            Some((l, id)) => (Some(l), Some(id)),
            None => (None, None),
        };
        Self {
            after_ledger,
            after_event_id,
        }
    }
}
