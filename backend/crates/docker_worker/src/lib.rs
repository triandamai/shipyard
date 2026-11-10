//! Docker Event Worker
//!
//! Long-running background task that subscribes to the Docker socket event
//! stream via bollard, reconciles container/service state into Postgres, and
//! publishes real-time MQTT events.
//!
//! # Usage
//!
//! ```rust,no_run
//! use std::sync::Arc;
//! use shipyard_docker_worker::DockerEventWorker;
//!
//! # async fn example(engine: Arc<dyn shipyard_docker::engine::DockerEngine>, db: sqlx::PgPool, mqtt: Arc<shipyard_mqtt::MqttPublisher>) -> shipyard_common::error::AppResult<()> {
//! let worker = DockerEventWorker::new(engine, db, mqtt, "platform")?;
//! worker.reconcile_on_startup().await?;
//! worker.run().await?;
//! # Ok(())
//! # }
//! ```

pub mod orphans;
pub mod worker;

pub use orphans::{
    auto_destruct_expired_orphans, cleanup_unused_registry_artifacts, delete_orphan,
    get_cleanup_settings, get_last_docker_image_prune_at, is_docker_image_prune_due, list_orphans,
    prune_unused_docker_images_if_due, record_docker_image_prune, save_cleanup_settings,
    scan_and_reconcile_orphans, scan_unused_registry_artifacts, CleanupSettings, OrphanResource,
    OrphanType, RegistryCleanupReport,
};
pub use worker::DockerEventWorker;
