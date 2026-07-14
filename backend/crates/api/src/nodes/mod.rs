use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use std::sync::Arc;

use shipyard_engine::DeploymentEngine;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::AppState;

// ─── Request / Response types ─────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateNodeRequest {
    pub name: String,
    pub provider: String,
    pub region: String,
    pub cpu_cores: Option<i32>,
    pub ram_mb: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct ComputeNodeResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub provider: String,
    pub provider_vm_id: Option<String>,
    pub region: String,
    pub ip_address: Option<String>,
    pub public_ip: Option<String>,
    pub status: String,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub cpu_cores: i32,
    pub ram_mb: i32,
    pub provision_error: Option<String>,
    pub provision_attempts: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Router ──────────────────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/nodes", get(list_nodes).post(create_node))
        .route("/nodes/:node_id", get(get_node).delete(delete_node))
        .route("/nodes/:node_id/migrate", post(migrate_node_services))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct NodeRow {
    id: Uuid,
    org_id: Uuid,
    name: String,
    provider: String,
    provider_vm_id: Option<String>,
    region: String,
    ip_address: Option<String>,
    public_ip: Option<String>,
    status: String,
    last_heartbeat_at: Option<DateTime<Utc>>,
    cpu_cores: i32,
    ram_mb: i32,
    provision_error: Option<String>,
    provision_attempts: i32,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl From<NodeRow> for ComputeNodeResponse {
    fn from(r: NodeRow) -> Self {
        Self {
            id: r.id,
            org_id: r.org_id,
            name: r.name,
            provider: r.provider,
            provider_vm_id: r.provider_vm_id,
            region: r.region,
            ip_address: r.ip_address,
            public_ip: r.public_ip,
            status: r.status,
            last_heartbeat_at: r.last_heartbeat_at,
            cpu_cores: r.cpu_cores,
            ram_mb: r.ram_mb,
            provision_error: r.provision_error,
            provision_attempts: r.provision_attempts,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

const NODE_SELECT: &str = r#"
    SELECT id, org_id, name, provider, provider_vm_id, region,
           ip_address, public_ip, status::text AS status, last_heartbeat_at,
           cpu_cores, ram_mb, provision_error, provision_attempts,
           created_at, updated_at
    FROM compute_nodes
"#;

// ─── Handlers ────────────────────────────────────────────────────────────────

async fn list_nodes(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
) -> Result<Json<ApiResponse<Vec<ComputeNodeResponse>>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let rows: Vec<NodeRow> = sqlx::query_as::<_, NodeRow>(
        &format!("{NODE_SELECT} WHERE org_id = $1 ORDER BY created_at DESC"),
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(
        rows.into_iter().map(ComputeNodeResponse::from).collect(),
    )))
}

async fn get_node(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<ComputeNodeResponse>>, ApiAppError> {
    crate::orgs::require_member(&state.db, org_id, auth.user_id).await?;

    let row: NodeRow = sqlx::query_as::<_, NodeRow>(
        &format!("{NODE_SELECT} WHERE id = $1 AND org_id = $2"),
    )
    .bind(node_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Node not found".to_string())))?;

    Ok(Json(ApiResponse::ok(ComputeNodeResponse::from(row))))
}

async fn create_node(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(org_id): Path<Uuid>,
    Json(body): Json<CreateNodeRequest>,
) -> Result<(StatusCode, Json<ApiResponse<ComputeNodeResponse>>), ApiAppError> {
    crate::orgs::require_admin(&state.db, org_id, auth.user_id).await?;

    if body.name.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("Node name is required".to_string())));
    }
    if body.provider.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("Provider is required".to_string())));
    }
    if body.region.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("Region is required".to_string())));
    }

    let cpu_cores = body.cpu_cores.unwrap_or(1);
    let ram_mb = body.ram_mb.unwrap_or(1024);

    let row: NodeRow = sqlx::query_as::<_, NodeRow>(
        &format!(
            r#"INSERT INTO compute_nodes
                   (id, org_id, name, provider, region, cpu_cores, ram_mb, status, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, 'provisioning', NOW(), NOW())
               RETURNING {}"#,
            "id, org_id, name, provider, provider_vm_id, region, ip_address, public_ip, \
             status::text AS status, last_heartbeat_at, cpu_cores, ram_mb, \
             provision_error, provision_attempts, created_at, updated_at"
        ),
    )
    .bind(Uuid::now_v7())
    .bind(org_id)
    .bind(&body.name)
    .bind(&body.provider)
    .bind(&body.region)
    .bind(cpu_cores)
    .bind(ram_mb)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok((StatusCode::CREATED, Json(ApiResponse::ok(ComputeNodeResponse::from(row)))))
}

