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
//! # async fn example(engine: Arc<dyn shipyard_docker::DockerEngine>, db: sqlx::PgPool, mqtt: Arc<shipyard_mqtt::publisher::MqttPublisher>) -> shipyard_common::error::AppResult<()> {
//! let worker = DockerEventWorker::new(engine, db, mqtt, "platform")?;
//! worker.reconcile_on_startup().await?;
//! worker.run().await?;
//! # Ok(())
//! # }
//! ```

pub mod worker;

pub use worker::DockerEventWorker;
