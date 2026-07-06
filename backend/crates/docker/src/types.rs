use bytes::Bytes;
use futures::Stream;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::pin::Pin;
use tokio::io::AsyncWrite;

/// Specification for creating/updating a swarm service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSpec {
    pub name: String,
    pub image: String,
    pub replicas: u64,
    pub env: Vec<String>,
    pub labels: HashMap<String, String>,
    pub mounts: Vec<MountSpec>,
    pub networks: Vec<String>,
    pub ports: Vec<PortSpec>,
    pub resources: Option<ResourceSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MountSpec {
    pub source: String,
    pub target: String,
    pub mount_type: MountType,
    pub readonly: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MountType {
    Volume,
    Bind,
    Tmpfs,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortSpec {
    pub target: u16,
    pub published: Option<u16>,
    pub protocol: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceSpec {
    pub cpu_limit: Option<f64>,
    pub memory_limit_mb: Option<u64>,
    pub cpu_reservation: Option<f64>,
    pub memory_reservation_mb: Option<u64>,
}

/// Information about a swarm task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskInfo {
    pub id: String,
    pub service_id: String,
    pub node_id: Option<String>,
    pub status: String,
    pub desired_state: String,
    pub container_id: Option<String>,
    pub image: String,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub error: Option<String>,
    pub exit_code: Option<i64>,
    pub slot: Option<i64>,
}

/// Detailed task/container information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDetail {
    pub id: String,
    pub service_id: String,
    pub node_id: Option<String>,
    pub container_id: Option<String>,
    pub status: String,
    pub message: Option<String>,
    pub image: String,
    pub slot: Option<i64>,
}

/// A single published port binding from Docker inspect.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortBinding {
    pub container_port: u16,
    pub protocol: String,
    pub host_ip: String,
    pub host_port: u16,
}

/// Detailed container information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerDetail {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i64>,
    pub restart_count: i64,
    pub platform: Option<String>,
    pub port_bindings: Vec<PortBinding>,
    pub env: Vec<String>,
    pub labels: HashMap<String, String>,
}

/// Swarm information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmInfo {
    pub node_id: String,
    pub node_addr: String,
    pub is_manager: bool,
    pub nodes: u64,
    pub managers: u64,
}

/// Swarm join tokens for worker and manager nodes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmJoinTokens {
    pub worker: String,
    pub manager: String,
    pub addr: String,
}

/// Log streaming options.
#[derive(Debug, Clone, Default)]
pub struct LogOpts {
    pub follow: bool,
    pub stdout: bool,
    pub stderr: bool,
    pub since: Option<i64>,
    pub until: Option<i64>,
    pub tail: Option<String>,
    pub timestamps: bool,
}

/// Filters for Docker event stream.
#[derive(Debug, Clone, Default)]
pub struct EventFilters {
    pub event_types: Vec<String>,
    pub actions: Vec<String>,
    pub labels: Vec<String>,
}

/// Summary of a container (from `docker ps -a`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSummary {
    pub id: String,
    pub names: Vec<String>,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: i64,
    pub ports: Vec<String>,
    pub labels: HashMap<String, String>,
}

/// Summary of a Docker volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeSummary {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub scope: String,
    pub labels: HashMap<String, String>,
    pub created_at: Option<String>,
}

/// Summary of a Docker network.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkSummary {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub internal: bool,
    pub attachable: bool,
    pub ipam_subnet: Option<String>,
    pub labels: HashMap<String, String>,
    pub containers: usize,
}

/// Summary of a swarm service.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceSummary {
    pub id: String,
    pub name: String,
    pub image: String,
    pub replicas_running: u64,
    pub replicas_desired: u64,
    pub mode: String,
    pub ports: Vec<String>,
    pub labels: HashMap<String, String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
}

/// Summary of a Docker image.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageSummary {
    pub id: String,
    pub tags: Vec<String>,
    pub size: i64,
    pub created: i64,
}

/// A live exec session attached to a container — stdin writer + stdout/stderr stream.
pub struct ExecHandle {
    pub exec_id: String,
    pub stdin: Pin<Box<dyn AsyncWrite + Send>>,
    pub output: Pin<Box<dyn Stream<Item = Bytes> + Send>>,
}

/// Summary of a swarm node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: String,
    pub hostname: String,
    pub role: String,       // "manager" | "worker"
    pub status: String,     // "ready" | "down" | "disconnected"
    pub availability: String, // "active" | "pause" | "drain"
    pub engine_version: Option<String>,
    pub addr: Option<String>,
}
