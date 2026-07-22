use std::sync::Arc;
use std::convert::Infallible;

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    http::{Request, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response, sse::{Event, KeepAlive, Sse}},
    routing::get,
    Json, Router,
};
use bollard::Docker;
use bollard::container::{LogsOptions, StatsOptions};
use bollard::exec::{CreateExecOptions, StartExecOptions, StartExecResults};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;

struct SpikeConfig {
    callback_url: String,
    node_id: String,
    token: String,
    cpu_threshold: f64,
    mem_threshold: f64,
    disk_threshold: f64,
    net_threshold_mbps: f64,
}

#[derive(Clone)]
struct AgentState {
    docker: Arc<Docker>,
    token: Arc<String>,
}

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info".into()),
        )
        .init();

    let token = std::env::var("AGENT_TOKEN").expect("AGENT_TOKEN env var required");
    let port: u16 = std::env::var("AGENT_PORT")
        .unwrap_or_else(|_| "7070".into())
        .parse()
        .expect("AGENT_PORT must be a number");
    let bind = std::env::var("AGENT_BIND").unwrap_or_else(|_| "0.0.0.0".into());

    let docker = Docker::connect_with_local_defaults().expect("Failed to connect to Docker socket");

    let docker = Arc::new(docker);

    let state = AgentState {
        docker: Arc::clone(&docker),
        token: Arc::new(token.clone()),
    };

    let auth_state = state.clone();
    let app = Router::new()
        .route("/health", get(health))
        .route("/containers/:docker_id/logs", get(container_logs))
        .route("/containers/:docker_id/stats", get(container_stats))
        .route("/containers/:docker_id/exec", get(container_exec))
        .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
        .with_state(state);

    // Spawn spike-detection loop if a callback URL is configured.
    if let Ok(callback_url) = std::env::var("AGENT_CALLBACK_URL") {
        let spike_cfg = SpikeConfig {
            callback_url,
            node_id: std::env::var("AGENT_NODE_ID").unwrap_or_else(|_| {
                hostname::get()
                    .ok()
                    .and_then(|h| h.into_string().ok())
                    .unwrap_or_else(|| "unknown".to_string())
            }),
            token: token.clone(),
            cpu_threshold: std::env::var("AGENT_CPU_THRESHOLD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(80.0),
            mem_threshold: std::env::var("AGENT_MEM_THRESHOLD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(85.0),
            disk_threshold: std::env::var("AGENT_DISK_THRESHOLD")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(90.0),
            net_threshold_mbps: std::env::var("AGENT_NET_THRESHOLD_MBPS")
                .ok().and_then(|v| v.parse().ok()).unwrap_or(100.0),
        };
        let spike_docker = Arc::clone(&docker);
        let http = reqwest::Client::new();
        tokio::spawn(spike_loop(spike_cfg, spike_docker, http));
    }

    let addr = format!("{bind}:{port}");
    tracing::info!("shipyard-node-agent listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .unwrap_or_else(|e| panic!("failed to bind node-agent listener on {addr}: {e}"));
    axum::serve(listener, app)
        .await
        .expect("node-agent HTTP server exited unexpectedly");
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"ok": true}))
}

async fn auth_middleware(
    State(state): State<AgentState>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    // Allow token via header or query param (query param needed for WS upgrade)
    let header_token = req
        .headers()
        .get("x-agent-token")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);

    let query_token = req.uri().query().and_then(|q| {
        url::form_urlencoded::parse(q.as_bytes())
            .find(|(k, _)| k == "token")
            .map(|(_, v)| v.into_owned())
    });

    let provided = header_token.or(query_token);

    match provided {
        Some(t) if t == *state.token => next.run(req).await,
        _ => StatusCode::UNAUTHORIZED.into_response(),
    }
}

#[derive(Deserialize)]
struct LogsQuery {
    #[serde(default = "default_tail")]
    tail: String,
    #[serde(default = "default_follow")]
    follow: bool,
}
fn default_tail() -> String { "100".to_string() }
fn default_follow() -> bool { true }

