use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{
    error::ApiAppError,
    middleware::rbac,
    AppState,
};
use shipyard_common::error::AppError;
use shipyard_db::models::GitProvider;

#[derive(Debug, Deserialize)]
pub struct CreateGitProviderRequest {
    pub name: String,
    pub provider_type: String,
    pub auth_type: String,
    pub token: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/:org_id/git-providers", get(list_git_providers).post(create_git_provider))
        .route("/orgs/:org_id/git-providers/:id", axum::routing::delete(delete_git_provider))
}

async fn list_git_providers(
    auth: crate::auth::AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<crate::ApiResponse<Vec<GitProvider>>>, ApiAppError> {
    let can_read = rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:providers:read")).await.is_ok()
        || rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:settings:read")).await.is_ok();
    if !can_read {
        return Err(ApiAppError(AppError::Forbidden("providers:read or settings:read required".to_string())));
    }

    let providers = sqlx::query_as::<_, GitProvider>(
        "SELECT id, org_id, name, provider_type, auth_type, token, username, created_at, updated_at
         FROM git_providers
         WHERE org_id = $1
         ORDER BY name ASC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(crate::ApiResponse::ok(providers)))
}

async fn create_git_provider(
    auth: crate::auth::AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<CreateGitProviderRequest>,
) -> Result<(StatusCode, Json<crate::ApiResponse<GitProvider>>), ApiAppError> {
    let can_write = rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:providers:write")).await.is_ok()
        || rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:settings:write")).await.is_ok();
    if !can_write {
        return Err(ApiAppError(AppError::Forbidden("providers:write or settings:write required".to_string())));
    }

    if body.name.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("name is required".to_string())));
    }
    if body.token.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("token is required".to_string())));
    }

    let username = resolve_git_username(&body.provider_type, &body.token).await.ok();

    let provider_id = Uuid::now_v7();
    let provider = sqlx::query_as::<_, GitProvider>(
        "INSERT INTO git_providers (id, org_id, name, provider_type, auth_type, token, username, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
         RETURNING id, org_id, name, provider_type, auth_type, token, username, created_at, updated_at",
    )
    .bind(provider_id)
    .bind(org_id)
    .bind(&body.name)
    .bind(&body.provider_type)
    .bind(&body.auth_type)
    .bind(&body.token)
    .bind(username)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok((StatusCode::CREATED, Json(crate::ApiResponse::ok(provider))))
}

async fn delete_git_provider(
    auth: crate::auth::AuthUser,
    Path((org_id, provider_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<crate::ApiResponse<()>>, ApiAppError> {
    let can_write = rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:providers:write")).await.is_ok()
        || rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:settings:write")).await.is_ok();
    if !can_write {
        return Err(ApiAppError(AppError::Forbidden("providers:write or settings:write required".to_string())));
    }

    sqlx::query("DELETE FROM git_providers WHERE id = $1 AND org_id = $2")
        .bind(provider_id)
        .bind(org_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(crate::ApiResponse::ok(())))
}

async fn resolve_git_username(provider_type: &str, token: &str) -> Result<String, String> {
    let client = reqwest::Client::new();
    match provider_type {
        "github" => {
            #[derive(Deserialize)]
            struct GitHubUser { login: String }
            let res = client.get("https://api.github.com/user")
                .header("User-Agent", "shipyard-api")
                .header("Authorization", format!("token {}", token))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if res.status().is_success() {
                let user: GitHubUser = res.json().await.map_err(|e| e.to_string())?;
                Ok(format!("@{}", user.login))
            } else {
                Err(format!("GitHub API returned status: {}", res.status()))
            }
        }
        "gitlab" => {
            #[derive(Deserialize)]
            struct GitLabUser { username: String }
            let res = client.get("https://gitlab.com/api/v4/user")
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if res.status().is_success() {
                let user: GitLabUser = res.json().await.map_err(|e| e.to_string())?;
                Ok(format!("@{}", user.username))
            } else {
                Err(format!("GitLab API returned status: {}", res.status()))
            }
        }
        _ => Err("Unsupported provider type for username resolution".to_string()),
    }
}
