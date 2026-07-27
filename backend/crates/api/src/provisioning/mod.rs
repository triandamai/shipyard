use std::collections::HashMap;
use std::sync::Arc;

use rcgen::{Certificate, KeyPair};
use reqwest::Client;
use shipyard_common::config::NodeMtlsConfig;
use shipyard_common::error::AppError;
use shipyard_mqtt::publisher::MqttPublisher;
use sqlx::PgPool;
use tokio::time::{Duration, sleep};
use uuid::Uuid;

use shipyard_docker::BollardDockerEngine;
use shipyard_docker::engine::DockerEngine;

use crate::compute::{ComputeProvider, CreateVmOptions, DigitalOceanProvider, HetznerProvider, VmStatus};

mod tls;
use tls::{generate_client_identity, generate_docker_server_identity};

pub struct ProvisioningWorker {
    db: PgPool,
    providers: HashMap<String, Box<dyn ComputeProvider>>,
    mqtt: Arc<MqttPublisher>,
    label_prefix: String,
    default_provider: String,
    hetzner_server_type: String,
    hetzner_region: String,
    do_server_type: String,
    do_region: String,
    node_agent_image: String,
    ca_cert: Certificate,
    ca_key: KeyPair,
}

impl ProvisioningWorker {
    pub fn new(
        db: PgPool,
        client: Client,
        hetzner_api_key: Option<String>,
        do_api_key: Option<String>,
        mqtt: Arc<MqttPublisher>,
        label_prefix: String,
        default_provider: String,
        hetzner_server_type: String,
        hetzner_region: String,
        do_server_type: String,
        do_region: String,
        node_agent_image: String,
        node_mtls: &NodeMtlsConfig,
    ) -> Result<Self, AppError> {
        let mut providers: HashMap<String, Box<dyn ComputeProvider>> = HashMap::new();

        if let Some(key) = hetzner_api_key.filter(|k| !k.is_empty()) {
            providers.insert("hetzner".to_string(), Box::new(HetznerProvider::new(client.clone(), key)));
        }
        if let Some(key) = do_api_key.filter(|k| !k.is_empty()) {
            providers.insert("digitalocean".to_string(), Box::new(DigitalOceanProvider::new(client.clone(), key)));
        }

        let (ca_cert, ca_key) = tls::load_or_generate_ca(node_mtls)?;

        Ok(Self {
            db,
            providers,
            mqtt,
            label_prefix,
            default_provider,
            hetzner_server_type,
            hetzner_region,
            do_server_type,
            do_region,
            node_agent_image,
            ca_cert,
            ca_key,
        })
    }

