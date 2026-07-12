use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    routing::{delete, get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::convert::Infallible;
use std::sync::OnceLock;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use shipyard_docker::{ContainerSummary, ImageSummary, NetworkSummary, NodeInfo, ServiceSummary, SwarmJoinTokens, VolumeSummary};
use crate::auth::AuthUser;
use crate::cache;
use crate::error::ApiAppError;
use crate::AppState;

/// Query param for admin routes that need per-org permission checks.
#[derive(Debug, Deserialize)]
struct OrgPermQuery {
    org_id: Option<uuid::Uuid>,
}

const SETTINGS_KEYS: &[&str] = &[
    "main_domain",
    "traefik_network",
    "traefik_entrypoint_http",
    "traefik_entrypoint_https",
    "traefik_cert_resolver",
    "git_github_token",
    "git_gitlab_token",
    "git_bitbucket_token",
    "git_webhook_secret",
    "max_parallel_deployments",
    "smtp_enabled",
    "smtp_host",
    "smtp_port",
    "smtp_username",
    "smtp_password",
    "smtp_from_address",
    "smtp_from_name",
    "smtp_security",
];

const TRAEFIK_CONTAINER: &str = "shipyard-traefik";
const TRAEFIK_STATIC_PATH: &str = "/etc/traefik/traefik.yml";
const TRAEFIK_DYNAMIC_DIR: &str = "/etc/traefik/dynamic";

const NGINX_STATIC_CONTAINER: &str = "shipyard-nginx-static";
const NGINX_CONF_DIR: &str = "/etc/nginx/conf.d";

#[derive(Debug, Serialize, Deserialize)]
pub struct PlatformSettings {
    pub main_domain: Option<String>,
    pub traefik_network: Option<String>,
    pub traefik_entrypoint_http: Option<String>,
    pub traefik_entrypoint_https: Option<String>,
    pub traefik_cert_resolver: Option<String>,
    pub git_github_token: Option<String>,
    pub git_gitlab_token: Option<String>,
    pub git_bitbucket_token: Option<String>,
    pub git_webhook_secret: Option<String>,
    pub max_parallel_deployments: Option<u32>,
    // SMTP
    pub smtp_enabled: Option<bool>,
    pub smtp_host: Option<String>,
    pub smtp_port: Option<u16>,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from_address: Option<String>,
    pub smtp_from_name: Option<String>,
    /// "starttls" | "tls" | "none"
    pub smtp_security: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TraefikFileResponse {
    pub path: String,
    pub content: Option<String>,
    pub exists: bool,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TraefikDirEntry {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct TraefikDynamicResponse {
    pub dir: String,
    pub files: Vec<TraefikDirEntry>,
    pub error: Option<String>,
}

/// Build info baked in at image build time via Docker build-args.
static BUILD_INFO: OnceLock<(String, String, String)> = OnceLock::new();

fn get_build_info() -> &'static (String, String, String) {
    BUILD_INFO.get_or_init(|| {
        let sha  = std::env::var("SHIPYARD_GIT_SHA").unwrap_or_else(|_| "dev".to_string());
        let date = std::env::var("SHIPYARD_BUILD_DATE").unwrap_or_else(|_| "unknown".to_string());
        let repo = std::env::var("SHIPYARD_DOCKER_REPO").unwrap_or_default();
        (sha, date, repo)
    })
}

/// In-memory cache for the Docker Hub update check (TTL: 1 hour).
static VERSION_CACHE: OnceLock<Mutex<Option<(Instant, VersionInfo)>>> = OnceLock::new();

fn version_cache() -> &'static Mutex<Option<(Instant, VersionInfo)>> {
    VERSION_CACHE.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    pub current: String,
    pub git_sha: String,
    pub build_date: String,
    pub update_available: bool,
    pub remote_sha: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/settings", get(get_settings).put(update_settings))
        .route("/settings/smtp", get(get_smtp_settings).put(update_smtp_settings))
        .route("/settings/deployments", get(get_deployments_settings).put(update_deployments_settings))
        .route("/settings/traefik/static", get(get_traefik_static))
        .route("/settings/traefik/dynamic", get(list_traefik_dynamic))
        .route("/settings/traefik/dynamic/:filename", get(get_traefik_dynamic_file))
        .route("/settings/traefik/logs/stream", get(traefik_log_stream))
        .route("/admin/version", get(get_version))
        .route("/admin/update", post(trigger_update))
        .route("/admin/update/stream", get(update_stream))
        .route("/admin/mqtt/clients", get(mqtt_clients))
        .route("/admin/mqtt/subscriptions", get(mqtt_subscriptions))
        .route("/admin/mqtt/topics", get(mqtt_topics))
        .route("/admin/system", get(system_info))
        .route("/admin/system/stream", get(system_info_stream))
        .route("/admin/docker/containers", get(docker_containers))
        .route("/admin/docker/containers/prune", post(docker_prune_containers))
        .route("/admin/docker/services", get(docker_services))
        .route("/admin/docker/volumes", get(docker_volumes))
        .route("/admin/docker/volumes/prune", post(docker_prune_volumes))
        .route("/admin/docker/containers/resource-stats", get(docker_container_resource_stats))
        .route("/admin/docker/networks", get(docker_networks))
        .route("/admin/docker/images", get(docker_images))
        .route("/admin/docker/images/prune", post(docker_prune_images))
        .route("/admin/docker/nodes", get(docker_nodes))
        .route("/admin/docker/swarm/join-tokens", get(docker_swarm_join_tokens))
        .route("/admin/nginx-static/confs", get(list_nginx_static_confs))
        .route("/admin/nginx-static/confs/:name", get(get_nginx_static_conf))
        .route("/admin/nginx-static/logs/stream", get(nginx_static_log_stream))
        .route("/admin/host-ip", get(get_host_ip))
        .route("/admin/api-keys", get(list_api_keys).post(create_api_key))
        .route("/admin/api-keys/:key_id", delete(revoke_api_key))
        .route("/admin/deployments", get(list_all_deployments))
        .route("/admin/smtp/test", post(test_smtp))
        .route("/admin/db/tables", get(list_db_tables))
        .route("/admin/db/tables/:table_name", delete(drop_db_table))
        .route("/admin/db/tables/:table_name/columns", get(list_table_columns))
        .route("/admin/db/tables/:table_name/rows", get(list_table_rows))
        .route("/admin/db/tables/:table_name/rows/:pk_value", axum::routing::patch(update_table_row).delete(delete_table_row))
}

// ── Helpers ─────────────────────────────────────────────────────────────────

async fn load_settings(state: &AppState) -> Result<PlatformSettings, ApiAppError> {
    let rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT key, value FROM system_config WHERE key = ANY($1)",
    )
    .bind(SETTINGS_KEYS)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let mut map: std::collections::HashMap<String, String> = rows
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect();

    Ok(PlatformSettings {
        main_domain:              map.remove("main_domain").or_else(|| std::env::var("DOMAIN").ok()),
        traefik_network:          map.remove("traefik_network").or_else(|| Some(state.config.traefik.network.clone())),
        traefik_entrypoint_http:  map.remove("traefik_entrypoint_http").or_else(|| Some(state.config.traefik.entrypoint_http.clone())),
        traefik_entrypoint_https: map.remove("traefik_entrypoint_https").or_else(|| Some(state.config.traefik.entrypoint_https.clone())),
        traefik_cert_resolver:    map.remove("traefik_cert_resolver").or_else(|| Some(state.config.traefik.cert_resolver.clone())),
        git_github_token:         map.remove("git_github_token"),
        git_gitlab_token:         map.remove("git_gitlab_token"),
        git_bitbucket_token:      map.remove("git_bitbucket_token"),
        git_webhook_secret:       map.remove("git_webhook_secret"),
        max_parallel_deployments: map.remove("max_parallel_deployments")
            .and_then(|v| v.parse::<u32>().ok()),
        smtp_enabled:      map.remove("smtp_enabled").map(|v| v == "true")
                              .or(Some(state.config.smtp.enabled)),
        smtp_host:         map.remove("smtp_host").or_else(|| Some(state.config.smtp.host.clone()).filter(|s| !s.is_empty())),
        smtp_port:         map.remove("smtp_port").and_then(|v| v.parse::<u16>().ok())
                              .or(Some(state.config.smtp.port)),
        smtp_username:     map.remove("smtp_username").or_else(|| Some(state.config.smtp.username.clone()).filter(|s| !s.is_empty())),
        smtp_password:     map.remove("smtp_password"),
        smtp_from_address: map.remove("smtp_from_address").or_else(|| Some(state.config.smtp.from_address.clone()).filter(|s| !s.is_empty())),
        smtp_from_name:    map.remove("smtp_from_name").or_else(|| Some(state.config.smtp.from_name.clone()).filter(|s| !s.is_empty())),
        smtp_security:     map.remove("smtp_security").or_else(|| Some(state.config.smtp.security.clone())),
    })
}

fn is_yaml(name: &str) -> bool {
    name.ends_with(".yml") || name.ends_with(".yaml")
}

/// Run a command inside a named container via `docker exec`.
async fn docker_exec_container(container: &str, args: &[&str]) -> Result<String, String> {
    let mut cmd_args = vec!["exec", container];
    cmd_args.extend_from_slice(args);
    let out = tokio::process::Command::new("docker")
        .args(&cmd_args)
        .output()
        .await
        .map_err(|e| format!("failed to spawn docker: {e}"))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

/// Run a command inside the Traefik container via `docker exec`.
/// Returns stdout on success, or an error string on failure.
async fn docker_exec(args: &[&str]) -> Result<String, String> {
    let mut cmd_args = vec!["exec", TRAEFIK_CONTAINER];
    cmd_args.extend_from_slice(args);

    let out = tokio::process::Command::new("docker")
        .args(&cmd_args)
        .output()
        .await
        .map_err(|e| format!("failed to spawn docker: {e}"))?;

    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let stderr = String::from_utf8_lossy(&out.stderr);
        Err(stderr.trim().to_string())
    }
}

// ── Handlers ─────────────────────────────────────────────────────────────────

async fn get_settings(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<PlatformSettings>>, ApiAppError> {
    const CACHE_KEY: &str = "settings";
    const CACHE_TTL: u64 = 60;

    if let Some(cached) = cache::get(&state.redis, CACHE_KEY).await {
        if let Ok(settings) = serde_json::from_str::<PlatformSettings>(&cached) {
            return Ok(Json(ApiResponse::ok(settings)));
        }
    }

    let settings = load_settings(&state).await?;

    if let Ok(json) = serde_json::to_string(&settings) {
        cache::set(&state.redis, CACHE_KEY, &json, CACHE_TTL).await;
    }

    Ok(Json(ApiResponse::ok(settings)))
}

async fn update_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<PlatformSettings>,
) -> Result<Json<ApiResponse<PlatformSettings>>, ApiAppError> {
    if !superadmin_bypass(&state.db, auth.user_id).await {
        let is_owner: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
            "SELECT TRUE FROM org_members WHERE user_id = $1 AND role = 'owner' LIMIT 1",
        )
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

        if is_owner.is_none() {
            return Err(ApiAppError(AppError::Forbidden(
                "Only platform owners or superadmins can update settings".to_string(),
            )));
        }
    }

    let pairs: Vec<(&str, Option<String>)> = vec![
        ("main_domain",              body.main_domain.clone()),
        ("traefik_network",          body.traefik_network.clone()),
        ("traefik_entrypoint_http",  body.traefik_entrypoint_http.clone()),
        ("traefik_entrypoint_https", body.traefik_entrypoint_https.clone()),
        ("traefik_cert_resolver",    body.traefik_cert_resolver.clone()),
        ("git_github_token",         body.git_github_token.clone()),
        ("git_gitlab_token",         body.git_gitlab_token.clone()),
        ("git_bitbucket_token",      body.git_bitbucket_token.clone()),
        ("git_webhook_secret",       body.git_webhook_secret.clone()),
        ("max_parallel_deployments", body.max_parallel_deployments.map(|v| v.to_string())),
        ("smtp_enabled",             body.smtp_enabled.map(|v| v.to_string())),
        ("smtp_host",                body.smtp_host.clone()),
        ("smtp_port",                body.smtp_port.map(|v| v.to_string())),
        ("smtp_username",            body.smtp_username.clone()),
        ("smtp_password",            body.smtp_password.clone()),
        ("smtp_from_address",        body.smtp_from_address.clone()),
        ("smtp_from_name",           body.smtp_from_name.clone()),
    ];

    for (key, val) in pairs {
        if let Some(v) = val {
            sqlx::query(
                "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            )
            .bind(key)
            .bind(Value::String(v))
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
        }
    }

    cache::del(&state.redis, "settings").await;

    let mqtt_payload = shipyard_common::types::MqttPayload::new("settings.traefik.updated");
    state.mqtt.publish_status("settings/traefik", &mqtt_payload).await.ok();

    get_settings(auth, State(state)).await
}

async fn require_keys_perm(db: &sqlx::PgPool, auth: &AuthUser, org_id: Option<uuid::Uuid>, write: bool) -> Result<(), ApiAppError> {
    let org_id = org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id query parameter is required".to_string())))?;
    let suffix = if write { "keys:write" } else { "keys:read" };
    let perm = format!("shipyard:{org_id}:{suffix}");
    crate::middleware::rbac::require_permission(db, auth.user_id, org_id, &perm)
        .await
        .map_err(ApiAppError)
}

async fn require_deployments_perm(db: &sqlx::PgPool, auth: &AuthUser, org_id: Option<uuid::Uuid>, write: bool) -> Result<(), ApiAppError> {
    let org_id = org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id query parameter is required".to_string())))?;
    let suffix = if write { "deployments:write" } else { "deployments:read" };
    let perm = format!("shipyard:{org_id}:{suffix}");
    crate::middleware::rbac::require_permission(db, auth.user_id, org_id, &perm)
        .await
        .map_err(ApiAppError)
}

async fn require_smtp_perm(db: &sqlx::PgPool, auth: &AuthUser, org_id: Option<uuid::Uuid>, write: bool) -> Result<(), ApiAppError> {
    if superadmin_bypass(db, auth.user_id).await { return Ok(()); }
    let org_id = org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id query parameter is required".to_string())))?;
    let suffix = if write { "smtp:write" } else { "smtp:read" };
    let perm = format!("shipyard:{org_id}:{suffix}");
    crate::middleware::rbac::require_permission(db, auth.user_id, org_id, &perm)
        .await
        .map_err(ApiAppError)
}

async fn get_smtp_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<PlatformSettings>>, ApiAppError> {
    require_smtp_perm(&state.db, &auth, q.org_id, false).await?;
    let settings = load_settings(&state).await?;
    Ok(Json(ApiResponse::ok(settings)))
}

async fn update_smtp_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
    Json(body): Json<PlatformSettings>,
) -> Result<Json<ApiResponse<PlatformSettings>>, ApiAppError> {
    require_smtp_perm(&state.db, &auth, q.org_id, true).await?;

    let pairs: Vec<(&str, Option<String>)> = vec![
        ("smtp_enabled",      body.smtp_enabled.map(|v| v.to_string())),
        ("smtp_host",         body.smtp_host.clone()),
        ("smtp_port",         body.smtp_port.map(|v| v.to_string())),
        ("smtp_username",     body.smtp_username.clone()),
        ("smtp_password",     body.smtp_password.clone()),
        ("smtp_from_address", body.smtp_from_address.clone()),
        ("smtp_from_name",    body.smtp_from_name.clone()),
        ("smtp_security",     body.smtp_security.clone()),
    ];

    for (key, val) in pairs {
        if let Some(v) = val {
            sqlx::query(
                "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, NOW())
                 ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
            )
            .bind(key)
            .bind(Value::String(v))
            .execute(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
        }
    }

    cache::del(&state.redis, "settings").await;

    let settings = load_settings(&state).await?;
    Ok(Json(ApiResponse::ok(settings)))
}

/// GET /settings/traefik/static
async fn get_traefik_static(
    _auth: AuthUser,
    _state: State<AppState>,
) -> Result<Json<ApiResponse<TraefikFileResponse>>, ApiAppError> {
    let resp = match docker_exec(&["cat", TRAEFIK_STATIC_PATH]).await {
        Ok(content) => TraefikFileResponse {
            path: TRAEFIK_STATIC_PATH.to_string(),
            content: Some(content),
            exists: true,
            error: None,
        },
        Err(e) => {
            let exists = !e.contains("No such file") && !e.contains("no such file");
            TraefikFileResponse {
                path: TRAEFIK_STATIC_PATH.to_string(),
                content: None,
                exists,
                error: Some(e),
            }
        }
    };
    Ok(Json(ApiResponse::ok(resp)))
}

/// GET /settings/traefik/dynamic — list .yml files from inside the container
async fn list_traefik_dynamic(
    _auth: AuthUser,
    _state: State<AppState>,
) -> Result<Json<ApiResponse<TraefikDynamicResponse>>, ApiAppError> {
    let mut resp = TraefikDynamicResponse {
        dir: TRAEFIK_DYNAMIC_DIR.to_string(),
        files: vec![],
        error: None,
    };

    match docker_exec(&["ls", "-1", TRAEFIK_DYNAMIC_DIR]).await {
        Err(e) => { resp.error = Some(e); }
        Ok(output) => {
            let mut files: Vec<TraefikDirEntry> = output
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && is_yaml(l))
                .map(|l| TraefikDirEntry { name: l.to_string() })
                .collect();
            files.sort_by(|a, b| a.name.cmp(&b.name));
            resp.files = files;
        }
    }

    Ok(Json(ApiResponse::ok(resp)))
}

