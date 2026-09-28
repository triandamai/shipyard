use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, Query, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::get,
    Router,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use uuid::Uuid;

use shipyard_docker::types::LogOpts;

use crate::auth::decode_token;
use crate::middleware::rbac::require_service_access;
use crate::AppState;

use super::manager::fetch_instance;

fn default_tail() -> String { "200".to_string() }

pub fn routes() -> Router<AppState> {
    Router::new().route("/apps/:service_id/logs", get(logs_ws))
}

#[derive(Debug, Deserialize)]
struct LogsParams {
    token: String,
    #[serde(default = "default_tail")]
    tail: String,
}

/// GET /apps/:service_id/logs (WebSocket upgrade) — a read-only tail of the
/// sandbox's dev server process, i.e. exactly what `install_cmd && dev_cmd`
/// prints to stdout/stderr (the container's own PID 1 output). Text frames
/// out only; there is no input direction, unlike `exec_ws`'s interactive
/// PTY. Reuses the same short-lived token minted by
/// `POST /apps/:service_id/exec/token` — the token only proves "some
/// authenticated user," so authorization is re-checked against this
/// specific service_id below, mirroring `exec_ws` exactly.
async fn logs_ws(
    State(state): State<AppState>,
    Path(service_id): Path<Uuid>,
    Query(params): Query<LogsParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let claims = match decode_token(&params.token, &state.config.auth.jwt_secret) {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };
    let user_id = Uuid::parse_str(&claims.sub).unwrap_or_default();

    if require_service_access(&state.db, user_id, service_id).await.is_err() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    ws.on_upgrade(move |socket| handle_logs_socket(socket, state, service_id, params))
}

async fn handle_logs_socket(socket: WebSocket, state: AppState, service_id: Uuid, params: LogsParams) {
    // 'starting' is accepted (not just 'running') so the waiting screen can
    // show live dev server output while finalize_sandbox_start's readiness
    // check is still polling — exactly when a slow/failing boot is most
    // useful to see.
    let instance = match fetch_instance(&state.db, service_id).await {
        Ok(Some(i)) if i.status == "running" || i.status == "starting" => i,
        _ => {
            let mut socket = socket;
            let _ = socket
                .send(Message::Text(
                    serde_json::json!({ "type": "error", "message": "Sandbox is not running" }).to_string(),
                ))
                .await;
            let _ = socket.close().await;
            return;
        }
    };
    let Some(container_id) = instance.container_id else {
        let mut socket = socket;
        let _ = socket
            .send(Message::Text(
                serde_json::json!({ "type": "error", "message": "Sandbox has no container" }).to_string(),
            ))
            .await;
        let _ = socket.close().await;
        return;
    };

    let (mut ws_sink, mut ws_stream) = socket.split();
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(256);

    let docker = state.docker.clone();
    let opts = LogOpts {
        follow: true,
        stdout: true,
        stderr: true,
        tail: Some(params.tail.clone()),
        ..Default::default()
    };
    let mut stream_task = tokio::spawn(async move {
        let _ = docker.container_logs_stream(&container_id, opts, tx).await;
    });

    let mut out_task = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            if ws_sink.send(Message::Text(line)).await.is_err() {
                break;
            }
        }
        let _ = ws_sink.close().await;
    });

    // Read-only: drain incoming frames purely to detect the client closing
    // the connection (or an idle-timeout keep-alive ping), never acting on
    // their content.
    let mut in_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_stream.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
    });

    tokio::select! {
        _ = &mut out_task => { in_task.abort(); stream_task.abort(); }
        _ = &mut in_task => { out_task.abort(); stream_task.abort(); }
        _ = &mut stream_task => { out_task.abort(); in_task.abort(); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tail_matches_docs_convention() {
        assert_eq!(default_tail(), "200");
    }
}