    /// Build a reqwest client authenticated as the API's stored per-node client
    /// identity, trusting only the platform CA. Used for all outbound calls to
    /// a tenant node's node_agent (health check, cert bootstrap push).
    /// node_agent's server cert is DNS-named (not IP-SAN'd, since it's minted
    /// before the VM's IP is known), so hostname verification is intentionally
    /// disabled here — chain-of-trust to our exclusive platform CA is the real
    /// authentication, not hostname-matches-dialed-IP.
    fn node_agent_client(&self, client_cert_pem: &str, client_key_pem: &str) -> Result<Client, AppError> {
        let ca = reqwest::Certificate::from_pem(self.ca_cert.pem().as_bytes())
            .map_err(|e| AppError::Internal(format!("parse platform CA for reqwest: {e}")))?;
        let identity_pem = format!("{client_cert_pem}\n{client_key_pem}");
        let identity = reqwest::Identity::from_pem(identity_pem.as_bytes())
            .map_err(|e| AppError::Internal(format!("build node client identity: {e}")))?;

        Client::builder()
            .add_root_certificate(ca)
            .identity(identity)
            .danger_accept_invalid_hostnames(true)
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| AppError::Internal(format!("build node_agent mTLS client: {e}")))
    }

    /// Returns (server_type, region) defaults for the given provider name.
    fn provider_defaults(&self, provider: &str) -> (&str, &str) {
        match provider {
            "digitalocean" => (&self.do_server_type, &self.do_region),
            _ => (&self.hetzner_server_type, &self.hetzner_region),
        }
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
            server_type: String,
        }

        let nodes: Vec<PendingNode> = sqlx::query_as::<_, PendingNode>(
            r#"SELECT id, name, provider, region, server_type
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

            // Generate this node's mTLS material up front: the API's own client
            // identity (used against both node_agent and, later, dockerd) and
            // node_agent's own server identity (DNS-named — no IP dependency,
            // so it can be baked into cloud-init before the VM exists). The
            // dockerd server cert is IP-SAN'd and generated later, once the
            // VM's public IP is known (see `advance_booting_nodes`).
            let client_identity = match generate_client_identity(&self.ca_cert, &self.ca_key, node.id) {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(node_id = %node.id, error = %e, "failed to generate node client identity");
                    continue;
                }
            };
            let agent_identity = match tls::generate_agent_server_identity(&self.ca_cert, &self.ca_key, node.id) {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(node_id = %node.id, error = %e, "failed to generate node_agent server identity");
                    continue;
                }
            };
            let callback_token = generate_random_token();
            let ca_cert_pem = self.ca_cert.pem();
            tracing::debug!(node_id = %node.id, agent_server_name = %agent_identity.server_name, "generated node mTLS identities");

            sqlx::query(
                r#"UPDATE compute_nodes
                   SET tls_ca_cert = $2, tls_client_cert = $3, tls_client_key = $4,
                       agent_callback_token = $5, updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(node.id)
            .bind(&ca_cert_pem)
            .bind(&client_identity.cert_pem)
            .bind(&client_identity.key_pem)
            .bind(&callback_token)
            .execute(&self.db)
            .await?;

            let cloud_init = build_cloud_init(
                node.id,
                &ca_cert_pem,
                &agent_identity.cert_pem,
                &agent_identity.key_pem,
                &callback_token,
                &self.node_agent_image,
            );
            let opts = CreateVmOptions {
                name: &node.name,
                region: &node.region,
                server_type: &node.server_type,
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
            tls_client_cert: Option<String>,
            tls_client_key: Option<String>,
        }

        let nodes: Vec<BootingNode> = sqlx::query_as::<_, BootingNode>(
            r#"SELECT id, provider, provider_vm_id, tls_client_cert, tls_client_key
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

            let vm_status = match provider.get_vm_status(&node.provider_vm_id).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(node_id = %node.id, error = %e, "VM status check failed");
                    continue;
                }
            };

            let public_ip = match vm_status {
                VmStatus::Running { public_ip } if !public_ip.is_empty() => public_ip,
                _ => {
                    tracing::debug!(node_id = %node.id, "VM still booting");
                    continue;
                }
            };

            let (client_cert, client_key) = match (&node.tls_client_cert, &node.tls_client_key) {
                (Some(c), Some(k)) if !c.is_empty() && !k.is_empty() => (c, k),
                _ => {
                    tracing::error!(node_id = %node.id, "node reached cloud_init_running with no client identity — skipping (should be impossible)");
                    continue;
                }
            };

            let agent_client = match self.node_agent_client(client_cert, client_key) {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(node_id = %node.id, error = %e, "failed to build node_agent client");
                    continue;
                }
            };

            // node_agent up yet? (proves cloud-init actually completed, not
            // just that the VM powered on — much stronger than the old
            // "VM status == Running" stub.)
            let health_url = format!("https://{public_ip}:7070/health");
            let healthy = agent_client.get(&health_url).send().await
                .map(|r| r.status().is_success())
                .unwrap_or(false);
            if !healthy {
                tracing::debug!(node_id = %node.id, ip = %public_ip, "node_agent not yet reachable");
                continue;
            }

            // Generate dockerd's server cert now that the IP is known, and
            // push it to the VM over the already-mTLS-secured node_agent
            // channel — bollard's TLS client (used from here on) does strict
            // IP verification with no override, unlike node_agent's client.
            let docker_id = match generate_docker_server_identity(&self.ca_cert, &self.ca_key, node.id, &public_ip) {
                Ok(id) => id,
                Err(e) => {
                    tracing::error!(node_id = %node.id, error = %e, "failed to generate dockerd server identity");
                    continue;
                }
            };
            let install_url = format!("https://{public_ip}:7070/install-docker-tls");
            let push_result = agent_client.post(&install_url)
                .json(&serde_json::json!({
                    "ca_pem": self.ca_cert.pem(),
                    "cert_pem": docker_id.cert_pem,
                    "key_pem": docker_id.key_pem,
                }))
                .send()
                .await;
            let pushed = match push_result {
                Ok(r) if r.status().is_success() => true,
                Ok(r) => {
                    tracing::warn!(node_id = %node.id, status = %r.status(), "install-docker-tls rejected");
                    false
                }
                Err(e) => {
                    tracing::debug!(node_id = %node.id, error = %e, "install-docker-tls call failed, will retry");
                    false
                }
            };
            if !pushed {
                continue;
            }

            sqlx::query(
                r#"UPDATE compute_nodes
                   SET status = 'wireguard_joined'::node_status,
                       public_ip = $2,
                       updated_at = NOW()
                   WHERE id = $1"#,
            )
            .bind(node.id)
            .bind(&public_ip)
            .execute(&self.db)
            .await?;
            tracing::info!(node_id = %node.id, "node_agent live and dockerd TLS installed — advanced to wireguard_joined");
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
            tls_ca_cert: Option<String>,
            tls_client_cert: Option<String>,
            tls_client_key: Option<String>,
        }

        let nodes: Vec<ReadyNode> = sqlx::query_as::<_, ReadyNode>(
            r#"SELECT id, org_id, name, public_ip, tls_ca_cert, tls_client_cert, tls_client_key
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

            let ok = docker_tls_ping(&ip, &node.tls_ca_cert, &node.tls_client_cert, &node.tls_client_key).await;
            if !ok {
                tracing::debug!(node_id = %node.id, ip = %ip, "Docker TLS not yet ready on wireguard_joined node");
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
            tls_ca_cert: Option<String>,
            tls_client_cert: Option<String>,
            tls_client_key: Option<String>,
        }

        let assignments: Vec<DrainableAssignment> = sqlx::query_as::<_, DrainableAssignment>(
            r#"SELECT cn.id AS node_id, sna.service_id, cn.public_ip,
                      cn.tls_ca_cert, cn.tls_client_cert, cn.tls_client_key
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
            if let (Some(ip), Some(ca), Some(cert), Some(key)) =
                (&asgn.public_ip, &asgn.tls_ca_cert, &asgn.tls_client_cert, &asgn.tls_client_key)
            {
                if !ip.is_empty() && !ca.is_empty() {
                    let addr = format!("tcp://{}:2376", ip);
                    if let Ok(engine) = BollardDockerEngine::with_tls(&addr, ca, cert, key) {
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
                               agent_callback_token = NULL,
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
            tls_ca_cert: Option<String>,
            tls_client_cert: Option<String>,
            tls_client_key: Option<String>,
        }

        let nodes: Vec<ActiveNode> = sqlx::query_as::<_, ActiveNode>(
            r#"SELECT id, public_ip, tls_ca_cert, tls_client_cert, tls_client_key
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

            let reachable = docker_tls_ping(&ip, &node.tls_ca_cert, &node.tls_client_cert, &node.tls_client_key).await;

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
        // Prefer the provider/region/server_type from the org's most recent node
        // (even if failed/stopped) so retries land on the same provider. Fall back
        // to the configured defaults when the org has never had a node.
        #[derive(sqlx::FromRow)]
        struct MissingNodeOrg {
            org_id: Uuid,
            org_name: String,
            upgraded_at: chrono::DateTime<chrono::Utc>,
            last_provider: Option<String>,
            last_region: Option<String>,
            last_server_type: Option<String>,
        }

        let orgs: Vec<MissingNodeOrg> = sqlx::query_as::<_, MissingNodeOrg>(
            r#"SELECT ob.org_id,
                      o.name AS org_name,
                      ob.updated_at AS upgraded_at,
                      (SELECT cn.provider FROM compute_nodes cn
                       WHERE cn.org_id = ob.org_id
                       ORDER BY cn.created_at DESC LIMIT 1) AS last_provider,
                      (SELECT cn.region FROM compute_nodes cn
                       WHERE cn.org_id = ob.org_id
                       ORDER BY cn.created_at DESC LIMIT 1) AS last_region,
                      (SELECT cn.server_type FROM compute_nodes cn
                       WHERE cn.org_id = ob.org_id
                       ORDER BY cn.created_at DESC LIMIT 1) AS last_server_type
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

            // Use the org's previous provider/region/server_type if known; otherwise
            // fall back to the platform defaults from config.
            let provider = org.last_provider.as_deref().unwrap_or(&self.default_provider).to_string();
            let (default_type, default_region) = self.provider_defaults(&provider);
            let server_type = org.last_server_type.as_deref().unwrap_or(default_type).to_string();
            let region = org.last_region.as_deref().unwrap_or(default_region).to_string();

            let node_name = format!("{}-node-1", org.org_name.to_lowercase().replace(' ', "-"));
            tracing::info!(
                org_id = %org.org_id,
                provider = %provider,
                region = %region,
                server_type = %server_type,
                "reconciliation: queuing new provisioning node for paid org",
            );

            sqlx::query(
                r#"INSERT INTO compute_nodes
                       (id, org_id, name, provider, region, server_type, status, cpu_cores, ram_mb, provision_attempts, created_at, updated_at)
                   VALUES (gen_random_uuid(), $1, $2, $3, $4, $5, 'provisioning'::node_status, 2, 4096, 0, NOW(), NOW())"#,
            )
            .bind(org.org_id)
            .bind(&node_name)
            .bind(&provider)
            .bind(&region)
            .bind(&server_type)
            .execute(&self.db)
            .await?;
        }

        Ok(())
    }
}

/// mTLS `_ping` against a tenant node's dockerd (port 2376). Returns `false`
/// (never panics/errors out) on any missing TLS material or connection
/// failure — callers treat that identically to "not ready yet".
async fn docker_tls_ping(
    ip: &str,
    ca: &Option<String>,
    cert: &Option<String>,
    key: &Option<String>,
) -> bool {
    let (ca, cert, key) = match (ca, cert, key) {
        (Some(ca), Some(cert), Some(key)) if !ca.is_empty() && !cert.is_empty() && !key.is_empty() => {
            (ca, cert, key)
        }
        _ => return false,
    };
    let addr = format!("tcp://{ip}:2376");
    match BollardDockerEngine::with_tls(&addr, ca, cert, key) {
        Ok(engine) => engine.ping().await.is_ok(),
        Err(_) => false,
    }
}

// ─── Cloud-init script for new tenant VMs ─────────────────────────────────────

/// Random per-node secret: authenticates node_agent's outbound spike-alert
/// callback to the API. 256 bits from two v4 UUIDs (avoids adding a `rand`
/// dependency purely for this — uuid's v4 generator is already CSPRNG-backed).
fn generate_random_token() -> String {
    format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple())
}

/// Cloud-init for a fresh tenant VM. Docker is installed local-socket-only —
/// it is never exposed on any network port until `advance_booting_nodes`
/// pushes an IP-SAN'd server cert over the (already mTLS-secured) node_agent
/// channel and the docker-tls-reload path unit picks it up. node_agent itself
/// comes up in full mTLS mode from first boot (its own server cert is baked
/// in here, DNS-named so it doesn't depend on the not-yet-known public IP).
fn build_cloud_init(
    node_id: Uuid,
    ca_cert_pem: &str,
    agent_cert_pem: &str,
    agent_key_pem: &str,
    agent_callback_token: &str,
    node_agent_image: &str,
) -> String {
    format!(
        r#"#!/bin/bash
set -e
apt-get update -y
apt-get install -y docker.io curl

systemctl enable docker
systemctl start docker

mkdir -p /etc/shipyard/agent-tls /etc/shipyard/docker-tls
cat > /etc/shipyard/agent-tls/ca.pem <<'CA_PEM'
{ca_cert_pem}
CA_PEM
cat > /etc/shipyard/agent-tls/server-cert.pem <<'AGENT_CERT'
{agent_cert_pem}
AGENT_CERT
cat > /etc/shipyard/agent-tls/server-key.pem <<'AGENT_KEY'
{agent_key_pem}
AGENT_KEY
chmod 600 /etc/shipyard/agent-tls/server-key.pem

# dockerd's TLS settings can't be hot-reloaded via SIGHUP — node_agent writes
# new cert material + touches .reload under /etc/shipyard/docker-tls once the
# API pushes it (see advance_booting_nodes); this unit restarts dockerd to
# pick it up. Docker itself is never exposed on a TCP port until that happens.
cat > /etc/systemd/system/shipyard-docker-tls-reload.path <<'PATHUNIT'
[Path]
PathExists=/etc/shipyard/docker-tls/.reload
[Install]
WantedBy=multi-user.target
PATHUNIT
cat > /etc/systemd/system/shipyard-docker-tls-reload.service <<'SVCUNIT'
[Service]
Type=oneshot
ExecStart=/bin/bash -c 'rm -f /etc/shipyard/docker-tls/.reload; systemctl restart docker'
SVCUNIT
systemctl daemon-reload
systemctl enable --now shipyard-docker-tls-reload.path

docker pull {node_agent_image}
docker run -d --restart=always --name shipyard-node-agent \
  -v /var/run/docker.sock:/var/run/docker.sock \
  -v /etc/shipyard/agent-tls:/etc/shipyard/agent-tls:ro \
  -v /etc/shipyard/docker-tls:/etc/shipyard/docker-tls \
  -v /etc/docker:/etc/docker \
  --network host \
  -e AGENT_TLS_CERT_DIR=/etc/shipyard/agent-tls \
  -e AGENT_DOCKER_TLS_DIR=/etc/shipyard/docker-tls \
  -e AGENT_NODE_ID={node_id} \
  -e AGENT_CALLBACK_TOKEN={agent_callback_token} \
  {node_agent_image}
"#
    )
}
