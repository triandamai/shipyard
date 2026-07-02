use bollard::container::{InspectContainerOptions, LogsOptions, RestartContainerOptions, StopContainerOptions};
use bollard::image::CreateImageOptions;
use bollard::models::{
    EndpointPortConfig, EndpointPortConfigProtocolEnum, EndpointPortConfigPublishModeEnum,
    EndpointSpec, EndpointSpecModeEnum, Limit, Mount, MountTypeEnum,
    NetworkAttachmentConfig, ResourceObject, ServiceSpec as BollardServiceSpec,
    ServiceSpecMode, ServiceSpecModeReplicated, ServiceSpecRollbackConfig,
    ServiceSpecUpdateConfig, ServiceSpecUpdateConfigFailureActionEnum,
    ServiceSpecUpdateConfigOrderEnum, SwarmInitRequest, TaskSpec,
    TaskSpecContainerSpec, TaskSpecResources,
};
use bollard::network::CreateNetworkOptions;
use bollard::service::{InspectServiceOptions, ListServicesOptions, UpdateServiceOptions};
use bollard::volume::{CreateVolumeOptions, RemoveVolumeOptions};
use bollard::Docker;
use futures::StreamExt;
use shipyard_common::error::{AppError, AppResult};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::types::*;

// ─── Trait definition ────────────────────────────────────────────────────────

/// Primary interface for all Docker / Swarm operations used by Shipyard.
///
/// Every method returns a typed `AppResult<T>` so callers never touch
/// bollard types directly.
#[async_trait::async_trait]
pub trait DockerEngine: Send + Sync {
    /// Create a new swarm service and return its ID.
    async fn create_service(&self, spec: ServiceSpec) -> AppResult<String>;

    /// Replace the spec of an existing service.
    async fn update_service(&self, id: &str, spec: ServiceSpec) -> AppResult<()>;

    /// Remove a service by ID or name.
    async fn remove_service(&self, id: &str) -> AppResult<()>;

    /// Change the replica count of a replicated service.
    async fn scale_service(&self, id: &str, replicas: u64) -> AppResult<()>;

    /// List all tasks (container instances) belonging to a service.
    async fn list_tasks(&self, service_id: &str) -> AppResult<Vec<TaskInfo>>;

    /// Fetch log lines from a container.
    async fn container_logs(
        &self,
        container_id: &str,
        opts: LogOpts,
    ) -> AppResult<Vec<String>>;

    /// Create an overlay network and return its ID.
    async fn create_network(&self, name: &str, driver: &str) -> AppResult<String>;

    /// Remove a network by ID or name.
    async fn remove_network(&self, id: &str) -> AppResult<()>;

    /// Create a named volume.
    async fn create_volume(&self, name: &str, driver: &str) -> AppResult<()>;

    /// Remove a named volume.
    async fn remove_volume(&self, name: &str) -> AppResult<()>;

    /// Pull an image; returns the status lines emitted by the daemon.
    async fn pull_image(&self, image: &str, tag: &str) -> AppResult<Vec<String>>;

    /// Build an image from a local directory context.
    /// `tag` is the full image reference (e.g. `shipyard/my-svc:abc1234`).
    /// `context_path` is the absolute path to the build context directory.
    /// `dockerfile` is the path to the Dockerfile relative to context_path.
    /// Returns all build output lines (stdout + stderr).
    async fn build_image(
        &self,
        tag: &str,
        context_path: &str,
        dockerfile: &str,
    ) -> AppResult<Vec<String>>;

    /// Bootstrap a new swarm on this node.
    async fn init_swarm(&self, advertise_addr: &str) -> AppResult<()>;

    /// Return high-level information about the swarm this node belongs to.
    async fn swarm_info(&self) -> AppResult<SwarmInfo>;

    /// Inspect a single swarm task.
    async fn inspect_task(&self, task_id: &str) -> AppResult<TaskDetail>;

    /// Inspect a container and return key details.
    async fn inspect_container(&self, container_id: &str) -> AppResult<ContainerDetail>;

    /// Stop a running container (SIGTERM, then SIGKILL after timeout_secs).
    async fn stop_container(&self, container_id: &str, timeout_secs: i64) -> AppResult<()>;

    /// Restart a container.
    async fn restart_container(&self, container_id: &str, timeout_secs: i64) -> AppResult<()>;

    /// Verify that the daemon is reachable.
    async fn ping(&self) -> AppResult<()>;

    /// List all swarm services, each summarised as a `TaskInfo`.
    async fn list_services(&self) -> AppResult<Vec<TaskInfo>>;

    /// Remove all stopped containers. Returns the number removed.
    async fn prune_containers(&self) -> AppResult<u64>;

    /// List all containers (running + stopped).
    async fn list_all_containers(&self) -> AppResult<Vec<ContainerSummary>>;

