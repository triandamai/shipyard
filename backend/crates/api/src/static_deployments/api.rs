use axum::{
    extract::{DefaultBodyLimit, Multipart, Path as AxumPath, Query, State},
    routing::{delete, get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac;
use crate::AppState;

use super::models::{
    validate_alias_name, validate_project_slug, StaticAlias, StaticDeployment,
    StaticDeploymentEvent, StaticProject, StaticProjectDomain,
};
use super::storage::{LocalStorage, StaticStorageBackend};
use super::traefik::sync_static_project_traefik_config;
use super::upload::ZipExtractor;
use super::verify::{verify_extracted_deployment, VerificationConfig};

// ─── DTOs ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateProjectRequest {
    pub name: String,
    pub slug: String,
    #[serde(default = "default_framework")]
    pub framework: String,
    #[serde(default)]
    pub spa_fallback: bool,
    pub preview_base_url: Option<String>,
}

fn default_framework() -> String {
    "custom".to_string()
}

#[derive(Debug, Deserialize)]
pub struct UpdateProjectRequest {
    pub name: Option<String>,
    pub framework: Option<String>,
    pub spa_fallback: Option<bool>,
    pub preview_base_url: Option<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct DeploymentResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub status: String,
    pub preview_url: String,
    pub preview_token: Option<String>,
    pub is_private: bool,
    pub archive_checksum_sha256: Option<String>,
    pub archive_size_bytes: Option<i64>,
    pub extracted_size_bytes: Option<i64>,
    pub file_count: Option<i32>,
    pub entry_file: String,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct SetAliasRequest {
    pub name: String,
    pub deployment_id: Uuid,
}

#[derive(Debug, Deserialize)]
pub struct AddDomainRequest {
    pub hostname: String,
    #[serde(default = "default_alias")]
    pub alias: String,
    #[serde(default = "default_true")]
    pub tls_enabled: bool,
    #[serde(default = "default_letsencrypt")]
    pub cert_provider: String,
}

fn default_alias() -> String {
    "production".to_string()
}

fn default_true() -> bool {
    true
}

fn default_letsencrypt() -> String {
    "letsencrypt".to_string()
}

#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        // Projects
        .route("/static/orgs/:org_id/projects", post(create_project).get(list_projects))
        .route("/static/projects/:project_id", get(get_project).put(update_project).delete(delete_project))
        // Deployments
        .route(
            "/static/projects/:project_id/deployments",
            post(upload_deployment)
                .layer(DefaultBodyLimit::disable())
                .get(list_deployments),
        )
        .route("/static/deployments/:deployment_id", get(get_deployment).delete(delete_deployment))
        .route("/static/deployments/:deployment_id/events", get(list_deployment_events))
        // Aliases
        .route("/static/projects/:project_id/aliases", get(list_aliases).post(set_alias))
        .route("/static/projects/:project_id/aliases/:alias_name", delete(delete_alias))
        // Domains
        .route("/static/projects/:project_id/domains", get(list_project_domains).post(add_project_domain))
        .route("/static/projects/:project_id/domains/:domain_id", delete(delete_project_domain))
}

// ─── RBAC Helper ─────────────────────────────────────────────────────────────

async fn check_org_access(state: &AppState, auth_user: &AuthUser, org_id: Uuid) -> Result<(), ApiAppError> {
    if auth_user.api_key_id.is_some() {
        return Ok(());
    }
    rbac::require_org_member(&state.db, auth_user.user_id, org_id)
        .await
        .map(|_| ())
        .map_err(ApiAppError)
}

async fn check_project_access(
    state: &AppState,
    auth_user: &AuthUser,
    project_id: Uuid,
) -> Result<StaticProject, ApiAppError> {
    let project: StaticProject = sqlx::query_as("SELECT * FROM static_projects WHERE id = $1")
        .bind(project_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| ApiAppError(AppError::NotFound(format!("Static project '{project_id}' not found"))))?;

    check_org_access(state, auth_user, project.org_id).await?;
    Ok(project)
}

// ─── Project Handlers ────────────────────────────────────────────────────────

async fn create_project(
    auth_user: AuthUser,
    AxumPath(org_id): AxumPath<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<CreateProjectRequest>,
) -> Result<Json<ApiResponse<StaticProject>>, ApiAppError> {
    check_org_access(&state, &auth_user, org_id).await?;
    validate_project_slug(&body.slug).map_err(ApiAppError)?;

    let id = Uuid::now_v7();
    let project: StaticProject = sqlx::query_as(
        r#"INSERT INTO static_projects (id, org_id, name, slug, framework, spa_fallback, preview_base_url)
           VALUES ($1, $2, $3, $4, $5, $6, $7)
           RETURNING *"#,
    )
    .bind(id)
    .bind(org_id)
    .bind(body.name)
    .bind(body.slug)
    .bind(body.framework)
    .bind(body.spa_fallback)
    .bind(body.preview_base_url)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(project)))
}

