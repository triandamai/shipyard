//! Webhook receivers for Git providers.
//!
//! Routes (no auth — the token in the URL is the authentication mechanism):
//!
//!   POST /webhooks/github/:service_id/:token
//!   POST /webhooks/gitlab/:service_id/:token
//!   POST /webhooks/gitea/:service_id/:token
//!
//! On a matching push event the handler triggers a deployment for the service,
//! identical to `POST /projects/:project_id/services/:service_id/deploy`.

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    routing::post,
    Json, Router,
};
use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::sync::Arc;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_engine::DeploymentEngine;

use crate::error::ApiAppError;
use crate::AppState;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/github/:service_id/:token", post(github_webhook))
        .route("/gitlab/:service_id/:token", post(gitlab_webhook))
        .route("/gitea/:service_id/:token", post(gitea_webhook))
}

// ─── GitHub ───────────────────────────────────────────────────────────────────

/// POST /webhooks/github/:service_id/:token
async fn github_webhook(
    Path((service_id, token)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    // 1. Load webhook token from service_envs
    let webhook_secret = load_webhook_token(&state, service_id).await?;

    // 2. Verify token in URL
    if token != webhook_secret {
        return Err(ApiAppError(AppError::Unauthorized(
            "Invalid webhook token".to_string(),
        )));
    }

    // 3. Optional HMAC-SHA256 signature verification (X-Hub-Signature-256)
    let sig_header = headers
        .get("x-hub-signature-256")
        .and_then(|v| v.to_str().ok());

    if !verify_github_signature(&webhook_secret, &body, sig_header) {
        return Err(ApiAppError(AppError::Unauthorized(
            "Webhook signature verification failed".to_string(),
        )));
    }

    // 4. Parse the push payload
    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {}", e))))?;

    let pushed_ref = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let pushed_branch = branch_from_ref(pushed_ref);

    handle_push(&state, service_id, pushed_branch, "github-webhook").await
}

// ─── GitLab ───────────────────────────────────────────────────────────────────

/// POST /webhooks/gitlab/:service_id/:token
async fn gitlab_webhook(
    Path((service_id, token)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    _headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let webhook_secret = load_webhook_token(&state, service_id).await?;

    if token != webhook_secret {
        return Err(ApiAppError(AppError::Unauthorized(
            "Invalid webhook token".to_string(),
        )));
    }

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {}", e))))?;

    // GitLab sends { "object_kind": "push", "ref": "refs/heads/main", ... }
    let kind = payload
        .get("object_kind")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    if kind != "push" {
        // Ignore non-push events gracefully
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "ignored non-push event",
            "object_kind": kind,
        }))));
    }

    let pushed_ref = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let pushed_branch = branch_from_ref(pushed_ref);

    handle_push(&state, service_id, pushed_branch, "gitlab-webhook").await
}

// ─── Gitea ────────────────────────────────────────────────────────────────────

/// POST /webhooks/gitea/:service_id/:token
async fn gitea_webhook(
    Path((service_id, token)): Path<(Uuid, String)>,
    State(state): State<AppState>,
    _headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let webhook_secret = load_webhook_token(&state, service_id).await?;

    if token != webhook_secret {
        return Err(ApiAppError(AppError::Unauthorized(
            "Invalid webhook token".to_string(),
        )));
    }

    let payload: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("invalid JSON: {}", e))))?;

    // Gitea push webhooks have the same shape as GitHub
    let pushed_ref = payload
        .get("ref")
        .and_then(|v| v.as_str())
        .unwrap_or_default();

    let pushed_branch = branch_from_ref(pushed_ref);

    handle_push(&state, service_id, pushed_branch, "gitea-webhook").await
}

// ─── Shared logic ─────────────────────────────────────────────────────────────

/// Extract the branch name from a Git ref like `refs/heads/main`.
fn branch_from_ref(git_ref: &str) -> &str {
    git_ref
        .strip_prefix("refs/heads/")
        .unwrap_or(git_ref)
}

