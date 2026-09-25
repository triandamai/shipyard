use axum::{
    extract::ws::{Message, WebSocket},
    extract::{Path, Query, State, WebSocketUpgrade},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use futures::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use shipyard_common::types::ApiResponse;

use crate::auth::{decode_token, AuthUser};
use crate::error::ApiAppError;
use crate::middleware::rbac::require_service_access;
use crate::AppState;

use super::manager::fetch_instance;

fn default_cmd() -> String { "/bin/sh".to_string() }
fn default_cols() -> u16 { 80 }
fn default_rows() -> u16 { 24 }

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/apps/:service_id/exec/token", post(exec_token))
        .route("/apps/:service_id/exec", get(exec_ws))
}

/// POST /apps/:service_id/exec/token — mints a 5-minute JWT for the exec
/// WebSocket's `?token=` query param, mirroring services::exec_token.
async fn exec_token(
    auth: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth.user_id, service_id).await.map_err(ApiAppError)?;

    let token = crate::auth::create_access_token(
        auth.user_id,
        &auth.email,
        &state.config.auth.jwt_secret,
        300,
        vec![],
    )
    .map_err(ApiAppError)?;

    Ok(Json(ApiResponse::ok(serde_json::json!({ "token": token }))))
}

#[derive(Debug, Deserialize)]
struct ExecParams {
    token: String,
    #[serde(default = "default_cmd")]
    cmd: String,
    #[serde(default = "default_cols")]
    cols: u16,
    #[serde(default = "default_rows")]
    rows: u16,
}

/// GET /apps/:service_id/exec (WebSocket upgrade) — same binary/JSON framing
/// as services::exec_ws: binary frames are raw PTY bytes both directions,
/// text frames are JSON control messages ({"type":"resize",...} in,
/// {"type":"error",...} out).
async fn exec_ws(
    State(state): State<AppState>,
    Path(service_id): Path<Uuid>,
    Query(params): Query<ExecParams>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    // Validate token before upgrading — reject invalid creds before WS handshake.
    let claims = match decode_token(&params.token, &state.config.auth.jwt_secret) {
        Ok(c) => c,
        Err(_) => return axum::http::StatusCode::UNAUTHORIZED.into_response(),
    };
    let user_id = Uuid::parse_str(&claims.sub).unwrap_or_default();

    // Authorization: a valid token proves "some authenticated user," not that
    // this user may access THIS service — check before upgrading, mirroring
    // exec_token's own require_service_access call.
    if require_service_access(&state.db, user_id, service_id).await.is_err() {
        return axum::http::StatusCode::FORBIDDEN.into_response();
    }

    ws.on_upgrade(move |socket| handle_exec_socket(socket, state, service_id, params, user_id))
}

async fn handle_exec_socket(
    socket: WebSocket,
    state: AppState,
    service_id: Uuid,
    params: ExecParams,
    user_id: Uuid,
) {
    let instance = match fetch_instance(&state.db, service_id).await {
        Ok(Some(i)) if i.status == "running" => i,
        _ => {
            let mut socket = socket;
            let _ = socket.send(Message::Text(
                serde_json::json!({ "type": "error", "message": "Sandbox is not running" }).to_string(),
            )).await;
            let _ = socket.close().await;
            return;
        }
    };
    let Some(container_id) = instance.container_id else {
        let mut socket = socket;
        let _ = socket.send(Message::Text(
            serde_json::json!({ "type": "error", "message": "Sandbox has no container" }).to_string(),
        )).await;
        let _ = socket.close().await;
        return;
    };

    let cmd: Vec<String> = params.cmd.split_whitespace().map(String::from).collect();
    let exec_handle = match state.docker.exec_container(&container_id, cmd, params.cols, params.rows).await {
        Ok(h) => h,
        Err(e) => {
            let mut socket = socket;
            let _ = socket.send(Message::Text(
                serde_json::json!({ "type": "error", "message": format!("exec failed: {e}") }).to_string(),
            )).await;
            let _ = socket.close().await;
            return;
        }
    };

    crate::middleware::audit::write_audit_log_user(
        &state.db, user_id, "", "exec_sandbox", Some("service"), Some(service_id), None, None,
    ).await;

    let (mut ws_sink, mut ws_stream) = socket.split();
    let exec_id = exec_handle.exec_id.clone();
    let mut stdin = exec_handle.stdin;
    let mut output = exec_handle.output;
    let docker = state.docker.clone();

    let mut out_task = tokio::spawn(async move {
        while let Some(chunk) = output.next().await {
            if chunk.is_empty() { continue; }
            if ws_sink.send(Message::Binary(chunk.to_vec())).await.is_err() {
                break;
            }
        }
        let _ = ws_sink.close().await;
    });

    let mut in_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = ws_stream.next().await {
            match msg {
                Message::Binary(data) => {
                    if stdin.write_all(&data).await.is_err() {
                        break;
                    }
                }
                Message::Text(text) => {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) {
                        if v["type"] == "resize" {
                            let cols = v["cols"].as_u64().unwrap_or(80) as u16;
                            let rows = v["rows"].as_u64().unwrap_or(24) as u16;
                            let _ = docker.resize_exec(&exec_id, cols, rows).await;
                        }
                    }
                }
                Message::Close(_) => break,
                _ => {}
            }
        }
        // Dropping stdin signals EOF to the container process.
        drop(stdin);
    });

    // Drive both tasks; cancel the other when one finishes.
    tokio::select! {
        _ = &mut out_task => {
            in_task.abort();
        }
        _ = &mut in_task => {
            out_task.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_services_exec_convention() {
        assert_eq!(default_cmd(), "/bin/sh");
        assert_eq!(default_cols(), 80);
        assert_eq!(default_rows(), 24);
    }
}