/// GET /settings/traefik/dynamic/:filename — read one dynamic config file from the container
async fn get_traefik_dynamic_file(
    _auth: AuthUser,
    _state: State<AppState>,
    Path(filename): Path<String>,
) -> Result<Json<ApiResponse<TraefikFileResponse>>, ApiAppError> {
    if filename.contains('/') || filename.contains('\\') || filename.starts_with('.') {
        return Err(ApiAppError(AppError::BadRequest("Invalid filename".to_string())));
    }
    if !is_yaml(&filename) {
        return Err(ApiAppError(AppError::BadRequest(
            "Only .yml and .yaml files are accessible".to_string(),
        )));
    }

    let path = format!("{}/{}", TRAEFIK_DYNAMIC_DIR, filename);

    let resp = match docker_exec(&["cat", &path]).await {
        Ok(content) => TraefikFileResponse {
            path,
            content: Some(content),
            exists: true,
            error: None,
        },
        Err(e) => {
            let exists = !e.contains("No such file") && !e.contains("no such file");
            TraefikFileResponse { path, content: None, exists, error: Some(e) }
        }
    };
    Ok(Json(ApiResponse::ok(resp)))
}

/// GET /settings/traefik/logs/stream — real-time SSE stream of Traefik container logs
async fn traefik_log_stream(
    _auth: AuthUser,
    _state: State<AppState>,
) -> Sse<ReceiverStream<Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(128);

    tokio::spawn(async move {
        let mut child = match tokio::process::Command::new("docker")
            .args(["logs", "-f", "--tail=200", TRAEFIK_CONTAINER])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send(Ok(Event::default()
                        .event("error")
                        .data(format!("Failed to start docker logs: {e}"))))
                    .await;
                return;
            }
        };

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        // Stream both stdout and stderr into the same channel.
        // Traefik process logs go to stderr; access logs go to stdout.
        let tx_out = tx.clone();
        let tx_err = tx.clone();
        let tx_closed = tx.clone();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tx_closed.closed().await;
            let _ = cancel_tx.send(());
        });

        let mut h_out = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_out.send(Ok(Event::default().data(line))).await.is_err() {
                    break;
                }
            }
        });

        let mut h_err = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_err.send(Ok(Event::default().data(line))).await.is_err() {
                    break;
                }
            }
        });

        tokio::select! {
            _ = async { tokio::join!(&mut h_out, &mut h_err) } => {}
            _ = &mut cancel_rx => {
                h_out.abort();
                h_err.abort();
            }
        }
        let _ = child.kill().await;
    });

    Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default())
}

