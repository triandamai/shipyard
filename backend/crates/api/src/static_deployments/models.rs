use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shipyard_common::error::{AppError, AppResult};
use uuid::Uuid;

// ─── Status Enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StaticDeploymentStatus {
    Pending,
    Uploading,
    Verifying,
    Ready,
    Failed,
    Deleting,
    Deleted,
}

impl StaticDeploymentStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::Uploading => "UPLOADING",
            Self::Verifying => "VERIFYING",
            Self::Ready => "READY",
            Self::Failed => "FAILED",
            Self::Deleting => "DELETING",
            Self::Deleted => "DELETED",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_uppercase().as_str() {
            "PENDING" => Some(Self::Pending),
            "UPLOADING" => Some(Self::Uploading),
            "VERIFYING" => Some(Self::Verifying),
            "READY" => Some(Self::Ready),
            "FAILED" => Some(Self::Failed),
            "DELETING" => Some(Self::Deleting),
            "DELETED" => Some(Self::Deleted),
            _ => None,
        }
    }

    /// Whether this status allows HTTP requests to serve contents.
    pub fn is_servable(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Allowed state transitions.
    pub fn can_transition_to(&self, next: Self) -> bool {
        match (self, next) {
            (Self::Pending, Self::Uploading) => true,
            (Self::Pending, Self::Failed) => true,
            (Self::Uploading, Self::Verifying) => true,
            (Self::Uploading, Self::Failed) => true,
            (Self::Verifying, Self::Ready) => true,
            (Self::Verifying, Self::Failed) => true,
            (Self::Ready, Self::Deleting) => true,
            (Self::Failed, Self::Deleting) => true,
            (Self::Deleting, Self::Deleted) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for StaticDeploymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ─── Database / Domain Models ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StaticProject {
    pub id: Uuid,
    pub org_id: Uuid,
    pub name: String,
    pub slug: String,
    pub framework: String,
    pub spa_fallback: bool,
    pub preview_base_url: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StaticDeployment {
    pub id: Uuid,
    pub project_id: Uuid,
    pub status: String,
    pub archive_checksum_sha256: Option<String>,
    pub archive_size_bytes: Option<i64>,
    pub extracted_size_bytes: Option<i64>,
    pub file_count: Option<i32>,
    pub entry_file: String,
    pub storage_path: String,
    pub preview_token: Option<String>,
    pub is_private: bool,
    pub error_message: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl StaticDeployment {
    pub fn typed_status(&self) -> StaticDeploymentStatus {
        StaticDeploymentStatus::parse(&self.status).unwrap_or(StaticDeploymentStatus::Failed)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StaticAlias {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub deployment_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StaticDeploymentEvent {
    pub id: Uuid,
    pub deployment_id: Uuid,
    pub status: String,
    pub message: String,
    pub details: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct StaticProjectDomain {
    pub id: Uuid,
    pub project_id: Uuid,
    pub hostname: String,
    pub alias: String,
    pub tls_enabled: bool,
    pub cert_provider: String,
    pub traefik_router_name: String,
    pub cloudflare_zone_id: Option<String>,
    pub cloudflare_record_id: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ─── Validators ──────────────────────────────────────────────────────────────

pub fn validate_project_slug(slug: &str) -> AppResult<()> {
    if slug.is_empty() {
        return Err(AppError::Validation("Project slug cannot be empty".into()));
    }
    if slug.len() > 63 {
        return Err(AppError::Validation("Project slug cannot exceed 63 characters".into()));
    }
    if slug.starts_with('-') || slug.ends_with('-') {
        return Err(AppError::Validation("Project slug cannot start or end with a hyphen".into()));
    }
    let valid = slug.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(AppError::Validation(
            "Project slug must only contain lowercase alphanumeric characters and hyphens".into(),
        ));
    }
    Ok(())
}

pub fn validate_alias_name(name: &str) -> AppResult<()> {
    if name.is_empty() {
        return Err(AppError::Validation("Alias name cannot be empty".into()));
    }
    if name.len() > 63 {
        return Err(AppError::Validation("Alias name cannot exceed 63 characters".into()));
    }
    if name.starts_with('-') || name.ends_with('-') {
        return Err(AppError::Validation("Alias name cannot start or end with a hyphen".into()));
    }
    let valid = name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    if !valid {
        return Err(AppError::Validation(
            "Alias name must only contain lowercase alphanumeric characters and hyphens".into(),
        ));
    }
    Ok(())
}
