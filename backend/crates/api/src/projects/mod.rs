use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, patch},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::models::Project;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchNodePositionsRequest {
    /// Map of node-id → canvas position, e.g. `{ "svc_uuid": { "x": 120, "y": 80 } }`
    pub positions: serde_json::Value,
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route(
            "/projects/:project_id",
            get(get_project).put(update_project).delete(delete_project),
        )
        .route(
            "/projects/:project_id/node-positions",
            patch(patch_node_positions),
        )
}

// ─── Helpers ──────────────────────────────────────────────────────────────────

/// Verify that the authenticated user is a member of the given org.
async fn require_org_member(
    db: &sqlx::PgPool,
    user_id: Uuid,
    org_id: Uuid,
) -> Result<(), ApiAppError> {
    let exists: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_members WHERE org_id = $1 AND user_id = $2",
    )
    .bind(org_id)
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if exists.is_none() {
        return Err(ApiAppError(AppError::Forbidden(
            "You are not a member of this organization".to_string(),
        )));
    }
    Ok(())
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /orgs/:org_id/projects
async fn list_projects(
    auth_user: AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Project>>>, ApiAppError> {
    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    let projects = sqlx::query_as::<_, Project>(
        "SELECT id, org_id, name, slug, directory_path, node_positions, created_at, updated_at
         FROM projects
         WHERE org_id = $1
         ORDER BY created_at ASC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(projects)))
}

/// POST /orgs/:org_id/projects
async fn create_project(
    auth_user: AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<(StatusCode, Json<ApiResponse<Project>>), ApiAppError> {
    if body.name.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("name is required".to_string())));
    }
    if body.slug.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("slug is required".to_string())));
    }

    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    // Verify org exists
    let org_exists: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM organizations WHERE id = $1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if org_exists.is_none() {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Organization '{}' not found",
            org_id
        ))));
    }

    let project_id = Uuid::now_v7();
    let directory_path = format!("{}/{}/{}", state.config.data_dir, org_id, project_id);

    let project = sqlx::query_as::<_, Project>(
        "INSERT INTO projects (id, org_id, name, slug, directory_path, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, NOW(), NOW())
         RETURNING id, org_id, name, slug, directory_path, node_positions, created_at, updated_at",
    )
    .bind(project_id)
    .bind(org_id)
    .bind(&body.name)
    .bind(&body.slug)
    .bind(&directory_path)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            ApiAppError(AppError::Conflict(format!(
                "A project with slug '{}' already exists in this organization",
                body.slug
            )))
        } else {
            ApiAppError(AppError::Database(msg))
        }
    })?;

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(project))))
}

/// GET /orgs/:org_id/projects/:project_id
async fn get_project(
    auth_user: AuthUser,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Project>>, ApiAppError> {
    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    let project = sqlx::query_as::<_, Project>(
        "SELECT id, org_id, name, slug, directory_path, node_positions, created_at, updated_at
         FROM projects
         WHERE id = $1 AND org_id = $2",
    )
    .bind(project_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Project '{}' not found", project_id))))?;

    Ok(Json(ApiResponse::ok(project)))
}

/// PUT /orgs/:org_id/projects/:project_id
async fn update_project(
    auth_user: AuthUser,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(body): Json<UpdateProjectRequest>,
) -> Result<Json<ApiResponse<Project>>, ApiAppError> {
    if body.name.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("name is required".to_string())));
    }

    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    let project = sqlx::query_as::<_, Project>(
        "UPDATE projects
         SET name = $1, updated_at = NOW()
         WHERE id = $2 AND org_id = $3
         RETURNING id, org_id, name, slug, directory_path, node_positions, created_at, updated_at",
    )
    .bind(&body.name)
    .bind(project_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Project '{}' not found", project_id))))?;

    Ok(Json(ApiResponse::ok(project)))
}

/// PATCH /orgs/:org_id/projects/:project_id/node-positions
async fn patch_node_positions(
    auth_user: AuthUser,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(body): Json<PatchNodePositionsRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    let rows = sqlx::query(
        "UPDATE projects SET node_positions = $1, updated_at = NOW()
         WHERE id = $2 AND org_id = $3",
    )
    .bind(&body.positions)
    .bind(project_id)
    .bind(org_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .rows_affected();

    if rows == 0 {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Project '{}' not found",
            project_id
        ))));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "ok": true }))))
}

/// DELETE /orgs/:org_id/projects/:project_id
///
/// Tears down all Docker Swarm services and Traefik configs for every service
/// in the project before deleting the project row (which cascades to services,
/// networks, volumes, domains, deployments, etc.).
async fn delete_project(
    auth_user: AuthUser,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_org_member(&state.db, auth_user.user_id, org_id).await?;

    // Verify project exists and belongs to org.
    let exists: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM projects WHERE id = $1 AND org_id = $2",
    )
    .bind(project_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if exists.is_none() {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Project '{}' not found",
            project_id
        ))));
    }

    // Collect every service (id + slug) so we can tear down Docker + Traefik.
    let services: Vec<(Uuid, String)> = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, slug FROM services WHERE project_id = $1",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    for (svc_id, svc_slug) in &services {
        // Remove Docker Swarm service (best-effort).
        let docker_name = format!("{}-{}", state.config.docker.label_prefix, svc_id);
        if let Err(e) = state.docker.remove_service(&docker_name).await {
            tracing::debug!(project_id = %project_id, service_id = %svc_id, "swarm remove: {e}");
        }

        // Remove Traefik dynamic config (best-effort).
        if let Some(dir) = state.config.traefik.dynamic_config_dir.as_deref() {
            let path = std::path::Path::new(dir).join(format!("{svc_slug}.yml"));
            let _ = tokio::fs::remove_file(&path).await;
        }
    }

    tracing::info!(
        project_id = %project_id,
        services_cleaned = services.len(),
        "Project Docker cleanup complete — deleting DB rows"
    );

    // Delete the project; FK CASCADE removes services, networks, volumes,
    // domains, deployments, deployment_logs, deployment_steps, service_envs, etc.
    sqlx::query("DELETE FROM projects WHERE id = $1 AND org_id = $2")
        .bind(project_id)
        .bind(org_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    crate::middleware::audit::write_audit_log(
        &state.db,
        Some(auth_user.user_id),
        "delete_project",
        Some("project"),
        Some(project_id),
        None,
        Some(serde_json::json!({ "org_id": org_id, "services_removed": services.len() })),
    ).await;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Project deleted successfully"
    }))))
}