// ── Version check ────────────────────────────────────────────────────────────

/// GET /admin/version[?force=true]
///
/// Returns the running git SHA, build date, and whether a newer image exists
/// on Docker Hub. Result is cached for 1 hour; pass ?force=true to bypass.
async fn get_version(
    _auth: AuthUser,
    State(_state): State<AppState>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<ApiResponse<VersionInfo>>, ApiAppError> {
    let (git_sha, build_date, docker_repo) = get_build_info();
    let current = env!("CARGO_PKG_VERSION").to_string();
    let force = params.get("force").map(|v| v == "true" || v == "1").unwrap_or(false);

    let cache = version_cache();
    let mut guard = cache.lock().await;

    if !force {
        if let Some((fetched_at, info)) = guard.as_ref() {
            if fetched_at.elapsed() < std::time::Duration::from_secs(3600) {
                return Ok(Json(ApiResponse::ok(info.clone())));
            }
        }
    }

    let (update_available, remote_sha) = check_hub_update(docker_repo, git_sha).await;

    let info = VersionInfo {
        current,
        git_sha: git_sha.clone(),
        build_date: build_date.clone(),
        update_available,
        remote_sha,
    };

    *guard = Some((Instant::now(), info.clone()));
    Ok(Json(ApiResponse::ok(info)))
}

/// Query Docker Hub for the most recently pushed SHA tag and compare it with
/// the SHA baked into this running container.
///
/// Docker Hub returns tags ordered by `last_updated` descending. We look for
/// the first tag that looks like a 7-char git short SHA (all hex digits) and
/// compare it against `current_sha`. If they differ, an update is available.
async fn check_hub_update(repo: &str, current_sha: &str) -> (bool, Option<String>) {
    if repo.is_empty() || current_sha == "dev" {
        return (false, None);
    }

    let url = format!(
        "https://hub.docker.com/v2/repositories/{repo}/tags/?page_size=25&ordering=last_updated"
    );
    let client = match reqwest::Client::builder()
        .user_agent(format!("shipyard/{}", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(8))
        .build()
    {
        Ok(c) => c,
        Err(_) => return (false, None),
    };

    let body: serde_json::Value = match client.get(&url).send().await {
        Ok(r) if r.status().is_success() => match r.json().await {
            Ok(v) => v,
            Err(_) => return (false, None),
        },
        _ => return (false, None),
    };

    let results = match body["results"].as_array() {
        Some(a) => a,
        None => return (false, None),
    };

    // The 7-char short SHA tag is the fingerprint for each CI push.
    let current_short = &current_sha[..7.min(current_sha.len())];
    let latest_sha = results
        .iter()
        .filter_map(|t| t["name"].as_str())
        .find(|name| name.len() == 7 && name.chars().all(|c| c.is_ascii_hexdigit()));

    match latest_sha {
        Some(sha) => (sha != current_short, Some(sha.to_string())),
        None => (false, None),
    }
}

// ── Self-update ───────────────────────────────────────────────────────────────

const COMPOSE_DIR: &str = "/opt/shipyard";
const DOTENV_PATH: &str = "/opt/shipyard/.env";

/// Parse an `image:tag` string into `(image, tag)`.
fn parse_image_ref(val: &str) -> Option<(String, String)> {
    let val = val.trim();
    if val.is_empty() { return None; }
    // rfind so we split on the last colon (handles registry:port/image:tag)
    if let Some(pos) = val.rfind(':') {
        Some((val[..pos].to_string(), val[pos + 1..].to_string()))
    } else {
        Some((val.to_string(), "latest".to_string()))
    }
}

/// Resolve images to pull.
///
/// Tries two sources in order:
/// 1. Process env vars `BACKEND_IMAGE` / `FRONTEND_IMAGE` (set when the compose
///    service definition includes `env_file: .env`).
/// 2. Reads `/opt/shipyard/.env` directly — the file is always mounted and is the
///    authoritative source regardless of how env vars are forwarded.
fn read_dotenv_key(content: &str, key: &str) -> String {
    let prefix = format!("{key}=");
    content
        .lines()
        .find(|l| l.starts_with(prefix.as_str()))
        .and_then(|l| l.strip_prefix(prefix.as_str()))
        .unwrap_or("")
        .trim()
        .to_string()
}

fn resolve_image_key(content: &str, key: &str) -> Option<(String, String)> {
    // Env var takes priority; fall back to .env file.
    let val = std::env::var(key)
        .unwrap_or_default()
        .trim()
        .to_string();
    let val = if val.is_empty() { read_dotenv_key(content, key) } else { val };
    parse_image_ref(&val)
}

fn images_to_pull() -> Vec<(String, String)> {
    const KEYS: &[&str] = &["BACKEND_IMAGE", "FRONTEND_IMAGE", "EDGE_RUNTIME_IMAGE"];
    let content = std::fs::read_to_string(DOTENV_PATH).unwrap_or_default();
    KEYS.iter().filter_map(|k| resolve_image_key(&content, k)).collect()
}

fn edge_runtime_image() -> Option<String> {
    let content = std::fs::read_to_string(DOTENV_PATH).unwrap_or_default();
    resolve_image_key(&content, "EDGE_RUNTIME_IMAGE").map(|(img, tag)| format!("{img}:{tag}"))
}

/// Core update logic shared by the streaming and one-shot handlers.
/// Pulls all platform images via the Docker API (no docker CLI required),
/// then spawns a detached `docker:cli` container to run `docker compose up -d`.
async fn run_platform_update(
    state: &AppState,
    tx: &tokio::sync::mpsc::Sender<Result<Event, Infallible>>,
) -> bool {
    let images = images_to_pull();
    if images.is_empty() {
        let _ = tx.send(Ok(Event::default().event("error").data(
            "No images configured — BACKEND_IMAGE or FRONTEND_IMAGE env vars are not set."
        ))).await;
        return false;
    }

    for (image, tag) in &images {
        let msg = format!("[shipyard] Pulling {image}:{tag}…");
        if tx.send(Ok(Event::default().data(msg))).await.is_err() { return false; }

        let (line_tx, mut line_rx) = tokio::sync::mpsc::channel::<String>(256);
        let pull_fut = state.docker.pull_image_stream(image, tag, line_tx);

        let tx_fwd = tx.clone();
        let forward = tokio::spawn(async move {
            while let Some(line) = line_rx.recv().await {
                if tx_fwd.send(Ok(Event::default().data(line))).await.is_err() { break; }
            }
        });

        let pull_result = pull_fut.await;
        let _ = forward.await;

        match pull_result {
            Ok(_) => {
                let ok_msg = format!("[shipyard] ✓ {image}:{tag} up to date");
                if tx.send(Ok(Event::default().data(ok_msg))).await.is_err() { return false; }
            }
            Err(e) => {
                let _ = tx.send(Ok(Event::default().event("error").data(
                    format!("Failed to pull {image}:{tag}: {e}")
                ))).await;
                return false;
            }
        }
    }

    if tx.send(Ok(Event::default().data(
        "[shipyard] All images pulled. Spawning detached updater to restart services…"
    ))).await.is_err() {
        return false;
    }

    match state.docker.spawn_compose_restart(COMPOSE_DIR).await {
        Ok(_) => {}
        Err(e) => {
            let _ = tx.send(Ok(Event::default().event("error").data(
                format!("Failed to start updater container: {e}")
            ))).await;
            return false;
        }
    }

    // Roll-update any running shipyard-edge-* Swarm services to the new image.
    if let Some(edge_image) = edge_runtime_image() {
        match state.docker.list_services().await {
            Ok(services) => {
                // list_services() stores the service name in the .id field.
                let edge_svcs: Vec<String> = services
                    .into_iter()
                    .filter(|s| s.id.starts_with("shipyard-edge-"))
                    .map(|s| s.id)
                    .collect();

                for svc in &edge_svcs {
                    let msg = format!("[shipyard] Updating edge runtime {svc}…");
                    if tx.send(Ok(Event::default().data(msg))).await.is_err() {
                        return false;
                    }
                    if let Err(e) = state.docker.update_service_image(svc, &edge_image).await {
                        tracing::warn!("Failed to update edge service {svc}: {e}");
                    }
                }

                if !edge_svcs.is_empty() {
                    let _ = tx.send(Ok(Event::default().data(
                        format!("[shipyard] ✓ {} edge runtime(s) updated", edge_svcs.len())
                    ))).await;
                }
            }
            Err(e) => {
                tracing::warn!("Could not list services for edge runtime update: {e}");
            }
        }
    }

    let _ = tx.send(Ok(Event::default().event("done").data(
        "Update complete. Services will restart in ~5 seconds."
    ))).await;
    *version_cache().lock().await = None;
    true
}

/// POST /admin/update — one-shot update (kept for backwards compat).
async fn trigger_update(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_update_perm(&state, &auth).await?;
    tracing::info!(user_id = %auth.user_id, "Platform update triggered (one-shot)");

    let images = images_to_pull();
    if images.is_empty() {
        return Err(ApiAppError(AppError::Internal(
            "No images configured — BACKEND_IMAGE or FRONTEND_IMAGE env vars are not set.".to_string(),
        )));
    }

    let mut output: Vec<String> = Vec::new();

    for (image, tag) in &images {
        output.push(format!("[shipyard] Pulling {image}:{tag}…"));
        match state.docker.pull_image(image, tag, None).await {
            Ok(lines) => {
                output.extend(lines);
                output.push(format!("[shipyard] ✓ {image}:{tag} up to date"));
            }
            Err(e) => {
                return Err(ApiAppError(AppError::Internal(format!(
                    "Failed to pull {image}:{tag}: {e}"
                ))));
            }
        }
    }

    output.push("[shipyard] Spawning detached updater to restart services…".to_string());

    state
        .docker
        .spawn_compose_restart(COMPOSE_DIR)
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to start updater: {e}"))))?;

    *version_cache().lock().await = None;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Update complete. Services will restart in ~5 seconds.",
        "output": output.join("\n"),
    }))))
}

