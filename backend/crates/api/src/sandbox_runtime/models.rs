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
}

#[derive(Debug, sqlx::FromRow)]
pub struct SandboxInstanceRow {
    pub service_id: Uuid,
    pub status: String,
    pub container_id: Option<String>,
    pub container_name: Option<String>,
    pub preview_url: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}
