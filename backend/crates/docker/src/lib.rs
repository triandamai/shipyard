//! Docker engine abstraction over bollard.
//!
//! Provides a clean async trait (`DockerEngine`) wrapping bollard's Docker client
//! for swarm service management, image operations, and event streaming.

pub mod engine;
pub mod types;

pub use engine::BollardDockerEngine;
pub use types::*;

/// Make a raw HTTP/1.1 request over the Docker Unix socket.
/// Exposed for callers that need to query Docker API endpoints not covered by the trait.
pub async fn raw_request(
    method: &str,
    path: &str,
    body: Option<&str>,
) -> shipyard_common::error::AppResult<String> {
    engine::BollardDockerEngine::raw_unix_request_pub(method, path, body).await
}
