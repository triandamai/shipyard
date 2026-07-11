//! MQTT topic schema definitions.
//!
//! Topic pattern:
//! `platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/...`

use uuid::Uuid;

/// Build MQTT topic for service status updates.
pub fn service_status(org_id: Uuid, project_id: Uuid, service_id: Uuid) -> String {
    format!(
        "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/status"
    )
}

/// Build MQTT topic for deployment step logs.
pub fn deployment_step_log(
    org_id: Uuid,
    project_id: Uuid,
    service_id: Uuid,
    deployment_id: Uuid,
    step_id: Uuid,
) -> String {
    format!(
        "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/deployments/{deployment_id}/steps/{step_id}/log"
    )
}

/// Build MQTT topic for a single deployment step's status changes.
pub fn deployment_step_status(
    org_id: Uuid,
    project_id: Uuid,
    service_id: Uuid,
    deployment_id: Uuid,
    step_id: Uuid,
) -> String {
    format!(
        "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/deployments/{deployment_id}/steps/{step_id}/status"
    )
}

/// Build MQTT topic for deployment status.
pub fn deployment_status(
    org_id: Uuid,
    project_id: Uuid,
    service_id: Uuid,
    deployment_id: Uuid,
) -> String {
    format!(
        "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/deployments/{deployment_id}/status"
    )
}

/// Build MQTT topic for service container logs (per replica).
pub fn service_logs(org_id: Uuid, project_id: Uuid, service_id: Uuid, replica_id: &str) -> String {
    format!(
        "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/logs/{replica_id}"
    )
}

/// Build MQTT topic for project topology updates.
pub fn topology(org_id: Uuid, project_id: Uuid) -> String {
    format!("platform/orgs/{org_id}/projects/{project_id}/topology")
}

/// Build MQTT topic for system health.
pub fn system_health() -> String {
    "platform/system/health".to_string()
}

/// Build MQTT topic for container list updates.
pub fn containers(org_id: Uuid, project_id: Uuid, service_id: Uuid) -> String {
    format!(
        "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/containers"
    )
}

/// Build MQTT topic for single container status.
pub fn container_status(
    org_id: Uuid,
    project_id: Uuid,
    service_id: Uuid,
    container_id: Uuid,
) -> String {
    format!(
        "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/containers/{container_id}/status"
    )
}

/// Build MQTT topic for service replica count.
pub fn replicas_count(org_id: Uuid, project_id: Uuid, service_id: Uuid) -> String {
    format!(
        "platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/replicas/count"
    )
}

/// Build MQTT topic for raw Docker events (debug/audit only).
pub fn docker_events_raw() -> String {
    "platform/docker/events/raw".to_string()
}

/// Build MQTT topic for org member events (join, remove, update, invite).
pub fn org_members(org_id: Uuid) -> String {
    format!("platform/orgs/{org_id}/members")
}

/// Build MQTT topic for platform-wide user events (register).
pub fn users() -> String {
    "platform/users".to_string()
}
