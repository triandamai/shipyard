use std::sync::Arc;

use sqlx::PgPool;
use uuid::Uuid;
use shipyard_common::config::AppConfig;
use shipyard_common::error::{AppError, AppResult};
use shipyard_docker::engine::DockerEngine;
use shipyard_docker::types::{ContainerSpec, MountSpec, MountType, ResourceSpec};

use super::models::{SandboxAppConfigRow, SandboxInstanceRow};
use super::quota;

// NOTE ON `AppState`: the brief for this task specifies `start_sandbox(state:
// &AppState, service_id: Uuid)`. `AppState` is defined in
// `backend/crates/api/src/main.rs`, which is the `[[bin]]` target of this
// package. `sandbox_runtime` (this module tree) is owned by
// `backend/crates/api/src/lib.rs` — a *separate* compilation unit, added in
// Task 6 specifically so `backend/crates/api/tests/*.rs` integration tests
// could reach `sandbox_runtime::quota::check_quota` without linking the
// binary. A `[[bin]]` target may depend on its package's lib target, but not
// the reverse — `crate::AppState` genuinely does not exist from inside this
// file (confirmed by compiling: `error[E0432]: unresolved import
// crate::AppState — no AppState in the root`). So `start_sandbox` here takes
// the three pieces of `AppState` it actually needs (`&state.db`,
// `&state.docker`, `&state.config`) instead of the aggregate struct. Thanks
// to `Arc`'s `Deref`, a future caller inside `main.rs` can pass
// `&state.db, &state.docker, &state.config` directly — no unpacking required
// at the call site.
//
// A second, non-decomposable instance of the same crate-boundary issue:
// `crate::resources::sync_traefik_dynamic_config` (declared
// `pub(crate) mod resources;` inside `main.rs`) is likewise unreachable from
// this file (confirmed: `error[E0433]: cannot find resources in the crate
// root`). Unlike `AppState`, this isn't a struct we can decompose into
// primitives — it's a private function with real logic (DB read + YAML file
// write) living in the sibling binary crate. There is no way to call it from
// here without either moving/re-exposing it out of `main.rs` (a change to a
// file outside this task's scope) or introducing a generic callback
// injection seam whose complexity/fragility isn't justified for one call.
// So: `start_sandbox` performs every DB and Docker side effect described in
// the brief, including upserting the preview `domains` row via
// `ensure_preview_domain`, but does **not** call
// `sync_traefik_dynamic_config` itself. The caller — which necessarily lives
// in the binary crate (e.g. Task 9's HTTP handler) — MUST call it
// immediately after a successful `start_sandbox`, using the returned row's
// `container_name` as the upstream override, e.g.:
//
// ```ignore
// let instance = sandbox_runtime::manager::start_sandbox(&state.db, &state.docker, &state.config, service_id).await?;
// crate::resources::sync_traefik_dynamic_config(
//     &state.db, service_id, &state.config.docker.label_prefix,
//     &state.config.traefik.entrypoint_http, &state.config.traefik.entrypoint_https,
//     state.config.traefik.dynamic_config_dir.as_deref(),
//     instance.container_name.as_deref(), false,
// ).await;
// ```
//
// Until that follow-up call is wired in (Task 9), a freshly-started sandbox's
// DB state (service, sandbox_instances, domains rows) is correct but Traefik
// will not yet be routing to it.

pub fn sandbox_container_name(service_id: Uuid) -> String {
    format!("shipyard-sandbox-{}", &service_id.to_string()[..8])
}

pub fn sandbox_volume_name(service_id: Uuid) -> String {
    format!("sandbox-vol-{}", &service_id.to_string()[..8])
}

fn preview_hostname(slug: &str, base_domain: &str) -> String {
    format!("preview-{slug}.{base_domain}")
}

