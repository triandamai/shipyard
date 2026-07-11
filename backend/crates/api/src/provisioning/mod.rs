use std::collections::HashMap;
use std::sync::Arc;

use reqwest::Client;
use shipyard_mqtt::publisher::MqttPublisher;
use sqlx::PgPool;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use shipyard_docker::BollardDockerEngine;
use shipyard_docker::engine::DockerEngine;

use crate::compute::{ComputeProvider, CreateVmOptions, DigitalOceanProvider, HetznerProvider, VmStatus};

pub struct ProvisioningWorker {
    db: PgPool,
    http_client: Client,
    providers: HashMap<String, Box<dyn ComputeProvider>>,
    mqtt: Arc<MqttPublisher>,
    label_prefix: String,
}

impl ProvisioningWorker {
    pub fn new(
        db: PgPool,
        client: Client,
        hetzner_api_key: Option<String>,
        do_api_key: Option<String>,
        mqtt: Arc<MqttPublisher>,
        label_prefix: String,
    ) -> Self {
        let mut providers: HashMap<String, Box<dyn ComputeProvider>> = HashMap::new();

        if let Some(key) = hetzner_api_key.filter(|k| !k.is_empty()) {
            providers.insert("hetzner".to_string(), Box::new(HetznerProvider::new(client.clone(), key)));
        }
        if let Some(key) = do_api_key.filter(|k| !k.is_empty()) {
            providers.insert("digitalocean".to_string(), Box::new(DigitalOceanProvider::new(client.clone(), key)));
        }

        Self { db, http_client: client, providers, mqtt, label_prefix }
    }

    fn get_provider(&self, name: &str) -> Option<&dyn ComputeProvider> {
        self.providers.get(name).map(|p| p.as_ref())
    }

    pub async fn run(&self) {
        loop {
            if let Err(e) = self.tick().await {
                tracing::warn!("provisioning worker tick error: {e}");
            }
            sleep(Duration::from_secs(30)).await;
        }
    }

    /// Runs every 5 minutes. Finds paid orgs with no active or in-flight node
    /// (indicating a missed provisioning after a successful checkout) and
    /// queues a retry. Logs a warning after 30 minutes for ops visibility.
    pub async fn run_reconciliation(&self) {
        loop {
            sleep(Duration::from_secs(300)).await;
            if let Err(e) = self.reconcile_missing_nodes().await {
                tracing::warn!("billing reconciliation error: {e}");
            }
        }
    }

    async fn tick(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.timeout_stuck_nodes().await?;
        self.create_pending_vms().await?;
        self.advance_booting_nodes().await?;
        self.advance_to_active().await?;
        self.check_heartbeats().await?;
        self.drain_stopped_nodes().await?;
        self.delete_stopped_vms().await?;
        Ok(())
    }

    async fn timeout_stuck_nodes(&self) -> Result<(), sqlx::Error> {
        let affected = sqlx::query(
            r#"UPDATE compute_nodes
               SET status = 'failed'::node_status,
                   provision_error = 'Provisioning timed out after 15 minutes',
                   updated_at = NOW()
               WHERE status IN ('provisioning'::node_status, 'cloud_init_running'::node_status, 'wireguard_joined'::node_status)
                 AND created_at < NOW() - INTERVAL '15 minutes'"#,
        )
        .execute(&self.db)
        .await?;

        if affected.rows_affected() > 0 {
            tracing::warn!(count = affected.rows_affected(), "marked timed-out provisioning nodes as failed");
        }
        Ok(())
    }

    async fn create_pending_vms(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Nodes in 'provisioning' with no provider_vm_id need a VM created.
        #[derive(sqlx::FromRow)]
        struct PendingNode {
            id: Uuid,
            name: String,
            provider: String,
            region: String,
        }

        let nodes: Vec<PendingNode> = sqlx::query_as::<_, PendingNode>(
            r#"SELECT id, name, provider, region
               FROM compute_nodes
               WHERE status = 'provisioning'::node_status
                 AND provider_vm_id IS NULL
                 AND provision_attempts < 3
               LIMIT 5"#,
        )
        .fetch_all(&self.db)
        .await?;

        for node in nodes {
            // Increment attempt counter first (prevents thundering herd on errors).
            sqlx::query(
                "UPDATE compute_nodes SET provision_attempts = provision_attempts + 1, updated_at = NOW() WHERE id = $1",
            )
            .bind(node.id)
            .execute(&self.db)
            .await?;

            let provider = match self.get_provider(&node.provider) {
                Some(p) => p,
                None => {
                    tracing::warn!(node_id = %node.id, provider = %node.provider, "no provider configured for this node");
                    continue;
                }
            };

            let cloud_init = build_cloud_init();
            let opts = CreateVmOptions {
                name: &node.name,
                region: &node.region,
                server_type: "cpx21",
                cloud_init: &cloud_init,
            };

            match provider.create_vm(&opts).await {
                Ok(details) => {
                    sqlx::query(
                        r#"UPDATE compute_nodes
                           SET provider_vm_id = $2, public_ip = $3, status = 'cloud_init_running'::node_status, updated_at = NOW()
                           WHERE id = $1"#,
                    )
                    .bind(node.id)
                    .bind(&details.provider_vm_id)
                    .bind(&details.public_ip)
                    .execute(&self.db)
                    .await?;

                    tracing::info!(node_id = %node.id, vm_id = %details.provider_vm_id, "VM created successfully");
                }
                Err(e) => {
                    let err_str = e.to_string();
                    sqlx::query(
                        "UPDATE compute_nodes SET provision_error = $2, updated_at = NOW() WHERE id = $1",
                    )
                    .bind(node.id)
                    .bind(&err_str)
                    .execute(&self.db)
                    .await?;

                    tracing::warn!(node_id = %node.id, error = %err_str, "VM creation failed");
                }
            }
        }
        Ok(())
    }

