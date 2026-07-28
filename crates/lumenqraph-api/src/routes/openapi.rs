//! OpenAPI documentation routes: /openapi.json, /docs, /redoc

use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use serde_json::json;
use utoipa::OpenApi;
use utoipa_redoc::Redoc;
use utoipa_swagger_ui::SwaggerUi;

use crate::openapi::ApiDoc;

/// Serve the OpenAPI 3.1 specification as JSON at `/openapi.json`
pub async fn openapi_json() -> impl IntoResponse {
    (StatusCode::OK, axum::Json(ApiDoc::openapi()))
}

/// Serve the Swagger UI at `/docs`
pub fn swagger_ui() -> Router {
    SwaggerUi::new("/docs/swagger-ui").url("/openapi.json", ApiDoc::openapi())
}

/// Serve the Redoc UI at `/redoc`
pub fn redoc_ui() -> Router {
    Redoc::with_url("/redoc", ApiDoc::openapi())
}

/// OpenAPI documentation endpoint
pub fn router() -> Router {
    Router::new()
        .route("/openapi.json", get(openapi_json))
        .merge(swagger_ui())
        .merge(redoc_ui())
}
