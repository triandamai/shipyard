use axum::{
    extract::{FromRef, State},
    http::{StatusCode, HeaderMap},
    response::{IntoResponse, Response},
    routing::{get, head, patch, post},
    Router,
};

use crate::{
    auth,
    blobs,
    manifests,
    RegistryState,
};

/// Build the OCI registry router.
///
/// Generic over the outer state `S` so this can be merged into any parent router
/// whose state implements `FromRef<S> for RegistryState`.  Callers that own
/// `RegistryState` directly can pass `S = RegistryState`.
///
/// ```rust
/// // In the API crate (where RegistryState: FromRef<AppState>):
/// let app = Router::new()
///     .merge(registry_router::<AppState>())
///     .with_state(app_state);
/// ```
pub fn registry_router<S>() -> Router<S>
where
    RegistryState: FromRef<S>,
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        // OCI Distribution Spec v1.1 base check
        .route("/v2/", get(oci_base))

        // Token auth endpoint (Docker token auth spec)
        .route("/auth/registry/token", get(auth::issue_token))

        // Blob upload flow  — three-segment name: /:org/:project/:repo
        .route(
            "/v2/:org/:project/:repo/blobs/uploads/",
            post(blobs::initiate_upload),
        )
        .route(
            "/v2/:org/:project/:repo/blobs/uploads/:session",
            patch(blobs::patch_upload)
                .put(blobs::finalise_upload),
        )

        // Blob fetch
        .route(
            "/v2/:org/:project/:repo/blobs/:digest",
            head(blobs::head_blob).get(blobs::get_blob),
        )

        // Manifest push / pull
        .route(
            "/v2/:org/:project/:repo/manifests/:reference",
            head(manifests::head_manifest)
                .get(manifests::get_manifest)
                .put(manifests::put_manifest),
        )
}

/// GET /v2/ — OCI base check.
/// Returns 200 if the registry is healthy and auth is not required for this endpoint,
/// or 401 with Www-Authenticate if a token is needed.
async fn oci_base(
    State(state): State<RegistryState>,
    headers: HeaderMap,
) -> Response {
    // If an Authorization header is present, accept immediately.
    if headers.contains_key("Authorization") {
        return StatusCode::OK.into_response();
    }

    // Challenge unauthenticated clients with the token auth endpoint.
    let challenge = format!(
        r#"Bearer realm="https://{}/auth/registry/token",service="{}""#,
        state.hostname, state.hostname,
    );

    (
        StatusCode::UNAUTHORIZED,
        [(
            "Www-Authenticate",
            challenge,
        )],
    ).into_response()
}
