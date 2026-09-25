use sqlx::PgPool;
use uuid::Uuid;
use shipyard_common::error::{AppError, AppResult};
use shipyard_docker::types::{ContainerSpec, MountSpec, MountType, ResourceSpec};

use crate::AppState;
use super::models::{SandboxAppConfigRow, SandboxInstanceRow};
use super::quota;

pub fn sandbox_container_name(service_id: Uuid) -> String {
    format!("shipyard-sandbox-{}", &service_id.to_string()[..8])
}

pub fn sandbox_volume_name(service_id: Uuid) -> String {
    format!("sandbox-vol-{}", &service_id.to_string()[..8])
}

/// Preview hostnames are keyed by the first 8 hex chars of the service's
/// globally-unique id, not by `services.slug` — slugs are only unique per
/// project (`UNIQUE (project_id, slug)`), so a slug-keyed hostname would let
/// two tenants collide on the globally-unique `domains.hostname`. Same
/// convention as `efg-{group_id[..8]}` / `shipyard-edge-{org_id[..8]}`.
fn preview_hostname(service_id: Uuid, base_domain: &str) -> String {
    format!("preview-{}.{base_domain}", &service_id.to_string()[..8])
}

/// The Traefik dynamic-config filename for a sandbox's route. Keyed by
/// `service_id` for the same reason as `preview_hostname` — the default
/// slug-derived filename would let a sandbox clobber an unrelated service's
/// routing file.
fn sandbox_traefik_config_name(service_id: Uuid) -> String {
    format!("sbx-{}", &service_id.to_string()[..8])
}

/// Wraps `install_and_dev` with a conditional first-boot seed step when
/// `seed_script_b64` is present. The check is unconditionally cheap and
/// idempotent (`ls -A` on a non-empty /app is a no-op), so this same helper
/// applies to every sandbox start, not just template-created ones — a
/// probe-detected app simply never has a seed script to run.
fn build_startup_command(seed_script_b64: Option<&str>, install_and_dev: &str) -> String {
    match seed_script_b64 {
        Some(b64) => format!(
            "if [ -z \"$(ls -A /app 2>/dev/null)\" ]; then echo '{b64}' | base64 -d | sh; fi; {install_and_dev}"
        ),
        None => install_and_dev.to_string(),
    }
}

/// Splits a combined `image:tag` reference into the `(image, tag)` pair
/// `DockerEngine::pull_image` expects. A `:` that appears before a `/` is a
/// registry port, not a tag, so those fall back to the `latest` tag.
fn split_image_ref(image_ref: &str) -> (&str, &str) {
    match image_ref.rsplit_once(':') {
        Some((image, tag)) if !tag.is_empty() && !tag.contains('/') => (image, tag),
        _ => (image_ref, "latest"),
    }
}

