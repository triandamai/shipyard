use std::sync::Arc;
use std::convert::Infallible;

use axum::{
    extract::{Path, Query, State, WebSocketUpgrade},
    extract::ws::{Message, WebSocket},
    http::{HeaderMap, Request, StatusCode},
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

    let state = AgentState {
        docker: Arc::new(docker),
        token: Arc::new(token),
    };

    let auth_state = state.clone();
    let app = Router::new()
        .route("/health", get(health))
        .route("/containers/:docker_id/logs", get(container_logs))
        .route("/containers/:docker_id/stats", get(container_stats))
        .route("/containers/:docker_id/exec", get(container_exec))
        .layer(middleware::from_fn_with_state(auth_state, auth_middleware))
        .with_state(state);

    let addr = format!("{bind}:{port}");
    tracing::info!("shipyard-node-agent listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
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