/// GET /admin/update/stream — SSE stream of update progress.
///
/// Pulls platform images via the Docker API and streams progress line-by-line.
/// Sends `event: done` or `event: error` when finished, then spawns a detached
/// container to run `docker compose up -d` so the restart happens after this
/// process exits.
async fn update_stream(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Sse<ReceiverStream<Result<Event, Infallible>>>, ApiAppError> {
    require_update_perm(&state, &auth).await?;

    tracing::info!(user_id = %auth.user_id, "Platform update stream started");

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(256);

    tokio::spawn(async move {
        run_platform_update(&state, &tx).await;
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}

async fn require_update_perm(state: &AppState, auth: &AuthUser) -> Result<(), ApiAppError> {
    let allowed: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM (
             SELECT 1 FROM org_members
             WHERE user_id = $1 AND role IN ('owner', 'admin')
             UNION ALL
             SELECT 1 FROM org_member_permissions
             WHERE user_id = $1 AND permission LIKE 'shipyard:%:system:update'
         ) t LIMIT 1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if allowed.is_none() {
        return Err(ApiAppError(AppError::Forbidden(
            "Only admins or members with the system:update permission can trigger updates".to_string()
        )));
    }
    Ok(())
}

// ── MQTT proxy ───────────────────────────────────────────────────────────────

fn mqtt_base_url() -> String {
    std::env::var("RMQTT_HTTP_API_URL")
        .unwrap_or_else(|_| "http://shipyard-mqtt:6060".to_string())
}

async fn mqtt_proxy(path: &str) -> Result<Json<ApiResponse<Value>>, ApiAppError> {
    let url = format!("{}/api/v1/{}", mqtt_base_url(), path);
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("MQTT API unreachable: {e}"))))?;

    let body: Value = resp
        .json()
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("MQTT API parse error: {e}"))))?;

    Ok(Json(ApiResponse::ok(body)))
}

async fn mqtt_clients(
    _auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, ApiAppError> {
    mqtt_proxy("clients").await
}

async fn mqtt_subscriptions(
    _auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, ApiAppError> {
    mqtt_proxy("subscriptions").await
}

async fn mqtt_topics(
    _auth: AuthUser,
) -> Result<Json<ApiResponse<Value>>, ApiAppError> {
    mqtt_proxy("topics").await
}

// ── System metrics ───────────────────────────────────────────────────────────

#[derive(Serialize)]
struct DiskInfo {
    mount: String,
    total_gb: f64,
    used_gb: f64,
    used_pct: f64,
}

#[derive(Serialize)]
struct NetInfo {
    iface: String,
    rx_bytes: u64,
    tx_bytes: u64,
}

#[derive(Serialize)]
struct SystemInfo {
    cpu_usage_pct: f64,
    memory_total_mb: u64,
    memory_used_mb: u64,
    memory_used_pct: f64,
    swap_total_mb: u64,
    swap_used_mb: u64,
    uptime_secs: u64,
    disks: Vec<DiskInfo>,
    networks: Vec<NetInfo>,
    container_stats: std::collections::HashMap<String, shipyard_docker::ContainerResourceStats>,
}

async fn system_info(
    _auth: AuthUser,
) -> Result<Json<ApiResponse<SystemInfo>>, ApiAppError> {
    use sysinfo::{Disks, Networks, System};

    let info = tokio::task::spawn_blocking(|| {
        let mut sys = System::new_all();
        // Two refreshes needed for an accurate CPU snapshot
        sys.refresh_all();
        std::thread::sleep(std::time::Duration::from_millis(200));
        sys.refresh_cpu_all();
        sys.refresh_memory();

        let cpu_usage_pct = {
            let cpus = sys.cpus();
            if cpus.is_empty() { 0.0 }
            else { cpus.iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / cpus.len() as f64 }
        };

        let mem_total = sys.total_memory();
        let mem_used  = sys.used_memory();
        let mem_pct   = if mem_total > 0 { mem_used as f64 / mem_total as f64 * 100.0 } else { 0.0 };

        let swap_total = sys.total_swap();
        let swap_used  = sys.used_swap();

        let disks: Vec<DiskInfo> = Disks::new_with_refreshed_list()
            .iter()
            .map(|d| {
                let total = d.total_space();
                let avail = d.available_space();
                let used  = total.saturating_sub(avail);
                DiskInfo {
                    mount:    d.mount_point().to_string_lossy().into_owned(),
                    total_gb: total as f64 / 1_073_741_824.0,
                    used_gb:  used  as f64 / 1_073_741_824.0,
                    used_pct: if total > 0 { used as f64 / total as f64 * 100.0 } else { 0.0 },
                }
            })
            .collect();

        let networks: Vec<NetInfo> = Networks::new_with_refreshed_list()
            .iter()
            .map(|(name, data)| NetInfo {
                iface:    name.clone(),
                rx_bytes: data.total_received(),
                tx_bytes: data.total_transmitted(),
            })
            .collect();

        SystemInfo {
            cpu_usage_pct,
            memory_total_mb: mem_total / 1_048_576,
            memory_used_mb:  mem_used  / 1_048_576,
            memory_used_pct: mem_pct,
            swap_total_mb:   swap_total / 1_048_576,
            swap_used_mb:    swap_used  / 1_048_576,
            uptime_secs:     System::uptime(),
            disks,
            networks,
            container_stats: std::collections::HashMap::new(),
        }
    })
    .await
    .map_err(|e| ApiAppError(AppError::Internal(format!("system info error: {e}"))))?;

    Ok(Json(ApiResponse::ok(info)))
}

/// GET /admin/system/stream — SSE stream of system metrics, emitted every 5 s.
///
/// The background task exits as soon as the client disconnects: `tx.send()` returns
/// `Err` when the `ReceiverStream` (held by the Axum response body) is dropped,
/// which Axum does when the TCP connection closes.
async fn system_info_stream(
    _auth: AuthUser,
    State(state): State<AppState>,
) -> Sse<ReceiverStream<Result<Event, Infallible>>> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(2);
    let docker = state.docker.clone();

    tokio::spawn(async move {
        use sysinfo::{Disks, Networks, System};
        let mut sys = System::new_all();

        loop {
            let (new_sys, snapshot) = match tokio::task::spawn_blocking(move || {
                let mut sys = sys;
                sys.refresh_all();
                std::thread::sleep(std::time::Duration::from_millis(200));
                sys.refresh_cpu_all();
                sys.refresh_memory();

                let cpu_usage_pct = {
                    let cpus = sys.cpus();
                    if cpus.is_empty() { 0.0 }
                    else { cpus.iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / cpus.len() as f64 }
                };
                let mem_total = sys.total_memory();
                let mem_used  = sys.used_memory();
                let mem_pct   = if mem_total > 0 { mem_used as f64 / mem_total as f64 * 100.0 } else { 0.0 };
                let swap_total = sys.total_swap();
                let swap_used  = sys.used_swap();

                let disks: Vec<DiskInfo> = Disks::new_with_refreshed_list()
                    .iter()
                    .map(|d| {
                        let total = d.total_space();
                        let avail = d.available_space();
                        let used  = total.saturating_sub(avail);
                        DiskInfo {
                            mount:    d.mount_point().to_string_lossy().into_owned(),
                            total_gb: total as f64 / 1_073_741_824.0,
                            used_gb:  used  as f64 / 1_073_741_824.0,
                            used_pct: if total > 0 { used as f64 / total as f64 * 100.0 } else { 0.0 },
                        }
                    })
                    .collect();

                let networks: Vec<NetInfo> = Networks::new_with_refreshed_list()
                    .iter()
                    .map(|(name, data)| NetInfo {
                        iface:    name.clone(),
                        rx_bytes: data.total_received(),
                        tx_bytes: data.total_transmitted(),
                    })
                    .collect();

                let partial = SystemInfo {
                    cpu_usage_pct,
                    memory_total_mb: mem_total / 1_048_576,
                    memory_used_mb:  mem_used  / 1_048_576,
                    memory_used_pct: mem_pct,
                    swap_total_mb:   swap_total / 1_048_576,
                    swap_used_mb:    swap_used  / 1_048_576,
                    uptime_secs:     System::uptime(),
                    disks,
                    networks,
                    container_stats: std::collections::HashMap::new(),
                };
                (sys, partial)
            })
            .await
            {
                Ok((ns, info)) => (ns, Ok(info)),
                Err(e) => {
                    tracing::warn!("system_info_stream: spawn_blocking panicked: {e}");
                    (System::new_all(), Err(e))
                }
            };

            sys = new_sys;

            match snapshot {
                Ok(mut info) => {
                    // Collect per-container resource stats for core services.
                    if let Ok(containers) = docker.list_all_containers().await {
                        let core_containers: Vec<String> = containers.into_iter()
                            .filter_map(|c| c.names.into_iter().next())
                            .filter(|n| {
                                let t = n.to_lowercase();
                                let t = t.trim_start_matches('/');
                                t.starts_with("shipyard-") || t.starts_with("shipyard_")
                            })
                            .map(|n| n.trim_start_matches('/').to_string())
                            .collect();

                        let futs: Vec<_> = core_containers.iter().map(|name| {
                            let docker = docker.clone();
                            let name = name.clone();
                            async move {
                                let stats = docker.container_resource_stats(&name).await;
                                (name, stats)
                            }
                        }).collect();

                        for (name, result) in futures::future::join_all(futs).await {
                            if let Ok(Some(s)) = result {
                                info.container_stats.insert(name, s);
                            }
                        }
                    }

                    let data = serde_json::to_string(&info).unwrap_or_default();
                    if tx.send(Ok(Event::default().data(data))).await.is_err() {
                        break;
                    }
                }
                Err(_) => {}
            }

            // Wait before the next sample. If the channel closes during the
            // sleep the next send will detect it and break.
            tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
        }

        tracing::debug!("system_info_stream: client disconnected, task exiting");
    });

    Sse::new(ReceiverStream::new(rx))
        .keep_alive(KeepAlive::new().interval(tokio::time::Duration::from_secs(15)))
}

// ── Docker engine routes ──────────────────────────────────────────────────────

async fn superadmin_bypass(db: &sqlx::PgPool, user_id: uuid::Uuid) -> bool {
    sqlx::query_scalar::<_, bool>("SELECT is_superadmin FROM users WHERE id = $1")
        .bind(user_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten()
        .unwrap_or(false)
}

async fn require_docker_read(db: &sqlx::PgPool, auth: &AuthUser, org_id: Option<uuid::Uuid>) -> Result<(), ApiAppError> {
    if superadmin_bypass(db, auth.user_id).await { return Ok(()); }
    let org_id = org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id query parameter is required".to_string())))?;
    let perm = format!("shipyard:{org_id}:docker:read");
    crate::middleware::rbac::require_permission(db, auth.user_id, org_id, &perm)
        .await
        .map_err(ApiAppError)
}

async fn require_docker_write(db: &sqlx::PgPool, auth: &AuthUser, org_id: Option<uuid::Uuid>) -> Result<(), ApiAppError> {
    if superadmin_bypass(db, auth.user_id).await { return Ok(()); }
    let org_id = org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id query parameter is required".to_string())))?;
    let perm = format!("shipyard:{org_id}:docker:write");
    crate::middleware::rbac::require_permission(db, auth.user_id, org_id, &perm)
        .await
        .map_err(ApiAppError)
}

async fn require_infra_read(db: &sqlx::PgPool, auth: &AuthUser, org_id: Option<uuid::Uuid>) -> Result<(), ApiAppError> {
    if superadmin_bypass(db, auth.user_id).await { return Ok(()); }
    let org_id = org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id query parameter is required".to_string())))?;
    let perm = format!("shipyard:{org_id}:infra:read");
    crate::middleware::rbac::require_permission(db, auth.user_id, org_id, &perm)
        .await
        .map_err(ApiAppError)
}

/// Accepts `static:read` (dedicated permission) or falls back to `infra:read`.
async fn require_static_read(db: &sqlx::PgPool, auth: &AuthUser, org_id: Option<uuid::Uuid>) -> Result<(), ApiAppError> {
    if superadmin_bypass(db, auth.user_id).await { return Ok(()); }
    let org_id = org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id query parameter is required".to_string())))?;
    let static_perm = format!("shipyard:{org_id}:static:read");
    if crate::middleware::rbac::require_permission(db, auth.user_id, org_id, &static_perm).await.is_ok() {
        return Ok(());
    }
    let infra_perm = format!("shipyard:{org_id}:infra:read");
    crate::middleware::rbac::require_permission(db, auth.user_id, org_id, &infra_perm)
        .await
        .map_err(ApiAppError)
}

async fn docker_containers(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<Vec<ContainerSummary>>>, ApiAppError> {
    require_docker_read(&state.db, &auth, q.org_id).await?;
    let data = state.docker.list_all_containers().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(data)))
}

async fn docker_prune_containers(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_docker_write(&state.db, &auth, q.org_id).await?;
    let removed = state.docker.prune_containers().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "removed": removed }))))
}

