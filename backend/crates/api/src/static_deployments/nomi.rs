use axum::{
    extract::{Multipart, Path as AxumPath, State},
    Json, Router, routing::post,
};
use serde::Serialize;
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use uuid::Uuid;

use crate::error::ApiAppError;
use crate::integration::IntegrationCaller;
use crate::AppState;

use super::models::{
    validate_alias_name, validate_project_slug, StaticProject,
};
use super::storage::{LocalStorage, StaticStorageBackend};
use super::upload::ZipExtractor;
use super::verify::{verify_extracted_deployment, VerificationConfig};

#[derive(Debug, Serialize)]
pub struct NomiStaticPublishResponse {
    pub project_id: Uuid,
    pub project_slug: String,
    pub deployment_id: Uuid,
    pub status: String,
    pub preview_url: String,
    pub alias: Option<String>,
    pub alias_url: Option<String>,
    pub archive_size_bytes: i64,
    pub extracted_size_bytes: i64,
    pub file_count: i32,
    pub checksum_sha256: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/:org_id/static-publish", post(nomi_publish_static))
}

/// One-shot static publish handler for Nomi and external AI agents.
/// Autocreates the static project if missing, extracts & verifies the artifact,
/// atomically points the requested alias (default: "production"), and returns immediate preview URLs.
async fn nomi_publish_static(
    caller: IntegrationCaller,
    AxumPath(org_id): AxumPath<Uuid>,
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<ApiResponse<NomiStaticPublishResponse>>, ApiAppError> {
    caller.require("manage")?;

    let mut project_slug: Option<String> = None;
    let mut project_name: Option<String> = None;
    let mut alias = Some("production".to_string());
    let mut spa_fallback = true;
    let mut entry_file = "index.html".to_string();
    let mut zip_bytes: Option<Vec<u8>> = None;
    let mut metadata = serde_json::json!({ "source": "nomi" });

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
            "app_key" | "project_slug" | "slug" => {
                if let Ok(val) = field.text().await {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        project_slug = Some(trimmed.to_string());
                    }
                }
            }
            "name" | "project_name" => {
                if let Ok(val) = field.text().await {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        project_name = Some(trimmed.to_string());
                    }
                }
            }
            "alias" | "env" | "environment" => {
                if let Ok(val) = field.text().await {
                    let trimmed = val.trim();
                    if !trimmed.is_empty() {
                        alias = Some(trimmed.to_string());
                    }
                }
            }
            "spa_fallback" => {
                if let Ok(val) = field.text().await {
                    spa_fallback = val.trim().eq_ignore_ascii_case("true") || val.trim() == "1";
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

    let slug = project_slug.ok_or_else(|| {
        ApiAppError(AppError::BadRequest("Missing required 'project_slug' or 'app_key'".into()))
    })?;
    validate_project_slug(&slug).map_err(ApiAppError)?;

    if let Some(ref a) = alias {
        validate_alias_name(a).map_err(ApiAppError)?;
    }

    let zip_bytes = zip_bytes.ok_or_else(|| {
        ApiAppError(AppError::BadRequest("No 'artifact' or 'file' archive field found in upload".into()))
    })?;

    let name = project_name.unwrap_or_else(|| slug.clone());

    // 1. Upsert project
    let project: StaticProject = sqlx::query_as(
        r#"INSERT INTO static_projects (id, org_id, name, slug, framework, spa_fallback)
           VALUES ($1, $2, $3, $4, 'custom', $5)
           ON CONFLICT (org_id, slug)
           DO UPDATE SET name = EXCLUDED.name, spa_fallback = EXCLUDED.spa_fallback, updated_at = NOW()
           RETURNING *"#,
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(&name)
    .bind(&slug)
    .bind(spa_fallback)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 2. Extract & Verify
    let deployment_id = Uuid::now_v7();
    let storage = LocalStorage::new(&state.config.data_dir);
    let dest_dir = storage.deployment_dir(project.id, deployment_id);
    let storage_path = dest_dir.to_string_lossy().to_string();

    let extractor = ZipExtractor::default();
    let extract_result = extractor.extract_from_bytes(&zip_bytes, &dest_dir)
        .map_err(ApiAppError)?;

    let v_cfg = VerificationConfig::default();
    verify_extracted_deployment(&dest_dir, &entry_file, &v_cfg)
        .map_err(ApiAppError)?;

    // 3. Insert Ready Deployment
    sqlx::query(
        r#"INSERT INTO static_deployments
           (id, project_id, status, archive_checksum_sha256, archive_size_bytes, extracted_size_bytes,
            file_count, entry_file, storage_path, is_private, metadata, created_at, updated_at)
           VALUES ($1, $2, 'READY', $3, $4, $5, $6, $7, $8, FALSE, $9, NOW(), NOW())"#,
    )
    .bind(deployment_id)
    .bind(project.id)
    .bind(&extract_result.checksum_sha256)
    .bind(zip_bytes.len() as i64)
    .bind(extract_result.extracted_size_bytes)
    .bind(extract_result.file_count)
    .bind(&entry_file)
    .bind(&storage_path)
    .bind(&metadata)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // 4. Update Alias
    let alias_url = if let Some(ref a) = alias {
        sqlx::query(
            r#"INSERT INTO static_aliases (id, project_id, name, deployment_id, created_at, updated_at)
               VALUES ($1, $2, $3, $4, NOW(), NOW())
               ON CONFLICT (project_id, name)
               DO UPDATE SET deployment_id = EXCLUDED.deployment_id, updated_at = NOW()"#,
        )
        .bind(Uuid::now_v7())
        .bind(project.id)
        .bind(a)
        .bind(deployment_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        Some(format!("/alias/{}/{}", project.id, a))
    } else {
        None
    };

    caller.audit(
        &state.db,
        "static_publish",
        "static_deployment",
        Some(deployment_id),
        serde_json::json!({
            "project_id": project.id,
            "project_slug": project.slug,
            "alias": alias,
        }),
    )
    .await;

    let preview_url = format!("/dpl/{deployment_id}");
    Ok(Json(ApiResponse::ok(NomiStaticPublishResponse {
        project_id: project.id,
        project_slug: project.slug,
        deployment_id,
        status: "READY".to_string(),
        preview_url,
        alias,
        alias_url,
        archive_size_bytes: zip_bytes.len() as i64,
        extracted_size_bytes: extract_result.extracted_size_bytes,
        file_count: extract_result.file_count,
        checksum_sha256: extract_result.checksum_sha256,
    })))
}
