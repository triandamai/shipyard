use axum::{
    body::Bytes,
    extract::{OriginalUri, Path, State},
    http::{HeaderMap, Method, StatusCode},
    response::{IntoResponse, Response},
    routing::any,
    Router,
};
use uuid::Uuid;

use crate::AppState;
use super::manager;

/// Public router mounted at `/fn` on the top-level app (not under `/api`).
/// No authentication — these are open function invocations.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/:org_slug/*fn_path", any(invoke))
        .route("/:org_slug", any(invoke_no_path))
}

// Hop-by-hop headers must never be forwarded.
const HOP_BY_HOP: &[&str] = &[
    "connection", "keep-alive", "te", "trailer",
    "transfer-encoding", "upgrade", "proxy-connection",
];

async fn invoke(
    State(state): State<AppState>,
    Path((org_slug, fn_path)): Path<(String, String)>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    proxy(&state, &org_slug, &format!("/{fn_path}{query}"), method, headers, body).await
}

async fn invoke_no_path(
    State(state): State<AppState>,
    Path(org_slug): Path<String>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let query = uri.query().map(|q| format!("?{q}")).unwrap_or_default();
    proxy(&state, &org_slug, &format!("/{query}"), method, headers, body).await
}

async fn proxy(
    state: &AppState,
    org_slug: &str,
    runtime_path: &str,
    method: Method,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    // 1. Resolve org_id from slug.
    let org_id: Option<Uuid> =
        sqlx::query_scalar("SELECT id FROM organizations WHERE slug = $1")
            .bind(org_slug)
            .fetch_optional(&state.db)
            .await
            .ok()
            .flatten();

    let org_id = match org_id {
        Some(id) => id,
        None => {
            return (StatusCode::NOT_FOUND, format!("org '{org_slug}' not found"))
                .into_response()
        }
    };

    // 2. Resolve the org's edge runtime URL (Swarm DNS).
    let runtime_url = match manager::runtime_url_for_org(state, org_id).await {
        Some(url) => url,
        None => {
            return (
                StatusCode::BAD_GATEWAY,
                "edge runtime is not running for this org",
            )
                .into_response()
        }
    };

    // 3. Forward the request, stripping hop-by-hop headers.
    let target = format!("{runtime_url}{runtime_path}");
    let rm = reqwest::Method::from_bytes(method.as_str().as_bytes())
        .unwrap_or(reqwest::Method::GET);
    let mut req = state.http_client.request(rm, &target);

    for (key, value) in &headers {
        let name = key.as_str();
        if name.eq_ignore_ascii_case("host") { continue; }
        if HOP_BY_HOP.iter().any(|h| h.eq_ignore_ascii_case(name)) { continue; }
        if let Ok(v) = value.to_str() {
            req = req.header(name, v);
        }
    }

    if !body.is_empty() {
        req = req.body(body.to_vec());
    }

    // 4. Stream the response back.
    match req.send().await {
        Ok(res) => {
            let status = StatusCode::from_u16(res.status().as_u16())
                .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);

            let mut out_headers = HeaderMap::new();
            for (k, v) in res.headers() {
                if HOP_BY_HOP.iter().any(|h| h.eq_ignore_ascii_case(k.as_str())) {
                    continue;
                }
                out_headers.insert(k.clone(), v.clone());
            }

            let bytes = res.bytes().await.unwrap_or_default();
            let mut response = Response::new(axum::body::Body::from(bytes));
            *response.status_mut() = status;
            *response.headers_mut() = out_headers;
            response
        }
        Err(e) => {
            tracing::warn!(org_slug, target, "edge fn invoke error: {e}");
            (StatusCode::BAD_GATEWAY, format!("runtime unreachable: {e}")).into_response()
        }
    }
}
