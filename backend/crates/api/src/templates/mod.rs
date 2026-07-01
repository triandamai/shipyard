use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_db::models::Template;

use crate::auth::AuthUser;
use crate::cache;
use crate::error::ApiAppError;
use crate::AppState;

// ─── Request types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateTemplateRequest {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[serde(rename = "type")]
    pub template_type: String,
    pub image: Option<String>,
    pub env: serde_json::Value,
    pub volumes: serde_json::Value,
    pub ports: serde_json::Value,
    pub icon: Option<String>,
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/templates", get(list_templates).post(create_template))
        .route("/templates/:id", get(get_template).delete(delete_template))
}

// ─── Handlers ─────────────────────────────────────────────────────────────────

/// GET /templates — list all templates (built-in first, then custom)
pub async fn list_templates(
    _auth_user: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<Template>>>, ApiAppError> {
    const CACHE_KEY: &str = "templates";
    const CACHE_TTL: u64 = 300;

    if let Some(cached) = cache::get(&state.redis, CACHE_KEY).await {
        if let Ok(templates) = serde_json::from_str::<Vec<Template>>(&cached) {
            return Ok(Json(ApiResponse::ok(templates)));
        }
    }

    let templates = sqlx::query_as::<_, Template>(
        r#"SELECT id, name, description, type, image, env, volumes, ports, icon, is_builtin, created_at
           FROM templates
           ORDER BY is_builtin DESC, name ASC"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if let Ok(json) = serde_json::to_string(&templates) {
        cache::set(&state.redis, CACHE_KEY, &json, CACHE_TTL).await;
    }

    Ok(Json(ApiResponse::ok(templates)))
}

/// GET /templates/:id — single template
async fn get_template(
    _auth_user: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Template>>, ApiAppError> {
    let template = sqlx::query_as::<_, Template>(
        r#"SELECT id, name, description, type, image, env, volumes, ports, icon, is_builtin, created_at
           FROM templates WHERE id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Template '{}' not found", id))))?;

    Ok(Json(ApiResponse::ok(template)))
}

/// POST /templates — create a custom template
async fn create_template(
    _auth_user: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<(StatusCode, Json<ApiResponse<Template>>), ApiAppError> {
    if body.id.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("id is required".to_string())));
    }
    if body.name.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("name is required".to_string())));
    }
    if body.template_type.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("type is required".to_string())));
    }

    let template = sqlx::query_as::<_, Template>(
        r#"INSERT INTO templates (id, name, description, type, image, env, volumes, ports, icon, is_builtin, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, false, NOW())
           RETURNING id, name, description, type, image, env, volumes, ports, icon, is_builtin, created_at"#,
    )
    .bind(&body.id)
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.template_type)
    .bind(&body.image)
    .bind(&body.env)
    .bind(&body.volumes)
    .bind(&body.ports)
    .bind(&body.icon)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            ApiAppError(AppError::Conflict(format!("Template '{}' already exists", body.id)))
        } else {
            ApiAppError(AppError::Database(msg))
        }
    })?;

    cache::del(&state.redis, "templates").await;

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(template))))
}

/// DELETE /templates/:id — delete a custom template (cannot delete built-ins)
async fn delete_template(
    _auth_user: AuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let result = sqlx::query(
        "DELETE FROM templates WHERE id = $1 AND is_builtin = false",
    )
    .bind(&id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if result.rows_affected() == 0 {
        let exists: Option<(bool,)> =
            sqlx::query_as("SELECT is_builtin FROM templates WHERE id = $1")
                .bind(&id)
                .fetch_optional(&state.db)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        return Err(match exists {
            Some(_) => ApiAppError(AppError::BadRequest(
                "Built-in templates cannot be deleted".to_string(),
            )),
            None => ApiAppError(AppError::NotFound(format!("Template '{}' not found", id))),
        });
    }

    cache::del(&state.redis, "templates").await;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Template deleted successfully"
    }))))
}
