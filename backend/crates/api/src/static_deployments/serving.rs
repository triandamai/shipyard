use axum::{
    body::Body,
    extract::{Path as AxumPath, Query, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use std::path::Path;
use uuid::Uuid;

use crate::AppState;
use super::models::{StaticAlias, StaticDeployment, StaticProject};
use super::storage::{LocalStorage, StaticStorageBackend};

#[derive(Debug, Clone)]
pub struct ServeOptions {
    pub spa_fallback: bool,
    pub entry_file: String,
    pub is_alias: bool,
}

/// Compute Cache-Control policy according to RFC 9111 and prompt specs:
/// - HTML and mutable alias requests: no-store
/// - Content-hashed immutable assets: 1 year immutable
/// - Non-hashed assets: conservative 1 hour
pub fn compute_cache_control(rel_path: &str, is_alias: bool) -> &'static str {
    if is_alias {
        return "no-cache, no-store, must-revalidate";
    }

    let p = Path::new(rel_path);
    let ext = p.extension().and_then(|s| s.to_str()).unwrap_or("");
    if ext.eq_ignore_ascii_case("html") || ext.eq_ignore_ascii_case("htm") {
        return "no-cache, no-store, must-revalidate";
    }

    let file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
    if is_hashed_asset(file_name) {
        "public, max-age=31536000, immutable"
    } else {
        "public, max-age=3600, must-revalidate"
    }
}

/// Detects common build tool hash patterns:
/// - `app-a1b2c3d4.js` (8+ hex chars after hyphen)
/// - `chunk.8f7e6d5c.css` (8+ hex chars between dots)
/// - `bundle.[hash].js`
fn is_hashed_asset(file_name: &str) -> bool {
    let parts: Vec<&str> = file_name.split(|c| c == '-' || c == '.').collect();
    for part in parts {
        if part.len() >= 8 && part.chars().all(|c| c.is_ascii_hexdigit()) {
            return true;
        }
    }
    false
}

/// Resolves a requested URL path within a deployment.
/// Handles clean URLs (e.g. `/about` -> `/about.html` -> `/about/index.html`) and SPA fallback.
pub async fn resolve_path_in_deployment<S: StaticStorageBackend>(
    storage: &S,
    project_id: Uuid,
    deployment_id: Uuid,
    req_path: &str,
    opts: &ServeOptions,
) -> Option<(Vec<u8>, String, &'static str)> {
    let clean = req_path.trim_matches('/');
    let target = if clean.is_empty() {
        opts.entry_file.clone()
    } else {
        clean.to_string()
    };

    // 1. Direct match
    if let Ok(Some((bytes, mime))) = storage.read_file(project_id, deployment_id, &target).await {
        let cache = compute_cache_control(&target, opts.is_alias);
        return Some((bytes, mime, cache));
    }

    // 2. Try adding .html (e.g. /about -> /about.html)
    let with_html = format!("{target}.html");
    if let Ok(Some((bytes, mime))) = storage.read_file(project_id, deployment_id, &with_html).await {
        let cache = compute_cache_control(&with_html, opts.is_alias);
        return Some((bytes, mime, cache));
    }

    // 3. Try directory index (e.g. /about -> /about/index.html)
    let with_dir_index = format!("{target}/{}", opts.entry_file);
    if let Ok(Some((bytes, mime))) = storage.read_file(project_id, deployment_id, &with_dir_index).await {
        let cache = compute_cache_control(&with_dir_index, opts.is_alias);
        return Some((bytes, mime, cache));
    }

    // 4. SPA fallback (if enabled)
    if opts.spa_fallback {
        if let Ok(Some((bytes, mime))) = storage.read_file(project_id, deployment_id, &opts.entry_file).await {
            let cache = compute_cache_control(&opts.entry_file, opts.is_alias);
            return Some((bytes, mime, cache));
        }
    }

    None
}

// ─── Axum Serving Routes ─────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ServeQuery {
    pub token: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/dpl/:deployment_id", get(serve_deployment_root))
        .route("/dpl/:deployment_id/*path", get(serve_deployment_path))
        .route("/alias/:project_id/:alias", get(serve_alias_root))
        .route("/alias/:project_id/:alias/*path", get(serve_alias_path))
}

async fn serve_deployment_root(
    State(state): State<AppState>,
    AxumPath(deployment_id): AxumPath<Uuid>,
    headers: HeaderMap,
    query: Query<ServeQuery>,
) -> Response {
    serve_deployment_inner(&state, deployment_id, "/", &headers, &query).await
}

async fn serve_deployment_path(
    State(state): State<AppState>,
    AxumPath((deployment_id, path)): AxumPath<(Uuid, String)>,
    headers: HeaderMap,
    query: Query<ServeQuery>,
) -> Response {
    serve_deployment_inner(&state, deployment_id, &format!("/{path}"), &headers, &query).await
}

