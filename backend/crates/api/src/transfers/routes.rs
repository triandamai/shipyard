use std::sync::Arc;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::AuthUser;
use crate::error::ApiAppError;
use crate::middleware::rbac::{require_org_member, require_org_owner};
use crate::AppState;

use super::models::{
    ClaimServiceTransferRequest, InitiateProjectTransferRequest, InitiateServiceTransferRequest,
    OrgTransfersResponse, ProjectTransferItemResponse, ServiceTransferItemResponse,
    TransferOrgSearchItem,
};
use super::worker::{execute_project_transfer, execute_service_claim};

#[derive(Debug, Deserialize)]
pub struct SearchOrgQuery {
    pub q: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/search", get(search_orgs))
        .route("/orgs/:org_id/projects/:project_id/transfer", post(initiate_project_transfer))
        .route("/services/:service_id/transfer", post(initiate_service_transfer))
        .route("/orgs/:org_id/transfers", get(list_org_transfers))
        .route("/service-transfers/:transfer_id/claim", post(claim_service_transfer))
        .route("/service-transfers/:transfer_id/cancel", post(cancel_service_transfer))
        .route("/service-transfers/:transfer_id/reject", post(reject_service_transfer))
}

/// Search organizations for ownership transfer target.
async fn search_orgs(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(query): Query<SearchOrgQuery>,
) -> Result<Json<ApiResponse<Vec<TransferOrgSearchItem>>>, ApiAppError> {
    let q = query.q.as_deref().unwrap_or("").trim();

    let items: Vec<TransferOrgSearchItem> = if q.is_empty() {
        // Return user's own organizations first
        sqlx::query_as::<_, TransferOrgSearchItem>(
            "SELECT o.id, o.name, o.slug
             FROM organizations o
             JOIN org_members m ON m.org_id = o.id
             WHERE m.user_id = $1
             ORDER BY o.name ASC
             LIMIT 20",
        )
        .bind(auth.user_id)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    } else {
        let pattern = format!("%{}%", q);
        sqlx::query_as::<_, TransferOrgSearchItem>(
            "SELECT id, name, slug
             FROM organizations
             WHERE name ILIKE $1 OR slug ILIKE $1
             ORDER BY name ASC
             LIMIT 20",
        )
        .bind(&pattern)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    };

    Ok(Json(ApiResponse::ok(items)))
}

/// Initiate transfer of an entire project to a target organization.
async fn initiate_project_transfer(
    auth: AuthUser,
    Path((org_id, project_id)): Path<(Uuid, Uuid)>,
    State(state): State<AppState>,
    Json(body): Json<InitiateProjectTransferRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    // Caller must be owner of the current project's org
    require_org_owner(&state.db, auth.user_id, org_id)
        .await
        .map_err(ApiAppError)?;

    if body.target_org_id == org_id {
        return Err(ApiAppError(AppError::Validation(
            "Target organization cannot be the same as the source organization".into(),
        )));
    }

    // Verify project exists and belongs to org_id
    let proj_exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM projects WHERE id = $1 AND org_id = $2",
    )
    .bind(project_id)
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if proj_exists.is_none() {
        return Err(ApiAppError(AppError::NotFound("Project not found".into())));
    }

    // Verify target organization exists
    let target_exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM organizations WHERE id = $1",
    )
    .bind(body.target_org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if target_exists.is_none() {
        return Err(ApiAppError(AppError::NotFound("Target organization not found".into())));
    }

    let transfer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO project_transfers (id, project_id, source_org_id, target_org_id, status, initiated_by)
         VALUES ($1, $2, $3, $4, 'in_progress', $5)",
    )
    .bind(transfer_id)
    .bind(project_id)
    .bind(org_id)
    .bind(body.target_org_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Spawn async background worker
    let db = state.db.clone();
    let data_dir = state.config.data_dir.clone();
    tokio::spawn(async move {
        if let Err(e) = execute_project_transfer(&db, transfer_id, project_id, body.target_org_id, &data_dir).await {
            tracing::error!("Project transfer {} failed: {}", transfer_id, e);
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(ApiResponse::ok(serde_json::json!({
            "transfer_id": transfer_id,
            "status": "in_progress",
            "message": "Project transfer initiated in background"
        }))),
    ))
}

/// Initiate transfer of a service to a target organization.
async fn initiate_service_transfer(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<InitiateServiceTransferRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    // Find service and its org
    let svc_meta: Option<(Uuid, Uuid)> = sqlx::query_as(
        "SELECT s.project_id, p.org_id
         FROM services s
         JOIN projects p ON p.id = s.project_id
         WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (project_id, org_id) = match svc_meta {
        Some(m) => m,
        None => return Err(ApiAppError(AppError::NotFound("Service not found".into()))),
    };

    // Caller must be owner of the service's current org
    require_org_owner(&state.db, auth.user_id, org_id)
        .await
        .map_err(ApiAppError)?;

    if body.target_org_id == org_id {
        return Err(ApiAppError(AppError::Validation(
            "Target organization cannot be the same as the source organization".into(),
        )));
    }

    // Verify target organization exists
    let target_exists: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM organizations WHERE id = $1",
    )
    .bind(body.target_org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if target_exists.is_none() {
        return Err(ApiAppError(AppError::NotFound("Target organization not found".into())));
    }

    let transfer_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO service_transfers (id, service_id, source_project_id, source_org_id, target_org_id, status, initiated_by)
         VALUES ($1, $2, $3, $4, $5, 'pending_claim', $6)",
    )
    .bind(transfer_id)
    .bind(service_id)
    .bind(project_id)
    .bind(org_id)
    .bind(body.target_org_id)
    .bind(auth.user_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(serde_json::json!({
            "transfer_id": transfer_id,
            "status": "pending_claim",
            "message": "Service transfer initiated. Waiting for target organization to claim."
        }))),
    ))
}

