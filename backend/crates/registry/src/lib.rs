pub mod error;
pub mod storage;
pub mod blobs;
pub mod manifests;
pub mod kinds;
pub mod auth;
pub mod router;
pub mod push;

use std::sync::Arc;
use storage::StorageBackend;

/// Shared state injected into every registry route handler.
#[derive(Clone)]
pub struct RegistryState {
    pub db:      sqlx::PgPool,
    pub storage: Arc<dyn StorageBackend>,
    /// Hostname advertised in Www-Authenticate challenges, e.g. "registry.shipyard.local"
    pub hostname: String,
    /// JWT secret used to sign and verify registry tokens (same key as the main API).
    pub jwt_secret: String,
}