async fn list_projects(
    auth_user: AuthUser,
    AxumPath(org_id): AxumPath<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<StaticProject>>>, ApiAppError> {
    check_org_access(&state, &auth_user, org_id).await?;

    let projects: Vec<StaticProject> = sqlx::query_as(
        "SELECT * FROM static_projects WHERE org_id = $1 ORDER BY created_at DESC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(projects)))
}

async fn get_project(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<StaticProject>>, ApiAppError> {
    let project = check_project_access(&state, &auth_user, project_id).await?;
    Ok(Json(ApiResponse::ok(project)))
}

async fn update_project(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<UpdateProjectRequest>,
) -> Result<Json<ApiResponse<StaticProject>>, ApiAppError> {
    let project = check_project_access(&state, &auth_user, project_id).await?;

    let name = body.name.unwrap_or(project.name);
    let framework = body.framework.unwrap_or(project.framework);
    let spa_fallback = body.spa_fallback.unwrap_or(project.spa_fallback);
    let preview_base_url = match body.preview_base_url {
        Some(v) => v,
        None => project.preview_base_url,
    };

    let updated: StaticProject = sqlx::query_as(
        r#"UPDATE static_projects
           SET name = $1, framework = $2, spa_fallback = $3, preview_base_url = $4, updated_at = NOW()
           WHERE id = $5
           RETURNING *"#,
    )
    .bind(name)
    .bind(framework)
    .bind(spa_fallback)
    .bind(preview_base_url)
    .bind(project_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(updated)))
}

async fn delete_project(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    check_project_access(&state, &auth_user, project_id).await?;

    let storage = LocalStorage::new(&state.config.data_dir);
    let deployments: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM static_deployments WHERE project_id = $1")
        .bind(project_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    for dep_id in deployments {
        let _ = storage.delete_deployment(project_id, dep_id).await;
    }

    sqlx::query("DELETE FROM static_projects WHERE id = $1")
        .bind(project_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true }))))
}

// ─── Deployment Handlers ─────────────────────────────────────────────────────

async fn record_event(
    db: &sqlx::PgPool,
    deployment_id: Uuid,
    status: &str,
    message: &str,
    details: Option<serde_json::Value>,
) {
    let _ = sqlx::query(
        "INSERT INTO static_deployment_events (id, deployment_id, status, message, details, created_at)
         VALUES ($1, $2, $3, $4, $5, NOW())",
    )
    .bind(Uuid::now_v7())
    .bind(deployment_id)
    .bind(status)
    .bind(message)
    .bind(details)
    .execute(db)
    .await;
}

