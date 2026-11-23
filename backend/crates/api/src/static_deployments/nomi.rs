use axum::{
    extract::{DefaultBodyLimit, FromRequest, Multipart, Path as AxumPath, Request, State},
    Json, Router, routing::post,
};
use serde::{Deserialize, Serialize};
use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use uuid::Uuid;

use crate::error::ApiAppError;
use crate::integration::IntegrationCaller;
use crate::AppState;

use crate::resources::hostname_to_router_name;
use super::models::{
    validate_alias_name, validate_project_slug, StaticProject,
};
use super::storage::{LocalStorage, StaticStorageBackend};
use super::traefik::sync_static_project_traefik_config;
use super::upload::ZipExtractor;
use super::verify::{verify_extracted_deployment, VerificationConfig};

#[derive(Debug, Deserialize)]
pub struct NomiStaticPublishJson {
    pub project_slug: Option<String>,
    pub app_key: Option<String>,
    pub slug: Option<String>,
    pub name: Option<String>,
    pub project_name: Option<String>,
    pub alias: Option<String>,
    pub env: Option<String>,
    pub environment: Option<String>,
    pub domain: Option<String>,
    pub hostname: Option<String>,
    pub custom_domain: Option<String>,
    pub spa_fallback: Option<bool>,
    pub entry_file: Option<String>,
    pub artifact_url: Option<String>,
    pub source_url: Option<String>,
    pub download_url: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct NomiStaticPublishResponse {
    pub project_id: Uuid,
    pub project_slug: String,
    pub deployment_id: Uuid,
    pub status: String,
    pub preview_url: String,
    pub alias: Option<String>,
    pub alias_url: Option<String>,
    pub domain: Option<String>,
    pub domain_url: Option<String>,
    pub archive_size_bytes: i64,
    pub extracted_size_bytes: i64,
    pub file_count: i32,
    pub checksum_sha256: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/orgs/:org_id/static-publish",
            post(nomi_publish_static).layer(DefaultBodyLimit::disable()),
        )
}

