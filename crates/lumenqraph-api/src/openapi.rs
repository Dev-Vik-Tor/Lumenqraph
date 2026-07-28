//! OpenAPI 3.1 specification for the Lumenqraph REST API.
//!
//! This module generates and serves a machine-readable OpenAPI specification
//! that describes all REST endpoints, parameters, and response schemas.

use serde::{Deserialize, Serialize};
use utoipa::{OpenApi, Modify};
use utoipa::openapi::{InfoBuilder, OpenApiBuilder};

/// Core data types used in OpenAPI responses
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct EventResponse {
    pub event_id: String,
    pub contract_id: String,
    pub ledger: i64,
    pub event_type: String,
    pub event_name: Option<String>,
    pub topics: serde_json::Value,
    pub decoded_topics: serde_json::Value,
    pub value: String,
    pub decoded_value: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransferResponse {
    pub event_id: String,
    pub contract_id: String,
    pub from_addr: Option<String>,
    pub to_addr: Option<String>,
    pub amount: String,
    pub ledger: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractResponse {
    pub contract_id: String,
    pub event_count: i64,
    pub first_seen_ledger: i64,
    pub last_seen_ledger: i64,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractInterfaceResponse {
    pub contract_id: String,
    pub has_events: bool,
    pub fetched_at: String,
    pub interface: serde_json::Value,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ContractDataResponse {
    pub contract_id: String,
    pub count: usize,
    pub keys: Vec<serde_json::Value>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WebhookSubscriptionResponse {
    pub id: String,
    pub url: String,
    pub contract_id: Option<String>,
    pub event_name: Option<String>,
    pub active: bool,
    pub created_at: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MetricsResponse {
    pub http_requests_total: u64,
    pub indexer_lag_ledgers: Option<i64>,
}

/// OpenAPI specification builder
pub struct OpenApiBuilder;

impl OpenApiBuilder {
    /// Build the OpenAPI 3.1 specification for the Lumenqraph API
    pub fn build() -> utoipa::openapi::OpenApi {
        let info = InfoBuilder::new()
            .title("Lumenqraph API")
            .version("0.1.0")
            .description(Some(
                "A high-performance indexer and query API for Soroban contract events on Stellar mainnet. \
                 Provides REST endpoints for contract discovery, event retrieval, state inspection, and \
                 webhook subscriptions."
            ))
            .build();

        let mut api = OpenApiBuilder::default()
            .info(info)
            .build();

        // TODO: Add route-specific OpenAPI documentation via Modify trait
        // The routes will be documented using utoipa attributes on handler functions

        api
    }
}

/// OpenAPI documentation struct for axum integration
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Lumenqraph API",
        version = "0.1.0",
        description = "High-performance indexer and query API for Soroban contract events on Stellar mainnet"
    ),
    servers(
        (url = "/", description = "Main API server")
    ),
    components(
        schemas(
            EventResponse,
            TransferResponse,
            ContractResponse,
            ContractInterfaceResponse,
            ContractDataResponse,
            WebhookSubscriptionResponse,
            HealthResponse,
            MetricsResponse
        )
    ),
    paths(
        // Core endpoints documented in routes
        get_health,
        get_metrics,
        list_contracts,
        get_contract_interface,
        list_contract_events,
        list_contract_transfers,
        get_contract_data,
        get_contract_data_history,
        list_webhooks,
        create_webhook,
        delete_webhook
    ),
    tags(
        (name = "Contracts", description = "Contract discovery and interface endpoints"),
        (name = "Events", description = "Contract event retrieval and filtering"),
        (name = "Transfers", description = "Token transfer tracking"),
        (name = "Data", description = "Per-key contract state snapshots"),
        (name = "Webhooks", description = "Event subscription and delivery management"),
        (name = "System", description = "Health and observability endpoints")
    )
)]
pub struct ApiDoc;

/// Placeholder operations for route documentation
/// These will be replaced with actual route annotations

#[utoipa::path(
    get,
    path = "/health",
    responses((status = 200, description = "Service is healthy", body = HealthResponse)),
    tag = "System"
)]
pub async fn get_health() {}

#[utoipa::path(
    get,
    path = "/metrics",
    responses((status = 200, description = "Prometheus metrics", body = MetricsResponse)),
    tag = "System"
)]
pub async fn get_metrics() {}