async fn upload_deployment(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<DeploymentResponse>>, ApiAppError> {
    let _project = check_project_access(&state, &auth_user, project_id).await?;

    let deployment_id = Uuid::now_v7();
    let mut zip_bytes: Option<Vec<u8>> = None;
    let mut artifact_url: Option<String> = None;
    let mut entry_file = "index.html".to_string();
    let mut is_private = false;
    let mut assign_alias: Option<String> = None;
    let mut metadata = serde_json::json!({});

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("Multipart read error: {e}"))))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "artifact" | "file" | "archive" => {
                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiAppError(AppError::BadRequest(format!("Failed to read archive bytes: {e}"))))?;
                zip_bytes = Some(data.to_vec());
            }
            "artifact_url" | "source_url" | "download_url" => {
                if let Ok(val) = field.text().await {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        artifact_url = Some(trimmed.to_string());
                    }
                }
            }
            "entry_file" => {
                if let Ok(val) = field.text().await {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        entry_file = trimmed.to_string();
                    }
                }
            }
            "is_private" => {
                if let Ok(val) = field.text().await {
                    is_private = val.trim().eq_ignore_ascii_case("true") || val.trim() == "1";
                }
            }
            "alias" => {
                if let Ok(val) = field.text().await {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        validate_alias_name(trimmed).map_err(ApiAppError)?;
                        assign_alias = Some(trimmed.to_string());
                    }
                }
            }
            "metadata" => {
                if let Ok(val) = field.text().await {
                    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&val) {
                        metadata = parsed;
                    }
                }
            }
            _ => {}
        }
    }

    let zip_bytes = match zip_bytes {
        Some(b) => b,
        None => {
            if let Some(ref url) = artifact_url {
                let max_bytes = state.config.integration.max_source_mb * 1024 * 1024;
                crate::download::download_url_to_bytes(&state.http_client, url, max_bytes)
                    .await
                    .map_err(ApiAppError)?
            } else {
                return Err(ApiAppError(AppError::BadRequest(
                    "No 'artifact' upload or 'artifact_url' provided".into(),
                )));
            }
        }
    };

    let preview_token = if is_private {
        Some(hex::encode(Sha256::digest(Uuid::now_v7().as_bytes())))
    } else {
        None
    };

    let storage = LocalStorage::new(&state.config.data_dir);
    let dest_dir = storage.deployment_dir(project_id, deployment_id);
    let storage_path = dest_dir.to_string_lossy().to_string();

    // 1. Initial Insert: PENDING
    sqlx::query(
        r#"INSERT INTO static_deployments
           (id, project_id, status, entry_file, storage_path, preview_token, is_private, metadata, created_at, updated_at)
           VALUES ($1, $2, 'PENDING', $3, $4, $5, $6, $7, NOW(), NOW())"#,
    )
    .bind(deployment_id)
    .bind(project_id)
    .bind(&entry_file)
    .bind(&storage_path)
    .bind(&preview_token)
    .bind(is_private)
    .bind(&metadata)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    record_event(&state.db, deployment_id, "PENDING", "Deployment initialized", None).await;

    // 2. Transition: UPLOADING
    sqlx::query("UPDATE static_deployments SET status = 'UPLOADING', updated_at = NOW() WHERE id = $1")
        .bind(deployment_id)
        .execute(&state.db)
        .await
        .ok();
    record_event(&state.db, deployment_id, "UPLOADING", "Extracting archive artifacts", None).await;

    let extractor = ZipExtractor::default();
    let extract_result = match extractor.extract_from_bytes(&zip_bytes, &dest_dir) {
        Ok(r) => r,
        Err(e) => {
            let err_msg = format!("Extraction failed: {e}");
            sqlx::query("UPDATE static_deployments SET status = 'FAILED', error_message = $1, updated_at = NOW() WHERE id = $2")
                .bind(&err_msg)
                .bind(deployment_id)
                .execute(&state.db)
                .await
                .ok();
            record_event(&state.db, deployment_id, "FAILED", &err_msg, None).await;
            return Err(ApiAppError(e));
        }
    };

    // 3. Transition: VERIFYING
    sqlx::query("UPDATE static_deployments SET status = 'VERIFYING', archive_checksum_sha256 = $1, archive_size_bytes = $2, extracted_size_bytes = $3, file_count = $4, updated_at = NOW() WHERE id = $5")
        .bind(&extract_result.checksum_sha256)
        .bind(zip_bytes.len() as i64)
        .bind(extract_result.extracted_size_bytes)
        .bind(extract_result.file_count)
        .bind(deployment_id)
        .execute(&state.db)
        .await
        .ok();
    record_event(&state.db, deployment_id, "VERIFYING", "Validating static deployment prerequisites", None).await;

    let v_cfg = VerificationConfig::default();
    if let Err(e) = verify_extracted_deployment(&dest_dir, &entry_file, &v_cfg) {
        let err_msg = format!("Verification failed: {e}");
        sqlx::query("UPDATE static_deployments SET status = 'FAILED', error_message = $1, updated_at = NOW() WHERE id = $2")
            .bind(&err_msg)
            .bind(deployment_id)
            .execute(&state.db)
            .await
            .ok();
        record_event(&state.db, deployment_id, "FAILED", &err_msg, None).await;
        return Err(ApiAppError(e));
    }

    // 4. Transition: READY
    sqlx::query("UPDATE static_deployments SET status = 'READY', updated_at = NOW() WHERE id = $1")
        .bind(deployment_id)
        .execute(&state.db)
        .await
        .ok();
    record_event(&state.db, deployment_id, "READY", "Deployment successfully verified and active", None).await;

    // Optional atomic alias assignment
    if let Some(alias) = &assign_alias {
        sqlx::query(
            r#"INSERT INTO static_aliases (id, project_id, name, deployment_id, created_at, updated_at)
               VALUES ($1, $2, $3, $4, NOW(), NOW())
               ON CONFLICT (project_id, name)
               DO UPDATE SET deployment_id = EXCLUDED.deployment_id, updated_at = NOW()"#,
        )
        .bind(Uuid::now_v7())
        .bind(project_id)
        .bind(alias)
        .bind(deployment_id)
        .execute(&state.db)
        .await
        .ok();
        record_event(
            &state.db,
            deployment_id,
            "ALIAS_ASSIGNED",
            &format!("Assigned to alias '{alias}'"),
            None,
        )
        .await;
    }

    let preview_url = format!("/dpl/{deployment_id}");
    let response = DeploymentResponse {
        id: deployment_id,
        project_id,
        status: "READY".to_string(),
        preview_url,
        preview_token,
        is_private,
        archive_checksum_sha256: Some(extract_result.checksum_sha256),
        archive_size_bytes: Some(zip_bytes.len() as i64),
        extracted_size_bytes: Some(extract_result.extracted_size_bytes),
        file_count: Some(extract_result.file_count),
        entry_file,
        error_message: None,
        metadata,
        created_at: chrono::Utc::now(),
    };

    Ok(Json(ApiResponse::ok(response)))
}