/// Load the `__WEBHOOK_TOKEN__` value from `service_envs`.
async fn load_webhook_token(state: &AppState, service_id: Uuid) -> Result<String, ApiAppError> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT value_encrypted FROM service_envs
         WHERE service_id = $1 AND key = '__WEBHOOK_TOKEN__'
         LIMIT 1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let stored = row.map(|(v,)| v).ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!(
            "No webhook token configured for service '{service_id}'"
        )))
    })?;

    Ok(shipyard_common::crypto::decrypt_or_passthrough(
        &state.config.auth.secret_key,
        &stored,
    ))
}

/// Core logic: if auto_deploy is enabled and the pushed branch matches the
/// service's git_branch, trigger a deployment.
/// Returns 200 in all non-error cases so providers don't disable the webhook.
async fn handle_push(
    state: &AppState,
    service_id: Uuid,
    pushed_branch: &str,
    source_ref: &str,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    // Read git_branch and auto_deploy from the services table directly.
    let row: Option<(String, bool)> = sqlx::query_as::<_, (String, bool)>(
        "SELECT git_branch, auto_deploy FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (configured_branch, auto_deploy) = row.ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!("Service '{}' not found", service_id)))
    })?;

    if !auto_deploy {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "auto-deploy disabled for this service",
        }))));
    }

    if configured_branch != pushed_branch {
        tracing::debug!(
            service_id = %service_id,
            pushed_branch,
            configured_branch,
            "webhook: branch mismatch, skipping deploy"
        );
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "push ignored: branch does not match configured branch",
            "pushed_branch": pushed_branch,
            "configured_branch": configured_branch,
        }))));
    }

    tracing::info!(
        service_id = %service_id,
        pushed_branch,
        "webhook: triggering deployment"
    );

    let source_ref = source_ref.to_string();

    let deployment_id = uuid::Uuid::now_v7();
    sqlx::query(
        "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
         VALUES ($1, $2, $3, $4, 'running'::deployment_status, NOW())",
    )
    .bind(deployment_id)
    .bind(service_id)
    .bind("webhook")
    .bind(&source_ref)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let engine = DeploymentEngine::new(
        Arc::clone(&state.docker),
        state.db.clone(),
        Arc::clone(&state.mqtt),
        state.config.docker.label_prefix.clone(),
        state.config.traefik.network.clone(),
        state.config.auth.secret_key.clone(),
        state.config.docker.port_proxy,
        state.config.data_dir.clone(),
        state.config.static_server.retention_versions,
    );

    let webhook_notify = Arc::clone(&state.swarm_sync_trigger);
    tokio::spawn(async move {
        if let Err(e) = engine.deploy(deployment_id, service_id, "webhook", &source_ref).await {
            tracing::error!(
                service_id = %service_id,
                deployment_id = %deployment_id,
                error = %e,
                "webhook-triggered deployment failed"
            );
        }
        webhook_notify.notify_one();
    });

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "deployment triggered",
        "service_id": service_id,
        "branch": pushed_branch,
    }))))
}

// ─── HMAC helpers ─────────────────────────────────────────────────────────────

/// Verify the `X-Hub-Signature-256` header from GitHub.
///
/// - If the header is absent (`signature` is `None`) the check is skipped and
///   `true` is returned (token-in-URL is still enforced above).
/// - If the header is present it must be `sha256=<hex>` and must match
///   `HMAC-SHA256(secret, body)`.
pub fn verify_github_signature(secret: &str, body: &[u8], signature: Option<&str>) -> bool {
    let sig_str = match signature {
        Some(s) => s,
        // Header absent → skip HMAC check (token validation already passed)
        None => return true,
    };

    let expected_hex = match sig_str.strip_prefix("sha256=") {
        Some(h) => h,
        None => return false,
    };

    let expected_bytes = match hex::decode(expected_hex) {
        Ok(b) => b,
        Err(_) => return false,
    };

    type HmacSha256 = Hmac<Sha256>;

    let mut mac = match HmacSha256::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);

    mac.verify_slice(&expected_bytes).is_ok()
}

// ─── Response helper ──────────────────────────────────────────────────────────

impl From<(StatusCode, Json<ApiResponse<serde_json::Value>>)> for ApiAppError {
    fn from(_: (StatusCode, Json<ApiResponse<serde_json::Value>>)) -> Self {
        ApiAppError(AppError::Internal("unexpected conversion".to_string()))
    }
}