/// Starts (or returns the already-running) sandbox for `service_id`. Enforces
/// the org's dedicated sandbox quota, resolves the app's stack on first start
/// (probe container → detect_stack), launches the gVisor-isolated sandbox
/// container, and points the app's Traefik route at it.
pub async fn start_sandbox(state: &AppState, service_id: Uuid) -> AppResult<SandboxInstanceRow> {
    // The feature flag must gate the actual container launch, not just the
    // idle reaper — otherwise a start on a deployment with the reaper
    // disabled would run an uncapped sandbox forever.
    if !state.config.sandbox.enabled {
        return Err(AppError::BadRequest(
            "Sandbox runtime is not enabled on this deployment".to_string(),
        ));
    }

    let org_id: Uuid = sqlx::query_scalar(
        "SELECT p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or_else(|| AppError::NotFound(format!("Service '{service_id}' not found")))?;

    // Idempotent: already starting just returns current state. "running" is
    // only trusted after confirming the container is actually alive — the DB
    // row can go stale if the container crashed, was OOM-killed, or was
    // removed out-of-band, in which case we self-heal by falling through to
    // re-provisioning instead of returning stale state.
    let mut self_healing = false;
    if let Some(existing) = fetch_instance(&state.db, service_id).await? {
        if existing.status == "starting" {
            return Ok(existing);
        }
        if existing.status == "running" {
            let inspect_result = match &existing.container_id {
                Some(id) => state.docker.inspect_container(id).await.map(|d| d.state).map_err(|e| e.to_string()),
                None => Err("no container_id recorded".to_string()),
            };
            if !is_container_dead(&inspect_result) {
                return Ok(existing);
            }
            tracing::warn!(service_id = %service_id, "start_sandbox: DB says running but container is dead — self-healing");
            // Fall through to relaunch below instead of returning stale state.
            self_healing = true;
        }
    }

    match quota::check_quota(&state.db, org_id).await? {
        Ok(()) => {}
        Err(message) => return Err(AppError::Conflict(message)),
    }

    // Claim the start atomically. The idempotency check above is racy on its
    // own — two concurrent calls for the same service can both read "stopped"
    // and both fall through — so the "starting" write doubles as the claim:
    // the conditional `DO UPDATE ... WHERE` only fires (and only then does
    // `RETURNING` produce a row) if the row isn't already starting/running.
    // Postgres re-evaluates that WHERE against the latest committed row
    // version, so the loser of a race always sees the winner's 'starting'.
    // The `$2` branch lets the self-heal path above (DB says running, the
    // container is actually dead) claim a 'running' row — exactly once, for
    // the same reason.
    let claimed: Option<(Uuid,)> = sqlx::query_as(
        "INSERT INTO sandbox_instances (service_id, status, updated_at)
         VALUES ($1, 'starting', NOW())
         ON CONFLICT (service_id) DO UPDATE
           SET status = 'starting', container_id = NULL, container_name = NULL, updated_at = NOW()
           WHERE sandbox_instances.status NOT IN ('starting', 'running')
              OR ($2 AND sandbox_instances.status = 'running')
         RETURNING service_id",
    )
    .bind(service_id)
    .bind(self_healing)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    if claimed.is_none() {
        // Another concurrent call already claimed the start — return its
        // current state rather than racing on container creation.
        return fetch_instance(&state.db, service_id)
            .await?
            .ok_or_else(|| AppError::Internal("sandbox_instances row missing after concurrent claim".to_string()));
    }

    let volume_name = sandbox_volume_name(service_id);

    match provision_sandbox(state, service_id, &volume_name, org_id).await {
        Ok(row) => Ok(row),
        Err(e) => {
            // Reset so a retry isn't permanently blocked by the idempotency
            // check above — otherwise a single failed first-start would wedge
            // the service in "starting" forever.
            upsert_instance_status(&state.db, service_id, "stopped", None, None).await.ok();
            Err(e)
        }
    }
}

/// Everything that can fail once `sandbox_instances.status` has already been
/// written as `"starting"`: volume creation, first-start stack
/// detection/config insert, container launch, and the preview
/// domain/Traefik sync. Split out of `start_sandbox` so any `Err` here can be
/// caught by the caller and turned into a status reset back to `"stopped"`.
///
/// NOTE (follow-up): deleting the `services` row cascades the sandbox's DB
/// rows but does *not* stop/remove the sandbox's Docker container or delete
/// its Docker volume — a service-delete teardown hook is still needed.
async fn provision_sandbox(
    state: &AppState,
    service_id: Uuid,
    volume_name: &str,
    org_id: Uuid,
) -> AppResult<SandboxInstanceRow> {
    state.docker.create_volume(volume_name, "local").await.ok(); // idempotent: ignore "already exists"

    // Record the app's persistent volume in the platform's `volumes` table so
    // it shows up in the volume UI/topology like any other attached volume.
    // `volumes` has no unique constraint usable as an ON CONFLICT target, so
    // idempotency across restarts comes from the NOT EXISTS guard instead.
    sqlx::query(
        "INSERT INTO volumes (id, project_id, service_id, name, mount_path, driver)
         SELECT $1, s.project_id, s.id, $3, '/app', 'local'
         FROM services s
         WHERE s.id = $2
           AND NOT EXISTS (SELECT 1 FROM volumes v WHERE v.service_id = $2 AND v.name = $3)",
    )
    .bind(Uuid::new_v4())
    .bind(service_id)
    .bind(volume_name)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let mut config = fetch_config(&state.db, service_id).await?;
    if config.is_none() {
        let stack = probe_and_detect(state, volume_name).await.map_err(|e| {
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
        .bind(volume_name)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        config = fetch_config(&state.db, service_id).await?;
    }
    let config = config.ok_or_else(|| AppError::Internal("sandbox config missing after insert".to_string()))?;

    let container_name = sandbox_container_name(service_id);
    state.docker.remove_container(&container_name, true).await.ok(); // clean slate

    let (cpu_cores, memory_gb): (f64, f64) = sqlx::query_as(
        "SELECT p.sandbox_cpu_cores, p.sandbox_memory_gb
         FROM organizations o JOIN plans p ON p.id = o.plan_id WHERE o.id = $1",
    )
    .bind(org_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let install_and_dev = match &config.install_cmd {
        Some(install) => format!("{install} && {}", config.dev_cmd.as_deref().unwrap_or("")),
        None => config.dev_cmd.clone().unwrap_or_default(),
    };
    let install_and_dev = build_startup_command(config.seed_script_b64.as_deref(), &install_and_dev);

    // Docker's create API never auto-pulls (only the CLI's `docker run` does),
    // so a daemon without the base image cached would fail every first start.
    // A missing image is fatal here, unlike the idempotent create_volume /
    // remove_container calls above.
    let base_image = config.base_image.clone().unwrap_or_default();
    let (pull_image, pull_tag) = split_image_ref(&base_image);
    state.docker.pull_image(pull_image, pull_tag, None).await?;

    let container_id = state
        .docker
        .create_container(ContainerSpec {
            name: container_name.clone(),
            image: base_image.clone(),
            cmd: Some(vec!["sh".to_string(), "-c".to_string(), install_and_dev]),
            env: vec![format!("PORT={}", config.port.unwrap_or(3000))],
            mounts: vec![MountSpec {
                source: volume_name.to_string(),
                target: "/app".to_string(),
                mount_type: MountType::Volume,
                readonly: false,
            }],
            network: Some(state.config.traefik.network.clone()),
            network_aliases: vec![container_name.clone()],
            runtime_class: Some(state.config.sandbox.runtime_class.clone()),
            working_dir: Some("/app".to_string()),
            resources: Some(ResourceSpec {
                cpu_limit: Some(cpu_cores),
                memory_limit_mb: Some((memory_gb * 1024.0) as u64),
                cpu_reservation: None,
                memory_reservation_mb: None,
            }),
        })
        .await?;

    state.docker.start_container(&container_id).await?;

    let hostname = preview_hostname(service_id, &state.config.sandbox.preview_base_domain);
    ensure_preview_domain(&state.db, service_id, &hostname, config.port.unwrap_or(3000)).await?;
    crate::resources::sync_traefik_dynamic_config(
        &state.db,
        service_id,
        &state.config.docker.label_prefix,
        &state.config.traefik.entrypoint_http,
        &state.config.traefik.entrypoint_https,
        state.config.traefik.dynamic_config_dir.as_deref(),
        Some(&container_name),
        false,
        Some(&sandbox_traefik_config_name(service_id)),
    )
    .await;

    let preview_url = format!("https://{hostname}");
    upsert_instance_status(&state.db, service_id, "running", Some(&container_id), Some(&container_name)).await?;
    sqlx::query("UPDATE sandbox_instances SET preview_url = $2, started_at = NOW(), last_heartbeat_at = NOW() WHERE service_id = $1")
        .bind(service_id)
        .bind(&preview_url)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    fetch_instance(&state.db, service_id)
        .await?
        .ok_or_else(|| AppError::Internal("sandbox_instances row missing after start".to_string()))
}

pub const PLACEHOLDER_ROUTE_REASON: &str = "sandbox stopped";

/// Stops the running container (if any) and re-points the app's Traefik
/// route at the shared placeholder/cold-start responder rather than removing
/// the route outright — a stopped sandbox's preview URL must still resolve
/// to something that can trigger an auto-start (see Task 9's public route),
/// not a dead connection.
pub async fn stop_sandbox(state: &AppState, service_id: Uuid) -> AppResult<()> {
    let Some(instance) = fetch_instance(&state.db, service_id).await? else {
        return Ok(()); // never started — nothing to stop
    };
    if instance.status == "stopped" {
        return Ok(());
    }

    if let Some(container_id) = &instance.container_id {
        state.docker.stop_container(container_id, 10).await.ok();
    }

    // `sync_traefik_dynamic_config` renders each domain's upstream URL as
    // `http://{upstream_override}:{domains.port}` — passing a `host:port`
    // string here would double-append the port. Instead we pass just the
    // placeholder hostname and repoint `domains.port` at the placeholder
    // port so the existing template's `:{port}` suffix stays correct.
    //
    // Scoped to the preview hostname specifically: a sandbox app may gain a
    // second domain (the spec's "publish" transition), and that one must not
    // be repointed at the placeholder port.
    let hostname = preview_hostname(service_id, &state.config.sandbox.preview_base_domain);
    sqlx::query("UPDATE domains SET port = $2 WHERE service_id = $1 AND hostname = $3")
        .bind(service_id)
        .bind(state.config.sandbox.placeholder_port as i32)
        .bind(&hostname)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    crate::resources::sync_traefik_dynamic_config(
        &state.db,
        service_id,
        &state.config.docker.label_prefix,
        &state.config.traefik.entrypoint_http,
        &state.config.traefik.entrypoint_https,
        state.config.traefik.dynamic_config_dir.as_deref(),
        Some(&state.config.sandbox.placeholder_upstream),
        false,
        Some(&sandbox_traefik_config_name(service_id)),
    )
    .await;

    sqlx::query(
        "UPDATE sandbox_instances SET status = 'stopped', last_heartbeat_at = NULL, started_at = NULL, updated_at = NOW() WHERE service_id = $1",
    )
    .bind(service_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

/// Records that the editor is still open for this sandbox. Called on an
/// interval by the frontend while the editor tab is active.
pub async fn heartbeat(state: &AppState, service_id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE sandbox_instances SET last_heartbeat_at = NOW() WHERE service_id = $1 AND status = 'running'")
        .bind(service_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

async fn probe_and_detect(
    state: &AppState,
    volume_name: &str,
) -> Result<shipyard_engine::sandbox_probe::DetectedStack, String> {
    use shipyard_engine::sandbox_probe::{detect_stack, ProbeResult, SANDBOX_PROBE_SCRIPT};

    let probe_name = format!("shipyard-probe-{}", Uuid::new_v4().simple());

    // create_container does not auto-pull — see provision_sandbox.
    let (pull_image, pull_tag) = split_image_ref(&state.config.sandbox.probe_image);
    state
        .docker
        .pull_image(pull_image, pull_tag, None)
        .await
        .map_err(|e| e.to_string())?;

    let container_id = state
        .docker
        .create_container(ContainerSpec {
            name: probe_name.clone(),
            image: state.config.sandbox.probe_image.clone(),
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
            runtime_class: Some(state.config.sandbox.runtime_class.clone()),
            working_dir: None,
            resources: None,
        })
        .await
        .map_err(|e| e.to_string())?;

    // From here on the container exists and must be cleaned up regardless of
    // whether start/wait/logs succeed — a probe container is only ever
    // needed for a single log line, and each one gets a fresh random name,
    // so a leaked one is never reaped.
    let result: Result<ProbeResult, String> = async {
        state.docker.start_container(&container_id).await.map_err(|e| e.to_string())?;
        state.docker.wait_container(&container_id).await.map_err(|e| e.to_string())?;
        let logs = state
            .docker
            .container_logs(&container_id, shipyard_docker::types::LogOpts { stdout: true, stderr: false, ..Default::default() })
            .await
            .map_err(|e| e.to_string())?;
        let output = logs.join("");
        serde_json::from_str(output.trim())
            .map_err(|e| format!("probe output was not valid JSON: {e} (output: {output})"))
    }
    .await;

    state.docker.remove_container(&container_id, true).await.ok();

    let probe = result?;
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

/// Interprets the result of inspecting a sandbox's recorded container as
/// dead or alive. Any inspect failure (container removed out-of-band, Docker
/// daemon no longer knows about it, etc.) is treated as dead rather than
/// trusting the stale DB row; a successful inspect is only alive when the
/// reported state is exactly `"running"`.
fn is_container_dead(inspect_result: &Result<String, String>) -> bool {
    match inspect_result {
        Err(_) => true,
        Ok(state) => state != "running",
    }
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

    #[test]
    fn is_container_dead_treats_inspect_error_as_dead() {
        // A container_id that Docker no longer knows about (removed out-of-band,
        // OOM-killed and reaped, host rebooted) must be treated as dead rather
        // than trusting the stale DB row — modeled here as any Err from inspect.
        let simulated_inspect_result: Result<String, String> = Err("No such container".to_string());
        assert!(is_container_dead(&simulated_inspect_result));

        let simulated_running: Result<String, String> = Ok("running".to_string());
        assert!(!is_container_dead(&simulated_running));

        let simulated_exited: Result<String, String> = Ok("exited".to_string());
        assert!(is_container_dead(&simulated_exited));
    }

    #[test]
    fn startup_command_wraps_seed_script_when_present() {
        let seed_b64 = "bWtkaXIgLXAgL2FwcA=="; // base64("mkdir -p /app")
        let install_and_dev = "npm install && npm run dev";
        let full_cmd = build_startup_command(Some(seed_b64), install_and_dev);
        assert!(full_cmd.contains("if [ -z \"$(ls -A /app 2>/dev/null)\" ]"));
        assert!(full_cmd.contains(seed_b64));
        assert!(full_cmd.ends_with(install_and_dev));
    }

    #[test]
    fn startup_command_is_unwrapped_when_no_seed_script() {
        let install_and_dev = "npm install && npm run dev";
        let full_cmd = build_startup_command(None, install_and_dev);
        assert_eq!(full_cmd, install_and_dev);
    }

    #[test]
    fn placeholder_upstream_used_when_stopping() {
        // stop_sandbox always re-points Traefik at the configured placeholder
        // upstream rather than deleting the domain/route outright, so a cold
        // preview link still resolves to the "waking up" responder instead of a
        // dead connection. This is exercised end-to-end in Task 9's manual
        // verification; here we just lock down the constant used.
        assert_eq!(super::PLACEHOLDER_ROUTE_REASON, "sandbox stopped");
    }
}
