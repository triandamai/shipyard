//! Bounded batching pipeline for log entries with backpressure protection.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use shipyard_observability::{LogEntry, LogStore};

pub const DEFAULT_CHANNEL_CAPACITY: usize = 10_000;
pub const DEFAULT_BATCH_SIZE: usize = 500;
pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(1000);

pub struct LogBuffer {
    tx: mpsc::Sender<LogEntry>,
    dropped_count: Arc<AtomicU64>,
}

impl LogBuffer {
    pub fn new(
        store: Arc<dyn LogStore>,
        capacity: usize,
        batch_size: usize,
        flush_interval: Duration,
    ) -> (Self, tokio::task::JoinHandle<()>) {
        let (tx, mut rx) = mpsc::channel(capacity);
        let dropped_count = Arc::new(AtomicU64::new(0));

        let handle = tokio::spawn(async move {
            let mut batch = Vec::with_capacity(batch_size);
            let mut interval = tokio::time::interval(flush_interval);

            loop {
                tokio::select! {
                    Some(entry) = rx.recv() => {
                        batch.push(entry);
                        if batch.len() >= batch_size {
                            if let Err(e) = store.write_entries(&batch).await {
                                tracing::error!("Failed to flush log batch to store: {e}");
                            }
                            batch.clear();
                        }
                    }
                    _ = interval.tick() => {
                        if !batch.is_empty() {
                            if let Err(e) = store.write_entries(&batch).await {
                                tracing::error!("Failed to flush log batch on interval: {e}");
                            }
                            batch.clear();
                        }
                    }
                    else => {
                        // Channel closed
                        if !batch.is_empty() {
                            store.write_entries(&batch).await.ok();
                        }
                        break;
                    }
                }
            }
        });

        (
            Self {
                tx,
                dropped_count,
            },
            handle,
        )
    }

    /// Try pushing an entry into the buffer. If the channel is full, drop the entry
    /// and increment the dropped count (backpressure protection).
    pub fn push(&self, entry: LogEntry) -> bool {
        match self.tx.try_send(entry) {
            Ok(()) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.dropped_count.fetch_add(1, Ordering::Relaxed);
                false
            }
            Err(mpsc::error::TrySendError::Closed(_)) => false,
        }
    }

    pub fn dropped_count(&self) -> u64 {
        self.dropped_count.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tokio::time::sleep;
    use shipyard_observability::{
        build_entry, LogContext, LogSource, Masker, MemLogStore, Stream,
    };

    #[tokio::test]
    async fn flushes_on_batch_size_and_interval() {
        let store = Arc::new(MemLogStore::new());
        let (buf, _handle) = LogBuffer::new(
            store.clone(),
            100,
            3,
            Duration::from_millis(100),
        );

        let masker = Masker::default();
        let ctx = LogContext {
            source: Some(LogSource::App),
            ..Default::default()
        };

        let e1 = build_entry(&ctx, Stream::Stdout, Utc::now(), "msg 1", &masker);
        let e2 = build_entry(&ctx, Stream::Stdout, Utc::now(), "msg 2", &masker);
        let e3 = build_entry(&ctx, Stream::Stdout, Utc::now(), "msg 3", &masker);

        assert!(buf.push(e1));
        assert!(buf.push(e2));
        assert!(buf.push(e3)); // Triggers batch flush

        // Allow task to process
        sleep(Duration::from_millis(50)).await;
        assert_eq!(store.len().await, 3);
        assert_eq!(buf.dropped_count(), 0);

        // Test interval flush with single item
        let e4 = build_entry(&ctx, Stream::Stdout, Utc::now(), "msg 4", &masker);
        assert!(buf.push(e4));
        sleep(Duration::from_millis(150)).await;
        assert_eq!(store.len().await, 4);
    }

    #[tokio::test]
    async fn drops_and_counts_when_buffer_full() {
        let store = Arc::new(MemLogStore::new());
        // Channel capacity 1, batch size 10, long interval
        let (buf, _handle) = LogBuffer::new(
            store.clone(),
            1,
            10,
            Duration::from_secs(10),
        );

        let masker = Masker::default();
        let ctx = LogContext::default();

        let e1 = build_entry(&ctx, Stream::Stdout, Utc::now(), "m1", &masker);
        let e2 = build_entry(&ctx, Stream::Stdout, Utc::now(), "m2", &masker);
        let e3 = build_entry(&ctx, Stream::Stdout, Utc::now(), "m3", &masker);

        buf.push(e1);
        let p2 = buf.push(e2);
        let p3 = buf.push(e3);

        assert!(!p2 || !p3, "At least one should be dropped when channel is full");
        assert!(buf.dropped_count() >= 1);
    }
}
