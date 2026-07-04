use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::post,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

// ─── Compose YAML types (minimal subset) ─────────────────────────────────────

#[derive(Debug, Deserialize, Default)]
struct ComposeFile {
    #[allow(dead_code)]
    version: Option<String>,
    services: Option<HashMap<String, ComposeService>>,
    networks: Option<HashMap<String, Option<ComposeNetwork>>>,
}

#[derive(Debug, Deserialize, Default)]
struct ComposeService {
    image: Option<String>,
    build: Option<Value>,
    environment: Option<Value>,
    replicas: Option<u32>,
    /// Optional list or map of network names this service connects to.
    networks: Option<Value>,
}

#[derive(Debug, Deserialize, Default)]
struct ComposeNetwork {
    driver: Option<String>,
    external: Option<bool>,
}

// ─── API types ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ImportComposeRequest {
    /// Raw docker-compose YAML content.
    pub compose_yaml: String,
    /// Display name for the root (parent) service that owns this stack.
    pub root_name: String,
    /// URL-safe slug for the root service.
    pub root_slug: String,
}

#[derive(Debug, Serialize)]
pub struct ImportComposeResponse {
    /// UUID of the root docker_compose service created.
    pub root_service_id: Uuid,
    pub services_created: usize,
    pub networks_created: usize,
    /// UUIDs of the child services (one per compose `services:` entry).
    pub service_ids: Vec<Uuid>,
    pub warnings: Vec<String>,
}

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new().route("/projects/:project_id/compose/import", post(import_compose))
}

// ─── Handler ─────────────────────────────────────────────────────────────────