    /// List all volumes.
    async fn list_all_volumes(&self) -> AppResult<Vec<VolumeSummary>>;

    /// List all networks.
    async fn list_all_networks(&self) -> AppResult<Vec<NetworkSummary>>;

    /// List all swarm services with full summary.
    async fn list_all_services(&self) -> AppResult<Vec<ServiceSummary>>;
}

// ─── Concrete implementation ──────────────────────────────────────────────────

/// Bollard-backed implementation of [`DockerEngine`].
pub struct BollardDockerEngine {
    client: Docker,
}

impl BollardDockerEngine {
    /// Connect to the local Docker socket (`/var/run/docker.sock`).
    pub fn new() -> AppResult<Self> {
        let client = Docker::connect_with_local_defaults()
            .map_err(|e| AppError::Docker(format!("Failed to connect to Docker: {e}")))?;
        tracing::info!("Connected to Docker daemon");
        Ok(Self { client })
    }

    /// Connect to a specific Docker socket path.
    pub fn with_socket(socket_path: &str) -> AppResult<Self> {
        let client =
            Docker::connect_with_socket(socket_path, 120, bollard::API_DEFAULT_VERSION)
                .map_err(|e| {
                    AppError::Docker(format!(
                        "Failed to connect to Docker at {socket_path}: {e}"
                    ))
                })?;
        tracing::info!("Connected to Docker daemon at {}", socket_path);
        Ok(Self { client })
    }

    /// Return a reference to the underlying bollard `Docker` client.
    pub fn client(&self) -> &Docker {
        &self.client
    }

    // ── helpers ──────────────────────────────────────────────────────────────

    /// Build a bollard `ServiceSpec` from our domain type.
    fn build_bollard_spec(spec: &ServiceSpec) -> BollardServiceSpec {
        // Mounts
        let mounts: Vec<Mount> = spec
            .mounts
            .iter()
            .map(|m| Mount {
                source: Some(m.source.clone()),
                target: Some(m.target.clone()),
                typ: Some(match m.mount_type {
                    MountType::Volume => MountTypeEnum::VOLUME,
                    MountType::Bind => MountTypeEnum::BIND,
                    MountType::Tmpfs => MountTypeEnum::TMPFS,
                }),
                read_only: Some(m.readonly),
                ..Default::default()
            })
            .collect();

        // Networks (task-level, preferred in API >= v1.44)
        let networks: Vec<NetworkAttachmentConfig> = spec
            .networks
            .iter()
            .map(|n| NetworkAttachmentConfig {
                target: Some(n.clone()),
                aliases: None,
                driver_opts: None,
            })
            .collect();

        // Resources
        let resources = spec.resources.as_ref().map(|r| {
            // 1 CPU = 1e9 nano-CPUs
            let cpu_to_nano = |cpus: f64| -> i64 { (cpus * 1_000_000_000.0) as i64 };
            let mb_to_bytes = |mb: u64| -> i64 { (mb * 1024 * 1024) as i64 };

            TaskSpecResources {
                limits: Some(Limit {
                    nano_cpus: r.cpu_limit.map(cpu_to_nano),
                    memory_bytes: r.memory_limit_mb.map(mb_to_bytes),
                    pids: None,
                }),
                reservations: Some(ResourceObject {
                    nano_cpus: r.cpu_reservation.map(cpu_to_nano),
                    memory_bytes: r.memory_reservation_mb.map(mb_to_bytes),
                    generic_resources: None,
                }),
            }
        });

        // Task template
        let task_template = TaskSpec {
            container_spec: Some(TaskSpecContainerSpec {
                image: Some(spec.image.clone()),
                env: if spec.env.is_empty() {
                    None
                } else {
                    Some(spec.env.clone())
                },
                mounts: if mounts.is_empty() { None } else { Some(mounts) },
                labels: if spec.labels.is_empty() {
                    None
                } else {
                    Some(spec.labels.clone())
                },
                ..Default::default()
            }),
            networks: if networks.is_empty() {
                None
            } else {
                Some(networks)
            },
            resources,
            ..Default::default()
        };

        // Build endpoint port configs. On macOS Docker Desktop, port_proxy mode is
        // used instead (socat containers), so spec.ports will be empty here. On
        // Linux production, ports are published directly via Swarm host mode.
        let endpoint_ports: Vec<EndpointPortConfig> = spec
            .ports
            .iter()
            .filter(|p| p.published.is_some())
            .map(|p| {
                let proto = match p.protocol.to_ascii_lowercase().as_str() {
                    "udp" => EndpointPortConfigProtocolEnum::UDP,
                    "sctp" => EndpointPortConfigProtocolEnum::SCTP,
                    _ => EndpointPortConfigProtocolEnum::TCP,
                };
                EndpointPortConfig {
                    protocol: Some(proto),
                    target_port: Some(p.target as i64),
                    published_port: Some(p.published.unwrap() as i64),
                    publish_mode: Some(EndpointPortConfigPublishModeEnum::HOST),
                    ..Default::default()
                }
            })
            .collect();

        let endpoint_spec = Some(EndpointSpec {
            mode: Some(EndpointSpecModeEnum::DNSRR),
            ports: if endpoint_ports.is_empty() { None } else { Some(endpoint_ports) },
        });

        // Start the new task before stopping the old one so there is no
        // availability gap during redeploys. Swarm will still record one
        // shutdown task per update (task history), which we keep at 1 via
        // the install-time `docker swarm update --task-history-limit 1`.
        let update_config = Some(ServiceSpecUpdateConfig {
            parallelism: Some(1),
            order: Some(ServiceSpecUpdateConfigOrderEnum::START_FIRST),
            failure_action: Some(ServiceSpecUpdateConfigFailureActionEnum::ROLLBACK),
            monitor: Some(5_000_000_000), // 5 s in nanoseconds
            ..Default::default()
        });

        let rollback_config = Some(ServiceSpecRollbackConfig {
            parallelism: Some(1),
            ..Default::default()
        });

        BollardServiceSpec {
            name: Some(spec.name.clone()),
            labels: if spec.labels.is_empty() {
                None
            } else {
                Some(spec.labels.clone())
            },
            task_template: Some(task_template),
            mode: Some(ServiceSpecMode {
                replicated: Some(ServiceSpecModeReplicated {
                    replicas: Some(spec.replicas as i64),
                }),
                ..Default::default()
            }),
            update_config,
            rollback_config,
            endpoint_spec,
            ..Default::default()
        }
    }

