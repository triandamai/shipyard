use std::sync::Arc;
use std::time::Instant;
use chrono::{Duration, Utc};
use uuid::Uuid;

use shipyard_observability::{
    build_entry, LogContext, LogPage, LogQuery, LogQueryParams, LogSource, LogStore, Masker,
    MemLogStore, Scope, Stream,
};

#[tokio::test]
async fn sustained_ingest_and_query_load_test() {
    let store = Arc::new(MemLogStore::new());
    let masker = Masker::default();

    let org_id = Uuid::new_v4();
    let project_id = Uuid::new_v4();
    let service_id = Uuid::new_v4();

    let ctx = LogContext {
        org_id: Some(org_id),
        project_id: Some(project_id),
        service_id: Some(service_id),
        deployment_id: Some(Uuid::new_v4()),
        source: Some(LogSource::App),
        container_id: Some("container_load_1".into()),
        replica: Some(1),
    };

    // 1. Ingest throughput test: 10,000 lines across 90 simulated days
    let count = 10_000;
    let now = Utc::now();
    let mut entries = Vec::with_capacity(count);

    let start_build = Instant::now();
    for i in 0..count {
        // Distribute timestamps over the last 90 days
        let mins_ago = (i as i64 * 90 * 24 * 60) / count as i64;
        let ts = now - Duration::minutes(mins_ago);

        let raw = if i % 20 == 0 {
            format!(r#"{{"level":"error","msg":"database timeout on step {i}","trace_id":"4bf92f3577b34da6a3ce929d0e0e{:04x}"}}"#, i % 1000)
        } else if i % 5 == 0 {
            format!("WARN: high latency on /api/v1/items user=usr_{i} token=ship_int_secretkey{:012x}", i)
        } else {
            format!("INFO: handled request id={i} status=200 duration=12ms")
        };

        let entry = build_entry(&ctx, Stream::Stdout, ts, &raw, &masker);
        entries.push(entry);
    }
    let build_elapsed = start_build.elapsed();

    // Write to store in batches of 500
    let start_write = Instant::now();
    for chunk in entries.chunks(500) {
        store.write_entries(chunk).await.unwrap();
    }
    let write_elapsed = start_write.elapsed();

    let total_ingest_elapsed = build_elapsed + write_elapsed;
    let lines_per_sec = (count as f64) / total_ingest_elapsed.as_secs_f64();

    println!(
        "Ingest load test: processed {} lines in {:.2?} ({:.0} lines/s)",
        count, total_ingest_elapsed, lines_per_sec
    );
    // Requirement: >= 2,000 lines/sec sustained
    assert!(
        lines_per_sec >= 2_000.0,
        "Ingest throughput ({:.0} lines/s) must meet or exceed 2000 lines/s",
        lines_per_sec
    );

    // 2. Query latency over 90-day span
    let query_start = Instant::now();
    let query = LogQuery::new(
        Scope::Org(org_id),
        LogQueryParams {
            from: Some(now - Duration::days(90)),
            to: Some(now),
            level: Some("error".into()),
            q: Some("timeout".into()),
            limit: Some(200),
            ..Default::default()
        },
        now,
    )
    .unwrap();

    let page: LogPage = store.query_logs(&query).await.unwrap();
    let query_elapsed = query_start.elapsed();

    println!(
        "90-day search test: returned {} entries in {:.2?}",
        page.entries.len(),
        query_elapsed
    );

    // Requirement: search over 90d p95 < 1.5s
    assert!(
        query_elapsed.as_secs_f64() < 1.5,
        "Query latency ({:.2?}) must be under 1.5 seconds",
        query_elapsed
    );
    assert!(!page.entries.is_empty(), "Should have matched error lines");
}
