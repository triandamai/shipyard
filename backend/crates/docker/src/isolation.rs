//! Tenant network isolation and node pools.
//!
//! Without isolation every tenant workload joins the shared Traefik network,
//! so any container can reach any other org's services, sandboxes and edge
//! runtimes, and platform services on that network (MQTT, frontend). With
//! isolation enabled each org gets its own attachable overlay network,
//! `{label_prefix}-edge-{org}`, and only Traefik and the backend are
//! connected to all of them.
//!
//! The settings are installed once at startup with [`init`] so the many call
//! sites that construct a deployment engine don't each need to thread them
//! through.

use std::collections::HashMap;
use std::sync::OnceLock;

use shipyard_common::config::AppConfig;
use shipyard_common::error::{AppError, AppResult};
use uuid::Uuid;

use crate::engine::DockerEngine;

#[derive(Debug, Clone, Default)]
pub struct IsolationSettings {
    pub tenant_isolation: bool,
    pub label_prefix: String,
    /// The shared network platform containers use. Tenants never join it
    /// while isolation is on.
    pub shared_network: String,
    pub traefik_container: String,
    pub backend_container: String,
    pub app_node_pool: Option<String>,
}

impl IsolationSettings {
    pub fn from_config(cfg: &AppConfig) -> Self {
        Self {
            tenant_isolation: cfg.docker.tenant_isolation,
            label_prefix: cfg.docker.label_prefix.clone(),
            shared_network: cfg.traefik.network.clone(),
            traefik_container: cfg.docker.traefik_container.clone(),
            backend_container: cfg.docker.backend_container.clone(),
            app_node_pool: cfg
                .docker
                .app_node_pool
                .as_ref()
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty()),
        }
    }

    /// Name of an org's edge network.
    pub fn org_network(&self, org_id: Uuid) -> String {
        org_edge_network_name(&self.label_prefix, org_id)
    }

    /// Whether a tenant workload in `org_id` may join `network`. Always true
    /// with isolation off. With it on, tenants may not join the shared
    /// network or another org's edge network.
    pub fn tenant_may_join(&self, org_id: Uuid, network: &str) -> bool {
        if !self.tenant_isolation {
            return true;
        }
        if network == self.shared_network {
            return false;
        }
        let edge_prefix = format!("{}-edge-", self.label_prefix);
        if network.starts_with(&edge_prefix) {
            return network == self.org_network(org_id);
        }
        true
    }

    /// Placement constraint for tenant workloads, if a pool is configured.
    pub fn pool_constraint(&self) -> Option<String> {
        self.app_node_pool.as_deref().map(pool_constraint)
    }
}

static SETTINGS: OnceLock<IsolationSettings> = OnceLock::new();

/// Install settings at startup. Later calls are ignored.
pub fn init(settings: IsolationSettings) {
    if settings.tenant_isolation {
        tracing::info!(
            "tenant network isolation on: tenant workloads use per-org {}-edge-<org> networks",
            settings.label_prefix
        );
    }
    let _ = SETTINGS.set(settings);
}

/// Current settings. Before [`init`], isolation is off and there is no pool.
pub fn settings() -> &'static IsolationSettings {
    static OFF: OnceLock<IsolationSettings> = OnceLock::new();
    SETTINGS
        .get()
        .unwrap_or_else(|| OFF.get_or_init(IsolationSettings::default))
}

pub fn org_edge_network_name(label_prefix: &str, org_id: Uuid) -> String {
    format!("{label_prefix}-edge-{}", org_id.simple())
}

pub fn project_network_name(label_prefix: &str, project_id: Uuid) -> String {
    format!("{label_prefix}-proj-{}", project_id.simple())
}

pub fn pool_constraint(pool: &str) -> String {
    format!("node.labels.pool=={pool}")
}

/// The network a tenant workload in `org_id` should join for Traefik
/// routing: the org's edge network when isolation is on (created and wired
/// to Traefik and the backend if needed), otherwise the shared network.
///
/// `docker` must be the local Swarm engine (the one Traefik runs on).
/// Fails if the network can't be created or Traefik can't be connected,
/// because a workload Traefik can't reach would serve 502s.
pub async fn tenant_network(docker: &dyn DockerEngine, org_id: Uuid) -> AppResult<String> {
    let s = settings();
    if !s.tenant_isolation {
        return Ok(s.shared_network.clone());
    }
    ensure_org_edge_network(docker, s, org_id).await
}

pub async fn ensure_org_edge_network(
    docker: &dyn DockerEngine,
    s: &IsolationSettings,
    org_id: Uuid,
) -> AppResult<String> {
    let name = s.org_network(org_id);

    let labels = HashMap::from([
        (format!("{}.managed", s.label_prefix), "true".to_string()),
        (format!("{}.edge_network", s.label_prefix), "true".to_string()),
        (format!("{}.org_id", s.label_prefix), org_id.to_string()),
    ]);
    docker.ensure_overlay_network(&name, labels).await?;

    docker
        .connect_container_to_network(&name, &s.traefik_container)
        .await
        .map_err(|e| AppError::Docker(format!("Traefik can't join {name}: {e}")))?;

    // The backend dials sandboxes and edge runtimes call it back. Running
    // the backend outside Docker (local development) is fine: it then
    // can't join, and only those two features are affected.
    if let Err(e) = docker
        .connect_container_to_network(&name, &s.backend_container)
        .await
    {
        tracing::warn!("backend container can't join {name}: {e}");
    }

    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> IsolationSettings {
        IsolationSettings {
            tenant_isolation: true,
            label_prefix: "platform".into(),
            shared_network: "platform_proxy".into(),
            traefik_container: "shipyard-traefik".into(),
            backend_container: "shipyard-backend".into(),
            app_node_pool: None,
        }
    }

    #[test]
    fn org_network_name_is_stable_and_short() {
        let org = Uuid::parse_str("0190f2a4-1b2c-7d3e-8f40-123456789abc").unwrap();
        assert_eq!(
            org_edge_network_name("platform", org),
            "platform-edge-0190f2a41b2c7d3e8f40123456789abc"
        );
    }

    #[test]
    fn project_network_name_is_stable_and_short() {
        let proj = Uuid::parse_str("0190f2a4-1b2c-7d3e-8f40-123456789abc").unwrap();
        assert_eq!(
            project_network_name("platform", proj),
            "platform-proj-0190f2a41b2c7d3e8f40123456789abc"
        );
    }

    #[test]
    fn isolation_off_allows_everything() {
        let s = IsolationSettings { tenant_isolation: false, ..on() };
        let org = Uuid::now_v7();
        assert!(s.tenant_may_join(org, "platform_proxy"));
        assert!(s.tenant_may_join(org, &s.org_network(Uuid::now_v7())));
    }

    #[test]
    fn isolation_on_blocks_shared_and_other_orgs() {
        let s = on();
        let org = Uuid::now_v7();
        let other = Uuid::now_v7();
        assert!(!s.tenant_may_join(org, "platform_proxy"));
        assert!(!s.tenant_may_join(org, &s.org_network(other)));
        assert!(s.tenant_may_join(org, &s.org_network(org)));
        assert!(s.tenant_may_join(org, "myproject_default"));
    }

    #[test]
    fn pool_constraint_only_when_set() {
        assert_eq!(on().pool_constraint(), None);
        let s = IsolationSettings { app_node_pool: Some("apps".into()), ..on() };
        assert_eq!(s.pool_constraint().as_deref(), Some("node.labels.pool==apps"));
    }
}
