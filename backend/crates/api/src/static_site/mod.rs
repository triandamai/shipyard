use std::sync::Arc;

use axum::{
    extract::{Multipart, Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_engine::DeploymentEngine;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac;
use crate::AppState;

// ─── Router ───────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/services/:service_id/static/config", get(get_static_config).put(put_static_config))
        .route("/services/:service_id/static/upload", post(upload_static))
}

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct StaticConfigResponse {
    pub service_id:      Uuid,
    pub source:          String,
    pub build_command:   String,
    pub output_dir:      String,
    pub node_version:    String,
    pub install_command: String,
    pub framework:       String,
    pub deploy_config:   Option<serde_json::Value>,
    pub git_deploy_strategy:    String,
    pub git_deploy_branch:      Option<String>,
    pub git_deploy_tag_pattern: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStaticConfigRequest {
    pub source:          Option<String>,
    pub build_command:   Option<String>,
    pub output_dir:      Option<String>,
    pub node_version:    Option<String>,
    pub install_command: Option<String>,
    pub framework:       Option<String>,
    pub git_deploy_strategy:    Option<String>,
    pub git_deploy_branch:      Option<Option<String>>,
    pub git_deploy_tag_pattern: Option<Option<String>>,
}

#[derive(Debug, Serialize)]
pub struct UploadResponse {
    pub deployment_id: Uuid,
}

// ─── RBAC helper ─────────────────────────────────────────────────────────────

async fn require_static_write(
    db: &sqlx::PgPool,
    user_id: Uuid,
    service_id: Uuid,
) -> Result<(), ApiAppError> {
    rbac::require_service_permission(db, user_id, service_id, "service:write")
        .await
        .map_err(ApiAppError)
}

// ─── GET /services/:service_id/static/config ─────────────────────────────────

async fn get_static_config(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<StaticConfigResponse>>, ApiAppError> {
    require_static_write(&state.db, auth_user.user_id, service_id).await?;

    // Upsert row — ensure it exists with defaults so the response is always non-null
    sqlx::query(
        "INSERT INTO static_site_configs (service_id) VALUES ($1)
         ON CONFLICT (service_id) DO NOTHING",
    )
    .bind(service_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let row = sqlx::query_as::<_, (Uuid, String, String, String, String, String, String, Option<serde_json::Value>, String, Option<String>, Option<String>)>(
        "SELECT service_id, source, build_command, output_dir, node_version,
                install_command, framework, deploy_config, git_deploy_strategy,
                git_deploy_branch, git_deploy_tag_pattern
         FROM static_site_configs WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(StaticConfigResponse {
        service_id:      row.0,
        source:          row.1,
        build_command:   row.2,
        output_dir:      row.3,
        node_version:    row.4,
        install_command: row.5,
        framework:       row.6,
        deploy_config:   row.7,
        git_deploy_strategy:    row.8,
        git_deploy_branch:      row.9,
        git_deploy_tag_pattern: row.10,
    })))
}

// ─── PUT /services/:service_id/static/config ─────────────────────────────────

async fn put_static_config(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<UpdateStaticConfigRequest>,
) -> Result<Json<ApiResponse<StaticConfigResponse>>, ApiAppError> {
    require_static_write(&state.db, auth_user.user_id, service_id).await?;

    // Ensure row exists
    sqlx::query(
        "INSERT INTO static_site_configs (service_id) VALUES ($1)
         ON CONFLICT (service_id) DO NOTHING",
    )
    .bind(service_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Apply partial updates
    if let Some(v) = &body.source {
        if v != "git" && v != "upload" {
            return Err(ApiAppError(AppError::BadRequest(
                "source must be 'git' or 'upload'".into(),
            )));
        }
        sqlx::query("UPDATE static_site_configs SET source = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = &body.build_command {
        sqlx::query("UPDATE static_site_configs SET build_command = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = &body.output_dir {
        sqlx::query("UPDATE static_site_configs SET output_dir = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = &body.node_version {
        sqlx::query("UPDATE static_site_configs SET node_version = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = &body.install_command {
        sqlx::query("UPDATE static_site_configs SET install_command = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = &body.framework {
        sqlx::query("UPDATE static_site_configs SET framework = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = &body.git_deploy_strategy {
        if v != "push" && v != "tag" && v != "pull_request" {
            return Err(ApiAppError(AppError::BadRequest(
                "git_deploy_strategy must be 'push', 'tag', or 'pull_request'".into(),
            )));
        }
        sqlx::query("UPDATE static_site_configs SET git_deploy_strategy = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = &body.git_deploy_branch {
        sqlx::query("UPDATE static_site_configs SET git_deploy_branch = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }
    if let Some(v) = &body.git_deploy_tag_pattern {
        sqlx::query("UPDATE static_site_configs SET git_deploy_tag_pattern = $1, updated_at = NOW() WHERE service_id = $2")
            .bind(v).bind(service_id).execute(&state.db).await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    // Re-fetch and return
    let row = sqlx::query_as::<_, (Uuid, String, String, String, String, String, String, Option<serde_json::Value>, String, Option<String>, Option<String>)>(
        "SELECT service_id, source, build_command, output_dir, node_version,
                install_command, framework, deploy_config, git_deploy_strategy,
                git_deploy_branch, git_deploy_tag_pattern
         FROM static_site_configs WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(StaticConfigResponse {
        service_id:      row.0,
        source:          row.1,
        build_command:   row.2,
        output_dir:      row.3,
        node_version:    row.4,
        install_command: row.5,
        framework:       row.6,
        deploy_config:   row.7,
        git_deploy_strategy:    row.8,
        git_deploy_branch:      row.9,
        git_deploy_tag_pattern: row.10,
    })))
}

// ─── POST /services/:service_id/static/upload ────────────────────────────────

async fn upload_static(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<UploadResponse>>, ApiAppError> {
    require_static_write(&state.db, auth_user.user_id, service_id).await?;

    // Verify this is actually a static service
    let svc_type: Option<(String,)> = sqlx::query_as::<_, (String,)>(
        "SELECT type::text FROM services WHERE id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    match svc_type {
        None => return Err(ApiAppError(AppError::NotFound("Service not found".into()))),
        Some((t,)) if t != "static" => {
            return Err(ApiAppError(AppError::BadRequest(
                "Upload is only supported for static services".into(),
            )))
        }
        _ => {}
    }

    let max_bytes = state.config.static_server.max_upload_mb * 1024 * 1024;
    let uploads_dir = format!(
        "{}/uploads/{}",
        state.config.static_server.sites_dir.as_deref()
            .unwrap_or(&format!("{}/static", state.config.data_dir)),
        service_id
    );
    tokio::fs::create_dir_all(&uploads_dir).await
        .map_err(|e| ApiAppError(AppError::Internal(format!("Cannot create uploads dir: {e}"))))?;

    let deployment_id = Uuid::now_v7();
    let mut artifact_path: Option<String> = None;
    let mut _deploy_message = String::new();

    while let Some(field) = multipart.next_field().await
        .map_err(|e| ApiAppError(AppError::BadRequest(format!("Multipart error: {e}"))))?
    {
        let field_name = field.name().unwrap_or("").to_string();

        if field_name == "message" {
            _deploy_message = field.text().await
                .unwrap_or_default()
                .chars().take(500).collect();
            continue;
        }

        if field_name != "artifact" {
            continue;
        }

        let fname = field.file_name().unwrap_or("artifact.zip").to_string();
        let is_zip = fname.ends_with(".zip");
        let is_tar = fname.ends_with(".tar.gz") || fname.ends_with(".tgz");
        if !is_zip && !is_tar {
            return Err(ApiAppError(AppError::BadRequest(
                "Artifact must be a .zip or .tar.gz file".into(),
            )));
        }

        let dest = format!("{uploads_dir}/{deployment_id}.zip");
        let mut file = tokio::fs::File::create(&dest).await
            .map_err(|e| ApiAppError(AppError::Internal(format!("Cannot create artifact file: {e}"))))?;

        let mut written: u64 = 0;
        let mut stream = field;
        // Read chunks and stream to disk — never buffer the whole file in RAM
        while let Some(chunk) = stream.chunk().await
            .map_err(|e| ApiAppError(AppError::BadRequest(format!("Upload error: {e}"))))?
        {
            written += chunk.len() as u64;
            if written > max_bytes {
                return Err(ApiAppError(AppError::BadRequest(format!(
                    "Upload exceeds {} MB limit",
                    state.config.static_server.max_upload_mb
                ))));
            }
            file.write_all(&chunk).await
                .map_err(|e| ApiAppError(AppError::Internal(format!("Write error: {e}"))))?;
        }
        file.flush().await.ok();
        artifact_path = Some(dest);
    }

    let _artifact = artifact_path.ok_or_else(|| {
        ApiAppError(AppError::BadRequest("No artifact field found in upload".into()))
    })?;

    let triggered_by = format!("user:{}", auth_user.user_id);

    let max_parallel = sqlx::query_as::<_, (String,)>(
        "SELECT value::text FROM system_config WHERE key = 'max_parallel_deployments'",
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten()
    .and_then(|(v,)| v.trim_matches('"').parse::<i64>().ok())
    .unwrap_or(2);

    let mut is_queued = false;

    if max_parallel > 0 {
        let running: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM deployments WHERE status = 'running'::deployment_status",
        )
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        if running.0 >= max_parallel {
            is_queued = true;
        }
    }

    let status = if is_queued { "queued" } else { "running" };

    // Insert deployment record
    sqlx::query(
        "INSERT INTO deployments
            (id, service_id, status, triggered_by, source_ref, created_at)
         VALUES ($1, $2, $3::deployment_status, $4, 'upload', NOW())",
    )
    .bind(deployment_id)
    .bind(service_id)
    .bind(status)
    .bind(&triggered_by)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Pre-insert deployment steps so the log panel can render them immediately.
    // NOTE: go_live (3) must appear before write_nginx_conf (4) so that the current
    // symlink exists when nginx reloads, avoiding a transient 404 window.
    let upload_steps = [
        (0i32, "extract_archive"),
        (1i32, "parse_shipyard_config"),
        (2i32, "publish_files"),
        (3i32, "go_live"),
        (4i32, "write_nginx_conf"),
        (5i32, "finalize"),
    ];
    for (order, name) in upload_steps {
        sqlx::query(
            "INSERT INTO deployment_steps
                (id, deployment_id, order_index, name, status)
             VALUES ($1, $2, $3, $4, 'pending'::step_status)",
        )
        .bind(Uuid::now_v7())
        .bind(deployment_id)
        .bind(order)
        .bind(name)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    if !is_queued {
        let engine = DeploymentEngine::new(
            Arc::clone(&state.docker),
            state.db.clone(),
            Arc::clone(&state.mqtt),
            state.config.docker.label_prefix.clone(),
            state.config.traefik.network.clone(),
            state.config.auth.secret_key.clone(),
            state.config.docker.port_proxy,
            state.config.data_dir.clone(),
            state.config.static_server.retention_versions,
        );
        tokio::spawn(async move {
            if let Err(e) = engine.deploy(deployment_id, service_id, &triggered_by, "upload").await {
                tracing::error!(deployment_id = %deployment_id, "Upload deployment error: {e}");
            }
        });
    }

    Ok(Json(ApiResponse::ok(UploadResponse { deployment_id })))
}
