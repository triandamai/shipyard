use axum::{
    extract::{DefaultBodyLimit, FromRef, State},
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

/// Upper bound for a single manifest push. Manifests are small JSON documents;
/// 16 MiB is far beyond any real image index while still capping abuse.
const MAX_MANIFEST_BYTES: usize = 16 * 1024 * 1024;

/// Build the OCI registry router.
///
/// Generic over the outer state `S` so this can be merged into any parent router
/// whose state implements `FromRef<S> for RegistryState`.  Callers that own
/// `RegistryState` directly can pass `S = RegistryState`.
///
/// ```text
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
    // Blob upload flow — three-segment name: /:org/:project/:repo.
    // Layer bodies (PATCH chunks and monolithic PUT) can be gigabytes, so
    // Axum's 2 MiB DefaultBodyLimit must be lifted here or `docker push`
    // fails with "413 Payload Too Large: length limit exceeded".
    let blob_uploads = Router::new()
        .route(
            "/v2/:org/:project/:repo/blobs/uploads/",
            post(blobs::initiate_upload),
        )
        .route(
            "/v2/:org/:project/:repo/blobs/uploads/:session",
            patch(blobs::patch_upload)
                .put(blobs::finalise_upload),
        )
        .layer(DefaultBodyLimit::disable());

    // Manifest push — raise the 2 MiB default to a generous manifest ceiling.
    let manifest_routes = Router::new()
        .route(
            "/v2/:org/:project/:repo/manifests/:reference",
            head(manifests::head_manifest)
                .get(manifests::get_manifest)
                .put(manifests::put_manifest),
        )
        .layer(DefaultBodyLimit::max(MAX_MANIFEST_BYTES));

    Router::new()
        // OCI Distribution Spec v1.1 base check
        .route("/v2/", get(oci_base))

        // Token auth endpoint (Docker token auth spec)
        .route("/auth/registry/token", get(auth::issue_token))

        // Blob fetch
        .route(
            "/v2/:org/:project/:repo/blobs/:digest",
            head(blobs::head_blob).get(blobs::get_blob),
        )

        .merge(blob_uploads)
        .merge(manifest_routes)
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
