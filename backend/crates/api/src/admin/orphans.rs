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
    cleanup_unused_registry_artifacts, delete_orphan, get_cleanup_settings, list_orphans,
    save_cleanup_settings, scan_and_reconcile_orphans, CleanupSettings, OrphanResource,
    RegistryCleanupReport,
};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/", get(get_orphans))
        .route("/scan", post(trigger_scan))
        .route("/settings", get(get_settings).put(update_settings))
        .route("/cleanup-registry", post(trigger_registry_cleanup))
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

    let mut items = list_orphans(
        &state.db,
        query.orphan_type.as_deref().filter(|s| !s.is_empty()),
        query.q.as_deref().filter(|s| !s.is_empty()),
    )
    .await
    .map_err(ApiAppError)?;

    // Automatically perform initial scan on first view if no orphans currently recorded
    if items.is_empty() && query.orphan_type.is_none() && query.q.is_none() {
        if let Ok(scanned) = scan_and_reconcile_orphans(
            &state.db,
            &state.config.data_dir,
            Some((&*state.docker, &state.config.docker.label_prefix)),
        )
        .await
        {
            items = scanned;
        }
    }

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

/// GET /admin/orphans/settings
async fn get_settings(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<CleanupSettings>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:view").await?;
    let settings = get_cleanup_settings(&state.db).await;
    Ok(Json(ApiResponse::ok(settings)))
}

/// PUT /admin/orphans/settings
async fn update_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CleanupSettings>,
) -> Result<Json<ApiResponse<CleanupSettings>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:manage").await?;
    save_cleanup_settings(&state.db, &body).await.map_err(ApiAppError)?;
    Ok(Json(ApiResponse::ok(body)))
}

/// POST /admin/orphans/cleanup-registry
async fn trigger_registry_cleanup(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<RegistryCleanupReport>>, ApiAppError> {
    require_admin_access(&state.db, auth.user_id, "shipyard:admin:infra:manage").await?;
    let settings = get_cleanup_settings(&state.db).await;
    let report = cleanup_unused_registry_artifacts(&state.db, settings.registry_unused_retention_days)
        .await
        .map_err(ApiAppError)?;
    Ok(Json(ApiResponse::ok(report)))
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
        let now = chrono::Utc::now();
        let item = OrphanResource {
            id: Uuid::now_v7(),
            orphan_type: shipyard_docker_worker::orphans::OrphanType::Git,
            identifier: "0190a4b2-0000-7000-8000-000000000001.git".to_string(),
            path: "/opt/shipyard/data/sources/0190a4b2-0000-7000-8000-000000000001.git".to_string(),
            size_bytes: 4096,
            details: Some("Orphan git repository".to_string()),
            detected_at: now,
            auto_destruct_at: now + chrono::Duration::days(30),
            updated_at: now,
        };

        let resp = OrphanListResponse {
            items: vec![item],
            total_count: 1,
            total_size_bytes: 4096,
        };

        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("orphan_type"));
        assert!(json.contains("git"));
        assert!(json.contains("auto_destruct_at"));
        assert!(json.contains("4096"));
    }

    #[test]
    fn test_cleanup_settings_serialization() {
        let settings = CleanupSettings {
            orphan_retention_days: 14,
            orphan_auto_destruct_enabled: true,
            registry_unused_retention_days: 60,
            registry_auto_removal_enabled: false,
        };
        let json = serde_json::to_string(&settings).unwrap();
        assert!(json.contains("14"));
        assert!(json.contains("60"));
        assert!(json.contains("registry_auto_removal_enabled"));
    }
}