async fn delete_node(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<ComputeNodeResponse>>, ApiAppError> {
    crate::orgs::require_admin(&state.db, org_id, auth.user_id).await?;

    let row: NodeRow = sqlx::query_as::<_, NodeRow>(
        &format!(
            r#"UPDATE compute_nodes SET status = 'stopped', updated_at = NOW()
               WHERE id = $1 AND org_id = $2
               RETURNING {}"#,
            "id, org_id, name, provider, provider_vm_id, region, ip_address, public_ip, \
             status::text AS status, last_heartbeat_at, cpu_cores, ram_mb, \
             provision_error, provision_attempts, created_at, updated_at"
        ),
    )
    .bind(node_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .ok_or_else(|| ApiAppError(AppError::NotFound("Node not found".to_string())))?;

    Ok(Json(ApiResponse::ok(ComputeNodeResponse::from(row))))
}

/// POST /orgs/:org_id/nodes/:node_id/migrate
///
/// Re-deploys all services assigned to this node. Useful after Free→Pro upgrade
/// to move running containers from the shared sandbox to the dedicated VM.
async fn migrate_node_services(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((org_id, node_id)): Path<(Uuid, Uuid)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    crate::orgs::require_admin(&state.db, org_id, auth.user_id).await?;

    // Verify the node belongs to this org and is active.
    let node_status: Option<String> = sqlx::query_scalar(
        "SELECT status::text FROM compute_nodes WHERE id = $1 AND org_id = $2",
    )
    .bind(node_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    match node_status.as_deref() {
        Some("active") => {}
        Some(s) => return Err(ApiAppError(AppError::BadRequest(
            format!("Node is not active (status: {s}). Cannot migrate services.")
        ))),
        None => return Err(ApiAppError(AppError::NotFound("Node not found".to_string()))),
    }

    // Fetch all services assigned to this node.
    #[derive(sqlx::FromRow)]
    struct AssignedService {
        service_id: Uuid,
        source_ref: String,
    }

    let services: Vec<AssignedService> = sqlx::query_as::<_, AssignedService>(
        r#"SELECT sna.service_id, COALESCE(s.git_branch, 'latest') AS source_ref
           FROM service_node_assignments sna
           JOIN services s ON s.id = sna.service_id
           JOIN projects p ON p.id = s.project_id
           WHERE sna.node_id = $1 AND p.org_id = $2"#,
    )
    .bind(node_id)
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if services.is_empty() {
        return Ok(Json(ApiResponse::ok(serde_json::json!({
            "message": "No services assigned to this node.",
            "deployment_ids": []
        }))));
    }

    let mut deployment_ids: Vec<Uuid> = Vec::new();

    for svc in &services {
        let deployment_id = Uuid::now_v7();

        // Pre-insert the deployment row.
        if let Err(e) = sqlx::query(
            "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
             VALUES ($1, $2, 'node_migration', $3, 'running'::deployment_status, NOW())",
        )
        .bind(deployment_id)
        .bind(svc.service_id)
        .bind(&svc.source_ref)
        .execute(&state.db)
        .await
        {
            tracing::warn!(service_id = %svc.service_id, "failed to insert migration deployment: {e}");
            continue;
        }

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
        ).with_registry(shipyard_registry::push::ArtifactPusher::new(
            state.db.clone(),
            Arc::clone(&state.registry_storage),
        ))
        .with_registry_hostname(state.config.registry.hostname.clone());

        let service_id = svc.service_id;
        let source_ref = svc.source_ref.clone();

        tokio::spawn(async move {
            if let Err(e) = engine.deploy(deployment_id, service_id, "node_migration", &source_ref).await {
                tracing::error!(deployment_id = %deployment_id, "Migration deployment failed: {e}");
            }
        });

        deployment_ids.push(deployment_id);
    }

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": format!("Started {} migration deployments.", deployment_ids.len()),
        "deployment_ids": deployment_ids,
    }))))
}
