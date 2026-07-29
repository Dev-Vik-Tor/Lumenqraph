//! API-key auth + per-key rate limiting, as one middleware layer over the data
//! routes. Keys are presented as `Authorization: Bearer <key>` or `x-api-key`,
//! and only their SHA-256 hash is ever compared against the database.
//! Anonymous requests are rate limited per client IP address.

use std::net::SocketAddr;
use std::sync::atomic::Ordering;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::error::{ApiError, ApiResult};
use crate::state::AppState;

/// SHA-256 hex of an API key. Used both here and by the key-generation script.
pub fn hash_key(key: &str) -> String {
    let mut h = Sha256::new();
    h.update(key.as_bytes());
    hex::encode(h.finalize())
}

fn extract_key(headers: &HeaderMap) -> Option<String> {
    if let Some(v) = headers.get("x-api-key").and_then(|v| v.to_str().ok()) {
        let v = v.trim();
        if !v.is_empty() {
            return Some(v.to_string());
        }
    }
    // RFC 7235 §2.1: the auth-scheme is case-insensitive. Split on the first
    // space so any extra spaces in the token are preserved, then trim both ends.
    let raw = headers.get("authorization").and_then(|v| v.to_str().ok())?;
    let raw = raw.trim();
    let (scheme, rest) = raw.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("bearer") {
        return None;
    }
    let token = rest.trim();
    if token.is_empty() {
        return None;
    }
    Some(token.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderName, HeaderValue};

    fn make_headers(name: &str, value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
        h
    }

    #[test]
    fn bearer_scheme_is_case_insensitive() {
        for scheme in ["Bearer", "bearer", "BEARER", "bEaReR"] {
            let h = make_headers("authorization", &format!("{scheme} mytoken"));
            assert_eq!(
                extract_key(&h).as_deref(),
                Some("mytoken"),
                "failed for scheme {scheme:?}"
            );
        }
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let h = make_headers("authorization", "  Bearer   mytoken  ");
        assert_eq!(extract_key(&h).as_deref(), Some("mytoken"));
    }

    #[test]
    fn missing_token_returns_none() {
        assert_eq!(extract_key(&make_headers("authorization", "Bearer ")), None);
        assert_eq!(extract_key(&make_headers("authorization", "Bearer")), None);
    }

    #[test]
    fn x_api_key_is_extracted() {
        let h = make_headers("x-api-key", "myapikey");
        assert_eq!(extract_key(&h).as_deref(), Some("myapikey"));
    }

    #[test]
    fn absent_auth_returns_none() {
        assert_eq!(extract_key(&HeaderMap::new()), None);
    }
}

/// Extract the client IP address, respecting X-Forwarded-For headers only when
/// behind a trusted proxy (controlled by RATE_LIMIT_TRUST_XFF environment variable).
fn extract_client_ip(headers: &HeaderMap, socket_addr: Option<SocketAddr>) -> String {
    let trust_xff = std::env::var("RATE_LIMIT_TRUST_XFF")
        .ok()
        .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(false);

    if trust_xff {
        if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
            if let Some(ip) = xff.split(',').next().map(|s| s.trim()) {
                return ip.to_string();
            }
        }
        if let Some(forwarded) = headers.get("forwarded").and_then(|v| v.to_str().ok()) {
            if let Some(start) = forwarded.find("for=") {
                let rest = &forwarded[start + 4..];
                if let Some(end) = rest.find(|c| c == ';' || c == ',') {
                    return rest[..end].trim_matches('"').to_string();
                } else {
                    return rest.trim_matches('"').to_string();
                }
            }
        }
    }

    socket_addr
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

pub async fn auth_and_rate_limit(
    State(state): State<AppState>,
    ConnectInfo(socket_addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> ApiResult<Response> {
    state.http_requests.fetch_add(1, Ordering::Relaxed);

    let (identity, limit) = match extract_key(&headers) {
        Some(key) => {
            let hash = hash_key(&key);
            let row: Option<(bool, i32)> = sqlx::query_as(
                "SELECT revoked, rate_limit_per_min FROM api_keys WHERE key_hash = $1",
            )
            .bind(&hash)
            .fetch_optional(&state.pool)
            .await?;
            match row {
                Some((false, limit)) => (format!("key:{hash}"), limit),
                Some((true, _)) => return Err(ApiError::unauthorized("API key revoked")),
                None => return Err(ApiError::unauthorized("invalid API key")),
            }
        }
        None => {
            if state.require_auth {
                return Err(ApiError::unauthorized("missing API key"));
            }
            let client_ip = extract_client_ip(&headers, Some(socket_addr));
            (format!("anon:{client_ip}"), state.anon_rate_limit)
        }
    };

    let rl_status = state.limiter.check(&identity, limit);
    if !rl_status.allowed {
        let mut response = (StatusCode::TOO_MANY_REQUESTS, crate::error::rate_limit_error()).into_response();

        // Add rate limit headers
        if let Some(retry_after) = rl_status.retry_after_secs {
            response.headers_mut().insert(
                "Retry-After",
                retry_after.to_string().parse().unwrap_or_else(|_| "60".parse().unwrap()),
            );
        }
        response.headers_mut().insert(
            "X-RateLimit-Limit",
            limit.to_string().parse().unwrap_or_else(|_| "0".parse().unwrap()),
        );
        response.headers_mut().insert(
            "X-RateLimit-Remaining",
            rl_status.tokens_remaining.to_string().parse().unwrap_or_else(|_| "0".parse().unwrap()),
        );

        return Ok(response);
    }

    Ok(next.run(req).await)
}

/// Middleware for expensive RPC-backed routes that hit upstream Soroban RPC.
/// These routes use a separate, tighter rate limit to prevent exhaustion of
/// shared RPC quota. Optionally requires authentication even when the main
/// API doesn't, providing additional protection for expensive operations.
pub async fn rpc_auth_and_rate_limit(
    State(state): State<AppState>,
    headers: HeaderMap,
    req: Request,
    next: Next,
) -> ApiResult<Response> {
    state.http_requests.fetch_add(1, Ordering::Relaxed);

    let (identity, limit) = match extract_key(&headers) {
        Some(key) => {
            let hash = hash_key(&key);
            let row: Option<(bool, i32)> = sqlx::query_as(
                "SELECT revoked, rate_limit_per_min FROM api_keys WHERE key_hash = $1",
            )
            .bind(&hash)
            .fetch_optional(&state.pool)
            .await?;
            match row {
                Some((false, limit)) => (format!("key:{hash}"), limit),
                Some((true, _)) => return Err(ApiError::unauthorized("API key revoked")),
                None => return Err(ApiError::unauthorized("invalid API key")),
            }
        }
        None => {
            if state.rpc_require_auth {
                return Err(ApiError::unauthorized(
                    "RPC routes require API key; missing or invalid key",
                ));
            }
            ("anon".to_string(), state.rpc_anon_rate_limit)
        }
    };

    if !state.rpc_limiter.check(&identity, limit).allowed {
        return Err(ApiError::too_many_requests());
    }

    Ok(next.run(req).await)
}