    async fn advance_booting_nodes(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Nodes in cloud_init_running with a provider_vm_id — check if boot is complete.
        #[derive(sqlx::FromRow)]
        struct BootingNode {
            id: Uuid,
            provider: String,
            provider_vm_id: String,
        }

        let nodes: Vec<BootingNode> = sqlx::query_as::<_, BootingNode>(
            r#"SELECT id, provider, provider_vm_id
               FROM compute_nodes
               WHERE status = 'cloud_init_running'::node_status
                 AND provider_vm_id IS NOT NULL
               LIMIT 10"#,
        )
        .fetch_all(&self.db)
        .await?;

        for node in nodes {
            let provider = match self.get_provider(&node.provider) {
                Some(p) => p,
                None => {
                    tracing::warn!(node_id = %node.id, provider = %node.provider, "no provider configured for this node");
                    continue;
                }
            };

            match provider.get_vm_status(&node.provider_vm_id).await {
                Ok(VmStatus::Running { public_ip }) => {
                    // VM is up — advance to wireguard_joined (stub: real WireGuard check TBD).
                    sqlx::query(
                        r#"UPDATE compute_nodes
                           SET status = 'wireguard_joined'::node_status,
                               public_ip = COALESCE(NULLIF($2, ''), public_ip),
                               updated_at = NOW()
                           WHERE id = $1"#,
                    )
                    .bind(node.id)
                    .bind(&public_ip)
                    .execute(&self.db)
                    .await?;
                    tracing::info!(node_id = %node.id, "VM running — advanced to wireguard_joined");
                }
                Ok(_) => {
                    // Still booting — leave in cloud_init_running.
                    tracing::debug!(node_id = %node.id, "VM still booting");
                }
                Err(e) => {
                    tracing::warn!(node_id = %node.id, error = %e, "VM status check failed");
                }
            }
        }
        Ok(())
    }

