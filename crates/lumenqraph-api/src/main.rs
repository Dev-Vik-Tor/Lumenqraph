//! Lumenqraph API — the public read + management surface. A separate binary
//! from the indexer, reading the same Postgres, so API traffic can never
//! interrupt ingestion.

mod auth;
mod error;
mod graphql;
mod metrics;
mod pagination;
mod rate_limit;
mod routes;
mod rpc;
mod specs;
mod state;
mod url_validation;

use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use anyhow::Context;
use axum::extract::DefaultBodyLimit;
use axum::http;
use sqlx::postgres::PgPoolOptions;
use tower_http::compression::CompressionLayer;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use rate_limit::RateLimiter;
use state::AppState;

fn env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| matches!(v.trim().to_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(default)
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn build_cors_layer() -> tower_http::cors::CorsLayer {
    use tower_http::cors::CorsLayer;

    let origins_str = std::env::var("API_CORS_ALLOWED_ORIGINS")
        .unwrap_or_else(|_| "same_origin".to_string());

    if origins_str == "*" {
        info!("CORS: allowing all origins (permissive mode)");
        CorsLayer::permissive()
    } else if origins_str.to_lowercase() == "same_origin" || origins_str.is_empty() {
        info!("CORS: allowing same-origin requests only");
        CorsLayer::very_permissive()
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::OPTIONS,
                axum::http::Method::DELETE,
            ])
            .allow_headers([axum::http::header::CONTENT_TYPE])
    } else {
        info!("CORS: allowing specific origins");
        let origins: Vec<&str> = origins_str
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

        let mut cors = CorsLayer::new();
        for origin_str in origins {
            match origin_str.parse::<http::HeaderValue>() {
                Ok(origin) => {
                    cors = cors.allow_origin(origin);
                }
                Err(_) => {
                    info!(origin = origin_str, "invalid origin in API_CORS_ALLOWED_ORIGINS, skipping");
                }
            }
        }
        cors.allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
            axum::http::Method::DELETE,
        ])
        .allow_headers([axum::http::header::CONTENT_TYPE])
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = dotenvy::dotenv();
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .with(fmt::layer())
        .init();

    let database_url = std::env::var("DATABASE_URL").context("missing DATABASE_URL")?;
    let bind_addr = std::env::var("API_BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let rpc_url = std::env::var("RPC_URL")
        .unwrap_or_else(|_| "https://soroban-testnet.stellar.org".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .context("failed to connect to Postgres")?;

    let state = AppState {
        pool,
        require_auth: env_bool("REQUIRE_API_KEY", false),
        anon_rate_limit: env_parse("ANON_RATE_LIMIT_PER_MIN", 60),
        limiter: Arc::new(RateLimiter::new()),
        http_requests: Arc::new(AtomicU64::new(0)),
        rpc: rpc::RpcClient::new(rpc_url),
        specs: Arc::new(specs::SpecCache::new()),
        mounts: Arc::new(routes::proxy::mounts_from_env()),
        rpc_limiter: Arc::new(RateLimiter::new()),
        rpc_require_auth: env_bool("RPC_REQUIRE_API_KEY", false),
        rpc_anon_rate_limit: env_parse("RPC_ROUTE_RATE_LIMIT_PER_MIN", 10),
    };

    let cors_layer = build_cors_layer();
    let max_body_bytes = env_parse::<u32>("API_MAX_BODY_BYTES", 256 * 1024);
    info!(max_body_bytes, "enforcing request body size limit");

    let app = routes::router(state)
        .layer(DefaultBodyLimit::max(max_body_bytes as usize))
        .layer(CompressionLayer::new())
        .layer(TraceLayer::new_for_http())
        .layer(cors_layer);

    let listener = tokio::net::TcpListener::bind(&bind_addr)
        .await
        .with_context(|| format!("failed to bind {bind_addr}"))?;
    info!(addr = %bind_addr, "lumenqraph api listening");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
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
    info!("shutdown signal received; stopping api");
}
