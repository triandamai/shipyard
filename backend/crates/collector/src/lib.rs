//! Shipyard observability collector:
//! Follows Docker container logs, tails Traefik access logs, and samples resource stats.

pub mod buffer;
pub mod docker_follower;
pub mod stats_sampler;
pub mod traefik_tailer;

pub use buffer::{LogBuffer, DEFAULT_BATCH_SIZE, DEFAULT_CHANNEL_CAPACITY, DEFAULT_FLUSH_INTERVAL};
pub use docker_follower::{extract_metadata, ContainerMetadata, CursorTracker, LogIngestEngine};
pub use stats_sampler::{ContainerResourceMetrics, StatsSampler};
pub use traefik_tailer::{TraefikAccessLogRecord, TraefikTailer};
