use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::{
    auth::{ApiKeyUser, SCOPE_READ},
    error::OpenApiError,
    response::{OkResponse, PageParams, PageResponse},
    OpenApiState,
};

pub fn routes() -> Router<OpenApiState> {
    Router::new()
        .route("/projects", get(list_projects))
        .route("/projects/:project_id", get(get_project))
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ProjectResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /openapi/v1/projects
async fn list_projects(
    caller: ApiKeyUser,
    Query(page): Query<PageParams>,
    State(state): State<OpenApiState>,
) -> Result<Json<PageResponse<ProjectResponse>>, OpenApiError> {
    caller.require_scope(SCOPE_READ)?;

    let total: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM projects WHERE org_id = $1",
    )
    .bind(caller.org_id)
    .fetch_one(&state.db)
    .await?;

    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, String, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, org_id, name, slug, created_at, updated_at
         FROM projects
         WHERE org_id = $1
         ORDER BY created_at DESC
         LIMIT $2 OFFSET $3",
    )
    .bind(caller.org_id)
    .bind(page.limit())
    .bind(page.offset())
    .fetch_all(&state.db)
    .await?;

    let projects = rows
        .into_iter()
        .map(|(id, org_id, name, slug, created_at, updated_at)| ProjectResponse {
            id, org_id, name, slug, created_at, updated_at,
        })
        .collect();

    Ok(Json(PageResponse::new(projects, total.0, page.page, page.per_page)))
}

/// GET /openapi/v1/projects/:project_id
async fn get_project(
    caller: ApiKeyUser,
    Path(project_id): Path<Uuid>,
    State(state): State<OpenApiState>,
) -> Result<Json<OkResponse<ProjectResponse>>, OpenApiError> {
    caller.require_scope(SCOPE_READ)?;

    let row = sqlx::query_as::<_, (Uuid, Uuid, String, String, DateTime<Utc>, DateTime<Utc>)>(
        "SELECT id, org_id, name, slug, created_at, updated_at
         FROM projects
         WHERE id = $1 AND org_id = $2",
    )
    .bind(project_id)
    .bind(caller.org_id)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| OpenApiError::NotFound(format!("Project '{}' not found", project_id)))?;

    let (id, org_id, name, slug, created_at, updated_at) = row;
    Ok(Json(OkResponse::new(ProjectResponse { id, org_id, name, slug, created_at, updated_at })))
}