async fn list_deployments(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
    Query(pagination): Query<PaginationQuery>,
) -> Result<Json<ApiResponse<Vec<StaticDeployment>>>, ApiAppError> {
    check_project_access(&state, &auth_user, project_id).await?;

    let limit = pagination.limit.unwrap_or(20).clamp(1, 100);
    let offset = pagination.offset.unwrap_or(0).max(0);

    let deployments: Vec<StaticDeployment> = sqlx::query_as(
        "SELECT * FROM static_deployments WHERE project_id = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
    )
    .bind(project_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(deployments)))
}

async fn get_deployment(
    auth_user: AuthUser,
    AxumPath(deployment_id): AxumPath<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<StaticDeployment>>, ApiAppError> {
    let dep: StaticDeployment = sqlx::query_as("SELECT * FROM static_deployments WHERE id = $1")
        .bind(deployment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| ApiAppError(AppError::NotFound("Static deployment not found".into())))?;

    check_project_access(&state, &auth_user, dep.project_id).await?;
    Ok(Json(ApiResponse::ok(dep)))
}

async fn delete_deployment(
    auth_user: AuthUser,
    AxumPath(deployment_id): AxumPath<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let dep: StaticDeployment = sqlx::query_as("SELECT * FROM static_deployments WHERE id = $1")
        .bind(deployment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| ApiAppError(AppError::NotFound("Static deployment not found".into())))?;

    check_project_access(&state, &auth_user, dep.project_id).await?;

    // Mark as DELETING
    sqlx::query("UPDATE static_deployments SET status = 'DELETING', updated_at = NOW() WHERE id = $1")
        .bind(deployment_id)
        .execute(&state.db)
        .await
        .ok();
    record_event(&state.db, deployment_id, "DELETING", "Removing deployment assets", None).await;

    // Remove files from storage
    let storage = LocalStorage::new(&state.config.data_dir);
    let _ = storage.delete_deployment(dep.project_id, deployment_id).await;

    // Mark as DELETED
    sqlx::query("UPDATE static_deployments SET status = 'DELETED', updated_at = NOW() WHERE id = $1")
        .bind(deployment_id)
        .execute(&state.db)
        .await
        .ok();
    record_event(&state.db, deployment_id, "DELETED", "Deployment fully deleted", None).await;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true }))))
}

