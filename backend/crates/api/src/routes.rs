use axum::{
    routing::get,
    Router,
    Json,
};

use crate::AppState;
use crate::admin;
use crate::auth;
use crate::compose;
use crate::setup;
use crate::orgs;
use crate::projects;
use crate::services;
use crate::resources;
use crate::containers;
use crate::deployments;
use crate::topology;
use crate::logs;
use crate::templates;
use crate::webhooks;
use crate::settings;
use crate::shorthand;
use crate::dbclient;
use crate::static_site;
use crate::git_providers;
use crate::billing;
use crate::nodes;
use crate::plans;
use crate::edge_functions;
use shipyard_common::types::ApiResponse;

/// Build the main API router with all route groups.
///
/// Route groups completed by milestone:
/// - /auth/*  (Milestone 2.1)
/// - /setup/* (Milestone 2.2)
/// - /orgs/*  (Milestone 2.3)
/// - /projects/* (Milestone 2.4–2.9)
/// - /webhooks/* (Milestone 2.10)
///
/// NOTE: The `setup::require_initialized_middleware` is applied in main.rs
/// via `axum::middleware::from_fn_with_state(state.clone(), ...)` after the
/// router is built, so the real AppState is available.
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/status", get(api_status))
        .nest("/auth", auth::routes::routes())
        .merge(auth::oauth::routes())
        .nest("/setup", setup::routes())
        .nest("/orgs", orgs::routes())
        // Public invite routes — no auth required
        .nest("/invite", orgs::public_routes())
        // Projects — /orgs/:org_id/projects/...
        .nest("/orgs/:org_id", projects::routes())
        // Services — /projects/:project_id/services/...
        .merge(services::routes())
        // Domains, Volumes, Networks
        .merge(resources::routes())
        // Containers & Docker events
        .merge(containers::routes())
        // Deployments — /projects/:project_id/services/:service_id/deploy, /deployments, etc.
        .merge(deployments::routes())
        // Topology — /projects/:project_id/topology
        .nest("/projects/:project_id", topology::routes())
        // Logs & Replicas — /services/:service_id/logs, /replicas, /logs/stream
        .merge(logs::routes())
        // Templates — /templates, /templates/:id
        .merge(templates::routes())
        // Webhooks — /webhooks/github/:service_id/:token, etc.
        .nest("/webhooks", webhooks::routes())
        // Docker Compose import — /projects/:project_id/compose/import
        .merge(compose::routes())
        // Platform settings — /settings
        .merge(settings::routes())
        // Shorthand service routes — /services/:id/... (no project prefix)
        .merge(shorthand::routes())
        // DB client — /services/:service_id/db/meta, /db/query
        .merge(dbclient::routes())
        // Static site — /services/:service_id/static/config, /static/upload
        .merge(static_site::routes())
        // Git providers — org-scoped Git integrations
        .merge(git_providers::routes())
        // Billing webhook — /billing/webhooks
        .merge(billing::routes())
        // Billing org routes — /orgs/:org_id/billing
        .nest("/orgs/:org_id", billing::org_routes())
        // Nodes — /orgs/:org_id/nodes
        .nest("/orgs/:org_id", nodes::routes())
        // Edge functions — /orgs/:org_id/edge-functions
        .nest("/orgs/:org_id/edge-functions", edge_functions::routes())
        // Internal: runtime container endpoints — /internal/...
        .nest("/internal", edge_functions::internal_routes())
        // Plans — GET /plans (public, no auth)
        .merge(plans::routes())
        // Admin — /admin/...
        .nest("/admin", admin::routes())
}

async fn api_status() -> Json<ApiResponse<serde_json::Value>> {
    Json(ApiResponse::ok(serde_json::json!({
        "name": "Shipyard PaaS",
        "version": env!("CARGO_PKG_VERSION"),
        "initialized": false,
    })))
}