    /// Build a bollard `ServiceSpec` with an explicit image string (already includes tag).
    fn build_bollard_spec_with_image(spec: &ServiceSpec, image: &str) -> BollardServiceSpec {
        let mut s = Self::build_bollard_spec(spec);
        if let Some(tt) = s.task_template.as_mut() {
            if let Some(cs) = tt.container_spec.as_mut() {
                cs.image = Some(image.to_string());
            }
        }
        s
    }

    /// Public wrapper so the docker crate's lib.rs can re-export this.
    pub async fn raw_unix_request_pub(
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> AppResult<String> {
        Self::raw_unix_request(method, path, body).await
    }

    /// Perform a raw HTTP/1.1 request over the Docker Unix socket.
    ///
    /// Returns the response body as a `String`.  This is used for Swarm and
    /// Task endpoints that bollard 0.17 does not expose as public methods.
    async fn raw_unix_request(
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> AppResult<String> {
        use tokio::net::UnixStream;

        let socket_path = "/var/run/docker.sock";
        let mut stream = UnixStream::connect(socket_path).await.map_err(|e| {
            AppError::Docker(format!("Cannot connect to Docker socket: {e}"))
        })?;

        let content_type = "application/json";
        let body_bytes = body.unwrap_or("").as_bytes();
        let request = if body.is_some() {
            format!(
                "{method} {path} HTTP/1.1\r\n\
                 Host: localhost\r\n\
                 Content-Type: {content_type}\r\n\
                 Content-Length: {}\r\n\
                 Connection: close\r\n\
                 \r\n\
                 {}",
                body_bytes.len(),
                body.unwrap_or("")
            )
        } else {
            format!(
                "{method} {path} HTTP/1.1\r\n\
                 Host: localhost\r\n\
                 Content-Length: 0\r\n\
                 Connection: close\r\n\
                 \r\n"
            )
        };

        stream
            .write_all(request.as_bytes())
            .await
            .map_err(|e| AppError::Docker(format!("Write to Docker socket failed: {e}")))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .map_err(|e| AppError::Docker(format!("Read from Docker socket failed: {e}")))?;

        // Split headers from body
        let body_start = response
            .find("\r\n\r\n")
            .map(|i| i + 4)
            .unwrap_or(response.len());
        let raw_body = &response[body_start..];

        // Handle chunked transfer encoding
        let first_line = response.lines().next().unwrap_or("");
        let is_chunked = response
            .lines()
            .any(|l| l.to_ascii_lowercase().contains("transfer-encoding: chunked"));

        if is_chunked {
            Ok(decode_chunked(raw_body))
        } else {
            // Check HTTP status
            if !first_line.contains("200") && !first_line.contains("201") && !first_line.contains("204") {
                return Err(AppError::Docker(format!(
                    "Docker API error: {}  body={}",
                    first_line,
                    raw_body.chars().take(300).collect::<String>()
                )));
            }
            Ok(raw_body.to_string())
        }
    }
}

/// Decode an HTTP/1.1 chunked body.
fn decode_chunked(input: &str) -> String {
    let mut out = String::new();
    let mut remaining = input;
    loop {
        // Find the chunk-size line
        let Some(nl) = remaining.find("\r\n") else {
            break;
        };
        let size_str = remaining[..nl].trim();
        let size = usize::from_str_radix(size_str, 16).unwrap_or(0);
        if size == 0 {
            break;
        }
        let data_start = nl + 2;
        if data_start + size > remaining.len() {
            break;
        }
        out.push_str(&remaining[data_start..data_start + size]);
        remaining = &remaining[data_start + size..];
        // skip trailing CRLF after chunk data
        if remaining.starts_with("\r\n") {
            remaining = &remaining[2..];
        }
    }
    out
}

// ─── DockerEngine impl ────────────────────────────────────────────────────────

#[async_trait::async_trait]
impl DockerEngine for BollardDockerEngine {
    // ── Service management ────────────────────────────────────────────────────

    async fn create_service(&self, spec: ServiceSpec) -> AppResult<String> {
        // Build a fully-qualified image reference that includes the tag.
        let image = if spec.image.contains(':') {
            spec.image.clone()
        } else {
            format!("{}:latest", spec.image)
        };

        let bollard_spec = Self::build_bollard_spec_with_image(&spec, &image);

        let resp = self
            .client
            .create_service(bollard_spec, None)
            .await
            .map_err(|e| AppError::Docker(format!("create_service failed: {e}")))?;

        if let Some(warnings) = &resp.warnings {
            for w in warnings {
                tracing::warn!("Docker create_service warning: {w}");
            }
        }

        resp.id
            .ok_or_else(|| AppError::Docker("create_service returned no ID".into()))
    }

    async fn update_service(&self, id: &str, spec: ServiceSpec) -> AppResult<()> {
        // We need the current version to avoid a conflict error.
        let current = self
            .client
            .inspect_service(id, None::<InspectServiceOptions>)
            .await
            .map_err(|e| AppError::Docker(format!("inspect_service failed: {e}")))?;

        let version = current
            .version
            .and_then(|v| v.index)
            .ok_or_else(|| AppError::Docker("Could not read service version".into()))?;

        let image = if spec.image.contains(':') {
            spec.image.clone()
        } else {
            format!("{}:latest", spec.image)
        };

        let bollard_spec = Self::build_bollard_spec_with_image(&spec, &image);

        self.client
            .update_service(
                id,
                bollard_spec,
                UpdateServiceOptions {
                    version,
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|e| AppError::Docker(format!("update_service failed: {e}")))?;

        Ok(())
    }

    async fn remove_service(&self, id: &str) -> AppResult<()> {
        self.client
            .delete_service(id)
            .await
            .map_err(|e| AppError::Docker(format!("delete_service failed: {e}")))?;
        Ok(())
    }

    async fn scale_service(&self, id: &str, replicas: u64) -> AppResult<()> {
        // Fetch the current service spec so we can preserve all other fields.
        let current = self
            .client
            .inspect_service(id, None::<InspectServiceOptions>)
            .await
            .map_err(|e| AppError::Docker(format!("inspect_service failed: {e}")))?;

        let version = current
            .version
            .and_then(|v| v.index)
            .ok_or_else(|| AppError::Docker("Could not read service version".into()))?;

        // Start from the existing spec, then patch the replica count.
        let mut bollard_spec: BollardServiceSpec =
            current.spec.unwrap_or_default();

        let mode = bollard_spec.mode.get_or_insert_with(Default::default);
        let replicated = mode
            .replicated
            .get_or_insert_with(Default::default);
        replicated.replicas = Some(replicas as i64);

        self.client
            .update_service(
                id,
                bollard_spec,
                UpdateServiceOptions {
                    version,
                    ..Default::default()
                },
                None,
            )
            .await
            .map_err(|e| AppError::Docker(format!("scale_service failed: {e}")))?;

        Ok(())
    }

    // ── Task inspection ───────────────────────────────────────────────────────

    async fn list_tasks(&self, service_id: &str) -> AppResult<Vec<TaskInfo>> {
        // bollard 0.17 does not expose a `list_tasks` method; we call the
        // Docker API directly over the Unix socket.
        let path = format!(
            "/v1.45/tasks?filters={{\"service\":[\"{service_id}\"]}}"
        );
        let body = BollardDockerEngine::raw_unix_request("GET", &path, None).await?;

        let tasks: Vec<serde_json::Value> = serde_json::from_str(&body)
            .map_err(|e| AppError::Docker(format!("Failed to parse task list: {e}")))?;

        Ok(tasks.into_iter().map(task_value_to_info).collect())
    }

    async fn inspect_task(&self, task_id: &str) -> AppResult<TaskDetail> {
        let path = format!("/v1.45/tasks/{task_id}");
        let body = BollardDockerEngine::raw_unix_request("GET", &path, None).await?;

        let v: serde_json::Value = serde_json::from_str(&body)
            .map_err(|e| AppError::Docker(format!("Failed to parse task: {e}")))?;

        Ok(task_value_to_detail(&v))
    }

    // ── Container logs ────────────────────────────────────────────────────────

    async fn container_logs(
        &self,
        container_id: &str,
        opts: LogOpts,
    ) -> AppResult<Vec<String>> {
        let log_opts = LogsOptions::<String> {
            stdout: opts.stdout,
            stderr: opts.stderr,
            follow: opts.follow,
            since: opts.since.unwrap_or(0),
            until: opts.until.unwrap_or(0),
            timestamps: opts.timestamps,
            tail: opts.tail.unwrap_or_else(|| "all".into()),
        };

        let mut stream = self.client.logs(container_id, Some(log_opts));
        let mut lines = Vec::new();

        while let Some(item) = stream.next().await {
            match item {
                Ok(log_output) => {
                    lines.push(log_output.to_string());
                }
                Err(e) => {
                    return Err(AppError::Docker(format!("Log stream error: {e}")));
                }
            }
        }

        Ok(lines)
    }

    // ── Network management ────────────────────────────────────────────────────

    async fn create_network(&self, name: &str, driver: &str) -> AppResult<String> {
        let resp = self
            .client
            .create_network(CreateNetworkOptions {
                name: name.to_string(),
                driver: driver.to_string(),
                check_duplicate: true,
                attachable: true,
                ..Default::default()
            })
            .await
            .map_err(|e| AppError::Docker(format!("create_network failed: {e}")))?;

        resp.id
            .ok_or_else(|| AppError::Docker("create_network returned no ID".into()))
    }

    async fn remove_network(&self, id: &str) -> AppResult<()> {
        self.client
            .remove_network(id)
            .await
            .map_err(|e| AppError::Docker(format!("remove_network failed: {e}")))?;
        Ok(())
    }

    // ── Volume management ─────────────────────────────────────────────────────

    async fn create_volume(&self, name: &str, driver: &str) -> AppResult<()> {
        self.client
            .create_volume(CreateVolumeOptions {
                name: name.to_string(),
                driver: driver.to_string(),
                ..Default::default()
            })
            .await
            .map_err(|e| AppError::Docker(format!("create_volume failed: {e}")))?;
        Ok(())
    }

    async fn remove_volume(&self, name: &str) -> AppResult<()> {
        self.client
            .remove_volume(name, None::<RemoveVolumeOptions>)
            .await
            .map_err(|e| AppError::Docker(format!("remove_volume failed: {e}")))?;
        Ok(())
    }

    // ── Image management ──────────────────────────────────────────────────────

    async fn pull_image(&self, image: &str, tag: &str) -> AppResult<Vec<String>> {
        let mut stream = self.client.create_image(
            Some(CreateImageOptions {
                from_image: image,
                tag,
                ..Default::default()
            }),
            None,
            None,
        );

        let mut lines = Vec::new();
        while let Some(item) = stream.next().await {
            match item {
                Ok(info) => {
                    // Collect meaningful status lines.
                    if let Some(status) = info.status {
                        let mut line = status;
                        if let Some(progress) = info.progress {
                            line.push(' ');
                            line.push_str(&progress);
                        }
                        lines.push(line);
                    }
                }
                Err(e) => {
                    return Err(AppError::Docker(format!("pull_image failed: {e}")));
                }
            }
        }

        Ok(lines)
    }

    // ── Image build ───────────────────────────────────────────────────────────

    async fn build_image(
        &self,
        tag: &str,
        context_path: &str,
        dockerfile: &str,
    ) -> AppResult<Vec<String>> {
        // Shell out to `docker build` so .dockerignore and all BuildKit
        // features are handled by the Docker CLI transparently.
        let dockerfile_abs = format!("{context_path}/{dockerfile}");
        let output = tokio::process::Command::new("docker")
            .args([
                "build",
                "--progress=plain",
                "-t", tag,
                "-f", &dockerfile_abs,
                context_path,
            ])
            .output()
            .await
            .map_err(|e| AppError::Docker(format!("Failed to spawn docker build: {e}")))?;

        // docker build writes progress to stderr; stdout gets the image digest.
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let lines: Vec<String> = stderr
            .lines()
            .chain(stdout.lines())
            .map(|l| l.to_string())
            .collect();

        if !output.status.success() {
            let summary = lines
                .iter()
                .filter(|l| !l.is_empty())
                .last()
                .cloned()
                .unwrap_or_else(|| "unknown build error".to_string());
            return Err(AppError::Docker(format!("docker build failed: {summary}")));
        }

        tracing::info!(%tag, "docker build completed ({} output lines)", lines.len());
        Ok(lines)
    }

    // ── Swarm management ──────────────────────────────────────────────────────

    async fn init_swarm(&self, advertise_addr: &str) -> AppResult<()> {
        // bollard 0.17 does not expose `init_swarm` as a public method; call
        // the Docker API directly.
        let req = SwarmInitRequest {
            advertise_addr: Some(advertise_addr.to_string()),
            listen_addr: Some("0.0.0.0:2377".to_string()),
            ..Default::default()
        };

        let body = serde_json::to_string(&req)
            .map_err(|e| AppError::Docker(format!("Serialize swarm init request: {e}")))?;

        BollardDockerEngine::raw_unix_request("POST", "/v1.45/swarm/init", Some(&body))
            .await
            .map(|_| ())
    }

    async fn swarm_info(&self) -> AppResult<SwarmInfo> {
        // `SystemInfo` (returned by `info()`) contains a `swarm` field of
        // type `bollard::models::SwarmInfo`.
        let sys = self
            .client
            .info()
            .await
            .map_err(|e| AppError::Docker(format!("docker info failed: {e}")))?;

        let si = sys
            .swarm
            .ok_or_else(|| AppError::Docker("Node is not part of a swarm".into()))?;

        let node_id = si.node_id.unwrap_or_default();
        let node_addr = si.node_addr.unwrap_or_default();
        let is_manager = si.control_available.unwrap_or(false);
        let nodes = si.nodes.unwrap_or(0) as u64;
        let managers = si.managers.unwrap_or(0) as u64;

        Ok(SwarmInfo {
            node_id,
            node_addr,
            is_manager,
            nodes,
            managers,
        })
    }

    // ── Container inspection ──────────────────────────────────────────────────

    async fn inspect_container(&self, container_id: &str) -> AppResult<ContainerDetail> {
        let resp = self
            .client
            .inspect_container(
                container_id,
                Some(InspectContainerOptions { size: false }),
            )
            .await
            .map_err(|e| AppError::Docker(format!("inspect_container failed: {e}")))?;

        let id = resp.id.unwrap_or_default();
        let name = resp
            .name
            .map(|n| n.trim_start_matches('/').to_string())
            .unwrap_or_default();
        let image = resp.image.unwrap_or_default();
        let created = resp.created;
        let platform = resp.platform;

        let (status, state, started_at, finished_at, exit_code, restart_count) =
            if let Some(st) = resp.state {
                let state_str = st.status.map(|s| s.to_string()).unwrap_or_default();
                let running = if st.running.unwrap_or(false) { "running" } else { "stopped" };
                (
                    running.to_string(),
                    state_str,
                    st.started_at,
                    st.finished_at,
                    st.exit_code,
                    st.restarting.map(|r| if r { 1i64 } else { 0 }).unwrap_or(0),
                )
            } else {
                ("unknown".into(), "unknown".into(), None, None, None, 0)
            };

        // Extract port bindings: map "80/tcp" → [{HostIp, HostPort}, ...]
        let port_bindings = resp
            .network_settings
            .as_ref()
            .and_then(|ns| ns.ports.as_ref())
            .map(|ports| {
                let mut bindings = Vec::new();
                for (port_proto, host_bindings) in ports {
                    // port_proto is like "80/tcp"
                    let (port_str, proto) = port_proto
                        .split_once('/')
                        .unwrap_or((port_proto.as_str(), "tcp"));
                    let container_port: u16 = port_str.parse().unwrap_or(0);
                    let protocol = proto.to_string();

                    if let Some(hbs) = host_bindings {
                        for hb in hbs {
                            let host_port: u16 = hb
                                .host_port
                                .as_deref()
                                .unwrap_or("0")
                                .parse()
                                .unwrap_or(0);
                            bindings.push(PortBinding {
                                container_port,
                                protocol: protocol.clone(),
                                host_ip: hb.host_ip.clone().unwrap_or_default(),
                                host_port,
                            });
                        }
                    }
                }
                bindings
            })
            .unwrap_or_default();

        let config = resp.config.unwrap_or_default();
        let labels = config.labels.unwrap_or_default();
        let env = config.env.unwrap_or_default();

        Ok(ContainerDetail {
            id,
            name,
            image,
            status,
            state,
            created,
            started_at,
            finished_at,
            exit_code,
            restart_count,
            platform,
            port_bindings,
            env,
            labels,
        })
    }

    async fn stop_container(&self, container_id: &str, timeout_secs: i64) -> AppResult<()> {
        self.client
            .stop_container(container_id, Some(StopContainerOptions { t: timeout_secs }))
            .await
            .map_err(|e| AppError::Docker(format!("stop_container failed: {e}")))?;
        Ok(())
    }

    async fn restart_container(&self, container_id: &str, timeout_secs: i64) -> AppResult<()> {
        self.client
            .restart_container(container_id, Some(RestartContainerOptions { t: timeout_secs as isize }))
            .await
            .map_err(|e| AppError::Docker(format!("restart_container failed: {e}")))?;
        Ok(())
    }

    // ── Daemon health ─────────────────────────────────────────────────────────

    async fn ping(&self) -> AppResult<()> {
        self.client
            .ping()
            .await
            .map_err(|e| AppError::Docker(format!("Docker ping failed: {e}")))?;
        Ok(())
    }

    // ── Service listing (for reconciliation) ──────────────────────────────────

    async fn list_services(&self) -> AppResult<Vec<TaskInfo>> {
        let services = self
            .client
            .list_services(None::<ListServicesOptions<String>>)
            .await
            .map_err(|e| AppError::Docker(format!("list_services failed: {e}")))?;

        let infos = services
            .into_iter()
            .map(|svc| {
                let docker_id = svc.id.unwrap_or_default();
                // Use the spec name (e.g. "platform-{uuid}") as the id so callers
                // can match against the name they used when creating the service.
                // Fall back to the Docker-generated ID only when the name is absent.
                let name = svc
                    .spec
                    .as_ref()
                    .and_then(|s| s.name.clone())
                    .unwrap_or_else(|| docker_id.clone());

                let image = svc
                    .spec
                    .as_ref()
                    .and_then(|s| s.task_template.as_ref())
                    .and_then(|t| t.container_spec.as_ref())
                    .and_then(|c| c.image.clone())
                    .unwrap_or_default();

                // Use the number of running replicas as a synthetic "status".
                let replicas = svc
                    .service_status
                    .as_ref()
                    .and_then(|ss| ss.running_tasks)
                    .unwrap_or(0);

                let desired = svc
                    .service_status
                    .as_ref()
                    .and_then(|ss| ss.desired_tasks)
                    .unwrap_or(0);

                let status = format!("{replicas}/{desired} replicas");

                TaskInfo {
                    id: name.clone(),
                    service_id: name,
                    node_id: None,
                    status,
                    desired_state: "running".into(),
                    container_id: None,
                    image,
                    created_at: None,
                    updated_at: None,
                    error: None,
                    exit_code: None,
                    slot: None,
                }
            })
            .collect();

        Ok(infos)
    }

    async fn prune_containers(&self) -> AppResult<u64> {
        use bollard::container::PruneContainersOptions;
        let result = self.client
            .prune_containers(None::<PruneContainersOptions<String>>)
            .await
            .map_err(|e| AppError::Docker(format!("prune_containers failed: {e}")))?;
        Ok(result.containers_deleted.map(|v| v.len() as u64).unwrap_or(0))
    }

    async fn list_all_containers(&self) -> AppResult<Vec<ContainerSummary>> {
        use bollard::container::ListContainersOptions;
        let opts = ListContainersOptions::<String> { all: true, ..Default::default() };
        let raw = self.client
            .list_containers(Some(opts))
            .await
            .map_err(|e| AppError::Docker(format!("list_containers failed: {e}")))?;

        Ok(raw.into_iter().map(|c| {
            let ports = c.ports.unwrap_or_default().into_iter()
                .filter_map(|p| {
                    let ip      = p.ip.as_deref().unwrap_or("0.0.0.0");
                    let private = p.private_port;
                    let proto   = p.typ.as_ref().map(|t| format!("{t:?}")).unwrap_or_default().to_lowercase();
                    match p.public_port {
                        Some(pub_p) => Some(format!("{ip}:{pub_p}->{private}/{proto}")),
                        None        => Some(format!("{private}/{proto}")),
                    }
                })
                .collect();

            ContainerSummary {
                id:      c.id.unwrap_or_default(),
                names:   c.names.unwrap_or_default().into_iter().map(|n| n.trim_start_matches('/').to_string()).collect(),
                image:   c.image.unwrap_or_default(),
                status:  c.status.unwrap_or_default(),
                state:   c.state.unwrap_or_default(),
                created: c.created.unwrap_or(0),
                ports,
                labels:  c.labels.unwrap_or_default(),
            }
        }).collect())
    }

    async fn list_all_volumes(&self) -> AppResult<Vec<VolumeSummary>> {
        use bollard::volume::ListVolumesOptions;
        let result = self.client
            .list_volumes(None::<ListVolumesOptions<String>>)
            .await
            .map_err(|e| AppError::Docker(format!("list_volumes failed: {e}")))?;

        Ok(result.volumes.unwrap_or_default().into_iter().map(|v| {
            VolumeSummary {
                name:       v.name,
                driver:     v.driver,
                mountpoint: v.mountpoint,
                scope:      v.scope.map(|s| format!("{s:?}")).unwrap_or_default().to_lowercase().trim_matches('"').to_string(),
                labels:     v.labels,
                created_at: v.created_at,
            }
        }).collect())
    }

    async fn list_all_networks(&self) -> AppResult<Vec<NetworkSummary>> {
        use bollard::network::ListNetworksOptions;
        let raw = self.client
            .list_networks(None::<ListNetworksOptions<String>>)
            .await
            .map_err(|e| AppError::Docker(format!("list_networks failed: {e}")))?;

        Ok(raw.into_iter().map(|n| {
            let subnet = n.ipam
                .as_ref()
                .and_then(|ipam| ipam.config.as_ref())
                .and_then(|cfgs| cfgs.first())
                .and_then(|cfg| cfg.subnet.clone());

            let containers = n.containers.as_ref().map(|m| m.len()).unwrap_or(0);

            NetworkSummary {
                id:         n.id.unwrap_or_default(),
                name:       n.name.unwrap_or_default(),
                driver:     n.driver.unwrap_or_default(),
                scope:      n.scope.unwrap_or_default(),
                internal:   n.internal.unwrap_or(false),
                attachable: n.attachable.unwrap_or(false),
                ipam_subnet: subnet,
                labels:     n.labels.unwrap_or_default(),
                containers,
            }
        }).collect())
    }

    async fn list_all_services(&self) -> AppResult<Vec<ServiceSummary>> {
        let raw = self.client
            .list_services(None::<ListServicesOptions<String>>)
            .await
            .map_err(|e| AppError::Docker(format!("list_services failed: {e}")))?;

        Ok(raw.into_iter().map(|svc| {
            let id   = svc.id.clone().unwrap_or_default();
            let spec = svc.spec.as_ref();
            let name  = spec.and_then(|s| s.name.clone()).unwrap_or_else(|| id.clone());
            let image = spec
                .and_then(|s| s.task_template.as_ref())
                .and_then(|t| t.container_spec.as_ref())
                .and_then(|c| c.image.clone())
                .unwrap_or_default();
            let labels = spec.and_then(|s| s.labels.clone()).unwrap_or_default();
            let mode = if spec.and_then(|s| s.mode.as_ref()).and_then(|m| m.global.as_ref()).is_some() {
                "global".to_string()
            } else {
                "replicated".to_string()
            };
            let replicas_running = svc.service_status.as_ref().and_then(|ss| ss.running_tasks).unwrap_or(0);
            let replicas_desired = svc.service_status.as_ref().and_then(|ss| ss.desired_tasks).unwrap_or(0);
            let ports = svc.endpoint
                .as_ref()
                .and_then(|ep| ep.ports.as_ref())
                .map(|ps| ps.iter().filter_map(|p| {
                    let target = p.target_port?;
                    let published = p.published_port?;
                    let proto = p.protocol.as_ref().map(|pr| format!("{pr:?}")).unwrap_or_default().to_lowercase().trim_matches('"').to_string();
                    Some(format!("{published}:{target}/{proto}"))
                }).collect())
                .unwrap_or_default();
            let created_at = svc.created_at.map(|dt| dt.to_string());
            let updated_at = svc.updated_at.map(|dt| dt.to_string());

            ServiceSummary { id, name, image, replicas_running, replicas_desired, mode, ports, labels, created_at, updated_at }
        }).collect())
    }
}

// ─── JSON helpers for raw task responses ─────────────────────────────────────

fn task_value_to_info(v: serde_json::Value) -> TaskInfo {
    let id = v["ID"].as_str().unwrap_or("").to_string();
    let service_id = v["ServiceID"].as_str().unwrap_or("").to_string();
    let node_id = v["NodeID"].as_str().map(String::from);
    let status = v["Status"]["State"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let desired_state = v["DesiredState"].as_str().unwrap_or("unknown").to_string();
    let container_id = v["Status"]["ContainerStatus"]["ContainerID"]
        .as_str()
        .map(String::from);
    let image = v["Spec"]["ContainerSpec"]["Image"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let created_at = v["CreatedAt"].as_str().map(String::from);
    let updated_at = v["UpdatedAt"].as_str().map(String::from);
    let error = v["Status"]["Err"].as_str().map(String::from);
    let exit_code = v["Status"]["ContainerStatus"]["ExitCode"].as_i64();
    let slot = v["Slot"].as_i64();

    TaskInfo {
        id,
        service_id,
        node_id,
        status,
        desired_state,
        container_id,
        image,
        created_at,
        updated_at,
        error,
        exit_code,
        slot,
    }
}

fn task_value_to_detail(v: &serde_json::Value) -> TaskDetail {
    let id = v["ID"].as_str().unwrap_or("").to_string();
    let service_id = v["ServiceID"].as_str().unwrap_or("").to_string();
    let node_id = v["NodeID"].as_str().map(String::from);
    let container_id = v["Status"]["ContainerStatus"]["ContainerID"]
        .as_str()
        .map(String::from);
    let status = v["Status"]["State"]
        .as_str()
        .unwrap_or("unknown")
        .to_string();
    let message = v["Status"]["Message"].as_str().map(String::from);
    let image = v["Spec"]["ContainerSpec"]["Image"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let slot = v["Slot"].as_i64();

    TaskDetail {
        id,
        service_id,
        node_id,
        container_id,
        status,
        message,
        image,
        slot,
    }
}