async fn docker_services(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<Vec<ServiceSummary>>>, ApiAppError> {
    require_docker_read(&state.db, &auth, q.org_id).await?;
    let data = state.docker.list_all_services().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(data)))
}

async fn docker_volumes(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<Vec<VolumeSummary>>>, ApiAppError> {
    require_docker_read(&state.db, &auth, q.org_id).await?;
    let data = state.docker.list_all_volumes().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(data)))
}

async fn docker_networks(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<Vec<NetworkSummary>>>, ApiAppError> {
    require_docker_read(&state.db, &auth, q.org_id).await?;
    let data = state.docker.list_all_networks().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(data)))
}

async fn docker_images(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<Vec<ImageSummary>>>, ApiAppError> {
    require_docker_read(&state.db, &auth, q.org_id).await?;
    let data = state.docker.list_all_images().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(data)))
}

async fn docker_prune_images(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_docker_write(&state.db, &auth, q.org_id).await?;
    let removed = state.docker.prune_images().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "removed": removed }))))
}

async fn docker_prune_volumes(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_docker_write(&state.db, &auth, q.org_id).await?;
    let removed = state.docker.prune_volumes().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "removed": removed }))))
}

#[derive(Debug, Deserialize)]
struct ResourceStatsQuery {
    org_id: Option<uuid::Uuid>,
    names: Option<String>, // comma-separated container names
}

async fn docker_container_resource_stats(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<ResourceStatsQuery>,
) -> Result<Json<ApiResponse<std::collections::HashMap<String, shipyard_docker::ContainerResourceStats>>>, ApiAppError> {
    require_infra_read(&state.db, &auth, q.org_id).await?;

    let names: Vec<String> = q.names
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();

    let mut result = std::collections::HashMap::new();

    let futs: Vec<_> = names.iter().map(|name| {
        let docker = state.docker.clone();
        let name = name.clone();
        async move {
            let stats = docker.container_resource_stats(&name).await;
            (name, stats)
        }
    }).collect();

    for (name, stats_result) in futures::future::join_all(futs).await {
        if let Ok(Some(s)) = stats_result {
            result.insert(name, s);
        }
    }

    Ok(Json(ApiResponse::ok(result)))
}

async fn docker_nodes(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<Vec<NodeInfo>>>, ApiAppError> {
    require_infra_read(&state.db, &auth, q.org_id).await?;
    let data = state.docker.list_nodes().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(data)))
}

async fn docker_swarm_join_tokens(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<SwarmJoinTokens>>, ApiAppError> {
    require_infra_read(&state.db, &auth, q.org_id).await?;
    let data = state.docker.get_join_tokens().await
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;
    Ok(Json(ApiResponse::ok(data)))
}

#[derive(Debug, Serialize)]
struct HostIpResponse {
    ip: String,
    is_public: bool,
}

/// Returns the server's detected public IP so the frontend can generate
/// appropriate wildcard domains (nip.io for VPS, traefik.me for localhost).
async fn get_host_ip(
    _auth: AuthUser,
) -> Json<ApiResponse<HostIpResponse>> {
    // 1. Prefer the DOMAIN env var if it looks like an IPv4 address
    if let Ok(domain) = std::env::var("DOMAIN") {
        let trimmed = domain.trim().to_string();
        if trimmed.parse::<std::net::IpAddr>().is_ok() {
            let is_public = !is_loopback_or_private(&trimmed);
            return Json(ApiResponse::ok(HostIpResponse { ip: trimmed, is_public }));
        }
    }

    // 2. Try to detect the public IP via external service (3 s timeout)
    let detected = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        fetch_public_ip(),
    )
    .await
    .ok()
    .flatten();

    if let Some(ip) = detected {
        let is_public = !is_loopback_or_private(&ip);
        return Json(ApiResponse::ok(HostIpResponse { ip, is_public }));
    }

    // 3. Fall back to localhost
    Json(ApiResponse::ok(HostIpResponse {
        ip: "127.0.0.1".to_string(),
        is_public: false,
    }))
}

async fn fetch_public_ip() -> Option<String> {
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3))
        .build()
        .ok()?;
    // api4.ipify.org returns only IPv4
    let text = client
        .get("https://api4.ipify.org")
        .send()
        .await
        .ok()?
        .text()
        .await
        .ok()?;
    let ip = text.trim().to_string();
    if ip.parse::<std::net::Ipv4Addr>().is_ok() { Some(ip) } else { None }
}

fn is_loopback_or_private(ip: &str) -> bool {
    match ip.parse::<std::net::IpAddr>() {
        Ok(addr) => addr.is_loopback() || is_private_ip(&addr),
        Err(_) => true,
    }
}

fn is_private_ip(addr: &std::net::IpAddr) -> bool {
    match addr {
        std::net::IpAddr::V4(v4) => {
            let o = v4.octets();
            o[0] == 10
                || (o[0] == 172 && o[1] >= 16 && o[1] <= 31)
                || (o[0] == 192 && o[1] == 168)
        }
        std::net::IpAddr::V6(_) => false,
    }
}

