use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EdgeFunction {
    pub id: Uuid,
    pub org_id: Uuid,
    pub group_id: Option<Uuid>,
    pub name: String,
    pub runtime: String,
    pub file_path: String,
    pub env_vars: serde_json::Value,
    pub env_whitelist: Vec<String>,
    pub timeout_secs: i32,
    pub status: String,
    pub last_deployed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EdgeFunctionDeployment {
    pub id: Uuid,
    pub function_id: Uuid,
    pub commit_sha: Option<String>,
    pub deployed_by: Option<Uuid>,
    pub status: String,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EdgeFunctionGroup {
    pub id: Uuid,
    pub org_id: Uuid,
    pub project_id: Option<Uuid>,
    pub provider: String,
    pub repo_url: String,
    pub branch: String,
    pub webhook_secret: String,
    pub last_deployed_sha: Option<String>,
    pub service_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EdgeFunctionInvocationLog {
    pub id: Uuid,
    pub function_id: Uuid,
    pub request_id: String,
    pub method: String,
    pub path: String,
    pub status_code: i32,
    pub duration_ms: i32,
    pub error: Option<String>,
    pub logged_at: DateTime<Utc>,
}

// ─── Request types ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateFunctionGroupRequest {
    pub provider: String,
    pub repo_url: String,
    pub branch: String,
    /// Project to associate this group with (shows on that project's canvas).
    pub project_id: Option<Uuid>,
    /// ID of the linked GitProvider row (for OAuth token + webhook registration).
    pub git_provider_id: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFunctionRequest {
    pub env_vars: Option<serde_json::Value>,
    pub env_whitelist: Option<Vec<String>>,
    pub timeout_secs: Option<i32>,
}

// ─── Response types ───────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct EdgeFunctionResponse {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub runtime: String,
    pub status: String,
    pub public_url: String,
    pub last_deployed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct DeployReport {
    pub deployed: Vec<String>,
    pub skipped: Vec<String>,
    pub failed: Vec<(String, String)>,
    pub deleted: Vec<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct EdgeFunctionDomain {
    pub id: Uuid,
    pub service_id: Uuid,
    pub hostname: String,
    pub tls_enabled: bool,
    pub cert_provider: String,
    pub port: Option<i32>,
    pub traefik_router_name: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEdgeFunctionDomainRequest {
    pub hostname: String,
    #[serde(default = "default_true")]
    pub tls_enabled: bool,
    #[serde(default = "default_cert_provider")]
    pub cert_provider: String,
    pub port: Option<i32>,
}

fn default_true() -> bool { true }
fn default_cert_provider() -> String { "letsencrypt".to_string() }

/// Used by the runtime container to load all live functions.
/// `artifact_path` points to the `current/` symlink dir on the shared volume;
/// the runtime reads source files from disk rather than receiving code as payload.
/// `code` is kept as fallback for pre-migration rows where `artifact_path` is NULL.
#[derive(Debug, Serialize)]
pub struct FunctionManifestEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub env: std::collections::HashMap<String, String>,
    pub timeout_secs: i32,
}
