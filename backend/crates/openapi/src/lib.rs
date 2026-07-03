//! `shipyard-openapi` — Public REST API for external integrations.
//!
//! Mount point: `/openapi/v1`
//! Auth:        `Authorization: Bearer ship_<key>` or `X-API-Key: ship_<key>`
//! Scopes:      read | deploy | write | admin

pub mod auth;
pub mod error;
pub mod response;
pub mod routes;

use std::sync::Arc;

use axum::{http::StatusCode, routing::get, Json, Router};
use serde::Serialize;
use shipyard_common::config::AppConfig;
use shipyard_docker::engine::DockerEngine;
use shipyard_mqtt::MqttPublisher;

// ─── Shared state ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct OpenApiState {
    pub db:     sqlx::PgPool,
    pub config: Arc<AppConfig>,
    pub docker: Arc<dyn DockerEngine>,
    pub mqtt:   Arc<MqttPublisher>,
}

// ─── Router ───────────────────────────────────────────────────────────────────

/// Returns the Open API router (`Router<OpenApiState>`).
///
/// In `main.rs`, call `.with_state(openapi_state)` on the result to get a
/// `Router<()>` that can be mounted with `nest_service("/openapi/v1", ...)`.
pub fn routes() -> Router<OpenApiState> {
    Router::new()
        .route("/", get(api_info))
        .merge(routes::keys::routes())
        .merge(routes::orgs::routes())
        .merge(routes::projects::routes())
        .merge(routes::services::routes())
        .merge(routes::deployments::routes())
}

// ─── Info endpoint ────────────────────────────────────────────────────────────

#[derive(Serialize)]
struct ApiInfo {
    name: &'static str,
    version: &'static str,
    docs: &'static str,
}

async fn api_info() -> (StatusCode, Json<ApiInfo>) {
    (
        StatusCode::OK,
        Json(ApiInfo {
            name: "Shipyard Open API",
            version: "v1",
            docs: "https://shipyard.io/docs/api",
        }),
    )
}