// ── All-deployments admin view ────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct DeploymentListQuery {
    pub org_id:   Option<uuid::Uuid>,
    pub status:   Option<String>,
    pub page:     Option<i64>,
    pub per_page: Option<i64>,
}

#[derive(Debug, Serialize)]
pub struct AdminDeploymentRow {
    pub id:           uuid::Uuid,
    pub service_id:   uuid::Uuid,
    pub service_name: String,
    pub project_id:   uuid::Uuid,
    pub project_name: String,
    pub triggered_by: String,
    pub source_ref:   String,
    pub status:       String,
    pub created_at:   DateTime<Utc>,
    pub finished_at:  Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct AdminDeploymentStats {
    pub total:   i64,
    pub running: i64,
    pub queued:  i64,
    pub failed:  i64,
    pub success: i64,
}

#[derive(Debug, Serialize)]
pub struct AdminDeploymentsResponse {
    pub data:     Vec<AdminDeploymentRow>,
    pub stats:    AdminDeploymentStats,
    pub page:     i64,
    pub per_page: i64,
    pub total:    i64,
}

async fn list_all_deployments(
    auth: AuthUser,
    Query(q): Query<DeploymentListQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<AdminDeploymentsResponse>>, ApiAppError> {
    let org_id = q.org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id is required".to_string())))?;
    // Accept deployments:read or deployments:write — both grant view access.
    let can_read  = crate::middleware::rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:deployments:read")).await.is_ok();
    let can_write = crate::middleware::rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:deployments:write")).await.is_ok();
    if !can_read && !can_write {
        return Err(ApiAppError(AppError::Forbidden("deployments:read or deployments:write permission required".to_string())));
    }

    let page     = q.page.unwrap_or(1).max(1);
    let per_page = q.per_page.unwrap_or(50).clamp(1, 200);
    let offset   = (page - 1) * per_page;

    // Stats (always for the whole org, regardless of status filter)
    let stats_rows = sqlx::query_as::<_, (String, i64)>(
        "SELECT d.status::text, COUNT(*) FROM deployments d
         JOIN services s  ON s.id = d.service_id
         JOIN projects p  ON p.id = s.project_id
         WHERE p.org_id = $1
         GROUP BY d.status",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let mut running = 0i64;
    let mut queued  = 0i64;
    let mut failed  = 0i64;
    let mut success = 0i64;
    let mut total_all = 0i64;
    for (st, cnt) in &stats_rows {
        total_all += cnt;
        match st.as_str() {
            "running"  => running  = *cnt,
            "queued"   => queued   = *cnt,
            "failed"   => failed   = *cnt,
            "success"  => success  = *cnt,
            _ => {}
        }
    }

    // Filtered count
    let total: i64 = if let Some(ref status_filter) = q.status {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM deployments d
             JOIN services s ON s.id = d.service_id
             JOIN projects p ON p.id = s.project_id
             WHERE p.org_id = $1 AND d.status::text = $2",
        )
        .bind(org_id)
        .bind(status_filter)
        .fetch_one(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
        row.0
    } else {
        total_all
    };

    // Row query
    let rows = if let Some(ref status_filter) = q.status {
        sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, String, uuid::Uuid, String, String, String, String, DateTime<Utc>, Option<DateTime<Utc>>)>(
            "SELECT d.id, s.id, s.name, p.id, p.name, d.triggered_by, d.source_ref,
                    d.status::text, d.created_at, d.finished_at
             FROM deployments d
             JOIN services s ON s.id = d.service_id
             JOIN projects p ON p.id = s.project_id
             WHERE p.org_id = $1 AND d.status::text = $2
             ORDER BY d.created_at DESC
             LIMIT $3 OFFSET $4",
        )
        .bind(org_id)
        .bind(status_filter)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    } else {
        sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, String, uuid::Uuid, String, String, String, String, DateTime<Utc>, Option<DateTime<Utc>>)>(
            "SELECT d.id, s.id, s.name, p.id, p.name, d.triggered_by, d.source_ref,
                    d.status::text, d.created_at, d.finished_at
             FROM deployments d
             JOIN services s ON s.id = d.service_id
             JOIN projects p ON p.id = s.project_id
             WHERE p.org_id = $1
             ORDER BY d.created_at DESC
             LIMIT $2 OFFSET $3",
        )
        .bind(org_id)
        .bind(per_page)
        .bind(offset)
        .fetch_all(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    };

    let data = rows
        .into_iter()
        .map(|(id, service_id, service_name, project_id, project_name, triggered_by, source_ref, status, created_at, finished_at)| {
            AdminDeploymentRow { id, service_id, service_name, project_id, project_name, triggered_by, source_ref, status, created_at, finished_at }
        })
        .collect();

    Ok(Json(ApiResponse::ok(AdminDeploymentsResponse {
        data,
        stats: AdminDeploymentStats { total: total_all, running, queued, failed, success },
        page,
        per_page,
        total,
    })))
}

// ── API Key Management (JWT-authenticated, for the settings UI) ───────────────

#[derive(Debug, Serialize)]
struct ApiKeyItem {
    id: uuid::Uuid,
    name: String,
    key_prefix: String,
    scopes: Vec<String>,
    last_used_at: Option<DateTime<Utc>>,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
struct CreatedApiKey {
    id: uuid::Uuid,
    name: String,
    key: String,
    key_prefix: String,
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    scopes: Vec<String>,
    expires_at: Option<DateTime<Utc>>,
}

/// POST /admin/smtp/test
///
/// Sends a test email with custom to/subject/body fields.
async fn test_smtp(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
    Json(body): Json<SendTestEmailRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_smtp_perm(&state.db, &auth, q.org_id, true).await?;
    let smtp_cfg = crate::email::load_smtp_config(&state.db, &state.config.smtp).await;

    if !smtp_cfg.enabled {
        return Err(ApiAppError(AppError::BadRequest(
            "SMTP is not enabled. Enable it and save settings first.".to_string(),
        )));
    }

    crate::email::send_test_email(&smtp_cfg, &body.to, &body.subject, &body.body)
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("SMTP test failed: {e}"))))?;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": format!("Test email sent to {}", body.to)
    }))))
}

#[derive(Debug, Deserialize)]
struct SendTestEmailRequest {
    to: String,
    subject: String,
    body: String,
}

fn generate_api_key() -> (String, String, String) {
    let raw = format!("{}{}", uuid::Uuid::now_v7().simple(), uuid::Uuid::now_v7().simple());
    let full_key = format!("ship_{}", raw);
    let prefix = format!("ship_{}", &raw[..8]);
    let hash = hex::encode(Sha256::digest(full_key.as_bytes()));
    (full_key, prefix, hash)
}


async fn get_deployments_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<PlatformSettings>>, ApiAppError> {
    require_deployments_perm(&state.db, &auth, q.org_id, false).await?;
    let settings = load_settings(&state).await?;
    Ok(Json(ApiResponse::ok(settings)))
}

async fn update_deployments_settings(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
    Json(body): Json<PlatformSettings>,
) -> Result<Json<ApiResponse<PlatformSettings>>, ApiAppError> {
    require_deployments_perm(&state.db, &auth, q.org_id, true).await?;

    if let Some(v) = body.max_parallel_deployments {
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, NOW())
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
        )
        .bind("max_parallel_deployments")
        .bind(Value::String(v.to_string()))
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    cache::del(&state.redis, "settings").await;
    let settings = load_settings(&state).await?;
    Ok(Json(ApiResponse::ok(settings)))
}

async fn require_owner(user_id: uuid::Uuid, db: &sqlx::PgPool) -> Result<(), ApiAppError> {
    if superadmin_bypass(db, user_id).await { return Ok(()); }
    let is_owner: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_members WHERE user_id = $1 AND role = 'owner' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    is_owner
        .map(|_| ())
        .ok_or_else(|| ApiAppError(AppError::Forbidden("Only owners can manage API keys".to_string())))
}

async fn list_api_keys(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<Vec<ApiKeyItem>>>, ApiAppError> {
    let org_id = q.org_id.ok_or_else(|| ApiAppError(AppError::BadRequest("org_id is required".to_string())))?;
    // Accept keys:read or keys:write — both grant view access.
    let can_read  = crate::middleware::rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:keys:read")).await.is_ok();
    let can_write = crate::middleware::rbac::require_permission(&state.db, auth.user_id, org_id, &format!("shipyard:{org_id}:keys:write")).await.is_ok();
    if !can_read && !can_write {
        return Err(ApiAppError(AppError::Forbidden("keys:read or keys:write permission required".to_string())));
    }

    let rows = sqlx::query_as::<_, (uuid::Uuid, String, String, Vec<String>, Option<DateTime<Utc>>, Option<DateTime<Utc>>, DateTime<Utc>)>(
        "SELECT id, name, key_prefix, scopes, last_used_at, expires_at, created_at
         FROM api_keys
         WHERE org_id = $1
         ORDER BY created_at DESC",
    )
    .bind(org_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let keys = rows
        .into_iter()
        .map(|(id, name, key_prefix, scopes, last_used_at, expires_at, created_at)| ApiKeyItem {
            id,
            name,
            key_prefix,
            scopes,
            last_used_at,
            expires_at,
            created_at,
        })
        .collect();

    Ok(Json(ApiResponse::ok(keys)))
}

async fn create_api_key(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
    Json(body): Json<CreateApiKeyRequest>,
) -> Result<(StatusCode, Json<ApiResponse<CreatedApiKey>>), ApiAppError> {
    require_keys_perm(&state.db, &auth, q.org_id, true).await?;
    let org_id = q.org_id.unwrap();

    if body.name.trim().is_empty() {
        return Err(ApiAppError(AppError::Validation("Key name must not be empty".to_string())));
    }

    let valid_scopes = ["read", "deploy", "write", "admin"];
    for s in &body.scopes {
        if !valid_scopes.contains(&s.as_str()) {
            return Err(ApiAppError(AppError::Validation(format!(
                "Unknown scope '{}'. Valid: {}",
                s,
                valid_scopes.join(", ")
            ))));
        }
    }

    let (full_key, prefix, hash) = generate_api_key();
    let key_id = uuid::Uuid::now_v7();
    let now = Utc::now();

    sqlx::query(
        "INSERT INTO api_keys (id, org_id, created_by, name, key_prefix, key_hash, scopes, expires_at, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
    .bind(key_id)
    .bind(org_id)
    .bind(auth.user_id)
    .bind(&body.name)
    .bind(&prefix)
    .bind(&hash)
    .bind(&body.scopes)
    .bind(body.expires_at)
    .bind(now)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    tracing::info!(key_id = %key_id, org_id = %org_id, "API key created: {}", body.name);

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::ok(CreatedApiKey {
            id: key_id,
            name: body.name,
            key: full_key,
            key_prefix: prefix,
            scopes: body.scopes,
            expires_at: body.expires_at,
            created_at: now,
        })),
    ))
}

