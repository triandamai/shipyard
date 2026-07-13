use std::sync::Arc;
use std::time::Duration;

use std::collections::HashMap;
use shipyard_docker::types::{MountSpec, MountType, ResourceSpec, ServiceSpec};
use uuid::Uuid;

use crate::AppState;

/// Runs every 60 seconds.
/// - Creates runtime containers for orgs with active edge functions.
/// - Removes containers for orgs with no active functions.
/// - Health-checks existing containers.
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        if let Err(e) = tick(&state).await {
            tracing::error!("EdgeRuntimeWorker error: {e}");
        }
    }
}

async fn tick(state: &AppState) -> Result<(), Box<dyn std::error::Error>> {
    // Orgs that have at least one active edge function
    let active_orgs: Vec<Uuid> = sqlx::query_scalar(
        "SELECT DISTINCT org_id FROM edge_functions WHERE status = 'active'"
    )
    .fetch_all(&state.db)
    .await?;

    for org_id in active_orgs.iter().copied() {
        match ensure_runtime_exists(state, org_id).await {
            Ok(newly_created) if newly_created => {
                // Bootstrap reload in the container handles initial function loading.
                tracing::info!("EdgeRuntimeWorker: created runtime for org {org_id}");
            }
            Err(e) => tracing::warn!("EdgeRuntimeWorker: could not ensure runtime for org {org_id}: {e}"),
            _ => {}
        }
    }

    // Orgs whose container should be removed (no active functions)
    cleanup_stale_containers(state, &active_orgs).await;

    Ok(())
}

/// Ensures the Deno runtime Swarm service exists for the given org.
/// Creates it if missing; recreates it if unhealthy.
/// Returns `true` if a new container was created (caller may want to wait for
/// it to become healthy before sending `/reload`), `false` if it was already up.
pub async fn ensure_runtime_exists(state: &AppState, org_id: Uuid) -> Result<bool, String> {
    let short = &org_id.to_string()[..8];
    let service_name = format!("shipyard-edge-{short}");

    let tasks = state
        .docker
        .list_tasks(&service_name)
        .await
        .unwrap_or_default();

    if !tasks.is_empty() {
        // Service exists. Quick health check via the internal URL.
        let url = format!("http://{service_name}:8000/health");
        if state.http_client.get(&url).send().await.map_or(false, |r| r.status().is_success()) {
            return Ok(false);
        }
        // Unhealthy — fall through to recreate
        tracing::warn!("edge runtime for org {org_id} is unhealthy, recreating");
        let _ = state.docker.remove_service(&service_name).await;
        // Small delay to let Docker clean up
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    create_runtime_service(state, org_id, &service_name).await?;
    Ok(true)
}

/// Polls the runtime's /health endpoint until it responds OK or the deadline passes.
/// Returns once healthy; callers should then send /reload.
pub async fn wait_for_runtime_ready(state: &AppState, org_id: Uuid) {
    let short = &org_id.to_string()[..8];
    let url = format!("http://shipyard-edge-{short}:8000/health");
    let deadline = tokio::time::Instant::now() + Duration::from_secs(30);
    while tokio::time::Instant::now() < deadline {
        if state
            .http_client
            .get(&url)
            .send()
            .await
            .map_or(false, |r| r.status().is_success())
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    tracing::warn!("edge runtime for org {org_id} did not become healthy within 30s");
}

async fn create_runtime_service(
    state: &AppState,
    org_id: Uuid,
    service_name: &str,
) -> Result<(), String> {
    let tier = get_org_tier(state, org_id).await;
    let (memory_mb, cpu_limit) = resource_limits_for_tier(&tier);

    let runtime_secret = state
        .config
        .edge_functions
        .runtime_secret
        .clone()
        .unwrap_or_default();

    // Use configured runtime_api_url so worker nodes in a multi-node Swarm can
    // reach the backend via Traefik (e.g. https://api-<domain>).
    // Falls back to the container name which only works on the manager node.
    let api_url = state
        .config
        .edge_functions
        .runtime_api_url
        .as_deref()
        .unwrap_or("http://shipyard-backend:3001");

    let mut labels = HashMap::new();
    labels.insert("traefik.enable".to_string(), "true".to_string());

    // Mount the shared data volume so the runtime can read artifact files written
    // by the backend. Uses the same host path as the target so that artifact_path
    // values are valid inside the container without any path translation.
    // On a multi-node Swarm this directory must live on a shared/NFS-backed volume
    // accessible from all worker nodes at the same path.
    let edge_data_path = format!("{}/edge", state.config.data_dir);
    let mounts = vec![MountSpec {
        source: edge_data_path.clone(),
        target: edge_data_path,
        mount_type: MountType::Bind,
        readonly: true,
    }];

    let spec = ServiceSpec {
        name: service_name.to_string(),
        image: state.config.edge_functions.runtime_image.clone(),
        replicas: 1,
        env: vec![
            format!("SHIPYARD_RUNTIME_API_URL={api_url}"),
            format!("SHIPYARD_RUNTIME_ORG_ID={org_id}"),
            format!("SHIPYARD_RUNTIME_SECRET={runtime_secret}"),
            format!(
                "RELOAD_INTERVAL_MS={}",
                state.config.edge_functions.reload_interval_ms.unwrap_or(30_000)
            ),
        ],
        labels,
        mounts,
        networks: vec![state.config.traefik.network.clone()],
        ports: vec![],
        resources: Some(ResourceSpec {
            memory_limit_mb: Some(memory_mb),
            cpu_limit: Some(cpu_limit),
            cpu_reservation: None,
            memory_reservation_mb: None,
        }),
    };

    state
        .docker
        .create_service(spec)
        .await
        .map_err(|e| e.to_string())?;

    tracing::info!("created edge runtime container for org {org_id}");
    Ok(())
}

async fn cleanup_stale_containers(state: &AppState, active_org_ids: &[Uuid]) {
    // List all Swarm services with the shipyard-edge- prefix and remove those
    // whose org is not in the active set.
    let services = match state.docker.list_all_services().await {
        Ok(s) => s,
        Err(_) => return,
    };

    for svc in services {
        let name = &svc.name;
        if !name.starts_with("shipyard-edge-") { continue; }

        let short = name.trim_start_matches("shipyard-edge-");
        let is_active = active_org_ids.iter().any(|id| id.to_string().starts_with(short));
        if !is_active {
            let _ = state.docker.remove_service(name).await;
            tracing::info!("removed stale edge runtime container: {name}");
        }
    }
}

async fn get_org_tier(state: &AppState, org_id: Uuid) -> String {
    sqlx::query_scalar("SELECT tier::text FROM org_billing WHERE org_id = $1")
        .bind(org_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "free".to_string())
}

fn resource_limits_for_tier(tier: &str) -> (u64, f64) {
    // returns (memory_limit_mb, cpu_limit)
    match tier {
        "pro" => (512, 0.5),
        "max" => (1024, 1.0),
        _     => (256, 0.25),   // free tier
    }
}
