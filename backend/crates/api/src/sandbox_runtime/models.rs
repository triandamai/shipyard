use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct SandboxAppConfigRow {
    pub service_id: Uuid,
    pub runtime: Option<String>,
    pub base_image: Option<String>,
    pub install_cmd: Option<String>,
    pub dev_cmd: Option<String>,
    pub port: Option<i32>,
    pub manifest_source: String,
    pub volume_name: String,
    pub seed_script_b64: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SandboxInstanceRow {
    pub service_id: Uuid,
    pub status: String,
    pub container_id: Option<String>,
    pub container_name: Option<String>,
    pub preview_url: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl SandboxInstanceRow {
    pub fn default_stopped(service_id: Uuid) -> Self {
        Self {
            service_id,
            status: "stopped".to_string(),
            container_id: None,
            container_name: None,
            preview_url: None,
            last_heartbeat_at: None,
            started_at: None,
        }
    }
}
