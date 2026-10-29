//! Shipyard observability core: log types, masking, level detection, the
//! query model, retention rules and the storage traits.
//!
//! No Docker and no HTTP here. The collector (`shipyard-collector`) feeds
//! [`ingest::build_entry`] and writes through a [`store::LogStore`]; the API
//! builds a [`query::LogQuery`] whose scope it forces from the caller.

pub mod ingest;
pub mod mask;
pub mod parse;
pub mod pg;
pub mod query;
pub mod retention;
pub mod stats;
pub mod store;
pub mod types;
pub mod visitor;

pub use ingest::build_entry;
pub use mask::{MaskConfig, Masker, MaskedJson, MaskedText};
pub use pg::{PgLogStore, PgMetricStore};
pub use query::{Cursor, LogPage, LogQuery, LogQueryParams, Scope};
pub use retention::RetentionPolicy;
pub use stats::{LogCountPoint, LogStatsParams, LogStatsQuery, LogStatsResult, StatBucket};
pub use store::{LogStore, MemLogStore, MetricStore, PruneSummary, StoreError};
pub use types::{LogContext, LogEntry, LogLevel, LogSource, StoredLogEntry, Stream};
pub use visitor::hash_visitor;