async fn revoke_api_key(
    auth: AuthUser,
    Path(key_id): Path<uuid::Uuid>,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<StatusCode, ApiAppError> {
    require_keys_perm(&state.db, &auth, q.org_id, true).await?;
    let org_id = q.org_id.unwrap();

    let result = sqlx::query(
        "DELETE FROM api_keys WHERE id = $1 AND org_id = $2",
    )
    .bind(key_id)
    .bind(org_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if result.rows_affected() == 0 {
        return Err(ApiAppError(AppError::NotFound(format!("API key '{}' not found", key_id))));
    }

    tracing::info!(key_id = %key_id, org_id = %org_id, user_id = %auth.user_id, "API key revoked");

    Ok(StatusCode::NO_CONTENT)
}

// ── DB table management ───────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
struct DbTableInfo {
    name: String,
    row_count: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
struct DbColMeta {
    name: String,
    data_type: String,
    udt_name: String,
    is_nullable: bool,
    is_primary_key: bool,
}

#[derive(Debug, Serialize)]
struct DbTableRowsResponse {
    columns: Vec<DbColMeta>,
    rows: Vec<Vec<serde_json::Value>>,
    total: i64,
    page: u32,
    per_page: u32,
}

#[derive(Debug, Deserialize)]
struct TableRowsQuery {
    #[serde(default)]
    search: String,
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_per_page_rows")]
    per_page: u32,
}
fn default_page() -> u32 { 1 }
fn default_per_page_rows() -> u32 { 50 }

#[derive(Debug, Deserialize)]
struct DbRowUpdateRequest {
    updates: std::collections::HashMap<String, serde_json::Value>,
}

/// Validate that a table exists in the public schema; return an error if not.
async fn require_table(db: &sqlx::PgPool, table_name: &str) -> Result<(), ApiAppError> {
    let exists: Option<(bool,)> = sqlx::query_as(
        "SELECT TRUE FROM information_schema.tables
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name = $1",
    )
    .bind(table_name)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    exists.map(|_| ()).ok_or_else(|| {
        ApiAppError(AppError::NotFound(format!("Table '{}' not found", table_name)))
    })
}

/// Fetch column metadata (name, type, nullable, pk flag) for a table.
async fn fetch_table_columns(db: &sqlx::PgPool, table_name: &str) -> Result<Vec<DbColMeta>, ApiAppError> {
    sqlx::query_as::<_, DbColMeta>(
        r#"
        SELECT
            c.column_name            AS name,
            c.data_type              AS data_type,
            c.udt_name               AS udt_name,
            (c.is_nullable = 'YES')  AS is_nullable,
            (pk.column_name IS NOT NULL) AS is_primary_key
        FROM information_schema.columns c
        LEFT JOIN (
            SELECT kcu.column_name
            FROM information_schema.table_constraints tc
            JOIN information_schema.key_column_usage kcu
                ON kcu.constraint_name = tc.constraint_name
               AND kcu.table_schema   = tc.table_schema
               AND kcu.table_name     = tc.table_name
            WHERE tc.constraint_type = 'PRIMARY KEY'
              AND tc.table_name      = $1
              AND tc.table_schema    = 'public'
        ) pk ON pk.column_name = c.column_name
        WHERE c.table_schema = 'public' AND c.table_name = $1
        ORDER BY c.ordinal_position
        "#,
    )
    .bind(table_name)
    .fetch_all(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))
}

/// Convert a Postgres row cell to a JSON value — handles common platform types.
fn platform_pg_to_json(row: &sqlx::postgres::PgRow, idx: usize) -> serde_json::Value {
    use sqlx::{Column, Row, TypeInfo};
    let type_name = row.column(idx).type_info().name().to_ascii_uppercase();
    match type_name.as_str() {
        "INT2" | "INT4"   => row.try_get::<i32, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
        "INT8" | "OID"    => row.try_get::<i64, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
        "FLOAT4"          => row.try_get::<f32, _>(idx).ok().map(|v| serde_json::Value::from(v as f64)).unwrap_or(serde_json::Value::Null),
        "FLOAT8"          => row.try_get::<f64, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
        "BOOL"            => row.try_get::<bool, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
        "JSON" | "JSONB"  => row.try_get::<serde_json::Value, _>(idx).unwrap_or(serde_json::Value::Null),
        _                 => row.try_get::<String, _>(idx).ok().map(Into::into).unwrap_or(serde_json::Value::Null),
    }
}

/// GET /admin/db/tables/:table_name/columns
async fn list_table_columns(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(table_name): Path<String>,
) -> Result<Json<ApiResponse<Vec<DbColMeta>>>, ApiAppError> {
    require_owner(auth.user_id, &state.db).await?;
    require_table(&state.db, &table_name).await?;
    let cols = fetch_table_columns(&state.db, &table_name).await?;
    Ok(Json(ApiResponse::ok(cols)))
}

/// GET /admin/db/tables/:table_name/rows?search=&page=&per_page=
async fn list_table_rows(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(table_name): Path<String>,
    Query(q): Query<TableRowsQuery>,
) -> Result<Json<ApiResponse<DbTableRowsResponse>>, ApiAppError> {
    require_owner(auth.user_id, &state.db).await?;
    require_table(&state.db, &table_name).await?;

    let cols = fetch_table_columns(&state.db, &table_name).await?;

    let per_page = q.per_page.clamp(1, 200);
    let page = q.page.max(1);
    let offset = ((page - 1) * per_page) as i64;
    let search = q.search.trim().to_string();

    let quoted = table_name.replace('"', "\"\"");

    // Cast every column to TEXT in the SELECT so we avoid custom-type decode failures
    // (enums, uuid, timestamptz all cast cleanly to text). Column names come from
    // information_schema so they are trusted — not user input.
    let select_cols: String = cols
        .iter()
        .map(|c| {
            let qc = c.name.replace('"', "\"\"");
            format!("CAST(\"{}\" AS TEXT) AS \"{}\"", qc, qc)
        })
        .collect::<Vec<_>>()
        .join(", ");

    // Search: ILIKE against every column's already-text cast alias
    let search_clause = if search.is_empty() {
        "TRUE".to_string()
    } else {
        let parts: Vec<String> = cols
            .iter()
            .map(|c| format!("CAST(\"{}\" AS TEXT) ILIKE $1", c.name.replace('"', "\"\"")))
            .collect();
        format!("({})", parts.join(" OR "))
    };

    // Total count
    let count_sql = format!("SELECT COUNT(*) FROM \"{quoted}\" WHERE {search_clause}");
    let total: i64 = if search.is_empty() {
        sqlx::query_scalar(&count_sql)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    } else {
        let pattern = format!("%{search}%");
        sqlx::query_scalar(&count_sql)
            .bind(&pattern)
            .fetch_one(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    };

    // Determine ORDER BY — prefer pk column, else first column
    let order_col = cols.iter().find(|c| c.is_primary_key).unwrap_or(&cols[0]);
    let row_sql = format!(
        "SELECT {select_cols} FROM \"{quoted}\" WHERE {search_clause} ORDER BY CAST(\"{}\" AS TEXT) LIMIT $2 OFFSET $3",
        order_col.name.replace('"', "\"\""),
    );

    let pg_rows = if search.is_empty() {
        sqlx::query(&row_sql)
            .bind("")
            .bind(per_page as i64)
            .bind(offset)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    } else {
        let pattern = format!("%{search}%");
        sqlx::query(&row_sql)
            .bind(&pattern)
            .bind(per_page as i64)
            .bind(offset)
            .fetch_all(&state.db)
            .await
            .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    };

    // Every column is now TEXT — use Option<String> to handle NULLs
    let rows: Vec<Vec<serde_json::Value>> = pg_rows
        .iter()
        .map(|row| {
            (0..cols.len())
                .map(|i| {
                    use sqlx::Row;
                    match row.try_get::<Option<String>, _>(i) {
                        Ok(Some(s)) => serde_json::Value::String(s),
                        _ => serde_json::Value::Null,
                    }
                })
                .collect()
        })
        .collect();

    Ok(Json(ApiResponse::ok(DbTableRowsResponse { columns: cols, rows, total, page, per_page })))
}

/// PATCH /admin/db/tables/:table_name/rows/:pk_value
///
/// Updates a single row identified by the table's primary key.
/// Body: `{"updates": {"col_name": "new_value", ...}}`
async fn update_table_row(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((table_name, pk_value)): Path<(String, String)>,
    Json(body): Json<DbRowUpdateRequest>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_owner(auth.user_id, &state.db).await?;
    require_table(&state.db, &table_name).await?;

    if body.updates.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("No columns to update".to_string())));
    }

    let cols = fetch_table_columns(&state.db, &table_name).await?;

    let pk_col = cols.iter().find(|c| c.is_primary_key).ok_or_else(|| {
        ApiAppError(AppError::BadRequest(format!("Table '{}' has no primary key", table_name)))
    })?;

    // Validate all update columns exist in the table.
    let col_map: std::collections::HashMap<&str, &DbColMeta> =
        cols.iter().map(|c| (c.name.as_str(), c)).collect();

    let mut update_cols: Vec<(&DbColMeta, String)> = Vec::new();
    for (col_name, new_val) in &body.updates {
        let meta = col_map.get(col_name.as_str()).ok_or_else(|| {
            ApiAppError(AppError::BadRequest(format!("Column '{}' does not exist in table '{}'", col_name, table_name)))
        })?;
        let val_str = match new_val {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Null => "".to_string(),
            other => other.to_string(),
        };
        update_cols.push((meta, val_str));
    }

    // Build SET clauses — $1 is pk_value, $2.. are update values.
    let quoted_table = table_name.replace('"', "\"\"");
    let set_clauses: Vec<String> = update_cols
        .iter()
        .enumerate()
        .map(|(i, (meta, _))| {
            let quoted_col = meta.name.replace('"', "\"\"");
            // Cast through text so string literals work for enums, uuids, etc.
            format!("\"{}\" = (${}::TEXT)::{}", quoted_col, i + 2, meta.udt_name)
        })
        .collect();

    let pk_quoted = pk_col.name.replace('"', "\"\"");
    let sql = format!(
        "UPDATE \"{}\" SET {} WHERE \"{}\" = ($1::TEXT)::{}",
        quoted_table,
        set_clauses.join(", "),
        pk_quoted,
        pk_col.udt_name,
    );

    let mut query = sqlx::query(&sql).bind(&pk_value);
    for (_, val) in &update_cols {
        query = query.bind(val);
    }

    let result = query.execute(&state.db).await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if result.rows_affected() == 0 {
        return Err(ApiAppError(AppError::NotFound(format!(
            "No row with pk '{}' in table '{}'", pk_value, table_name
        ))));
    }

    tracing::info!(
        user_id = %auth.user_id,
        table = %table_name,
        pk = %pk_value,
        cols = ?body.updates.keys().collect::<Vec<_>>(),
        "Admin DB row updated"
    );

    Ok(Json(ApiResponse::ok(serde_json::json!({ "rows_affected": result.rows_affected() }))))
}

