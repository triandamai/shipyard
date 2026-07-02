use axum::{
    extract::{Path, Query, State},
    response::sse::{Event, KeepAlive, Sse},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::Infallible;
use std::sync::OnceLock;
use std::time::Instant;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex;
use tokio_stream::wrappers::ReceiverStream;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;
use crate::auth::AuthUser;
use crate::cache;
use crate::error::ApiAppError;
use crate::AppState;

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
];

const TRAEFIK_CONTAINER: &str = "shipyard-traefik";
const TRAEFIK_STATIC_PATH: &str = "/etc/traefik/traefik.yml";
const TRAEFIK_DYNAMIC_DIR: &str = "/etc/traefik/dynamic";

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
        main_domain:              map.remove("main_domain"),
        traefik_network:          map.remove("traefik_network").or_else(|| Some(state.config.traefik.network.clone())),
        traefik_entrypoint_http:  map.remove("traefik_entrypoint_http").or_else(|| Some(state.config.traefik.entrypoint_http.clone())),
        traefik_entrypoint_https: map.remove("traefik_entrypoint_https").or_else(|| Some(state.config.traefik.entrypoint_https.clone())),
        traefik_cert_resolver:    map.remove("traefik_cert_resolver").or_else(|| Some(state.config.traefik.cert_resolver.clone())),
        git_github_token:         map.remove("git_github_token"),
        git_gitlab_token:         map.remove("git_gitlab_token"),
        git_bitbucket_token:      map.remove("git_bitbucket_token"),
        git_webhook_secret:       map.remove("git_webhook_secret"),
    })
}

fn is_yaml(name: &str) -> bool {
    name.ends_with(".yml") || name.ends_with(".yaml")
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
    _auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<PlatformSettings>,
) -> Result<Json<ApiResponse<PlatformSettings>>, ApiAppError> {
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

    get_settings(_auth, State(state)).await
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

        let h_out = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_out.send(Ok(Event::default().data(line))).await.is_err() {
                    break;
                }
            }
        });

        let h_err = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_err.send(Ok(Event::default().data(line))).await.is_err() {
                    break;
                }
            }
        });

        // When the client disconnects, the channel closes, unblocking the sends.
        let _ = tokio::join!(h_out, h_err);
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

fn resolve_update_command() -> Option<(&'static str, Vec<&'static str>)> {
    if std::path::Path::new("/opt/shipyard/update.sh").exists() {
        Some(("bash", vec!["/opt/shipyard/update.sh"]))
    } else if std::path::Path::new("/opt/shipyard/docker-compose.yml").exists() {
        Some(("docker", vec!["compose", "-f", "/opt/shipyard/docker-compose.yml", "pull"]))
    } else {
        None
    }
}

/// POST /admin/update — blocking one-shot update (kept for backwards compat).
async fn trigger_update(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    let is_owner: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_members WHERE user_id = $1 AND role = 'owner' LIMIT 1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if is_owner.is_none() {
        return Err(ApiAppError(AppError::Forbidden(
            "Only platform owners can trigger updates".to_string()
        )));
    }

    let (program, args) = resolve_update_command().ok_or_else(|| {
        ApiAppError(AppError::Internal(
            "No update script or compose file found at /opt/shipyard".to_string()
        ))
    })?;

    tracing::info!(user_id = %auth.user_id, "Platform update triggered (blocking)");

    let out = tokio::process::Command::new(program)
        .args(&args)
        .output()
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to run update: {e}"))))?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    if !out.status.success() {
        return Err(ApiAppError(AppError::Internal(format!("Update failed:\n{stderr}"))));
    }

    *version_cache().lock().await = None;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Update complete.",
        "output": stdout,
    }))))
}

/// GET /admin/update/stream — SSE stream of update progress.
///
/// Streams stdout + stderr of update.sh line-by-line as `data:` events.
/// Sends an `event: done` or `event: error` event when finished.
/// The SSE connection will drop when services restart — the frontend should
/// handle this as an expected "services restarting" disconnect.
async fn update_stream(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Sse<ReceiverStream<Result<Event, Infallible>>>, ApiAppError> {
    let is_owner: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_members WHERE user_id = $1 AND role = 'owner' LIMIT 1",
    )
    .bind(auth.user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    if is_owner.is_none() {
        return Err(ApiAppError(AppError::Forbidden(
            "Only platform owners can trigger updates".to_string()
        )));
    }

    let (program, args) = resolve_update_command().ok_or_else(|| {
        ApiAppError(AppError::Internal(
            "No update script or compose file found at /opt/shipyard".to_string()
        ))
    })?;

    tracing::info!(user_id = %auth.user_id, "Platform update stream started");

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(256);

    tokio::spawn(async move {
        let mut child = match tokio::process::Command::new(program)
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
            Ok(c) => c,
            Err(e) => {
                let _ = tx.send(Ok(
                    Event::default().event("error").data(format!("Failed to start update: {e}"))
                )).await;
                return;
            }
        };

        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let tx_out = tx.clone();
        let tx_err = tx.clone();

        let h_out = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_out.send(Ok(Event::default().data(line))).await.is_err() { break; }
            }
        });

        let h_err = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if tx_err.send(Ok(Event::default().data(line))).await.is_err() { break; }
            }
        });

        let _ = tokio::join!(h_out, h_err);

        let status = child.wait().await;
        let ok = status.map(|s| s.success()).unwrap_or(false);

        *version_cache().lock().await = None;

        let final_event = if ok {
            Event::default().event("done").data("Update complete. Services are restarting…")
        } else {
            Event::default().event("error").data("Update script exited with an error.")
        };
        let _ = tx.send(Ok(final_event)).await;
    });

    Ok(Sse::new(ReceiverStream::new(rx)).keep_alive(KeepAlive::default()))
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
        }
    })
    .await
    .map_err(|e| ApiAppError(AppError::Internal(format!("system info error: {e}"))))?;

    Ok(Json(ApiResponse::ok(info)))
}
