use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::error::ApiAppError;
use crate::middleware::rbac::require_service_access;
use crate::AppState;

use super::manager;

#[derive(Debug, Serialize)]
struct SandboxStatusResponse {
    status: String,
    preview_url: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/apps/:service_id/sandbox/start", post(start))
        .route("/apps/:service_id/sandbox/stop", post(stop))
        .route("/apps/:service_id/sandbox/heartbeat", post(heartbeat))
}

async fn start(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SandboxStatusResponse>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let instance = manager::start_sandbox(&state, service_id).await?;
    Ok(Json(ApiResponse::ok(SandboxStatusResponse {
        status: instance.status,
        preview_url: instance.preview_url,
    })))
}

async fn stop(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    manager::stop_sandbox(&state, service_id).await?;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "status": "stopped" }))))
}

async fn heartbeat(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    manager::heartbeat(&state, service_id).await?;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "ok": true }))))
}

// ── Public: cold-preview auto-start ─────────────────────────────────────────
//
// Preview links must work for people without editor access, so this endpoint
// takes no required auth (OptionalAuthUser is accepted but unused today —
// reserved for a future "private preview" check) and is idempotent: a second
// call while already starting/running just returns current state, so an
// anonymous visitor can trigger a start but never inflate or bypass the
// owning org's quota (start_sandbox's quota check still applies).

pub fn public_routes() -> Router<AppState> {
    Router::new().route("/preview/:slug/start", post(public_start))
}

async fn public_start(
    _maybe_auth: OptionalAuthUser,
    Path(slug): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SandboxStatusResponse>>, ApiAppError> {
    let service_id: Uuid = sqlx::query_scalar("SELECT id FROM services WHERE slug = $1 AND type = 'sandbox_app'")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| ApiAppError(AppError::NotFound(format!("App '{slug}' not found"))))?;

    let instance = manager::start_sandbox(&state, service_id).await?;
    Ok(Json(ApiResponse::ok(SandboxStatusResponse {
        status: instance.status,
        preview_url: instance.preview_url,
    })))
}