/// One-shot static publish handler for Nomi and external AI agents.
/// Autocreates the org (if missing for an integration key) and the static project if missing,
/// extracts & verifies the artifact, atomically points the requested alias (default: "production"),
/// and returns immediate preview URLs. Supports direct multipart artifact upload or remote artifact_url.
async fn nomi_publish_static(
    caller: IntegrationCaller,
    AxumPath(org_identifier): AxumPath<String>,
    State(state): State<AppState>,
    req: Request,
) -> Result<Json<ApiResponse<NomiStaticPublishResponse>>, ApiAppError> {
    caller.require("manage")?;
    let org_id =
        crate::integration::orgs::resolve_or_create_org(&state.db, &caller, &org_identifier)
            .await?;
    crate::deployments::check_billing_for_org(&state.db, org_id).await?;

    let (parts, body) = req.into_parts();
    let content_type = parts
        .headers
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let mut project_slug: Option<String> = None;
    let mut project_name: Option<String> = None;
    let mut alias = Some("production".to_string());
    let mut domain_input: Option<String> = None;
    let mut spa_fallback = true;
    let mut entry_file = "index.html".to_string();
    let mut zip_bytes: Option<Vec<u8>> = None;
    let mut artifact_url: Option<String> = None;
    let mut metadata = serde_json::json!({ "source": "nomi" });

    if content_type.contains("application/json") {
        let max_json_bytes = 2 * 1024 * 1024;
        let bytes = axum::body::to_bytes(body, max_json_bytes)
            .await
            .map_err(|e| ApiAppError(AppError::BadRequest(format!("Failed to read JSON body: {e}"))))?;
        let json: NomiStaticPublishJson = serde_json::from_slice(&bytes)
            .map_err(|e| ApiAppError(AppError::BadRequest(format!("Invalid JSON body: {e}"))))?;

        project_slug = json.project_slug.or(json.app_key).or(json.slug);
        project_name = json.name.or(json.project_name);
        if let Some(a) = json.alias.or(json.env).or(json.environment) {
            alias = Some(a);
        }
        domain_input = json.domain.or(json.hostname).or(json.custom_domain);
        if let Some(spa) = json.spa_fallback {
            spa_fallback = spa;
        }
        if let Some(entry) = json.entry_file {
            entry_file = entry;
        }
        artifact_url = json.artifact_url.or(json.source_url).or(json.download_url);
        if let Some(meta) = json.metadata {
            metadata = meta;
        }
    } else {
        let req = Request::from_parts(parts, body);
        let mut multipart = Multipart::from_request(req, &state)
            .await
            .map_err(|e| ApiAppError(AppError::BadRequest(format!("Multipart read error: {e}"))))?;

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
                "domain" | "hostname" | "custom_domain" => {
                    if let Ok(val) = field.text().await {
                        let trimmed = val.trim();
                        if !trimmed.is_empty() {
                            domain_input = Some(trimmed.to_string());
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
    }

    let slug = project_slug.ok_or_else(|| {
        ApiAppError(AppError::BadRequest("Missing required 'project_slug' or 'app_key'".into()))
    })?;
    validate_project_slug(&slug).map_err(ApiAppError)?;

    if let Some(ref a) = alias {
        validate_alias_name(a).map_err(ApiAppError)?;
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

    // 5. Handle Custom Domain (if requested)
    let (domain_hostname, domain_url) = if let Some(raw_domain) = domain_input {
        let hostname = raw_domain
            .trim()
            .trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_end_matches('/')
            .to_ascii_lowercase();

        if !hostname.is_empty() {
            let target_alias = alias.as_deref().unwrap_or("production");
            let router_name = hostname_to_router_name(&hostname);
            let domain_id = Uuid::now_v7();

            // Upsert into static_project_domains
            let mut domain_row: super::models::StaticProjectDomain = sqlx::query_as(
                r#"INSERT INTO static_project_domains
                   (id, project_id, hostname, alias, tls_enabled, cert_provider, traefik_router_name, created_at, updated_at)
                   VALUES ($1, $2, $3, $4, TRUE, 'letsencrypt', $5, NOW(), NOW())
                   ON CONFLICT (hostname)
                   DO UPDATE SET project_id = EXCLUDED.project_id, alias = EXCLUDED.alias, updated_at = NOW()
                   RETURNING *"#,
            )
            .bind(domain_id)
            .bind(project.id)
            .bind(&hostname)
            .bind(target_alias)
            .bind(&router_name)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            // Best-effort Cloudflare DNS record creation if org has connected Cloudflare
            if domain_row.cloudflare_record_id.is_none() {
                if let Some((zone_id, record_id)) =
                    crate::resources::try_create_cloudflare_record_for_org(&state.db, org_id, &hostname).await
                {
                    let _ = sqlx::query(
                        "UPDATE static_project_domains SET cloudflare_zone_id = $1, cloudflare_record_id = $2 WHERE id = $3",
                    )
                    .bind(&zone_id)
                    .bind(&record_id)
                    .bind(domain_row.id)
                    .execute(&state.db)
                    .await;
                    domain_row.cloudflare_zone_id = Some(zone_id);
                    domain_row.cloudflare_record_id = Some(record_id);
                }
            }

            // Sync Traefik dynamic configuration
            sync_static_project_traefik_config(
                &state.db,
                project.id,
                state.config.traefik.dynamic_config_dir.as_deref(),
                &state.config.traefik.entrypoint_http,
                &state.config.traefik.entrypoint_https,
            )
            .await;

            let full_url = format!("https://{hostname}");
            (Some(hostname), Some(full_url))
        } else {
            (None, None)
        }
    } else {
        (None, None)
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
            "domain": domain_hostname,
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
        domain: domain_hostname,
        domain_url,
        archive_size_bytes: zip_bytes.len() as i64,
        extracted_size_bytes: extract_result.extracted_size_bytes,
        file_count: extract_result.file_count,
        checksum_sha256: extract_result.checksum_sha256,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nomi_static_publish_json_deserialization() {
        let payload = serde_json::json!({
            "project_slug": "my-app",
            "artifact_url": "https://s3.amazonaws.com/bucket/dist.zip?sig=123",
            "alias": "staging",
            "domain": "app.example.com",
            "spa_fallback": false,
            "entry_file": "app.html",
            "metadata": { "commit": "abc1234" }
        });

        let parsed: NomiStaticPublishJson = serde_json::from_value(payload).unwrap();
        assert_eq!(parsed.project_slug.as_deref(), Some("my-app"));
        assert_eq!(
            parsed.artifact_url.as_deref(),
            Some("https://s3.amazonaws.com/bucket/dist.zip?sig=123")
        );
        assert_eq!(parsed.alias.as_deref(), Some("staging"));
        assert_eq!(parsed.domain.as_deref(), Some("app.example.com"));
        assert_eq!(parsed.spa_fallback, Some(false));
        assert_eq!(parsed.entry_file.as_deref(), Some("app.html"));
        assert_eq!(parsed.metadata.unwrap()["commit"], "abc1234");
    }
}
