use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub mqtt: MqttConfig,
    pub docker: DockerConfig,
    pub auth: AuthConfig,
    pub traefik: TraefikConfig,
    pub smtp: SmtpConfig,
    pub tls: TlsConfig,
    pub git: GitOAuthConfig,
    #[serde(default)]
    pub redis: RedisConfig,
    #[serde(default)]
    pub static_server: StaticServerConfig,
    pub data_dir: String,
    /// Public URL of the frontend app, used for invitation links and similar.
    /// Set via SHIPYARD__APP_URL. Defaults to http://localhost:5173.
    #[serde(default = "default_app_url")]
    pub app_url: String,
    /// Stripe webhook signing secret (whsec_...). Required to verify Stripe webhook signatures.
    /// Set via SHIPYARD__STRIPE_WEBHOOK_SECRET.
    #[serde(default)]
    pub stripe_webhook_secret: Option<String>,
    /// Stripe secret API key (sk_...). Required to create Checkout Sessions.
    /// Set via SHIPYARD__STRIPE_SECRET_KEY.
    #[serde(default)]
    pub stripe_secret_key: Option<String>,
    /// Hetzner Cloud API token. Set via SHIPYARD__HETZNER_API_KEY.
    #[serde(default)]
    pub hetzner_api_key: Option<String>,
    /// Hetzner server type for new tenant VMs. Default: cpx21 (2 vCPU, 4GB RAM).
    #[serde(default = "default_hetzner_server_type")]
    pub hetzner_server_type: String,
    /// Hetzner datacenter region for new tenant VMs. Default: eu-central (Nuremberg).
    #[serde(default = "default_hetzner_region")]
    pub hetzner_region: String,
    /// Default cloud provider for new tenant VMs. Default: hetzner.
    #[serde(default = "default_cloud_provider")]
    pub default_cloud_provider: String,
    /// DigitalOcean API token. Set via SHIPYARD__DO_API_KEY.
    #[serde(default)]
    pub do_api_key: Option<String>,
    /// DigitalOcean droplet size for new tenant VMs. Default: s-2vcpu-4gb.
    #[serde(default = "default_do_size")]
    pub do_size: String,
    /// DigitalOcean region for new tenant VMs. Default: fra1 (Frankfurt).
    #[serde(default = "default_do_region")]
    pub do_region: String,
    /// Stripe price ID for the Pro tier. Set via SHIPYARD__STRIPE_PRICE_PRO.
    #[serde(default)]
    pub stripe_price_pro: Option<String>,
    /// Stripe price ID for the Max tier. Set via SHIPYARD__STRIPE_PRICE_MAX.
    #[serde(default)]
    pub stripe_price_max: Option<String>,
}

fn default_app_url() -> String {
    "http://localhost:5173".to_string()
}

fn default_hetzner_server_type() -> String { "cpx21".to_string() }
fn default_hetzner_region() -> String { "eu-central".to_string() }
fn default_cloud_provider() -> String { "hetzner".to_string() }
fn default_do_size() -> String { "s-2vcpu-4gb".to_string() }
fn default_do_region() -> String { "fra1".to_string() }

#[derive(Debug, Clone, Deserialize)]
pub struct GitOAuthConfig {
    /// Base URL of this API server, used to build OAuth callback URLs.
    /// e.g. "http://localhost:3001"
    pub api_base_url: String,
    /// Frontend URL to redirect to after OAuth completes.
    /// e.g. "http://localhost:5173"
    pub frontend_url: String,
    pub github_client_id: String,
    pub github_client_secret: String,
    pub gitlab_client_id: String,
    pub gitlab_client_secret: String,
    pub bitbucket_client_id: String,
    pub bitbucket_client_secret: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    /// Allowed CORS origins, e.g. ["https://app.example.com"].
    /// Defaults to ["*"] which permits all origins — restrict this in production.
    #[serde(default = "default_cors_origins")]
    pub cors_allowed_origins: Vec<String>,
}

