use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectTransfer {
    pub id: Uuid,
    pub project_id: Uuid,
    pub source_org_id: Uuid,
    pub target_org_id: Uuid,
    pub status: String,
    pub initiated_by: Uuid,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServiceTransfer {
    pub id: Uuid,
    pub service_id: Uuid,
    pub source_project_id: Uuid,
    pub source_org_id: Uuid,
    pub target_org_id: Uuid,
    pub target_project_id: Option<Uuid>,
    pub status: String,
    pub initiated_by: Uuid,
    pub claimed_by: Option<Uuid>,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct TransferOrgSearchItem {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InitiateProjectTransferRequest {
    pub target_org_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InitiateServiceTransferRequest {
    pub target_org_id: Uuid,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ClaimServiceTransferRequest {
    pub target_project_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ProjectTransferItemResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub project_name: String,
    pub project_slug: String,
    pub source_org_id: Uuid,
    pub source_org_name: String,
    pub source_org_slug: String,
    pub target_org_id: Uuid,
    pub target_org_name: String,
    pub target_org_slug: String,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServiceTransferItemResponse {
    pub id: Uuid,
    pub service_id: Uuid,
    pub service_name: String,
    pub service_slug: String,
    pub source_project_id: Uuid,
    pub source_project_name: String,
    pub source_org_id: Uuid,
    pub source_org_name: String,
    pub source_org_slug: String,
    pub target_org_id: Uuid,
    pub target_org_name: String,
    pub target_org_slug: String,
    pub target_project_id: Option<Uuid>,
    pub target_project_name: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrgTransfersResponse {
    pub incoming_service_transfers: Vec<ServiceTransferItemResponse>,
    pub outgoing_service_transfers: Vec<ServiceTransferItemResponse>,
    pub project_transfers: Vec<ProjectTransferItemResponse>,
}