async fn import_compose(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(project_id): Path<Uuid>,
    Json(body): Json<ImportComposeRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ImportComposeResponse>>), ApiAppError> {
    // Validate root name / slug
    if body.root_name.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("root_name is required".to_string())));
    }
    if body.root_slug.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("root_slug is required".to_string())));
    }

    let compose: ComposeFile = serde_yaml::from_str(&body.compose_yaml)
        .map_err(|e| ApiAppError(AppError::Validation(format!("Invalid compose YAML: {e}"))))?;

    // Verify project exists and caller is a member
    let project: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT p.id, p.org_id, p.directory_path FROM projects p WHERE p.id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (_, org_id, project_dir) = project
        .ok_or_else(|| ApiAppError(AppError::NotFound("Project not found".to_string())))?;

    let member_count: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM org_members WHERE org_id = $1 AND user_id = $2")
            .bind(org_id)
            .bind(auth.user_id)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if member_count.0 == 0 {
        return Err(ApiAppError(AppError::Forbidden(
            "Not a member of this organization".to_string(),
        )));
    }

    let mut tx = state
        .db
        .begin()
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let mut warnings = Vec::new();
    let mut service_ids = Vec::new();
    let mut networks_created = 0usize;

    // ── 1. Create root (docker_compose) service ───────────────────────────────

    let root_id = Uuid::now_v7();
    let root_dir = format!("{project_dir}/{root_id}/{}", body.root_slug);

    // Write compose file to disk now so the deploy engine can find it later.
    // We do this before the transaction so a disk error doesn't leave orphaned DB rows.
    tokio::fs::create_dir_all(&root_dir).await.map_err(|e| {
        ApiAppError(AppError::Internal(format!(
            "Failed to create stack directory '{root_dir}': {e}"
        )))
    })?;
    tokio::fs::write(format!("{root_dir}/docker-compose.yml"), &body.compose_yaml)
        .await
        .map_err(|e| {
            ApiAppError(AppError::Internal(format!(
                "Failed to write docker-compose.yml: {e}"
            )))
        })?;

    sqlx::query(
        r#"INSERT INTO services
               (id, project_id, name, slug, type, image, directory_path, ports,
                status, replicas, service_parent_id, created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'docker_compose'::service_type, '', $5, '[]'::jsonb,
                   'stopped', 0, NULL, NOW(), NOW())"#,
    )
    .bind(root_id)
    .bind(project_id)
    .bind(body.root_name.trim())
    .bind(body.root_slug.trim())
    .bind(&root_dir)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unique") || msg.contains("duplicate") {
            ApiAppError(AppError::Conflict(format!(
                "A service with slug '{}' already exists in this project",
                body.root_slug
            )))
        } else {
            ApiAppError(AppError::Database(msg))
        }
    })?;

    // ── 2. Import networks ────────────────────────────────────────────────────

    // name → DB uuid for every non-external network we create/find.
    let mut network_id_map: HashMap<String, Uuid> = HashMap::new();

    if let Some(networks) = &compose.networks {
        for (name, net_def) in networks {
            let net = net_def.as_ref();
            if net.and_then(|n| n.external).unwrap_or(false) {
                warnings.push(format!("Network '{name}' is external — skipped"));
                continue;
            }
            let driver = net
                .and_then(|n| n.driver.clone())
                .unwrap_or_else(|| "bridge".to_string());

            sqlx::query(
                "INSERT INTO networks (id, project_id, name, driver, subnet, created_at)
                 VALUES ($1, $2, $3, $4, '', NOW())
                 ON CONFLICT (project_id, name) DO NOTHING",
            )
            .bind(Uuid::now_v7())
            .bind(project_id)
            .bind(name)
            .bind(&driver)
            .execute(&mut *tx)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            // Fetch back the actual row id (may be a pre-existing one).
            let (net_id,): (Uuid,) = sqlx::query_as(
                "SELECT id FROM networks WHERE project_id = $1 AND name = $2",
            )
            .bind(project_id)
            .bind(name)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

            network_id_map.insert(name.clone(), net_id);
            networks_created += 1;
        }
    }

    // ── 3. Import child services ──────────────────────────────────────────────

    if let Some(services) = &compose.services {
        for (name, svc) in services {
            let image = svc.image.clone().unwrap_or_else(|| {
                let img = format!("{name}:latest");
                warnings.push(format!(
                    "Service '{name}' has no image — defaulting to '{img}'"
                ));
                img
            });

            if svc.build.is_some() {
                warnings.push(format!(
                    "Service '{name}' uses 'build:' — build-from-source is not yet supported; using image field"
                ));
            }

            let replicas = svc.replicas.unwrap_or(1) as i32;
            let slug: String = name
                .to_lowercase()
                .chars()
                .map(|c| if c.is_alphanumeric() { c } else { '-' })
                .collect();

            let child_id = Uuid::now_v7();
            let child_dir = format!("{project_dir}/{child_id}/{slug}");

            sqlx::query(
                r#"INSERT INTO services
                       (id, project_id, name, slug, type, image, directory_path, ports,
                        status, replicas, service_parent_id, created_at, updated_at)
                   VALUES ($1, $2, $3, $4, 'docker'::service_type, $5, $6, '[]'::jsonb,
                           'stopped', $7, $8, NOW(), NOW())"#,
            )
            .bind(child_id)
            .bind(project_id)
            .bind(name)
            .bind(&slug)
            .bind(&image)
            .bind(&child_dir)
            .bind(replicas)
            .bind(root_id)             // ← links child to root
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                ApiAppError(AppError::Database(format!(
                    "Failed to create service '{name}': {e}"
                )))
            })?;

            // Insert environment variables from compose
            let env_vars: HashMap<String, String> = match &svc.environment {
                Some(Value::Object(map)) => map
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect(),
                Some(Value::Array(arr)) => arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .filter_map(|s| {
                        let mut parts = s.splitn(2, '=');
                        Some((
                            parts.next()?.to_string(),
                            parts.next().unwrap_or("").to_string(),
                        ))
                    })
                    .collect(),
                _ => HashMap::new(),
            };

            for (key, value) in &env_vars {
                sqlx::query(
                    "INSERT INTO service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
                     VALUES ($1, $2, $3, $4, FALSE, NOW())
                     ON CONFLICT (service_id, key) DO UPDATE SET value_encrypted = EXCLUDED.value_encrypted",
                )
                .bind(Uuid::now_v7())
                .bind(child_id)
                .bind(key)
                .bind(value)
                .execute(&mut *tx)
                .await
                .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
            }

            // Link child to its compose networks in service_networks.
            // Resolve which networks this service connects to:
            // explicit list/map → those names; absent → all non-external networks.
            let svc_net_names: Vec<String> = match &svc.networks {
                Some(Value::Array(arr)) => arr
                    .iter()
                    .filter_map(|v| v.as_str())
                    .map(String::from)
                    .collect(),
                Some(Value::Object(map)) => map.keys().cloned().collect(),
                _ => network_id_map.keys().cloned().collect(),
            };

            for net_name in &svc_net_names {
                if let Some(&net_id) = network_id_map.get(net_name) {
                    sqlx::query(
                        "INSERT INTO service_networks (service_id, network_id)
                         VALUES ($1, $2)
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(child_id)
                    .bind(net_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
                }
            }

            service_ids.push(child_id);
        }
    }

    tx.commit()
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    crate::middleware::audit::write_audit_log(
        &state.db,
        Some(auth.user_id),
        "import_compose",
        Some("service"),
        Some(root_id),
        None,
        Some(serde_json::json!({
            "root_service_id": root_id,
            "services_created": service_ids.len(),
            "networks_created": networks_created,
        })),
    )
    .await;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(ImportComposeResponse {
            root_service_id: root_id,
            services_created: service_ids.len(),
            networks_created,
            service_ids,
            warnings,
        })),
    ))
}