/// List incoming and outgoing transfers for an organization.
async fn list_org_transfers(
    auth: AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<OrgTransfersResponse>>, ApiAppError> {
    require_org_member(&state.db, auth.user_id, org_id)
        .await
        .map_err(ApiAppError)?;

    // Incoming service transfers: target_org_id == org_id
    let incoming_services: Vec<ServiceTransferItemResponse> = sqlx::query_as::<_, ServiceTransferItemResponse>(
        "SELECT st.id, st.service_id, s.name AS service_name, s.slug AS service_slug,
                st.source_project_id, sp.name AS source_project_name,
                st.source_org_id, so.name AS source_org_name, so.slug AS source_org_slug,
                st.target_org_id, to_org.name AS target_org_name, to_org.slug AS target_org_slug,
                st.target_project_id, tp.name AS target_project_name,
                st.status, st.error_message, st.created_at, st.updated_at
         FROM service_transfers st
         JOIN services s ON s.id = st.service_id
         JOIN projects sp ON sp.id = st.source_project_id
         JOIN organizations so ON so.id = st.source_org_id
         JOIN organizations to_org ON to_org.id = st.target_org_id
         LEFT JOIN projects tp ON tp.id = st.target_project_id
         WHERE st.target_org_id = $1
         ORDER BY st.created_at DESC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Outgoing service transfers: source_org_id == org_id
    let outgoing_services: Vec<ServiceTransferItemResponse> = sqlx::query_as::<_, ServiceTransferItemResponse>(
        "SELECT st.id, st.service_id, s.name AS service_name, s.slug AS service_slug,
                st.source_project_id, sp.name AS source_project_name,
                st.source_org_id, so.name AS source_org_name, so.slug AS source_org_slug,
                st.target_org_id, to_org.name AS target_org_name, to_org.slug AS target_org_slug,
                st.target_project_id, tp.name AS target_project_name,
                st.status, st.error_message, st.created_at, st.updated_at
         FROM service_transfers st
         JOIN services s ON s.id = st.service_id
         JOIN projects sp ON sp.id = st.source_project_id
         JOIN organizations so ON so.id = st.source_org_id
         JOIN organizations to_org ON to_org.id = st.target_org_id
         LEFT JOIN projects tp ON tp.id = st.target_project_id
         WHERE st.source_org_id = $1
         ORDER BY st.created_at DESC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Project transfers: source_org_id == org_id OR target_org_id == org_id
    let project_transfers: Vec<ProjectTransferItemResponse> = sqlx::query_as::<_, ProjectTransferItemResponse>(
        "SELECT pt.id, pt.project_id, p.name AS project_name, p.slug AS project_slug,
                pt.source_org_id, so.name AS source_org_name, so.slug AS source_org_slug,
                pt.target_org_id, to_org.name AS target_org_name, to_org.slug AS target_org_slug,
                pt.status, pt.error_message, pt.created_at, pt.updated_at
         FROM project_transfers pt
         JOIN projects p ON p.id = pt.project_id
         JOIN organizations so ON so.id = pt.source_org_id
         JOIN organizations to_org ON to_org.id = pt.target_org_id
         WHERE pt.source_org_id = $1 OR pt.target_org_id = $1
         ORDER BY pt.created_at DESC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(OrgTransfersResponse {
        incoming_service_transfers: incoming_services,
        outgoing_service_transfers: outgoing_services,
        project_transfers,
    })))
}

/// Target organization owner claims an incoming service transfer into target_project_id.
async fn claim_service_transfer(
    auth: AuthUser,
    Path(transfer_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<ClaimServiceTransferRequest>,
) -> Result<(StatusCode, Json<ApiResponse<serde_json::Value>>), ApiAppError> {
    // 1. Fetch transfer details
    let transfer: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT service_id, target_org_id, status FROM service_transfers WHERE id = $1",
    )
    .bind(transfer_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (service_id, target_org_id, status) = match transfer {
        Some(t) => t,
        None => return Err(ApiAppError(AppError::NotFound("Service transfer not found".into()))),
    };

    if status != "pending_claim" {
        return Err(ApiAppError(AppError::Validation(format!(
            "Cannot claim transfer with status '{}'", status
        ))));
    }

    // Caller must be owner of target organization
    require_org_owner(&state.db, auth.user_id, target_org_id)
        .await
        .map_err(ApiAppError)?;

    // Verify target_project_id belongs to target_org_id
    let proj_valid: Option<(Uuid,)> = sqlx::query_as(
        "SELECT id FROM projects WHERE id = $1 AND org_id = $2",
    )
    .bind(body.target_project_id)
    .bind(target_org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if proj_valid.is_none() {
        return Err(ApiAppError(AppError::Validation(
            "Target project does not belong to the target organization".into(),
        )));
    }

    // Update status to in_progress and record claimed_by
    sqlx::query(
        "UPDATE service_transfers SET status = 'in_progress', target_project_id = $1, claimed_by = $2, updated_at = NOW() WHERE id = $3",
    )
    .bind(body.target_project_id)
    .bind(auth.user_id)
    .bind(transfer_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    // Spawn background worker to complete the transfer
    let db = state.db.clone();
    let docker = Arc::clone(&state.docker);
    let mqtt = Arc::clone(&state.mqtt);
    let config = state.config.clone();
    let registry_storage = Arc::clone(&state.registry_storage);
    let triggered_by = auth.email.clone();

    tokio::spawn(async move {
        if let Err(e) = execute_service_claim(&db, transfer_id, service_id, body.target_project_id, target_org_id).await {
            tracing::error!("Service claim {} failed: {}", transfer_id, e);
            return;
        }

        // Trigger redeploy / rebuild of the service if it has an image configured
        let svc_image: Option<(String,)> = sqlx::query_as(
            "SELECT COALESCE(image, '') FROM services WHERE id = $1"
        )
        .bind(service_id)
        .fetch_optional(&db)
        .await
        .unwrap_or(None);

        if let Some((img,)) = svc_image {
            if !img.trim().is_empty() {
                let deployment_id = Uuid::now_v7();
                let source_ref = img.rfind(':').map(|pos| img[pos + 1..].trim().to_string()).filter(|t| !t.is_empty()).unwrap_or_else(|| "transfer".to_string());
                let _ = sqlx::query(
                    "INSERT INTO deployments (id, service_id, triggered_by, source_ref, status, created_at)
                     VALUES ($1, $2, $3, $4, 'running'::deployment_status, NOW())",
                )
                .bind(deployment_id)
                .bind(service_id)
                .bind(&triggered_by)
                .bind(&source_ref)
                .execute(&db)
                .await;

                let engine = shipyard_engine::DeploymentEngine::new(
                    docker,
                    db.clone(),
                    mqtt,
                    config.docker.label_prefix.clone(),
                    config.traefik.network.clone(),
                    config.auth.secret_key.clone(),
                    config.docker.port_proxy,
                    config.data_dir.clone(),
                    config.static_server.retention_versions,
                ).with_registry(shipyard_registry::push::ArtifactPusher::new(
                    db.clone(),
                    registry_storage,
                ))
                .with_registry_hostname(config.registry.hostname.clone());

                let _ = engine.deploy(deployment_id, service_id, &triggered_by, &source_ref).await;
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(ApiResponse::ok(serde_json::json!({
            "transfer_id": transfer_id,
            "status": "in_progress",
            "message": "Service transfer claimed and migration started in background"
        }))),
    ))
}

/// Source organization owner cancels a pending service transfer.
async fn cancel_service_transfer(
    auth: AuthUser,
    Path(transfer_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let transfer: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT source_org_id, status FROM service_transfers WHERE id = $1",
    )
    .bind(transfer_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (source_org_id, status) = match transfer {
        Some(t) => t,
        None => return Err(ApiAppError(AppError::NotFound("Service transfer not found".into()))),
    };

    if status != "pending_claim" {
        return Err(ApiAppError(AppError::Validation(format!(
            "Cannot cancel transfer with status '{}'", status
        ))));
    }

    require_org_owner(&state.db, auth.user_id, source_org_id)
        .await
        .map_err(ApiAppError)?;

    sqlx::query(
        "UPDATE service_transfers SET status = 'cancelled', updated_at = NOW() WHERE id = $1",
    )
    .bind(transfer_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "transfer_id": transfer_id,
        "status": "cancelled",
        "message": "Service transfer cancelled"
    }))))
}

/// Target organization owner rejects an incoming service transfer.
async fn reject_service_transfer(
    auth: AuthUser,
    Path(transfer_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let transfer: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT target_org_id, status FROM service_transfers WHERE id = $1",
    )
    .bind(transfer_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let (target_org_id, status) = match transfer {
        Some(t) => t,
        None => return Err(ApiAppError(AppError::NotFound("Service transfer not found".into()))),
    };

    if status != "pending_claim" {
        return Err(ApiAppError(AppError::Validation(format!(
            "Cannot reject transfer with status '{}'", status
        ))));
    }

    require_org_owner(&state.db, auth.user_id, target_org_id)
        .await
        .map_err(ApiAppError)?;

    sqlx::query(
        "UPDATE service_transfers SET status = 'rejected', updated_at = NOW() WHERE id = $1",
    )
    .bind(transfer_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "transfer_id": transfer_id,
        "status": "rejected",
        "message": "Service transfer rejected"
    }))))
}
