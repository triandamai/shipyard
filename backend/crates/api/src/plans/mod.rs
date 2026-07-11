use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use uuid::Uuid;

use shipyard_common::{error::AppError, types::ApiResponse};

use crate::{error::ApiAppError, AppState};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PlanResponse {
    pub id: Uuid,
    pub name: String,
    pub enabled: bool,
    pub cpu_cores: i32,
    pub memory_gb: i32,
    pub max_replicas: i32,
    pub node_count: i32,
    pub max_members: i32,
    pub max_projects: i32,
    pub max_orgs: i32,
    pub max_parallel_deployments: i32,
    pub max_git_providers: i32,
    pub price_monthly: f64,
}

pub fn routes() -> Router<AppState> {
    Router::new().route("/plans", get(list_plans))
}

/// GET /plans — public, no auth required.
/// Returns all enabled plans ordered by monthly price.
async fn list_plans(
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<PlanResponse>>>, ApiAppError> {
    let plans: Vec<PlanResponse> = sqlx::query_as::<_, PlanResponse>(
        r#"SELECT id, name, enabled, cpu_cores, memory_gb, max_replicas, node_count,
                  max_members, max_projects, max_orgs, max_parallel_deployments,
                  max_git_providers, price_monthly
           FROM plans
           WHERE enabled = true
           ORDER BY price_monthly ASC"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(plans)))
}