    async fn advance_to_active(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        #[derive(sqlx::FromRow)]
        struct ReadyNode {
            id: Uuid,
            org_id: Uuid,
            name: String,
            public_ip: Option<String>,
        }

        let nodes: Vec<ReadyNode> = sqlx::query_as::<_, ReadyNode>(
            r#"SELECT id, org_id, name, public_ip
               FROM compute_nodes
               WHERE status = 'wireguard_joined'::node_status
                 AND public_ip IS NOT NULL
               LIMIT 10"#,
        )
        .fetch_all(&self.db)
        .await?;

        for node in nodes {
            let ip = match &node.public_ip {
                Some(ip) if !ip.is_empty() => ip.clone(),
                _ => continue,
            };

            // Ping Docker daemon — use public_ip for now (WireGuard overlay pending).
            let ping_url = format!("http://{}:2375/_ping", ip);
            let ok = self.http_client
                .get(&ping_url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);

            if !ok {
                tracing::debug!(node_id = %node.id, ip = %ip, "Docker not yet ready on wireguard_joined node");
                continue;
            }

            // Docker responded — mark active.
            sqlx::query(
                r#"UPDATE compute_nodes
                   SET status = 'active'::node_status,
                       last_heartbeat_at = NOW(),
                       updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(node.id)
            .execute(&self.db)
            .await?;

            tracing::info!(node_id = %node.id, "node is now active");

            // Batch-assign all existing unassigned org services to this node.
            let assigned = sqlx::query(
                r#"INSERT INTO service_node_assignments (service_id, node_id, assigned_at)
                   SELECT s.id, $1, NOW()
                   FROM services s
                   JOIN projects p ON p.id = s.project_id
                   WHERE p.org_id = $2
                     AND s.id NOT IN (SELECT service_id FROM service_node_assignments)
                   ON CONFLICT (service_id) DO NOTHING"#,
            )
            .bind(node.id)
            .bind(node.org_id)
            .execute(&self.db)
            .await?;

            tracing::info!(
                node_id = %node.id,
                services_assigned = assigned.rows_affected(),
                "auto-assigned existing org services to active node"
            );

            // Publish MQTT so the billing page can react immediately.
            let topic = format!("platform/orgs/{}/nodes", node.org_id);
            let payload = shipyard_common::types::MqttPayload::new("node.active")
                .with_meta(serde_json::json!({
                    "node_id": node.id,
                    "node_name": node.name,
                    "org_id": node.org_id,
                }));
            self.mqtt.publish_status(&topic, &payload).await.ok();
        }

        Ok(())
    }

    /// Scale all swarm services assigned to newly-stopped nodes to 0, then
    /// remove the assignments so the nodes can be deleted after the grace period.
    async fn drain_stopped_nodes(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        #[derive(sqlx::FromRow)]
        struct DrainableAssignment {
            node_id: Uuid,
            service_id: Uuid,
            public_ip: Option<String>,
        }

        let assignments: Vec<DrainableAssignment> = sqlx::query_as::<_, DrainableAssignment>(
            r#"SELECT cn.id AS node_id, sna.service_id, cn.public_ip
               FROM service_node_assignments sna
               JOIN compute_nodes cn ON cn.id = sna.node_id
               WHERE cn.status = 'stopped'::node_status
               LIMIT 50"#,
        )
        .fetch_all(&self.db)
        .await?;

        if assignments.is_empty() {
            return Ok(());
        }

        for asgn in &assignments {
            if let Some(ip) = &asgn.public_ip {
                if !ip.is_empty() {
                    let addr = format!("tcp://{}:2375", ip);
                    if let Ok(engine) = BollardDockerEngine::with_http(&addr) {
                        let svc_name = format!("{}-{}", self.label_prefix, asgn.service_id);
                        if let Err(e) = engine.scale_service(&svc_name, 0).await {
                            tracing::debug!(
                                service_id = %asgn.service_id,
                                node_id = %asgn.node_id,
                                "scale-to-0 on stopped node (best-effort): {e}"
                            );
                        }
                    }
                }
            }

            // Remove the assignment regardless — node is leaving.
            sqlx::query("DELETE FROM service_node_assignments WHERE service_id = $1 AND node_id = $2")
                .bind(asgn.service_id)
                .bind(asgn.node_id)
                .execute(&self.db)
                .await?;
        }

        let drained = assignments.len();
        if drained > 0 {
            tracing::info!(count = drained, "drained service assignments from stopped nodes");
        }

        Ok(())
    }

    async fn delete_stopped_vms(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        #[derive(sqlx::FromRow)]
        struct StoppedNode {
            id: Uuid,
            provider: String,
            provider_vm_id: String,
        }

        // Only delete VMs that have been stopped for >24h (grace period for data export).
        let nodes: Vec<StoppedNode> = sqlx::query_as::<_, StoppedNode>(
            r#"SELECT id, provider, provider_vm_id
               FROM compute_nodes
               WHERE status = 'stopped'::node_status
                 AND provider_vm_id IS NOT NULL
                 AND updated_at < NOW() - INTERVAL '24 hours'
               LIMIT 10"#,
        )
        .fetch_all(&self.db)
        .await?;

        for node in nodes {
            let provider = match self.get_provider(&node.provider) {
                Some(p) => p,
                None => {
                    tracing::warn!(node_id = %node.id, provider = %node.provider, "no provider configured for this node");
                    continue;
                }
            };

            match provider.delete_vm(&node.provider_vm_id).await {
                Ok(()) => {
                    sqlx::query(
                        r#"UPDATE compute_nodes
                           SET provider_vm_id = NULL,
                               public_ip = NULL,
                               ip_address = NULL,
                               tls_ca_cert = NULL,
                               tls_client_cert = NULL,
                               tls_client_key = NULL,
                               updated_at = NOW()
                           WHERE id = $1"#,
                    )
                    .bind(node.id)
                    .execute(&self.db)
                    .await?;
                    tracing::info!(node_id = %node.id, "VM deleted successfully");
                }
                Err(e) => {
                    tracing::warn!(node_id = %node.id, error = %e, "VM deletion failed — will retry next tick");
                }
            }
        }

        Ok(())
    }

    async fn check_heartbeats(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Check active nodes whose heartbeat is overdue (>2 min since last ping).
        #[derive(sqlx::FromRow)]
        struct ActiveNode {
            id: Uuid,
            public_ip: Option<String>,
        }

        let nodes: Vec<ActiveNode> = sqlx::query_as::<_, ActiveNode>(
            r#"SELECT id, public_ip
               FROM compute_nodes
               WHERE status IN ('active'::node_status, 'degraded'::node_status)
                 AND public_ip IS NOT NULL
                 AND (last_heartbeat_at IS NULL OR last_heartbeat_at < NOW() - INTERVAL '2 minutes')
               LIMIT 20"#,
        )
        .fetch_all(&self.db)
        .await?;

        for node in nodes {
            let ip = match &node.public_ip {
                Some(ip) if !ip.is_empty() => ip.clone(),
                _ => continue,
            };

            let ping_url = format!("http://{}:2375/_ping", ip);
            let reachable = self.http_client
                .get(&ping_url)
                .timeout(std::time::Duration::from_secs(5))
                .send()
                .await
                .map(|r| r.status().is_success())
                .unwrap_or(false);

            if reachable {
                // Update heartbeat and ensure status is active.
                sqlx::query(
                    r#"UPDATE compute_nodes
                       SET last_heartbeat_at = NOW(),
                           status = 'active'::node_status,
                           updated_at = NOW()
                       WHERE id = $1"#,
                )
                .bind(node.id)
                .execute(&self.db)
                .await?;
            } else {
                // Missed heartbeat — mark degraded.
                sqlx::query(
                    r#"UPDATE compute_nodes
                       SET status = 'degraded'::node_status,
                           updated_at = NOW()
                       WHERE id = $1 AND status = 'active'::node_status"#,
                )
                .bind(node.id)
                .execute(&self.db)
                .await?;
                tracing::warn!(node_id = %node.id, "node missed heartbeat — marked degraded");
            }
        }

        Ok(())
    }

    async fn reconcile_missing_nodes(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Find paid orgs that have no compute node in a live state.
        // "Live" = provisioning, cloud_init_running, wireguard_joined, active, degraded.
        #[derive(sqlx::FromRow)]
        struct MissingNodeOrg {
            org_id: Uuid,
            org_name: String,
            provider: String,
            region: String,
            upgraded_at: chrono::DateTime<chrono::Utc>,
        }

        let orgs: Vec<MissingNodeOrg> = sqlx::query_as::<_, MissingNodeOrg>(
            r#"SELECT ob.org_id,
                      o.name AS org_name,
                      'hetzner'::text AS provider,
                      'eu-central'::text AS region,
                      ob.updated_at AS upgraded_at
               FROM org_billing ob
               JOIN organizations o ON o.id = ob.org_id
               WHERE ob.tier != 'free'
                 AND ob.sub_status = 'active'
                 AND NOT EXISTS (
                     SELECT 1 FROM compute_nodes cn
                     WHERE cn.org_id = ob.org_id
                       AND cn.status NOT IN ('failed'::node_status, 'stopped'::node_status)
                 )
               LIMIT 20"#,
        )
        .fetch_all(&self.db)
        .await?;

        for org in orgs {
            let elapsed = chrono::Utc::now() - org.upgraded_at;

            if elapsed > chrono::Duration::minutes(30) {
                tracing::warn!(
                    org_id = %org.org_id,
                    org_name = %org.org_name,
                    elapsed_minutes = elapsed.num_minutes(),
                    "paid org has no live node for >30 min — manual review needed"
                );
                continue;
            }

            // Within the 30-minute window — retry provisioning.
            let node_name = format!("{}-node-1", org.org_name.to_lowercase().replace(' ', "-"));
            tracing::info!(org_id = %org.org_id, "reconciliation: queuing new provisioning node for paid org");

            sqlx::query(
                r#"INSERT INTO compute_nodes
                       (id, org_id, name, provider, region, status, cpu_cores, ram_mb, provision_attempts, created_at, updated_at)
                   VALUES (gen_random_uuid(), $1, $2, $3, $4, 'provisioning'::node_status, 2, 4096, 0, NOW(), NOW())"#,
            )
            .bind(org.org_id)
            .bind(&node_name)
            .bind(&org.provider)
            .bind(&org.region)
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }
}

// ─── Cloud-init script for new tenant VMs ─────────────────────────────────────

fn build_cloud_init() -> String {
    r#"#!/bin/bash
set -e
apt-get update -y
apt-get install -y docker.io wireguard curl

systemctl enable docker
systemctl start docker

# Open Docker daemon on port 2375 for WireGuard-internal access.
mkdir -p /etc/docker
cat > /etc/docker/daemon.json <<'DAEMON'
{
  "hosts": ["unix:///var/run/docker.sock", "tcp://0.0.0.0:2375"]
}
DAEMON

# Reload systemd drop-in to suppress -H fd:// default.
mkdir -p /etc/systemd/system/docker.service.d
cat > /etc/systemd/system/docker.service.d/override.conf <<'OVERRIDE'
[Service]
ExecStart=
ExecStart=/usr/bin/dockerd
OVERRIDE

systemctl daemon-reload
systemctl restart docker
"#.to_string()
}
