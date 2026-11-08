use axum::{
    extract::{Path, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::admin::require_admin_access;
use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_docker_worker::orphans::{
    delete_orphan, list_orphans, scan_and_reconcile_orphans, OrphanResource,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_orphans))
        .route("/scan", post(trigger_scan))
        .route("/delete-batch", post(delete_batch))
        .route("/:id", delete(delete_single))
}

#[derive(Debug, Deserialize)]
pub struct OrphanQuery {
    pub orphan_type: Option<String>,
    pub q: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct OrphanListResponse {
    pub items: Vec<OrphanResource>,
    pub total_count: usize,
    pub total_size_bytes: i64,
}

#[derive(Debug, Deserialize)]
pub struct DeleteBatchRequest {
    pub ids: Vec<Uuid>,
}

/// GET /admin/orphans
async fn get_orphans(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<OrphanQuery>,
) -> Result<Json<ApiResponse<OrphanListResponse>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:view").await?;

    let items = list_orphans(
        &state.db,
        query.orphan_type.as_deref().filter(|s| !s.is_empty()),
        query.q.as_deref().filter(|s| !s.is_empty()),
    )
    .await
    .map_err(ApiAppError)?;

    let total_count = items.len();
    let total_size_bytes: i64 = items.iter().map(|i| i.size_bytes).sum();

    Ok(Json(ApiResponse::ok(OrphanListResponse {
        items,
        total_count,
        total_size_bytes,
    })))
}

/// POST /admin/orphans/scan
async fn trigger_scan(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<OrphanListResponse>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:manage").await?;

    let items = scan_and_reconcile_orphans(
        &state.db,
        &state.config.data_dir,
        Some((&*state.docker, &state.config.docker.label_prefix)),
    )
    .await
    .map_err(ApiAppError)?;

    let total_count = items.len();
    let total_size_bytes: i64 = items.iter().map(|i| i.size_bytes).sum();

    Ok(Json(ApiResponse::ok(OrphanListResponse {
        items,
        total_count,
        total_size_bytes,
    })))
}

/// DELETE /admin/orphans/:id
async fn delete_single(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(orphan_id): Path<Uuid>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:manage").await?;

    delete_orphan(
        &state.db,
        orphan_id,
        &state.config.data_dir,
        Some(&*state.docker),
    )
    .await
    .map_err(ApiAppError)?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true, "id": orphan_id }))))
}

/// POST /admin/orphans/delete-batch
async fn delete_batch(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(payload): Json<DeleteBatchRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:manage").await?;

    let mut deleted_count = 0;
    let mut errors = Vec::new();

    for id in payload.ids {
        match delete_orphan(
            &state.db,
            id,
            &state.config.data_dir,
            Some(&*state.docker),
        )
        .await
        {
            Ok(_) => deleted_count += 1,
            Err(e) => errors.push(format!("{id}: {e}")),
        }
    }

    if !errors.is_empty() && deleted_count == 0 {
        return Err(ApiAppError(AppError::Internal(format!(
            "Failed to delete orphans: {}",
            errors.join(", ")
        ))));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "deleted_count": deleted_count,
        "errors": errors,
    }))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_orphan_routes_build() {
        let _router: Router<AppState> = routes();
    }

    #[test]
    fn test_orphan_response_serialization() {
        let item = OrphanResource {
            id: Uuid::now_v7(),
            orphan_type: shipyard_docker_worker::orphans::OrphanType::Org,
            identifier: "0190a4b2-0000-7000-8000-000000000001".to_string(),
            path: "/opt/shipyard/data/0190a4b2-0000-7000-8000-000000000001".to_string(),
            size_bytes: 4096,
            details: Some("Orphan org directory".to_string()),
            detected_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let resp = OrphanListResponse {
            items: vec![item],
            total_count: 1,
            total_size_bytes: 4096,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("orphan_type"));
        assert!(json.contains("org"));
        assert!(json.contains("4096"));
    }
}
