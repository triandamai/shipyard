use std::sync::Arc;
use std::time::Instant;
use dashmap::DashMap;
use axum::extract::FromRef;

use shipyard_common::config::AppConfig;
use shipyard_common::types::ApiResponse;
use shipyard_docker::engine::DockerEngine;
use shipyard_mqtt::MqttPublisher;
use tokio::sync::Notify;

use middleware::rate_limit::SharedRateLimiter;

use shipyard_registry::{storage::StorageBackend, RegistryState};

pub mod admin;
pub mod alerts;
pub mod auth;
pub mod cache;
pub mod compose;
pub mod email;
pub mod error;
pub mod routes;
pub mod setup;
pub mod orgs;
pub mod projects;
pub mod services;
pub mod resources;
pub mod containers;
pub mod deployments;
pub mod topology;
pub mod logs;
pub mod middleware;
pub mod templates;
pub mod webhooks;
pub mod settings;
pub mod shorthand;
pub mod dbclient;
pub mod static_site;
pub mod git_providers;
pub mod billing;
pub mod nodes;
pub mod plans;
pub mod provisioning;
pub mod compute;
pub mod edge_functions;
pub mod artifactory;
pub mod artifact_source;
pub mod sandbox_runtime;

/// Short-lived OAuth state entries keyed by state UUID → (provider, org_id, created_at).
/// `org_id` is passed through the flow so the callback redirect lands on the right org settings page.
pub type OAuthStates = Arc<DashMap<String, (String, Option<String>, Instant)>>;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: sqlx::PgPool,
    pub docker: Arc<dyn DockerEngine>,
    pub mqtt: Arc<MqttPublisher>,
    pub oauth_states: OAuthStates,
    pub redis: Option<redis::aio::ConnectionManager>,
    /// Shared HTTP client — reuses the connection pool across all outbound requests.
    pub http_client: reqwest::Client,
    /// Tight per-IP rate limiter for /auth/login and /auth/register (10 req/min).
    pub auth_limiter: SharedRateLimiter,
    /// Notified whenever a deployment completes so the Swarm sync loop wakes immediately.
    pub swarm_sync_trigger: Arc<Notify>,
    /// Artifact registry storage backend (local or S3).
    pub registry_storage: Arc<dyn StorageBackend>,
}

/// Allow registry route handlers typed `State<RegistryState>` to be used inside
/// the main `Router<AppState>` — axum calls this to extract the sub-state.
impl FromRef<AppState> for RegistryState {
    fn from_ref(s: &AppState) -> Self {
        RegistryState {
            db:         s.db.clone(),
            storage:    Arc::clone(&s.registry_storage),
            hostname:   s.config.registry.hostname.clone(),
            jwt_secret: s.config.auth.jwt_secret.clone(),
        }
    }
}