async fn list_deployment_events(
    auth_user: AuthUser,
    AxumPath(deployment_id): AxumPath<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<StaticDeploymentEvent>>>, ApiAppError> {
    let dep: StaticDeployment = sqlx::query_as("SELECT * FROM static_deployments WHERE id = $1")
        .bind(deployment_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| ApiAppError(AppError::NotFound("Static deployment not found".into())))?;

    check_project_access(&state, &auth_user, dep.project_id).await?;

    let events: Vec<StaticDeploymentEvent> = sqlx::query_as(
        "SELECT * FROM static_deployment_events WHERE deployment_id = $1 ORDER BY created_at ASC",
    )
    .bind(deployment_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(events)))
}

// ─── Alias Handlers ──────────────────────────────────────────────────────────

async fn list_aliases(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<StaticAlias>>>, ApiAppError> {
    check_project_access(&state, &auth_user, project_id).await?;

    let aliases: Vec<StaticAlias> = sqlx::query_as(
        "SELECT * FROM static_aliases WHERE project_id = $1 ORDER BY name ASC",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(aliases)))
}

async fn set_alias(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<SetAliasRequest>,
) -> Result<Json<ApiResponse<StaticAlias>>, ApiAppError> {
    check_project_access(&state, &auth_user, project_id).await?;
    validate_alias_name(&body.name).map_err(ApiAppError)?;

    // Ensure the targeted deployment exists and is READY
    let dep: StaticDeployment = sqlx::query_as(
        "SELECT * FROM static_deployments WHERE id = $1 AND project_id = $2",
    )
    .bind(body.deployment_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Target deployment not found for this project".into())))?;

    if !dep.typed_status().is_servable() {
        return Err(ApiAppError(AppError::BadRequest(
            "Cannot point alias to a deployment that is not READY".into(),
        )));
    }

    let alias: StaticAlias = sqlx::query_as(
        r#"INSERT INTO static_aliases (id, project_id, name, deployment_id, created_at, updated_at)
           VALUES ($1, $2, $3, $4, NOW(), NOW())
           ON CONFLICT (project_id, name)
           DO UPDATE SET deployment_id = EXCLUDED.deployment_id, updated_at = NOW()
           RETURNING *"#,
    )
    .bind(Uuid::now_v7())
    .bind(project_id)
    .bind(&body.name)
    .bind(body.deployment_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    record_event(
        &state.db,
        body.deployment_id,
        "ALIAS_POINTED",
        &format!("Alias '{}' atomically updated to point to this deployment", body.name),
        None,
    )
    .await;

    Ok(Json(ApiResponse::ok(alias)))
}

async fn delete_alias(
    auth_user: AuthUser,
    AxumPath((project_id, alias_name)): AxumPath<(Uuid, String)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    check_project_access(&state, &auth_user, project_id).await?;

    let res = sqlx::query("DELETE FROM static_aliases WHERE project_id = $1 AND name = $2")
        .bind(project_id)
        .bind(&alias_name)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if res.rows_affected() == 0 {
        return Err(ApiAppError(AppError::NotFound(format!("Alias '{alias_name}' not found"))));
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true }))))
}