#[utoipa::path(
    get,
    path = "/contracts",
    responses((status = 200, description = "List of contracts with event counts", body = Vec<ContractResponse>)),
    tag = "Contracts"
)]
pub async fn list_contracts() {}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/interface",
    params(("contract_id" = String, Path, description = "Soroban contract ID")),
    responses((status = 200, description = "Contract interface specification", body = ContractInterfaceResponse)),
    tag = "Contracts"
)]
pub async fn get_contract_interface() {}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/events",
    params(
        ("contract_id" = String, Path, description = "Soroban contract ID"),
        ("limit" = Option<i64>, Query, description = "Max events to return (1-1000, default 50)"),
        ("offset" = Option<i64>, Query, description = "Pagination offset"),
        ("after" = Option<String>, Query, description = "Cursor for keyset pagination"),
        ("event_name" = Option<String>, Query, description = "Filter by event name")
    ),
    responses((status = 200, description = "Contract events", body = Vec<EventResponse>)),
    tag = "Events"
)]
pub async fn list_contract_events() {}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/transfers",
    params(
        ("contract_id" = String, Path, description = "Soroban contract ID"),
        ("limit" = Option<i64>, Query, description = "Max transfers to return (1-1000, default 50)"),
        ("offset" = Option<i64>, Query, description = "Pagination offset"),
        ("after" = Option<String>, Query, description = "Cursor for keyset pagination"),
        ("from" = Option<String>, Query, description = "Filter by sender address"),
        ("to" = Option<String>, Query, description = "Filter by recipient address")
    ),
    responses((status = 200, description = "Token transfers for the contract", body = Vec<TransferResponse>)),
    tag = "Transfers"
)]
pub async fn list_contract_transfers() {}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/data",
    params(
        ("contract_id" = String, Path, description = "Soroban contract ID"),
        ("label" = Option<String>, Query, description = "Filter by discovery label (e.g., 'balance')"),
        ("limit" = Option<i64>, Query, description = "Max keys to return (1-1000, default 100)")
    ),
    responses((status = 200, description = "Current contract data (latest per key)", body = ContractDataResponse)),
    tag = "Data"
)]
pub async fn get_contract_data() {}

#[utoipa::path(
    get,
    path = "/contracts/{contract_id}/data/{key_hash}",
    params(
        ("contract_id" = String, Path, description = "Soroban contract ID"),
        ("key_hash" = String, Path, description = "Hex SHA-256 hash of the storage key"),
        ("limit" = Option<i64>, Query, description = "Max versions to return (1-500, default 50)")
    ),
    responses((status = 200, description = "Version history of a single contract data entry", body = ContractDataResponse)),
    tag = "Data"
)]
pub async fn get_contract_data_history() {}

#[utoipa::path(
    get,
    path = "/webhooks",
    responses((status = 200, description = "List of webhook subscriptions", body = Vec<WebhookSubscriptionResponse>)),
    tag = "Webhooks"
)]
pub async fn list_webhooks() {}

#[utoipa::path(
    post,
    path = "/webhooks",
    request_body = serde_json::Value,
    responses((status = 201, description = "Webhook created", body = WebhookSubscriptionResponse)),
    tag = "Webhooks"
)]
pub async fn create_webhook() {}

#[utoipa::path(
    delete,
    path = "/webhooks/{id}",
    params(("id" = String, Path, description = "Webhook subscription ID")),
    responses((status = 204, description = "Webhook deleted")),
    tag = "Webhooks"
)]
pub async fn delete_webhook() {}
