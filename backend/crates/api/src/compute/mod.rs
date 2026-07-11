use reqwest::Client;
use serde::Deserialize;

// ─── Provider abstraction ─────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ProviderError {
    Request(String),
    Api(String),
    NotFound,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProviderError::Request(e) => write!(f, "request error: {e}"),
            ProviderError::Api(e) => write!(f, "API error: {e}"),
            ProviderError::NotFound => write!(f, "resource not found"),
        }
    }
}

pub struct VmDetails {
    pub provider_vm_id: String,
    pub public_ip: String,
    pub region: String,
}

pub enum VmStatus {
    Running { public_ip: String },
    Initializing,
    Off,
    Unknown(String),
}

pub struct CreateVmOptions<'a> {
    pub name: &'a str,
    pub region: &'a str,
    pub server_type: &'a str,  // e.g. "cpx21"
    pub cloud_init: &'a str,
}

#[async_trait::async_trait]
pub trait ComputeProvider: Send + Sync {
    async fn create_vm(&self, opts: &CreateVmOptions<'_>) -> Result<VmDetails, ProviderError>;
    async fn get_vm_status(&self, vm_id: &str) -> Result<VmStatus, ProviderError>;
    async fn delete_vm(&self, vm_id: &str) -> Result<(), ProviderError>;
}

// ─── Hetzner Cloud provider ───────────────────────────────────────────────────

pub struct HetznerProvider {
    client: Client,
    api_key: String,
}

impl HetznerProvider {
    pub fn new(client: Client, api_key: String) -> Self {
        Self { client, api_key }
    }
}

#[derive(Deserialize)]
struct HetznerServerResponse {
    server: HetznerServer,
}

#[derive(Deserialize)]
struct HetznerServer {
    id: i64,
    status: String,
    public_net: HetznerPublicNet,
    datacenter: HetznerDatacenter,
}

#[derive(Deserialize)]
struct HetznerPublicNet {
    ipv4: Option<HetznerIpv4>,
}

#[derive(Deserialize)]
struct HetznerIpv4 {
    ip: String,
}

#[derive(Deserialize)]
struct HetznerDatacenter {
    location: HetznerLocation,
}

#[derive(Deserialize)]
struct HetznerLocation {
    name: String,
}

#[async_trait::async_trait]
impl ComputeProvider for HetznerProvider {
    async fn create_vm(&self, opts: &CreateVmOptions<'_>) -> Result<VmDetails, ProviderError> {
        let body = serde_json::json!({
            "name": opts.name,
            "server_type": opts.server_type,
            "location": opts.region,
            "image": "ubuntu-24.04",
            "user_data": opts.cloud_init,
            "start_after_create": true,
        });

        let res = self.client
            .post("https://api.hetzner.cloud/v1/servers")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!("create_vm: {text}")));
        }

        let data: HetznerServerResponse = res.json().await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        let ip = data.server.public_net.ipv4
            .map(|v| v.ip)
            .unwrap_or_default();

        Ok(VmDetails {
            provider_vm_id: data.server.id.to_string(),
            public_ip: ip,
            region: data.server.datacenter.location.name,
        })
    }

    async fn get_vm_status(&self, vm_id: &str) -> Result<VmStatus, ProviderError> {
        let url = format!("https://api.hetzner.cloud/v1/servers/{vm_id}");
        let res = self.client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if res.status() == 404 {
            return Err(ProviderError::NotFound);
        }
        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!("get_vm_status: {text}")));
        }

        let data: HetznerServerResponse = res.json().await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        let ip = data.server.public_net.ipv4
            .map(|v| v.ip)
            .unwrap_or_default();

        Ok(match data.server.status.as_str() {
            "running" => VmStatus::Running { public_ip: ip },
            "initializing" | "starting" => VmStatus::Initializing,
            "off" | "stopping" => VmStatus::Off,
            other => VmStatus::Unknown(other.to_string()),
        })
    }

    async fn delete_vm(&self, vm_id: &str) -> Result<(), ProviderError> {
        let url = format!("https://api.hetzner.cloud/v1/servers/{vm_id}");
        let res = self.client
            .delete(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if res.status() == 404 {
            return Ok(()); // already gone
        }
        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!("delete_vm: {text}")));
        }
        Ok(())
    }
}

// ─── DigitalOcean provider ────────────────────────────────────────────────────

pub struct DigitalOceanProvider {
    client: Client,
    api_key: String,
}

impl DigitalOceanProvider {
    pub fn new(client: Client, api_key: String) -> Self {
        Self { client, api_key }
    }
}

#[derive(Deserialize)]
struct DoDropletResponse {
    droplet: DoDroplet,
}

#[derive(Deserialize)]
struct DoDroplet {
    id: i64,
    status: String,
    networks: DoNetworks,
}

#[derive(Deserialize)]
struct DoNetworks {
    v4: Vec<DoV4Network>,
}

#[derive(Deserialize)]
struct DoV4Network {
    #[serde(rename = "type")]
    network_type: String,
    ip_address: String,
}

fn do_public_ip(networks: &DoNetworks) -> String {
    networks.v4.iter()
        .find(|n| n.network_type == "public")
        .map(|n| n.ip_address.clone())
        .unwrap_or_default()
}

#[async_trait::async_trait]
impl ComputeProvider for DigitalOceanProvider {
    async fn create_vm(&self, opts: &CreateVmOptions<'_>) -> Result<VmDetails, ProviderError> {
        let body = serde_json::json!({
            "name": opts.name,
            "region": opts.region,
            "size": opts.server_type,
            "image": "ubuntu-24-04-x64",
            "user_data": opts.cloud_init,
            "backups": false,
            "tags": ["shipyard-tenant"],
        });

        let res = self.client
            .post("https://api.digitalocean.com/v2/droplets")
            .bearer_auth(&self.api_key)
            .json(&body)
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!("create_vm: {text}")));
        }

        let data: DoDropletResponse = res.json().await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        let public_ip = do_public_ip(&data.droplet.networks);

        Ok(VmDetails {
            provider_vm_id: data.droplet.id.to_string(),
            public_ip,
            region: opts.region.to_string(),
        })
    }

    async fn get_vm_status(&self, vm_id: &str) -> Result<VmStatus, ProviderError> {
        let url = format!("https://api.digitalocean.com/v2/droplets/{vm_id}");
        let res = self.client
            .get(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if res.status() == 404 {
            return Err(ProviderError::NotFound);
        }
        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!("get_vm_status: {text}")));
        }

        let data: DoDropletResponse = res.json().await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        let public_ip = do_public_ip(&data.droplet.networks);

        Ok(match data.droplet.status.as_str() {
            "active" => VmStatus::Running { public_ip },
            "new" => VmStatus::Initializing,
            "off" => VmStatus::Off,
            other => VmStatus::Unknown(other.to_string()),
        })
    }

    async fn delete_vm(&self, vm_id: &str) -> Result<(), ProviderError> {
        let url = format!("https://api.digitalocean.com/v2/droplets/{vm_id}");
        let res = self.client
            .delete(&url)
            .bearer_auth(&self.api_key)
            .send()
            .await
            .map_err(|e| ProviderError::Request(e.to_string()))?;

        if res.status() == 404 {
            return Ok(()); // already gone
        }
        if !res.status().is_success() {
            let text = res.text().await.unwrap_or_default();
            return Err(ProviderError::Api(format!("delete_vm: {text}")));
        }
        Ok(())
    }
}