// ─── Domain Handlers ──────────────────────────────────────────────────────────

async fn list_project_domains(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<StaticProjectDomain>>>, ApiAppError> {
    check_project_access(&state, &auth_user, project_id).await?;

    let domains: Vec<StaticProjectDomain> = sqlx::query_as(
        "SELECT * FROM static_project_domains WHERE project_id = $1 ORDER BY created_at ASC",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(domains)))
}

async fn add_project_domain(
    auth_user: AuthUser,
    AxumPath(project_id): AxumPath<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<AddDomainRequest>,
) -> Result<Json<ApiResponse<StaticProjectDomain>>, ApiAppError> {
    let project = check_project_access(&state, &auth_user, project_id).await?;
    let org_id = project.org_id;

    let hostname = body
        .hostname
        .trim()
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .to_ascii_lowercase();

    if hostname.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("Hostname cannot be empty".into())));
    }

    let router_name = crate::resources::hostname_to_router_name(&hostname);
    let domain_id = Uuid::now_v7();

    let mut domain: StaticProjectDomain = sqlx::query_as(
        r#"INSERT INTO static_project_domains
           (id, project_id, hostname, alias, tls_enabled, cert_provider, traefik_router_name, created_at, updated_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
           ON CONFLICT (hostname)
           DO UPDATE SET project_id = EXCLUDED.project_id, alias = EXCLUDED.alias, updated_at = NOW()
           RETURNING *"#,
    )
    .bind(domain_id)
    .bind(project_id)
    .bind(&hostname)
    .bind(&body.alias)
    .bind(body.tls_enabled)
    .bind(&body.cert_provider)
    .bind(&router_name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            ApiAppError(AppError::Conflict(format!("Domain '{hostname}' already exists")))
        } else {
            ApiAppError(AppError::Database(msg))
        }
    })?;

    // Best-effort Cloudflare DNS record
    if domain.cloudflare_record_id.is_none() {
        if let Some((zone_id, record_id)) =
            crate::resources::try_create_cloudflare_record_for_org(&state.db, org_id, &hostname).await
        {
            let _ = sqlx::query(
                "UPDATE static_project_domains SET cloudflare_zone_id = $1, cloudflare_record_id = $2 WHERE id = $3",
            )
            .bind(&zone_id)
            .bind(&record_id)
            .bind(domain.id)
            .execute(&state.db)
            .await;
            domain.cloudflare_zone_id = Some(zone_id);
            domain.cloudflare_record_id = Some(record_id);
        }
    }

    // Sync Traefik dynamic config
    sync_static_project_traefik_config(
        &state.db,
        project_id,
        state.config.traefik.dynamic_config_dir.as_deref(),
        &state.config.traefik.entrypoint_http,
        &state.config.traefik.entrypoint_https,
    )
    .await;

    Ok(Json(ApiResponse::ok(domain)))
}

async fn delete_project_domain(
    auth_user: AuthUser,
    AxumPath((project_id, domain_id)): AxumPath<(Uuid, Uuid)>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let project = check_project_access(&state, &auth_user, project_id).await?;
    let org_id = project.org_id;

    let row: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "DELETE FROM static_project_domains WHERE id = $1 AND project_id = $2 RETURNING cloudflare_zone_id, cloudflare_record_id",
    )
    .bind(domain_id)
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let Some((zone_id, record_id)) = row else {
        return Err(ApiAppError(AppError::NotFound("Domain not found".into())));
    };

    if let (Some(zid), Some(rid)) = (zone_id, record_id) {
        crate::resources::try_delete_cloudflare_record_for_org(&state.db, org_id, &zid, &rid).await;
    }

    // Sync Traefik dynamic config
    sync_static_project_traefik_config(
        &state.db,
        project_id,
        state.config.traefik.dynamic_config_dir.as_deref(),
        &state.config.traefik.entrypoint_http,
        &state.config.traefik.entrypoint_https,
    )
    .await;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "deleted": true }))))
}
