use axum::{
    extract::{Path, State},
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

/// GitHub repository for release checks (owner/repo).
const GITHUB_REPO: &str = "shipyard-paas/shipyard";

/// In-memory cache for the latest GitHub release (TTL: 1 hour).
static VERSION_CACHE: OnceLock<Mutex<Option<(Instant, VersionInfo)>>> = OnceLock::new();

fn version_cache() -> &'static Mutex<Option<(Instant, VersionInfo)>> {
    VERSION_CACHE.get_or_init(|| Mutex::new(None))
}

#[derive(Debug, Clone, Serialize)]
pub struct VersionInfo {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
    pub release_url: String,
    pub release_notes: Option<String>,
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

/// GET /admin/version
///
/// Returns the current running version and the latest release on GitHub.
/// Result is cached in-process for 1 hour to avoid rate-limiting.
async fn get_version(
    _auth: AuthUser,
    State(_state): State<AppState>,
) -> Result<Json<ApiResponse<VersionInfo>>, ApiAppError> {
    let current = env!("CARGO_PKG_VERSION").to_string();
    let cache = version_cache();
    let mut guard = cache.lock().await;

    // Return cached value if fresh (< 1 hour old).
    if let Some((fetched_at, info)) = guard.as_ref() {
        if fetched_at.elapsed() < std::time::Duration::from_secs(3600) {
            return Ok(Json(ApiResponse::ok(info.clone())));
        }
    }

    // Fetch from GitHub releases API.
    let url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let client = reqwest::Client::builder()
        .user_agent(format!("shipyard/{current}"))
        .timeout(std::time::Duration::from_secs(8))
        .build()
        .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;

    let info = match client.get(&url).send().await {
        Ok(resp) if resp.status().is_success() => {
            let body: serde_json::Value = resp.json().await
                .map_err(|e| ApiAppError(AppError::Internal(e.to_string())))?;

            let latest = body["tag_name"]
                .as_str()
                .unwrap_or(&current)
                .trim_start_matches('v')
                .to_string();

            let release_url = body["html_url"]
                .as_str()
                .unwrap_or("")
                .to_string();

            let release_notes = body["body"]
                .as_str()
                .map(|s| s.chars().take(500).collect());

            let update_available = version_gt(&latest, &current);

            VersionInfo { current, latest, update_available, release_url, release_notes }
        }
        _ => {
            // GitHub unreachable — return current version with no update info.
            VersionInfo {
                latest: current.clone(),
                update_available: false,
                release_url: format!("https://github.com/{GITHUB_REPO}/releases"),
                release_notes: None,
                current,
            }
        }
    };

    *guard = Some((Instant::now(), info.clone()));
    Ok(Json(ApiResponse::ok(info)))
}

/// Compare two semver strings. Returns true if `a` is strictly greater than `b`.
fn version_gt(a: &str, b: &str) -> bool {
    let parse = |s: &str| -> (u32, u32, u32) {
        let mut parts = s.splitn(3, '.').map(|p| p.parse::<u32>().unwrap_or(0));
        (parts.next().unwrap_or(0), parts.next().unwrap_or(0), parts.next().unwrap_or(0))
    };
    parse(a) > parse(b)
}

// ── Self-update ───────────────────────────────────────────────────────────────

/// POST /admin/update
///
/// Pulls the latest Docker images and recreates containers in-place.
/// Requires the caller to be an owner of any org (platform-level action).
async fn trigger_update(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    // Require platform owner: the caller must be owner of at least one org.
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

    // Run the update script if present, otherwise fall back to docker compose commands.
    let update_script = "/opt/shipyard/update.sh";
    let compose_dir = "/opt/shipyard";

    let (program, args): (&str, Vec<&str>) = if std::path::Path::new(update_script).exists() {
        ("bash", vec![update_script])
    } else if std::path::Path::new(compose_dir).join("docker-compose.yml").exists() {
        ("docker", vec!["compose", "-f", "/opt/shipyard/docker-compose.yml", "pull"])
    } else {
        return Err(ApiAppError(AppError::Internal(
            "No update script or compose file found at /opt/shipyard".to_string()
        )));
    };

    tracing::info!(user_id = %auth.user_id, "Platform update triggered");

    let out = tokio::process::Command::new(program)
        .args(&args)
        .output()
        .await
        .map_err(|e| ApiAppError(AppError::Internal(format!("Failed to run update: {e}"))))?;

    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();

    if !out.status.success() {
        return Err(ApiAppError(AppError::Internal(format!(
            "Update failed: {stderr}"
        ))));
    }

    // Invalidate version cache so the next /admin/version reflects the new tag.
    *version_cache().lock().await = None;

    Ok(Json(ApiResponse::ok(serde_json::json!({
        "message": "Update complete. Restart services to apply.",
        "output": stdout,
    }))))
}