/// Starts (or returns the already-running) sandbox for `service_id`. Enforces
/// the org's dedicated sandbox quota, resolves the app's stack on first start
/// (probe container → detect_stack), launches the gVisor-isolated sandbox
/// container, and upserts the preview `domains` row.
///
/// Does **not** sync Traefik's dynamic config file — see the module-level
/// note above. The caller must do that immediately after this returns Ok.
pub async fn start_sandbox(
    db: &PgPool,
    docker: &Arc<dyn DockerEngine>,
    config: &AppConfig,
    service_id: Uuid,
) -> AppResult<SandboxInstanceRow> {
    let (org_id, slug): (Uuid, String) = sqlx::query_as(
        "SELECT p.org_id, s.slug FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or_else(|| AppError::NotFound(format!("Service '{service_id}' not found")))?;

    // Idempotent: already starting/running just returns current state.
    if let Some(existing) = fetch_instance(db, service_id).await? {
        if existing.status == "starting" || existing.status == "running" {
            return Ok(existing);
        }
    }

    match quota::check_quota(db, org_id).await? {
        Ok(()) => {}
        Err(message) => return Err(AppError::BadRequest(message)),
    }

    upsert_instance_status(db, service_id, "starting", None, None).await?;

    let volume_name = sandbox_volume_name(service_id);
    docker.create_volume(&volume_name, "local").await.ok(); // idempotent: ignore "already exists"

    let mut app_config = fetch_config(db, service_id).await?;
    if app_config.is_none() {
        let stack = probe_and_detect(docker, config, &volume_name).await.map_err(|e| {
            AppError::BadRequest(format!("Could not start sandbox: {e}"))
        })?;
        sqlx::query(
            "INSERT INTO sandbox_app_configs (service_id, runtime, base_image, install_cmd, dev_cmd, port, manifest_source, volume_name)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(service_id)
        .bind(&stack.runtime)
        .bind(&stack.base_image)
        .bind(&stack.install_cmd)
        .bind(&stack.dev_cmd)
        .bind(stack.port as i32)
        .bind(match stack.source {
            shipyard_engine::sandbox_probe::DetectionSource::Detected => "detected",
            shipyard_engine::sandbox_probe::DetectionSource::Manifest => "manifest",
        })
        .bind(&volume_name)
        .execute(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        app_config = fetch_config(db, service_id).await?;
    }
    let app_config = app_config.ok_or_else(|| AppError::Internal("sandbox config missing after insert".to_string()))?;

    let container_name = sandbox_container_name(service_id);
    docker.remove_container(&container_name, true).await.ok(); // clean slate

    let (cpu_cores, memory_gb): (f64, f64) = sqlx::query_as(
        "SELECT p.sandbox_cpu_cores, p.sandbox_memory_gb
         FROM organizations o JOIN plans p ON p.id = o.plan_id WHERE o.id = $1",
    )
    .bind(org_id)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let install_and_dev = match &app_config.install_cmd {
        Some(install) => format!("{install} && {}", app_config.dev_cmd.as_deref().unwrap_or("")),
        None => app_config.dev_cmd.clone().unwrap_or_default(),
    };

    let container_id = docker
        .create_container(ContainerSpec {
            name: container_name.clone(),
            image: app_config.base_image.clone().unwrap_or_default(),
            cmd: Some(vec!["sh".to_string(), "-c".to_string(), install_and_dev]),
            env: vec![format!("PORT={}", app_config.port.unwrap_or(3000))],
            mounts: vec![MountSpec {
                source: volume_name.clone(),
                target: "/app".to_string(),
                mount_type: MountType::Volume,
                readonly: false,
            }],
            network: Some(config.traefik.network.clone()),
            network_aliases: vec![container_name.clone()],
            runtime_class: Some(config.sandbox.runtime_class.clone()),
            resources: Some(ResourceSpec {
                cpu_limit: Some(cpu_cores),
                memory_limit_mb: Some((memory_gb * 1024.0) as u64),
                cpu_reservation: None,
                memory_reservation_mb: None,
            }),
        })
        .await?;

    docker.start_container(&container_id).await?;

    let hostname = preview_hostname(&slug, &config.sandbox.preview_base_domain);
    ensure_preview_domain(db, service_id, &hostname, app_config.port.unwrap_or(3000)).await?;

    let preview_url = format!("https://{hostname}");
    upsert_instance_status(db, service_id, "running", Some(&container_id), Some(&container_name)).await?;
    sqlx::query("UPDATE sandbox_instances SET preview_url = $2, started_at = NOW(), last_heartbeat_at = NOW() WHERE service_id = $1")
        .bind(service_id)
        .bind(&preview_url)
        .execute(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    fetch_instance(db, service_id)
        .await?
        .ok_or_else(|| AppError::Internal("sandbox_instances row missing after start".to_string()))
}

async fn probe_and_detect(
    docker: &Arc<dyn DockerEngine>,
    config: &AppConfig,
    volume_name: &str,
) -> Result<shipyard_engine::sandbox_probe::DetectedStack, String> {
    use shipyard_engine::sandbox_probe::{detect_stack, ProbeResult, SANDBOX_PROBE_SCRIPT};

    let probe_name = format!("shipyard-probe-{}", Uuid::new_v4().simple());
    let container_id = docker
        .create_container(ContainerSpec {
            name: probe_name.clone(),
            image: config.sandbox.probe_image.clone(),
            cmd: Some(vec!["sh".to_string(), "-c".to_string(), SANDBOX_PROBE_SCRIPT.to_string()]),
            env: vec![],
            mounts: vec![MountSpec {
                source: volume_name.to_string(),
                target: "/app".to_string(),
                mount_type: MountType::Volume,
                readonly: true,
            }],
            network: None,
            network_aliases: vec![],
            runtime_class: Some(config.sandbox.runtime_class.clone()),
            resources: None,
        })
        .await
        .map_err(|e| e.to_string())?;

    docker.start_container(&container_id).await.map_err(|e| e.to_string())?;
    docker.wait_container(&container_id).await.map_err(|e| e.to_string())?;
    let logs = docker
        .container_logs(&container_id, shipyard_docker::types::LogOpts { stdout: true, stderr: false, ..Default::default() })
        .await
        .map_err(|e| e.to_string())?;
    docker.remove_container(&container_id, true).await.ok();

    let output = logs.join("");
    let probe: ProbeResult = serde_json::from_str(output.trim())
        .map_err(|e| format!("probe output was not valid JSON: {e} (output: {output})"))?;
    detect_stack(&probe)
}

async fn ensure_preview_domain(db: &PgPool, service_id: Uuid, hostname: &str, port: i32) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO domains (id, service_id, hostname, tls_enabled, cert_provider, port, traefik_router_name)
         VALUES ($1, $2, $3, TRUE, 'letsencrypt', $4, '')
         ON CONFLICT (service_id, hostname) DO UPDATE SET port = EXCLUDED.port",
    )
    .bind(Uuid::new_v4())
    .bind(service_id)
    .bind(hostname)
    .bind(port)
    .execute(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

async fn fetch_config(db: &PgPool, service_id: Uuid) -> AppResult<Option<SandboxAppConfigRow>> {
    sqlx::query_as("SELECT * FROM sandbox_app_configs WHERE service_id = $1")
        .bind(service_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

pub(crate) async fn fetch_instance(db: &PgPool, service_id: Uuid) -> AppResult<Option<SandboxInstanceRow>> {
    sqlx::query_as("SELECT * FROM sandbox_instances WHERE service_id = $1")
        .bind(service_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))
}

async fn upsert_instance_status(
    db: &PgPool,
    service_id: Uuid,
    status: &str,
    container_id: Option<&str>,
    container_name: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO sandbox_instances (service_id, status, container_id, container_name, updated_at)
         VALUES ($1, $2, $3, $4, NOW())
         ON CONFLICT (service_id) DO UPDATE
           SET status = EXCLUDED.status, container_id = EXCLUDED.container_id,
               container_name = EXCLUDED.container_name, updated_at = NOW()",
    )
    .bind(service_id)
    .bind(status)
    .bind(container_id)
    .bind(container_name)
    .execute(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn sandbox_container_name_is_stable_across_calls_for_same_service() {
        let service_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        let name1 = sandbox_container_name(service_id);
        let name2 = sandbox_container_name(service_id);
        assert_eq!(name1, name2);
        assert_eq!(name1, "shipyard-sandbox-11111111");
    }

    #[test]
    fn sandbox_volume_name_is_stable_and_distinct_from_container_name() {
        let service_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        let vol = sandbox_volume_name(service_id);
        assert_eq!(vol, "sandbox-vol-11111111");
        assert_ne!(vol, sandbox_container_name(service_id));
    }
}