fn default_cors_origins() -> Vec<String> {
    vec!["*".to_string()]
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    /// Optional broker credentials. Set both for password-auth brokers.
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DockerConfig {
    pub socket_path: Option<String>,
    pub label_prefix: String,
    /// When true, port bindings are handled by a socat proxy container instead of
    /// Swarm's native EndpointSpec. Required on macOS Docker Desktop; leave false
    /// on Linux production servers where Swarm host-mode ports bind directly.
    #[serde(default)]
    pub port_proxy: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub access_token_expiry: u64,
    pub refresh_token_expiry: u64,
    pub secret_key: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TraefikConfig {
    pub network: String,
    pub entrypoint_http: String,
    pub entrypoint_https: String,
    pub cert_resolver: String,
    /// Optional directory where Traefik file-provider dynamic configs are written.
    /// When set, adding/removing domains writes/updates a YAML file Traefik hot-reloads.
    #[serde(default)]
    pub dynamic_config_dir: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SmtpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: String,
    pub from_address: String,
    pub from_name: String,
    /// TLS mode: "starttls" (port 587), "tls" (implicit TLS, port 465), "none" (no TLS)
    pub security: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TlsConfig {
    pub enabled: bool,
    pub cert_path: String,
    pub key_path: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct RedisConfig {
    /// Redis connection URL, e.g. "redis://localhost:6379".
    /// Leave unset to disable Redis caching (falls back to DB queries).
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StaticServerConfig {
    /// Swarm service / container name for the shared nginx server.
    #[serde(default = "default_static_service_name")]
    pub service_name: String,
    /// Absolute host path where per-site dirs are stored.
    /// Defaults to {data_dir}/static — resolved at runtime.
    pub sites_dir: Option<String>,
    /// Max upload size in MB for direct-upload deployments.
    #[serde(default = "default_max_upload_mb")]
    pub max_upload_mb: u64,
    /// Number of past deployment versions to retain per site for rollback.
    #[serde(default = "default_retention_versions")]
    pub retention_versions: usize,
}

fn default_static_service_name() -> String { "shipyard-static".to_string() }
fn default_max_upload_mb() -> u64 { 256 }
fn default_retention_versions() -> usize { 5 }

impl Default for StaticServerConfig {
    fn default() -> Self {
        Self {
            service_name: default_static_service_name(),
            sites_dir: None,
            max_upload_mb: default_max_upload_mb(),
            retention_versions: default_retention_versions(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            server: ServerConfig {
                host: "0.0.0.0".to_string(),
                port: 3001,
                cors_allowed_origins: default_cors_origins(),
            },
            database: DatabaseConfig {
                url: "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string(),
                max_connections: 5,
            },
            mqtt: MqttConfig {
                host: "localhost".to_string(),
                port: 1883,
                client_id: "shipyard-api".to_string(),
                username: None,
                password: None,
            },
            docker: DockerConfig {
                socket_path: None,
                label_prefix: "platform".to_string(),
                port_proxy: false,
            },
            auth: AuthConfig {
                jwt_secret: "change-me-in-production".to_string(),
                access_token_expiry: 3600,
                refresh_token_expiry: 604800,
                secret_key: "0".repeat(64),
            },
            traefik: TraefikConfig {
                network: "platform_proxy".to_string(),
                entrypoint_http: "web".to_string(),
                entrypoint_https: "websecure".to_string(),
                cert_resolver: "letsencrypt".to_string(),
                dynamic_config_dir: None,
            },
            smtp: SmtpConfig {
                enabled: false,
                host: "localhost".to_string(),
                port: 587,
                username: String::new(),
                password: String::new(),
                from_address: "noreply@example.com".to_string(),
                from_name: "Shipyard".to_string(),
                security: "starttls".to_string(),
            },
            tls: TlsConfig {
                enabled: false,
                cert_path: "/etc/shipyard/tls/cert.pem".to_string(),
                key_path: "/etc/shipyard/tls/key.pem".to_string(),
            },
            git: GitOAuthConfig {
                api_base_url: "http://localhost:3001".to_string(),
                frontend_url: "http://localhost:5173".to_string(),
                github_client_id: String::new(),
                github_client_secret: String::new(),
                gitlab_client_id: String::new(),
                gitlab_client_secret: String::new(),
                bitbucket_client_id: String::new(),
                bitbucket_client_secret: String::new(),
            },
            redis: RedisConfig { url: None },
            static_server: StaticServerConfig::default(),
            data_dir: "/opt/shipyard/data".to_string(),
            app_url: default_app_url(),
            stripe_webhook_secret: None,
            stripe_secret_key: None,
            hetzner_api_key: None,
            hetzner_server_type: default_hetzner_server_type(),
            hetzner_region: default_hetzner_region(),
            default_cloud_provider: default_cloud_provider(),
            do_api_key: None,
            do_size: default_do_size(),
            do_region: default_do_region(),
            stripe_price_pro: None,
            stripe_price_max: None,
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self, config::ConfigError> {
        let _ = dotenvy::dotenv();

        let cfg = config::Config::builder()
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 3001)?
            .set_default("database.url", "postgres://shipyard:shipyard@localhost:5432/shipyard")?
            .set_default("database.max_connections", 5)?
            .set_default("mqtt.host", "localhost")?
            .set_default("mqtt.port", 1883)?
            .set_default("mqtt.client_id", "shipyard-api")?
            .set_default("mqtt.username", Option::<String>::None)?
            .set_default("mqtt.password", Option::<String>::None)?
            .set_default("docker.label_prefix", "platform")?
            .set_default("auth.jwt_secret", "change-me-in-production")?
            .set_default("auth.access_token_expiry", 3600)?
            .set_default("auth.refresh_token_expiry", 604800)?
            .set_default("auth.secret_key", "0000000000000000000000000000000000000000000000000000000000000000")?
            .set_default("traefik.network", "platform_proxy")?
            .set_default("traefik.entrypoint_http", "web")?
            .set_default("traefik.entrypoint_https", "websecure")?
            .set_default("traefik.cert_resolver", "letsencrypt")?
            .set_default("smtp.enabled", false)?
            .set_default("smtp.host", "localhost")?
            .set_default("smtp.port", 587)?
            .set_default("smtp.username", "")?
            .set_default("smtp.password", "")?
            .set_default("smtp.from_address", "noreply@example.com")?
            .set_default("smtp.from_name", "Shipyard")?
            .set_default("smtp.security", "starttls")?
            .set_default("tls.enabled", false)?
            .set_default("tls.cert_path", "/etc/shipyard/tls/cert.pem")?
            .set_default("tls.key_path", "/etc/shipyard/tls/key.pem")?
            .set_default("git.api_base_url", "http://localhost:3001")?
            .set_default("git.frontend_url", "http://localhost:5173")?
            .set_default("git.github_client_id", "")?
            .set_default("git.github_client_secret", "")?
            .set_default("git.gitlab_client_id", "")?
            .set_default("git.gitlab_client_secret", "")?
            .set_default("git.bitbucket_client_id", "")?
            .set_default("git.bitbucket_client_secret", "")?
            .set_default("data_dir", "/opt/shipyard/data")?
            .set_default("app_url", "http://localhost:5173")?
            .set_default("hetzner_server_type", "cpx21")?
            .set_default("hetzner_region", "eu-central")?
            .set_default("default_cloud_provider", "hetzner")?
            .set_default("do_size", "s-2vcpu-4gb")?
            .set_default("do_region", "fra1")?
            .add_source(config::File::with_name("config").required(false))
            .add_source(config::Environment::with_prefix("SHIPYARD").separator("__"))
            .build()?;

        cfg.try_deserialize()
    }

    /// Override sensitive config values from Docker secrets mounted at /run/secrets/.
    pub fn apply_docker_secrets(&mut self) {
        let secrets = [
            ("shipyard_db_url",       "db_url"),
            ("shipyard_jwt_secret",   "jwt_secret"),
            ("shipyard_secret_key",   "secret_key"),
            ("shipyard_smtp_password","smtp_password"),
        ];

        for (filename, kind) in secrets {
            let path = format!("/run/secrets/{filename}");
            if let Ok(value) = std::fs::read_to_string(&path) {
                let value = value.trim().to_string();
                match kind {
                    "db_url"        => self.database.url = value,
                    "jwt_secret"    => self.auth.jwt_secret = value,
                    "secret_key"    => self.auth.secret_key = value,
                    "smtp_password" => self.smtp.password = value,
                    _               => {}
                }
                tracing::info!("Loaded secret from /run/secrets/{filename}");
            }
        }
    }
}