async fn container_logs(
    State(state): State<AgentState>,
    Path(docker_id): Path<String>,
    Query(q): Query<LogsQuery>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let docker = state.docker.clone();
    let stream = async_stream::stream! {
        let opts = LogsOptions::<String> {
            stdout: true,
            stderr: true,
            follow: q.follow,
            tail: q.tail.clone(),
            timestamps: false,
            ..Default::default()
        };
        let mut log_stream = docker.logs(&docker_id, Some(opts));
        while let Some(item) = log_stream.next().await {
            match item {
                Ok(output) => {
                    use bollard::container::LogOutput;
                    let (stream_name, line) = match output {
                        LogOutput::StdOut { message } => ("stdout", String::from_utf8_lossy(&message).into_owned()),
                        LogOutput::StdErr { message } => ("stderr", String::from_utf8_lossy(&message).into_owned()),
                        LogOutput::Console { message } => ("stdout", String::from_utf8_lossy(&message).into_owned()),
                        _ => continue,
                    };
                    let data = serde_json::json!({"stream": stream_name, "line": line});
                    yield Ok(Event::default().data(data.to_string()));
                }
                Err(e) => {
                    let data = serde_json::json!({"error": e.to_string()});
                    yield Ok(Event::default().event("error").data(data.to_string()));
                    break;
                }
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Serialize)]
struct StatSnapshot {
    cpu_pct: f64,
    mem_used_mb: f64,
    mem_limit_mb: f64,
    mem_pct: f64,
}

async fn container_stats(
    State(state): State<AgentState>,
    Path(docker_id): Path<String>,
) -> Sse<impl futures::Stream<Item = Result<Event, Infallible>>> {
    let docker = state.docker.clone();
    let stream = async_stream::stream! {
        loop {
            let opts = StatsOptions { stream: false, one_shot: true };
            let mut s = docker.stats(&docker_id, Some(opts));
            match s.next().await {
                Some(Ok(stat)) => {
                    let cpu_delta = stat.cpu_stats.cpu_usage.total_usage
                        .saturating_sub(stat.precpu_stats.cpu_usage.total_usage) as f64;
                    let sys_delta = stat.cpu_stats.system_cpu_usage.unwrap_or(0)
                        .saturating_sub(stat.precpu_stats.system_cpu_usage.unwrap_or(0)) as f64;
                    let num_cpus = stat.cpu_stats.online_cpus.unwrap_or(1) as f64;
                    let cpu_pct = if sys_delta > 0.0 {
                        (cpu_delta / sys_delta) * num_cpus * 100.0
                    } else { 0.0 };

                    let mem_used = stat.memory_stats.usage.unwrap_or(0) as f64 / 1_048_576.0;
                    let mem_limit = stat.memory_stats.limit.unwrap_or(1) as f64 / 1_048_576.0;
                    let mem_pct = if mem_limit > 0.0 { (mem_used / mem_limit) * 100.0 } else { 0.0 };

                    let snap = StatSnapshot { cpu_pct, mem_used_mb: mem_used, mem_limit_mb: mem_limit, mem_pct };
                    let data = serde_json::to_string(&snap).unwrap_or_default();
                    yield Ok(Event::default().data(data));
                }
                Some(Err(e)) => {
                    let data = serde_json::json!({"error": e.to_string()});
                    yield Ok(Event::default().event("error").data(data.to_string()));
                    break;
                }
                None => break,
            }
            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::default())
}

#[derive(Deserialize)]
struct ExecQuery {
    #[serde(default = "default_cmd")]
    cmd: String,
    #[serde(default = "default_cols")]
    cols: u16,
    #[serde(default = "default_rows")]
    rows: u16,
    #[allow(dead_code)]
    token: Option<String>,
}
fn default_cmd() -> String { "/bin/sh".to_string() }
fn default_cols() -> u16 { 80 }
fn default_rows() -> u16 { 24 }

async fn container_exec(
    State(state): State<AgentState>,
    Path(docker_id): Path<String>,
    Query(q): Query<ExecQuery>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_exec(socket, state.docker, docker_id, q))
}

async fn handle_exec(socket: WebSocket, docker: Arc<Docker>, docker_id: String, q: ExecQuery) {
    let (mut ws_sink, mut ws_stream) = socket.split();

    let cmd: Vec<String> = q.cmd.split_whitespace().map(String::from).collect();

    let exec_id = match docker
        .create_exec(&docker_id, CreateExecOptions::<String> {
            attach_stdin: Some(true),
            attach_stdout: Some(true),
            attach_stderr: Some(true),
            tty: Some(true),
            cmd: Some(cmd),
            ..Default::default()
        })
        .await
    {
        Ok(r) => r.id,
        Err(e) => {
            let _ = ws_sink.send(Message::Text(
                serde_json::json!({"type":"error","message":e.to_string()}).to_string(),
            )).await;
            return;
        }
    };

    let result = match docker
        .start_exec(&exec_id, Some(StartExecOptions { detach: false, tty: true, ..Default::default() }))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            let _ = ws_sink.send(Message::Text(
                serde_json::json!({"type":"error","message":e.to_string()}).to_string(),
            )).await;
            return;
        }
    };

    let (mut stdin, mut output) = match result {
        StartExecResults::Attached { input, output } => (input, output),
        StartExecResults::Detached => {
            let _ = ws_sink.send(Message::Text(
                serde_json::json!({"type":"error","message":"exec started detached"}).to_string(),
            )).await;
            return;
        }
    };

    let _ = docker
        .resize_exec(&exec_id, bollard::exec::ResizeExecOptions { width: q.cols, height: q.rows })
        .await;

    let docker_resize = docker.clone();
    let exec_id_clone = exec_id.clone();

    let mut out_task = tokio::spawn(async move {
        while let Some(item) = output.next().await {
            match item {
                Ok(log) => {
                    let bytes = bytes::Bytes::from(log.into_bytes());
                    if bytes.is_empty() { continue; }
                    if ws_sink.send(Message::Binary(bytes.to_vec())).await.is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
        let _ = ws_sink.close().await;
    });

    let mut in_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_stream.next().await {
            match msg {
                Message::Binary(data) => {
                    if stdin.write_all(&data).await.is_err() { break; }
                }
                Message::Text(text) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if v["type"] == "resize" {
                            let cols = v["cols"].as_u64().unwrap_or(80) as u16;
                            let rows = v["rows"].as_u64().unwrap_or(24) as u16;
                            let _ = docker_resize
                                .resize_exec(&exec_id_clone, bollard::exec::ResizeExecOptions { width: cols, height: rows })
                                .await;
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        drop(stdin);
    });

    tokio::select! {
        _ = &mut out_task => { in_task.abort(); }
        _ = &mut in_task => { out_task.abort(); }
    }
}

async fn spike_loop(cfg: SpikeConfig, docker: Arc<Docker>, http: reqwest::Client) {
    use bollard::container::ListContainersOptions;
    use sysinfo::{Disks, System};
    use std::collections::HashMap;

    let post_url = format!("{}/api/internal/node-agent/spike", cfg.callback_url.trim_end_matches('/'));
    let mut prev_net: HashMap<String, (u64, u64)> = HashMap::new();

    loop {
        let now_ts = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // ── Host metrics (blocking, run on dedicated thread) ─────────────────
        let (cpu_pct, mem_pct, disk_pct) = tokio::task::spawn_blocking(|| {
            let mut sys = System::new_all();
            sys.refresh_all();
            std::thread::sleep(std::time::Duration::from_millis(200));
            sys.refresh_cpu_all();
            sys.refresh_memory();

            let cpu = {
                let cpus = sys.cpus();
                if cpus.is_empty() { 0.0 }
                else { cpus.iter().map(|c| c.cpu_usage() as f64).sum::<f64>() / cpus.len() as f64 }
            };
            let mem = if sys.total_memory() > 0 {
                sys.used_memory() as f64 / sys.total_memory() as f64 * 100.0
            } else { 0.0 };
            let disk = Disks::new_with_refreshed_list()
                .iter()
                .filter(|d| d.mount_point().to_str() == Some("/"))
                .map(|d| {
                    let total = d.total_space();
                    let used  = total.saturating_sub(d.available_space());
                    if total > 0 { used as f64 / total as f64 * 100.0 } else { 0.0 }
                })
                .next()
                .unwrap_or(0.0);
            (cpu, mem, disk)
        }).await.unwrap_or((0.0, 0.0, 0.0));

        let mut spikes: Vec<(String, f64, f64, Option<String>)> = Vec::new();

        if cpu_pct >= cfg.cpu_threshold {
            spikes.push(("cpu".into(), cpu_pct, cfg.cpu_threshold, None));
        }
        if mem_pct >= cfg.mem_threshold {
            spikes.push(("mem".into(), mem_pct, cfg.mem_threshold, None));
        }
        if disk_pct >= cfg.disk_threshold {
            spikes.push(("disk".into(), disk_pct, cfg.disk_threshold, None));
        }

        // ── Per-container network check ───────────────────────────────────────
        let list_opts = ListContainersOptions::<String> { all: false, ..Default::default() };
        if let Ok(containers) = docker.list_containers(Some(list_opts)).await {
            for c in &containers {
                let id = match c.id.as_deref() { Some(id) => id, None => continue };
                let opts = StatsOptions { stream: false, one_shot: true };
                let mut s = docker.stats(id, Some(opts));
                if let Some(Ok(stat)) = s.next().await {
                    let rx = stat.networks.as_ref()
                        .map(|n| n.values().map(|v| v.rx_bytes).sum::<u64>())
                        .unwrap_or(0);
                    let tx = stat.networks.as_ref()
                        .map(|n| n.values().map(|v| v.tx_bytes).sum::<u64>())
                        .unwrap_or(0);

                    if let Some(&(prev_rx, prev_tx)) = prev_net.get(id) {
                        let delta_bytes = (rx.saturating_sub(prev_rx) + tx.saturating_sub(prev_tx)) as f64;
                        let mbps = delta_bytes * 8.0 / 1_000_000.0 / 10.0; // per-second over 10s window
                        if mbps >= cfg.net_threshold_mbps {
                            spikes.push(("net".into(), mbps, cfg.net_threshold_mbps, Some(id.to_string())));
                        }
                    }
                    prev_net.insert(id.to_string(), (rx, tx));
                }
            }
        }

        // ── POST each spike to the main API ──────────────────────────────────
        for (metric, value, threshold, container_id) in spikes {
            let body = serde_json::json!({
                "metric": metric,
                "value": value,
                "threshold": threshold,
                "container_id": container_id,
                "node_id": cfg.node_id,
                "ts": now_ts,
            });
            if let Err(e) = http.post(&post_url)
                .header("x-agent-token", &cfg.token)
                .json(&body)
                .send()
                .await
            {
                tracing::warn!("spike_loop: failed to POST spike to API: {e}");
            }
        }

        tokio::time::sleep(std::time::Duration::from_secs(10)).await;
    }
}
