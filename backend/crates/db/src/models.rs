//! Database models — Rust structs mapping to PostgreSQL tables.
//! These use sqlx::FromRow for direct query mapping.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use uuid::Uuid;

// ─── Auth & Org ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Organization {
    pub id: Uuid,
    pub name: String,
    pub slug: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrgMember {
    pub id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub role: String, // member_role enum as text
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Invitation {
    pub id: Uuid,
    pub org_id: Uuid,
    pub email: String,
    pub role: String,
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub permissions: Vec<String>,
    /// JSON array of { project_id, permissions } — applied to project_members on accept.
    pub project_assignments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OrgMemberPermission {
    pub id: Uuid,
    pub org_id: Uuid,
    pub user_id: Uuid,
    pub permission: String,
    pub granted_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

// ─── Projects ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Project {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub slug: String,
    pub directory_path: String,
    pub node_positions: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Services ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Service {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub slug: String,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub service_type: String,
    pub image: String,
    pub git_repo_url: Option<String>,
    pub git_branch: String,
    pub auto_deploy: bool,
    pub directory_path: String,
    pub ports: serde_json::Value,
    pub status: String,
    pub replicas: i32,
    pub cpu_limit: Option<f64>,
    pub memory_limit_mb: Option<i64>,
    pub service_parent_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AuditLog {
    pub id: Uuid,
    pub user_id: Option<Uuid>,
    pub action: String,
    pub resource_type: Option<String>,
    pub resource_id: Option<Uuid>,
    pub ip_address: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ServiceEnv {
    pub id: Uuid,
    pub service_id: Uuid,
    pub key: String,
    pub value_encrypted: String,
    pub is_secret: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Volume {
    pub id: Uuid,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
    pub name: String,
    pub mount_path: String,
    pub driver: String,
    pub size_mb: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Network {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub driver: String,
    pub subnet: String,
    /// Docker-assigned network ID (hash). Populated when created via the API;
    /// used by the event worker to match `network destroy` events back to DB rows.
    pub docker_network_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Domain {
    pub id: Uuid,
    pub service_id: Uuid,
    pub hostname: String,
    pub tls_enabled: bool,
    pub traefik_router_name: String,
    pub cert_provider: String,
    pub port: Option<i32>,
    pub created_at: DateTime<Utc>,
}

// ─── Containers ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Container {
    pub id: Uuid,
    pub service_id: Uuid,
    pub docker_container_id: String,
    pub docker_task_id: Option<String>,
    pub node_id: Option<String>,
    pub replica_index: Option<i32>,
    pub status: String, // container_status enum as text
    pub status_message: Option<String>,
    pub image: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Docker Events ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DockerEvent {
    pub id: i64,
    pub event_type: String,
    pub action: String,
    pub actor_id: String,
    pub actor_attributes: Option<serde_json::Value>,
    pub scope: Option<String>,
    pub raw: serde_json::Value,
    pub received_at: DateTime<Utc>,
}

// ─── Deployments ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Deployment {
    pub id: Uuid,
    pub service_id: Uuid,
    pub triggered_by: String,
    pub source_ref: String,
    pub status: String, // deployment_status enum as text
    pub created_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DeploymentStep {
    pub id: Uuid,
    pub deployment_id: Uuid,
    pub name: String,
    pub status: String, // step_status enum as text
    pub order_index: i32,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct DeploymentLog {
    pub id: Uuid,
    pub deployment_id: Uuid,
    pub step_id: Option<Uuid>,
    pub level: String, // log_level enum as text
    pub message: String,
    pub timestamp: DateTime<Utc>,
}

// ─── Topology ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct TopologyEdge {
    pub id: Uuid,
    pub project_id: Uuid,
    pub source_node_id: String,
    pub target_node_id: String,
    pub edge_type: String,
    pub created_at: DateTime<Utc>,
}

// ─── System Config ───────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct SystemConfig {
    pub key: String,
    pub value: serde_json::Value,
    pub updated_at: DateTime<Utc>,
}

// ─── Templates ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Template {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    #[sqlx(rename = "type")]
    pub template_type: String,
    pub image: Option<String>,
    pub env: serde_json::Value,
    pub volumes: serde_json::Value,
    pub ports: serde_json::Value,
    pub icon: Option<String>,
    pub is_builtin: bool,
    pub created_at: DateTime<Utc>,
}