/// DELETE /admin/db/tables/:table_name/rows/:pk_value
///
/// Deletes a single row identified by the table's primary key.
async fn delete_table_row(
    auth: AuthUser,
    State(state): State<AppState>,
    Path((table_name, pk_value)): Path<(String, String)>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_owner(auth.user_id, &state.db).await?;
    require_table(&state.db, &table_name).await?;

    let cols = fetch_table_columns(&state.db, &table_name).await?;

    let pk_col = cols.iter().find(|c| c.is_primary_key).ok_or_else(|| {
        ApiAppError(AppError::BadRequest(format!("Table '{}' has no primary key", table_name)))
    })?;

    let quoted_table = table_name.replace('"', "\"\"");
    let pk_quoted = pk_col.name.replace('"', "\"\"");
    let sql = format!(
        "DELETE FROM \"{}\" WHERE \"{}\" = ($1::TEXT)::{}",
        quoted_table, pk_quoted, pk_col.udt_name,
    );

    let result = sqlx::query(&sql)
        .bind(&pk_value)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if result.rows_affected() == 0 {
        return Err(ApiAppError(AppError::NotFound(format!(
            "No row with pk '{}' in table '{}'", pk_value, table_name
        ))));
    }

    tracing::info!(
        user_id = %auth.user_id,
        table = %table_name,
        pk = %pk_value,
        "Admin DB row deleted"
    );

    Ok(Json(ApiResponse::ok(serde_json::json!({ "rows_affected": result.rows_affected() }))))
}

/// GET /admin/db/tables — list all user tables with live row counts.
async fn list_db_tables(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<Vec<DbTableInfo>>>, ApiAppError> {
    require_owner(auth.user_id, &state.db).await?;

    let rows: Vec<(String, i64)> = sqlx::query_as::<_, (String, i64)>(
        "SELECT t.table_name, COALESCE(s.n_live_tup, 0)::bigint
         FROM information_schema.tables t
         LEFT JOIN pg_stat_user_tables s ON s.relname = t.table_name
         WHERE t.table_schema = 'public' AND t.table_type = 'BASE TABLE'
         ORDER BY t.table_name",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let tables = rows
        .into_iter()
        .map(|(name, row_count)| DbTableInfo { name, row_count })
        .collect();

    Ok(Json(ApiResponse::ok(tables)))
}

/// DELETE /admin/db/tables/:table_name — drop a table (CASCADE).
async fn drop_db_table(
    auth: AuthUser,
    State(state): State<AppState>,
    Path(table_name): Path<String>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_owner(auth.user_id, &state.db).await?;

    // Validate the table actually exists to prevent any injection via the path segment.
    let exists: Option<(String,)> = sqlx::query_as::<_, (String,)>(
        "SELECT table_name FROM information_schema.tables
         WHERE table_schema = 'public' AND table_type = 'BASE TABLE' AND table_name = $1",
    )
    .bind(&table_name)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if exists.is_none() {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Table '{}' not found",
            table_name
        ))));
    }

    // Double any embedded double-quotes so the identifier is always valid SQL.
    let quoted = table_name.replace('"', "\"\"");
    sqlx::query(&format!("DROP TABLE IF EXISTS \"{quoted}\" CASCADE"))
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    tracing::warn!(user_id = %auth.user_id, table = %table_name, "Platform DB table dropped");

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": format!("Table '{}' dropped", table_name)
    }))))
}

// ── Nginx-static conf endpoints ───────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct NginxConfEntry {
    pub name: String,
}

#[derive(Debug, Serialize)]
pub struct NginxConfList {
    pub dir: String,
    pub files: Vec<NginxConfEntry>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct NginxConfFile {
    pub name: String,
    pub content: Option<String>,
    pub exists: bool,
    pub error: Option<String>,
}

async fn list_nginx_static_confs(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Json<ApiResponse<NginxConfList>>, ApiAppError> {
    require_static_read(&state.db, &auth, q.org_id).await?;

    let mut resp = NginxConfList {
        dir: NGINX_CONF_DIR.to_string(),
        files: vec![],
        error: None,
    };

    match docker_exec_container(NGINX_STATIC_CONTAINER, &["ls", "-1", NGINX_CONF_DIR]).await {
        Err(e) => { resp.error = Some(e); }
        Ok(output) => {
            let mut files: Vec<NginxConfEntry> = output
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && l.ends_with(".conf"))
                .map(|l| NginxConfEntry { name: l.to_string() })
                .collect();
            files.sort_by(|a, b| a.name.cmp(&b.name));
            resp.files = files;
        }
    }

    Ok(Json(ApiResponse::ok(resp)))
}

async fn get_nginx_static_conf(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
    Path(name): Path<String>,
) -> Result<Json<ApiResponse<NginxConfFile>>, ApiAppError> {
    require_static_read(&state.db, &auth, q.org_id).await?;

    if name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(ApiAppError(AppError::BadRequest("Invalid filename".to_string())));
    }
    if !name.ends_with(".conf") {
        return Err(ApiAppError(AppError::BadRequest("Only .conf files are accessible".to_string())));
    }

    let path = format!("{}/{}", NGINX_CONF_DIR, name);
    let resp = match docker_exec_container(NGINX_STATIC_CONTAINER, &["cat", &path]).await {
        Ok(content) => NginxConfFile { name, content: Some(content), exists: true, error: None },
        Err(e) => {
            let exists = !e.contains("No such file") && !e.contains("no such file");
            NginxConfFile { name, content: None, exists, error: Some(e) }
        }
    };
    Ok(Json(ApiResponse::ok(resp)))
}

async fn nginx_static_log_stream(
    auth: AuthUser,
    State(state): State<AppState>,
    Query(q): Query<OrgPermQuery>,
) -> Result<Sse<ReceiverStream<Result<Event, std::convert::Infallible>>>, ApiAppError> {
    require_static_read(&state.db, &auth, q.org_id).await?;

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, std::convert::Infallible>>(128);

    tokio::spawn(async move {
        let mut child = match tokio::process::Command::new("docker")
            .args(["logs", "-f", "--tail=200", NGINX_STATIC_CONTAINER])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx
                    .send(Ok(Event::default()
                        .event("error")
                        .data(format!("Failed to start docker logs: {e}"))))
                    .await;
                return;
            }
        };

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");

        let tx_out = tx.clone();
        let tx_err = tx.clone();
        let tx_closed = tx.clone();
        let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            tx_closed.closed().await;
            let _ = cancel_tx.send(());
        });

        let mut h_out = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_out.send(Ok(Event::default().data(line))).await.is_err() {
                    break;
                }
            }
        });

        let mut h_err = tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_err.send(Ok(Event::default().data(line))).await.is_err() {
                    break;
                }
            }
        });

        tokio::select! {
            _ = async { tokio::join!(&mut h_out, &mut h_err) } => {}
            _ = &mut cancel_rx => {
                h_out.abort();
                h_err.abort();
            }
        }
        let _ = child.kill().await;
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
}
