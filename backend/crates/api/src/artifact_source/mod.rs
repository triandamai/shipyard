use axum::{
    extract::{Path, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac;
use crate::AppState;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/services/:service_id/artifact-source",
            get(get_artifact_source)
                .put(put_artifact_source)
                .delete(delete_artifact_source),
        )
}

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ArtifactSourceResponse {
    pub service_id:   Uuid,
    pub namespace_id: Uuid,
    pub repo:         String,
    pub tag:          String,
    pub updated_at:   DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct UpsertArtifactSourceRequest {
    pub namespace_id: Uuid,
    pub repo:         String,
    /// Defaults to "latest" when omitted.
    pub tag:          Option<String>,
}

// ─── RBAC helper ─────────────────────────────────────────────────────────────

async fn require_service_write(
    db: &sqlx::PgPool,
    user_id: Uuid,
    service_id: Uuid,
) -> Result<(), ApiAppError> {
    rbac::require_service_permission(db, user_id, service_id, "service:write")
        .await
        .map_err(ApiAppError)
}

// ─── GET /services/:service_id/artifact-source ────────────────────────────────

async fn get_artifact_source(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<ArtifactSourceResponse>>, ApiAppError> {
    require_service_write(&state.db, auth_user.user_id, service_id).await?;

    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, DateTime<Utc>)>(
        "SELECT service_id, namespace_id, repo, tag, updated_at
         FROM service_artifact_sources
         WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    match row {
        None => Err(ApiAppError(AppError::NotFound(
            "No artifact source configured for this service".into(),
        ))),
        Some((sid, namespace_id, repo, tag, updated_at)) => {
            Ok(Json(ApiResponse::ok(ArtifactSourceResponse {
                service_id: sid,
                namespace_id,
                repo,
                tag,
                updated_at,
            })))
        }
    }
}

// ─── PUT /services/:service_id/artifact-source ────────────────────────────────

async fn put_artifact_source(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<UpsertArtifactSourceRequest>,
) -> Result<Json<ApiResponse<ArtifactSourceResponse>>, ApiAppError> {
    require_service_write(&state.db, auth_user.user_id, service_id).await?;

    // Validate the referenced service exists.
    let svc_exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM services WHERE id = $1)")
        .bind(service_id)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    if !svc_exists {
        return Err(ApiAppError(AppError::NotFound("Service not found".into())));
    }

    // Validate the referenced namespace exists.
    let ns_exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM registry_namespaces WHERE id = $1)",
    )
    .bind(body.namespace_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    if !ns_exists {
        return Err(ApiAppError(AppError::NotFound(
            "Registry namespace not found".into(),
        )));
    }

    let tag = body.tag.unwrap_or_else(|| "latest".to_string());

    sqlx::query(
        "INSERT INTO service_artifact_sources (service_id, namespace_id, repo, tag, updated_at)
         VALUES ($1, $2, $3, $4, NOW())
         ON CONFLICT (service_id) DO UPDATE
           SET namespace_id = EXCLUDED.namespace_id,
               repo         = EXCLUDED.repo,
               tag          = EXCLUDED.tag,
               updated_at   = NOW()",
    )
    .bind(service_id)
    .bind(body.namespace_id)
    .bind(&body.repo)
    .bind(&tag)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Re-fetch for the canonical response (includes server-set updated_at).
    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, DateTime<Utc>)>(
        "SELECT service_id, namespace_id, repo, tag, updated_at
         FROM service_artifact_sources
         WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(ArtifactSourceResponse {
        service_id: row.0,
        namespace_id: row.1,
        repo: row.2,
        tag: row.3,
        updated_at: row.4,
    })))
}

// ─── DELETE /services/:service_id/artifact-source ────────────────────────────

async fn delete_artifact_source(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_write(&state.db, auth_user.user_id, service_id).await?;

    sqlx::query("DELETE FROM service_artifact_sources WHERE service_id = $1")
        .bind(service_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true }))))
}
