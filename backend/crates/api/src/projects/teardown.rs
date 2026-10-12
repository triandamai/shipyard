//! Removing a project and everything it runs.

use std::time::Duration;

use uuid::Uuid;

use crate::error::ApiAppError;
use crate::AppState;

/// What to remove besides running workloads and DB rows.
#[derive(Debug, Clone, Copy, Default)]
pub struct TeardownOptions {
    /// Remove the project's Docker volumes (all data). Dashboard deletes keep
    /// them so an accidental delete is recoverable; integration terminates
    /// remove them because the integrator asked for everything to go.
    pub remove_volumes: bool,
}

#[derive(Debug, Default, serde::Serialize)]
pub struct TeardownReport {
    pub services: usize,
    pub domains: usize,
    pub volumes: Vec<String>,
}

/// Stop every service, remove routes, DNS records and on-disk files, delete
/// the project rows (cascading to services, domains, envs, deployments), and
/// optionally remove volumes in the background once their containers are gone.
pub async fn teardown_project(
    state: &AppState,
    project_id: Uuid,
    opts: TeardownOptions,
) -> Result<TeardownReport, ApiAppError> {
    let mut report = TeardownReport::default();

    // Root services only: compose stacks remove their own children.
    let services: Vec<(Uuid, String, String, String)> = sqlx::query_as(
        "SELECT id, slug, type::text, COALESCE(directory_path, '')
         FROM services WHERE project_id = $1 AND service_parent_id IS NULL",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(super::db_error)?;

    // Collect volumes before the rows cascade away. Names are scoped to this
    // project's services, so another tenant's volume can't be listed here.
    let volumes: Vec<String> = sqlx::query_scalar(
        "SELECT v.name FROM volumes v
         LEFT JOIN services s ON s.id = v.service_id
         WHERE (v.project_id = $1 OR s.project_id = $1) AND v.driver = 'local'",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(super::db_error)?;

    // Project networks (Docker ids), removed once their services are gone.
    let networks: Vec<String> = sqlx::query_scalar(
        "SELECT docker_network_id FROM networks WHERE project_id = $1 AND docker_network_id IS NOT NULL",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(super::db_error)?;

    // DNS records we created for this project's domains.
    let dns: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT d.service_id, d.cloudflare_zone_id, d.cloudflare_record_id
         FROM domains d JOIN services s ON s.id = d.service_id
         WHERE s.project_id = $1 AND d.cloudflare_zone_id IS NOT NULL AND d.cloudflare_record_id IS NOT NULL",
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(super::db_error)?;

    for (service_id, slug, svc_type, dir) in &services {
        if let Err(e) = crate::services::teardown_service_runtime(
            state,
            project_id,
            *service_id,
            svc_type,
            slug,
            dir,
        )
        .await
        {
            tracing::warn!(%project_id, %service_id, "service teardown: {}", e.0);
        }
        if let Some(dir) = state.config.traefik.dynamic_config_dir.as_deref() {
            let _ =
                tokio::fs::remove_file(std::path::Path::new(dir).join(format!("{slug}.yml"))).await;
        }
        // Source repos written by integration publish.
        let _ = tokio::fs::remove_dir_all(format!(
            "{}/sources/{service_id}.git",
            state.config.data_dir
        ))
        .await;
    }
    report.services = services.len();

    for (service_id, zone_id, record_id) in &dns {
        crate::resources::try_delete_cloudflare_record(&state.db, *service_id, zone_id, record_id)
            .await;
    }
    report.domains = dns.len();

    sqlx::query("DELETE FROM projects WHERE id = $1")
        .bind(project_id)
        .execute(&state.db)
        .await
        .map_err(super::db_error)?;

    let volumes = if opts.remove_volumes { volumes } else { vec![] };
    report.volumes = volumes.clone();
    if !volumes.is_empty() || !networks.is_empty() {
        let docker = std::sync::Arc::clone(&state.docker);
        tokio::spawn(remove_when_free(docker, volumes, networks));
    }

    Ok(report)
}

/// Swarm stops tasks asynchronously, so volumes and networks stay "in use"
/// for a while after their services are removed. Retry for about two minutes.
async fn remove_when_free(
    docker: std::sync::Arc<dyn shipyard_docker::engine::DockerEngine>,
    mut volumes: Vec<String>,
    mut networks: Vec<String>,
) {
    let gone = |e: &shipyard_common::error::AppError| {
        let m = e.to_string();
        m.contains("404") || m.contains("no such") || m.contains("not found")
    };
    for attempt in 0..12 {
        tokio::time::sleep(Duration::from_secs(if attempt == 0 { 3 } else { 10 })).await;
        let mut busy_volumes = Vec::new();
        for name in volumes {
            match docker.remove_volume(&name).await {
                Ok(()) => tracing::info!("removed volume {name}"),
                Err(e) if gone(&e) => {}
                Err(e) => {
                    tracing::debug!("volume {name} not removable yet: {e}");
                    busy_volumes.push(name);
                }
            }
        }
        let mut busy_networks = Vec::new();
        for id in networks {
            match docker.remove_network(&id).await {
                Ok(()) => tracing::info!("removed network {id}"),
                Err(e) if gone(&e) => {}
                Err(e) => {
                    tracing::debug!("network {id} not removable yet: {e}");
                    busy_networks.push(id);
                }
            }
        }
        if busy_volumes.is_empty() && busy_networks.is_empty() {
            return;
        }
        volumes = busy_volumes;
        networks = busy_networks;
    }
    tracing::warn!(
        "gave up removing resources still in use: volumes [{}] networks [{}]",
        volumes.join(", "),
        networks.join(", ")
    );
}
