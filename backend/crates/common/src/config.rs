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
    pub data_dir: String,
}

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
                max_connections: 10,
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
            data_dir: "/opt/shipyard/data".to_string(),
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
            .set_default("database.max_connections", 10)?
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