async fn serve_deployment_inner(
    state: &AppState,
    deployment_id: Uuid,
    req_path: &str,
    headers: &HeaderMap,
    query: &Query<ServeQuery>,
) -> Response {
    let dep: Option<StaticDeployment> = match sqlx::query_as(
        "SELECT * FROM static_deployments WHERE id = $1",
    )
    .bind(deployment_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(d) => d,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };

    let dep = match dep {
        Some(d) if d.typed_status().is_servable() => d,
        _ => return (StatusCode::NOT_FOUND, "Deployment not found or not ready").into_response(),
    };

    // Private preview access control
    if dep.is_private {
        let provided_token = query.token.as_deref().or_else(|| {
            headers
                .get("x-preview-token")
                .and_then(|v| v.to_str().ok())
        });

        match (dep.preview_token.as_deref(), provided_token) {
            (Some(expected), Some(provided)) if expected == provided => {}
            _ => {
                return (
                    StatusCode::UNAUTHORIZED,
                    "Unauthorized: Valid preview token required for this private preview",
                )
                    .into_response();
            }
        }
    }

    let project: Option<StaticProject> = match sqlx::query_as(
        "SELECT * FROM static_projects WHERE id = $1",
    )
    .bind(dep.project_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(p) => p,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };

    let project = match project {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, "Project not found").into_response(),
    };

    let storage = LocalStorage::new(&state.config.data_dir);
    let opts = ServeOptions {
        spa_fallback: project.spa_fallback,
        entry_file: dep.entry_file,
        is_alias: false,
    };

    match resolve_path_in_deployment(&storage, project.id, dep.id, req_path, &opts).await {
        Some((bytes, mime, cache)) => build_file_response(bytes, &mime, cache),
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

async fn serve_alias_root(
    State(state): State<AppState>,
    AxumPath((project_id, alias)): AxumPath<(Uuid, String)>,
    headers: HeaderMap,
    query: Query<ServeQuery>,
) -> Response {
    serve_alias_inner(&state, project_id, &alias, "/", &headers, &query).await
}

async fn serve_alias_path(
    State(state): State<AppState>,
    AxumPath((project_id, alias, path)): AxumPath<(Uuid, String, String)>,
    headers: HeaderMap,
    query: Query<ServeQuery>,
) -> Response {
    serve_alias_inner(&state, project_id, &alias, &format!("/{path}"), &headers, &query).await
}

async fn serve_alias_inner(
    state: &AppState,
    project_id: Uuid,
    alias_name: &str,
    req_path: &str,
    _headers: &HeaderMap,
    _query: &Query<ServeQuery>,
) -> Response {
    let alias_row: Option<StaticAlias> = match sqlx::query_as(
        "SELECT * FROM static_aliases WHERE project_id = $1 AND name = $2",
    )
    .bind(project_id)
    .bind(alias_name)
    .fetch_optional(&state.db)
    .await
    {
        Ok(a) => a,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };

    let alias = match alias_row {
        Some(a) => a,
        None => return (StatusCode::NOT_FOUND, format!("Alias '{alias_name}' not found")).into_response(),
    };

    let dep: Option<StaticDeployment> = match sqlx::query_as(
        "SELECT * FROM static_deployments WHERE id = $1",
    )
    .bind(alias.deployment_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(d) => d,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };

    let dep = match dep {
        Some(d) if d.typed_status().is_servable() => d,
        _ => return (StatusCode::NOT_FOUND, "Deployment pointed to by alias is not ready").into_response(),
    };

    let project: Option<StaticProject> = match sqlx::query_as(
        "SELECT * FROM static_projects WHERE id = $1",
    )
    .bind(project_id)
    .fetch_optional(&state.db)
    .await
    {
        Ok(p) => p,
        Err(_) => return (StatusCode::INTERNAL_SERVER_ERROR, "Database error").into_response(),
    };

    let project = match project {
        Some(p) => p,
        None => return (StatusCode::NOT_FOUND, "Project not found").into_response(),
    };

    let storage = LocalStorage::new(&state.config.data_dir);
    let opts = ServeOptions {
        spa_fallback: project.spa_fallback,
        entry_file: dep.entry_file,
        is_alias: true,
    };

    match resolve_path_in_deployment(&storage, project.id, dep.id, req_path, &opts).await {
        Some((bytes, mime, cache)) => build_file_response(bytes, &mime, cache),
        None => (StatusCode::NOT_FOUND, "404 Not Found").into_response(),
    }
}

fn build_file_response(bytes: Vec<u8>, mime: &str, cache_control: &str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, cache_control),
            (header::HeaderName::from_static("x-content-type-options"), "nosniff"),
            (header::HeaderName::from_static("x-frame-options"), "SAMEORIGIN"),
            (header::HeaderName::from_static("referrer-policy"), "strict-origin-when-cross-origin"),
        ],
        Body::from(bytes),
    )
        .into_response()
}
