//! Metrics integration API for Nomi Monitor sheet:
//! Bucketed traffic (visitors, requests, 4xx/5xx errors, p50/p95 latency) and
//! container resource history (CPU %, memory %, memory bytes/limits).

use axum::{
    extract::{Path, Query, State},
    routing::get,
    Json, Router,
};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use super::{
    app_ref, db_err, env_project, IntegrationCaller, SCOPE_MANAGE,
};
use crate::error::ApiAppError;
use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route(
        "/orgs/:org/apps/:app/envs/:env/metrics",
        get(get_environment_metrics),
    )
}

#[derive(Debug, Deserialize)]
pub struct MetricsQuery {
    pub range: Option<String>,
    pub step: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricsResponse {
    pub range: String,
    pub step: String,
    pub buckets: Vec<MetricBucket>,
    pub totals: MetricTotals,
    pub previous_totals: MetricTotals,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricBucket {
    pub at: String,
    pub visitors: Option<u64>,
    pub requests: Option<u64>,
    pub errors_5xx: Option<u64>,
    pub errors_4xx: Option<u64>,
    pub p50_ms: Option<u64>,
    pub p95_ms: Option<u64>,
    pub containers: Vec<ContainerMetricBucket>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ContainerMetricBucket {
    pub service: String,
    pub container_id: String,
    pub cpu_percent: Option<f64>,
    pub memory_percent: Option<f64>,
    pub memory_bytes: Option<u64>,
    pub memory_limit_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricTotals {
    pub visitors: Option<u64>,
    pub requests: Option<u64>,
    pub errors_5xx: Option<u64>,
    pub errors_4xx: Option<u64>,
    pub p50_ms: Option<u64>,
    pub p95_ms: Option<u64>,
}

/// Parse duration strings like "1m", "5m", "1h", "24h", "7d".
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("duration cannot be empty".into());
    }
    let (num_part, unit_part) = s.split_at(s.len() - 1);
    let num: i64 = num_part
        .parse()
        .map_err(|_| format!("invalid duration number in '{s}'"))?;
    if num <= 0 {
        return Err("duration must be greater than zero".into());
    }
    match unit_part {
        "m" => Ok(Duration::minutes(num)),
        "h" => Ok(Duration::hours(num)),
        "d" => Ok(Duration::days(num)),
        _ => Err(format!("unknown duration unit in '{s}' (expected m, h, or d)")),
    }
}

/// Parse range and step, providing Nomi-compatible defaults:
/// - 1h -> 1m
/// - 24h -> 1h
/// - 7d -> 1d
pub fn parse_range_and_step(
    range_str: Option<&str>,
    step_str: Option<&str>,
) -> Result<(Duration, Duration, String, String), String> {
    let range_s = range_str.unwrap_or("24h");
    let range_dur = parse_duration(range_s)?;

    let (step_dur, step_s) = if let Some(st) = step_str {
        (parse_duration(st)?, st.to_string())
    } else if range_dur <= Duration::hours(1) {
        (Duration::minutes(1), "1m".to_string())
    } else if range_dur <= Duration::hours(24) {
        (Duration::hours(1), "1h".to_string())
    } else {
        (Duration::days(1), "1d".to_string())
    };

    if step_dur > range_dur {
        return Err("step cannot be larger than range".into());
    }

    Ok((range_dur, step_dur, range_s.to_string(), step_s))
}

/// Calculate percentile (e.g. 0.50 for p50, 0.95 for p95) from a sorted slice.
pub fn compute_percentile(sorted_asc: &[u64], pct: f64) -> Option<u64> {
    if sorted_asc.is_empty() {
        return None;
    }
    let n = sorted_asc.len();
    if n == 1 {
        return Some(sorted_asc[0]);
    }
    let rank = pct * (n - 1) as f64;
    let lower_idx = rank.floor() as usize;
    let upper_idx = rank.ceil() as usize;
    if lower_idx == upper_idx {
        return Some(sorted_asc[lower_idx]);
    }
    let weight = rank - lower_idx as f64;
    let interpolated =
        sorted_asc[lower_idx] as f64 * (1.0 - weight) + sorted_asc[upper_idx] as f64 * weight;
    Some(interpolated.round() as u64)
}

/// Privacy-preserving visitor hash: daily-rotating SHA-256 hash of IP + user agent.
pub fn generate_visitor_hash(ip: &str, user_agent: &str, date_str: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(date_str.as_bytes());
    hasher.update(b":");
    hasher.update(ip.trim().as_bytes());
    hasher.update(b":");
    hasher.update(user_agent.trim().as_bytes());
    hex::encode(hasher.finalize())
}

/// Generate discrete bucket start timestamps covering [start, end] at step intervals.
pub fn generate_bucket_timestamps(
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    step: Duration,
) -> Vec<DateTime<Utc>> {
    let mut buckets = vec![];
    let step_secs = step.num_seconds().max(1);
    // Align start to step boundary
    let start_ts = (start.timestamp() / step_secs) * step_secs;
    let mut cur = DateTime::from_timestamp(start_ts, 0).unwrap_or(start);
    while cur <= end {
        buckets.push(cur);
        cur = cur + step;
    }
    buckets
}

/// GET /openapi/v1/integration/orgs/:org/apps/:app/envs/:env/metrics
pub async fn get_environment_metrics(
    caller: IntegrationCaller,
    Path((org, app_key, env)): Path<(String, String, String)>,
    Query(q): Query<MetricsQuery>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<MetricsResponse>>, ApiAppError> {
    caller.require(SCOPE_MANAGE)?;
    let app = app_ref(&state.db, &caller, &org, &app_key).await?;
    let project_id = env_project(&state.db, &app, &env).await?;

    let (range_dur, step_dur, range_str, step_str) =
        parse_range_and_step(q.range.as_deref(), q.step.as_deref())
            .map_err(|e| ApiAppError(AppError::BadRequest(e)))?;

    let now = Utc::now();
    let current_start = now - range_dur;
    let previous_start = current_start - range_dur;
    let previous_end = current_start;

    let response = fetch_metrics(
        &state.db,
        project_id,
        current_start,
        now,
        previous_start,
        previous_end,
        step_dur,
        range_str,
        step_str,
    )
    .await
    .map_err(db_err)?;

    Ok(Json(ApiResponse::ok(response)))
}

#[derive(sqlx::FromRow)]
struct DbTrafficBucket {
    bucket_time: DateTime<Utc>,
    visitors: i64,
    requests: i64,
    errors_5xx: i64,
    errors_4xx: i64,
    p50_ms: Option<f64>,
    p95_ms: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct DbTrafficTotals {
    visitors: i64,
    requests: i64,
    errors_5xx: i64,
    errors_4xx: i64,
    p50_ms: Option<f64>,
    p95_ms: Option<f64>,
}

#[derive(sqlx::FromRow)]
struct DbResourceBucket {
    bucket_time: DateTime<Utc>,
    service_slug: String,
    container_id: String,
    cpu_percent: Option<f64>,
    memory_percent: Option<f64>,
    memory_bytes: Option<i64>,
    memory_limit_bytes: Option<i64>,
}

pub async fn fetch_metrics(
    db: &sqlx::PgPool,
    project_id: Uuid,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    prev_start: DateTime<Utc>,
    prev_end: DateTime<Utc>,
    step: Duration,
    range_str: String,
    step_str: String,
) -> Result<MetricsResponse, sqlx::Error> {
    let step_secs = step.num_seconds().max(1);
    let step_interval = format!("{step_secs} seconds");
    let origin = DateTime::from_timestamp(0, 0).unwrap();

    // 1. Current range traffic totals
    let cur_totals_row: Option<DbTrafficTotals> = sqlx::query_as(
        r#"
        SELECT
            COUNT(DISTINCT visitor_hash) AS visitors,
            COUNT(*) AS requests,
            COUNT(*) FILTER (WHERE status_code >= 500) AS errors_5xx,
            COUNT(*) FILTER (WHERE status_code >= 400 AND status_code < 500) AS errors_4xx,
            percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms) AS p50_ms,
            percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95_ms
        FROM environment_traffic_samples
        WHERE project_id = $1 AND recorded_at >= $2 AND recorded_at <= $3
        "#,
    )
    .bind(project_id)
    .bind(start)
    .bind(end)
    .fetch_optional(db)
    .await?;

    // Check if any traffic samples exist at all for this project
    let has_any_traffic: bool = sqlx::query_scalar(
        "SELECT EXISTS (SELECT 1 FROM environment_traffic_samples WHERE project_id = $1)",
    )
    .bind(project_id)
    .fetch_one(db)
    .await?;

    let totals = if !has_any_traffic {
        MetricTotals {
            visitors: None,
            requests: None,
            errors_5xx: None,
            errors_4xx: None,
            p50_ms: None,
            p95_ms: None,
        }
    } else if let Some(t) = cur_totals_row {
        MetricTotals {
            visitors: Some(t.visitors as u64),
            requests: Some(t.requests as u64),
            errors_5xx: Some(t.errors_5xx as u64),
            errors_4xx: Some(t.errors_4xx as u64),
            p50_ms: t.p50_ms.map(|v| v.round() as u64),
            p95_ms: t.p95_ms.map(|v| v.round() as u64),
        }
    } else {
        MetricTotals {
            visitors: Some(0),
            requests: Some(0),
            errors_5xx: Some(0),
            errors_4xx: Some(0),
            p50_ms: None,
            p95_ms: None,
        }
    };

    // 2. Previous range traffic totals
    let prev_totals_row: Option<DbTrafficTotals> = sqlx::query_as(
        r#"
        SELECT
            COUNT(DISTINCT visitor_hash) AS visitors,
            COUNT(*) AS requests,
            COUNT(*) FILTER (WHERE status_code >= 500) AS errors_5xx,
            COUNT(*) FILTER (WHERE status_code >= 400 AND status_code < 500) AS errors_4xx,
            percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms) AS p50_ms,
            percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95_ms
        FROM environment_traffic_samples
        WHERE project_id = $1 AND recorded_at >= $2 AND recorded_at <= $3
        "#,
    )
    .bind(project_id)
    .bind(prev_start)
    .bind(prev_end)
    .fetch_optional(db)
    .await?;

    let previous_totals = if !has_any_traffic {
        MetricTotals {
            visitors: None,
            requests: None,
            errors_5xx: None,
            errors_4xx: None,
            p50_ms: None,
            p95_ms: None,
        }
    } else if let Some(t) = prev_totals_row {
        MetricTotals {
            visitors: Some(t.visitors as u64),
            requests: Some(t.requests as u64),
            errors_5xx: Some(t.errors_5xx as u64),
            errors_4xx: Some(t.errors_4xx as u64),
            p50_ms: t.p50_ms.map(|v| v.round() as u64),
            p95_ms: t.p95_ms.map(|v| v.round() as u64),
        }
    } else {
        MetricTotals {
            visitors: Some(0),
            requests: Some(0),
            errors_5xx: Some(0),
            errors_4xx: Some(0),
            p50_ms: None,
            p95_ms: None,
        }
    };

    // 3. Traffic buckets
    let traffic_bucket_rows: Vec<DbTrafficBucket> = sqlx::query_as(
        r#"
        SELECT
            date_bin($1::interval, recorded_at, $2::timestamptz) AS bucket_time,
            COUNT(DISTINCT visitor_hash) AS visitors,
            COUNT(*) AS requests,
            COUNT(*) FILTER (WHERE status_code >= 500) AS errors_5xx,
            COUNT(*) FILTER (WHERE status_code >= 400 AND status_code < 500) AS errors_4xx,
            percentile_cont(0.50) WITHIN GROUP (ORDER BY duration_ms) AS p50_ms,
            percentile_cont(0.95) WITHIN GROUP (ORDER BY duration_ms) AS p95_ms
        FROM environment_traffic_samples
        WHERE project_id = $3 AND recorded_at >= $4 AND recorded_at <= $5
        GROUP BY bucket_time
        ORDER BY bucket_time ASC
        "#,
    )
    .bind(&step_interval)
    .bind(origin)
    .bind(project_id)
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    let mut traffic_by_ts: std::collections::HashMap<i64, DbTrafficBucket> =
        traffic_bucket_rows.into_iter().map(|b| (b.bucket_time.timestamp(), b)).collect();

    // 4. Resource buckets
    let resource_bucket_rows: Vec<DbResourceBucket> = sqlx::query_as(
        r#"
        SELECT
            date_bin($1::interval, recorded_at, $2::timestamptz) AS bucket_time,
            service_slug,
            container_id,
            AVG(cpu_percent) AS cpu_percent,
            AVG(memory_percent) AS memory_percent,
            AVG(memory_bytes)::BIGINT AS memory_bytes,
            MAX(memory_limit_bytes)::BIGINT AS memory_limit_bytes
        FROM container_resource_samples
        WHERE project_id = $3 AND recorded_at >= $4 AND recorded_at <= $5
        GROUP BY bucket_time, service_slug, container_id
        ORDER BY bucket_time ASC
        "#,
    )
    .bind(&step_interval)
    .bind(origin)
    .bind(project_id)
    .bind(start)
    .bind(end)
    .fetch_all(db)
    .await?;

    let mut resources_by_ts: std::collections::HashMap<i64, Vec<ContainerMetricBucket>> =
        std::collections::HashMap::new();

    for r in resource_bucket_rows {
        let ts = r.bucket_time.timestamp();
        let bucket = ContainerMetricBucket {
            service: r.service_slug,
            container_id: r.container_id,
            cpu_percent: r.cpu_percent.map(|v| (v * 10.0).round() / 10.0),
            memory_percent: r.memory_percent.map(|v| (v * 10.0).round() / 10.0),
            memory_bytes: r.memory_bytes.map(|v| v as u64),
            memory_limit_bytes: r.memory_limit_bytes.map(|v| v as u64),
        };
        resources_by_ts.entry(ts).or_default().push(bucket);
    }

    // 5. Generate continuous intervals
    let timestamps = generate_bucket_timestamps(start, end, step);
    let mut buckets = Vec::with_capacity(timestamps.len());

    for ts in timestamps {
        let key = ts.timestamp();
        let containers = resources_by_ts.remove(&key).unwrap_or_default();

        if !has_any_traffic {
            buckets.push(MetricBucket {
                at: ts.to_rfc3339(),
                visitors: None,
                requests: None,
                errors_5xx: None,
                errors_4xx: None,
                p50_ms: None,
                p95_ms: None,
                containers,
            });
        } else if let Some(tb) = traffic_by_ts.remove(&key) {
            buckets.push(MetricBucket {
                at: ts.to_rfc3339(),
                visitors: Some(tb.visitors as u64),
                requests: Some(tb.requests as u64),
                errors_5xx: Some(tb.errors_5xx as u64),
                errors_4xx: Some(tb.errors_4xx as u64),
                p50_ms: tb.p50_ms.map(|v| v.round() as u64),
                p95_ms: tb.p95_ms.map(|v| v.round() as u64),
                containers,
            });
        } else {
            // Measured environment, but zero traffic in this bucket
            buckets.push(MetricBucket {
                at: ts.to_rfc3339(),
                visitors: Some(0),
                requests: Some(0),
                errors_5xx: Some(0),
                errors_4xx: Some(0),
                p50_ms: None,
                p95_ms: None,
                containers,
            });
        }
    }

    Ok(MetricsResponse {
        range: range_str,
        step: step_str,
        buckets,
        totals,
        previous_totals,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct TraefikAccessLog {
    pub client_ip: String,
    pub user_agent: String,
    pub status_code: u16,
    pub duration_ms: u64,
    pub router_name: Option<String>,
    pub host: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

pub fn parse_traefik_access_log(line: &str) -> Option<TraefikAccessLog> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    if line.starts_with('{') {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            let client_ip = v
                .get("ClientHost")
                .or_else(|| v.get("client_ip"))
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1")
                .to_string();

            let user_agent = v
                .get("RequestHeader")
                .and_then(|h| h.get("User-Agent"))
                .or_else(|| v.get("user_agent"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let status_code = v
                .get("DownstreamStatus")
                .or_else(|| v.get("status"))
                .and_then(|v| v.as_u64())
                .unwrap_or(200) as u16;

            let duration_ms = v
                .get("Duration")
                .and_then(|v| v.as_u64())
                .map(|ns| ns / 1_000_000)
                .or_else(|| v.get("duration_ms").and_then(|v| v.as_u64()))
                .unwrap_or(0);

            let router_name = v
                .get("RouterName")
                .or_else(|| v.get("router"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let host = v
                .get("RequestHost")
                .or_else(|| v.get("host"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let recorded_at = v
                .get("time")
                .or_else(|| v.get("StartUTC"))
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            return Some(TraefikAccessLog {
                client_ip,
                user_agent,
                status_code,
                duration_ms,
                router_name,
                host,
                recorded_at,
            });
        }
    }

    None
}

pub fn check_error_rate(
    requests: u64,
    errors_5xx: u64,
    min_requests: u64,
    threshold: f64,
) -> Option<f64> {
    if requests < min_requests {
        return None;
    }
    let rate = errors_5xx as f64 / requests as f64;
    if rate > threshold {
        Some((rate * 1000.0).round() / 1000.0)
    } else {
        None
    }
}

pub async fn record_traffic_sample(
    db: &sqlx::PgPool,
    project_id: Uuid,
    visitor_hash: &str,
    status_code: u16,
    duration_ms: u64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        INSERT INTO environment_traffic_samples
            (project_id, visitor_hash, status_code, duration_ms, recorded_at)
        VALUES ($1, $2, $3, $4, NOW())
        "#,
    )
    .bind(project_id)
    .bind(visitor_hash)
    .bind(status_code as i16)
    .bind(duration_ms as i32)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn run_metrics_collector(state: AppState) {
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        interval.tick().await;

        // 1. Sample running containers
        if let Err(e) = sample_running_containers(&state).await {
            tracing::warn!("Metrics collector container sampling error: {e}");
        }

        // 2. Check error-rate alerts
        if let Err(e) = check_and_fire_error_rate_alerts(&state).await {
            tracing::warn!("Metrics collector error-rate alert check error: {e}");
        }

        // 3. Rolling retention cleanup (> 8 days)
        let _ = sqlx::query(
            "DELETE FROM container_resource_samples WHERE recorded_at < NOW() - INTERVAL '8 days'",
        )
        .execute(&state.db)
        .await;
        let _ = sqlx::query(
            "DELETE FROM environment_traffic_samples WHERE recorded_at < NOW() - INTERVAL '8 days'",
        )
        .execute(&state.db)
        .await;
        let _ = sqlx::query(
            "DELETE FROM service_error_rate_alerts WHERE fired_at < NOW() - INTERVAL '8 days'",
        )
        .execute(&state.db)
        .await;
    }
}

async fn sample_running_containers(state: &AppState) -> Result<(), AppError> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT s.project_id, s.slug, c.docker_container_id
         FROM containers c
         JOIN services s ON c.service_id = s.id
         WHERE c.status = 'running'",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    for (project_id, service_slug, container_id) in rows {
        let path = format!("/v1.45/containers/{container_id}/stats?stream=false");
        if let Ok(body) = shipyard_docker::raw_request("GET", &path, None).await {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&body) {
                let snap = crate::shorthand::parse_stats_snapshot(&val);
                let _ = sqlx::query(
                    r#"
                    INSERT INTO container_resource_samples
                        (project_id, service_slug, container_id, cpu_percent, memory_percent, memory_bytes, memory_limit_bytes, recorded_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
                    "#,
                )
                .bind(project_id)
                .bind(&service_slug)
                .bind(&container_id)
                .bind(snap.cpu_percent)
                .bind(snap.memory_percent)
                .bind(snap.memory_usage_bytes as i64)
                .bind(snap.memory_limit_bytes as i64)
                .execute(&state.db)
                .await;
            }
        }
    }
    Ok(())
}

async fn check_and_fire_error_rate_alerts(state: &AppState) -> Result<(), AppError> {
    let rows: Vec<(Uuid, i64, i64)> = sqlx::query_as(
        r#"
        SELECT
            project_id,
            COUNT(*) AS reqs,
            COUNT(*) FILTER (WHERE status_code >= 500) AS errors_5xx
        FROM environment_traffic_samples
        WHERE recorded_at >= NOW() - INTERVAL '5 minutes'
        GROUP BY project_id
        HAVING COUNT(*) >= 20
        "#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    for (project_id, reqs, errors_5xx) in rows {
        if let Some(rate) = check_error_rate(reqs as u64, errors_5xx as u64, 20, 0.05) {
            let recently_fired: bool = sqlx::query_scalar(
                r#"
                SELECT EXISTS (
                    SELECT 1 FROM service_error_rate_alerts
                    WHERE project_id = $1 AND fired_at > NOW() - INTERVAL '15 minutes'
                )
                "#,
            )
            .bind(project_id)
            .fetch_one(&state.db)
            .await
            .unwrap_or(false);

            if !recently_fired {
                let _ = sqlx::query(
                    r#"
                    INSERT INTO service_error_rate_alerts
                        (project_id, service_slug, error_rate, threshold, fired_at)
                    VALUES ($1, 'web', $2, 0.05, NOW())
                    "#,
                )
                .bind(project_id)
                .bind(rate)
                .execute(&state.db)
                .await;

                shipyard_db::events::emit_for_project(
                    &state.db,
                    project_id,
                    "service.error_rate_alert",
                    serde_json::json!({
                        "error_rate": rate,
                        "threshold": 0.05,
                        "requests": reqs,
                        "errors_5xx": errors_5xx,
                    }),
                )
                .await;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_valid() {
        assert_eq!(parse_duration("1m").unwrap(), Duration::minutes(1));
        assert_eq!(parse_duration("5m").unwrap(), Duration::minutes(5));
        assert_eq!(parse_duration("1h").unwrap(), Duration::hours(1));
        assert_eq!(parse_duration("24h").unwrap(), Duration::hours(24));
        assert_eq!(parse_duration("7d").unwrap(), Duration::days(7));
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("0h").is_err());
        assert!(parse_duration("-5m").is_err());
        assert!(parse_duration("10x").is_err());
    }

    #[test]
    fn parse_range_and_step_defaults() {
        // Defaults to 24h range, 1h step
        let (range, step, r_str, s_str) = parse_range_and_step(None, None).unwrap();
        assert_eq!(range, Duration::hours(24));
        assert_eq!(step, Duration::hours(1));
        assert_eq!(r_str, "24h");
        assert_eq!(s_str, "1h");

        // 1h range defaults to 1m step
        let (range, step, r_str, s_str) = parse_range_and_step(Some("1h"), None).unwrap();
        assert_eq!(range, Duration::hours(1));
        assert_eq!(step, Duration::minutes(1));
        assert_eq!(r_str, "1h");
        assert_eq!(s_str, "1m");

        // 7d range defaults to 1d step
        let (range, step, r_str, s_str) = parse_range_and_step(Some("7d"), None).unwrap();
        assert_eq!(range, Duration::days(7));
        assert_eq!(step, Duration::days(1));
        assert_eq!(r_str, "7d");
        assert_eq!(s_str, "1d");
    }

    #[test]
    fn parse_range_and_step_custom() {
        let (range, step, _, _) = parse_range_and_step(Some("24h"), Some("5m")).unwrap();
        assert_eq!(range, Duration::hours(24));
        assert_eq!(step, Duration::minutes(5));

        // step cannot exceed range
        assert!(parse_range_and_step(Some("1h"), Some("24h")).is_err());
    }

    #[test]
    fn compute_percentile_calculation() {
        let values = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
        assert_eq!(compute_percentile(&values, 0.50), Some(55));
        assert_eq!(compute_percentile(&values, 0.95), Some(96));

        assert_eq!(compute_percentile(&[], 0.50), None);
        assert_eq!(compute_percentile(&[42], 0.50), Some(42));
    }

    #[test]
    fn visitor_hash_privacy_and_rotation() {
        let h1 = generate_visitor_hash("192.168.1.1", "Mozilla/5.0", "2026-10-26", "salt123");
        let h2 = generate_visitor_hash("192.168.1.1", "Mozilla/5.0", "2026-10-26", "salt123");
        assert_eq!(h1, h2);

        // Rotating date changes hash for same user
        let h_next_day = generate_visitor_hash("192.168.1.1", "Mozilla/5.0", "2026-10-27", "salt123");
        assert_ne!(h1, h_next_day);

        // Different user agents produce different hashes
        let h_other_ua = generate_visitor_hash("192.168.1.1", "Chrome/120.0", "2026-10-26", "salt123");
        assert_ne!(h1, h_other_ua);

        // Raw IP / UA not in hash
        assert!(!h1.contains("192.168.1.1"));
        assert!(!h1.contains("Mozilla"));
    }

    #[test]
    fn bucket_timestamps_generation() {
        let start = DateTime::parse_from_rfc3339("2026-10-26T00:00:00Z").unwrap().with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-10-26T03:00:00Z").unwrap().with_timezone(&Utc);
        let buckets = generate_bucket_timestamps(start, end, Duration::hours(1));
        assert_eq!(buckets.len(), 4);
        assert_eq!(buckets[0], start);
        assert_eq!(buckets[3], end);
    }

    #[test]
    fn parse_traefik_access_log_json() {
        let json_line = r#"{"ClientHost":"203.0.113.195","DownstreamStatus":502,"Duration":124000000,"RouterName":"web-prod@docker","RequestHost":"todo.apps.example.dev","RequestHeader":{"User-Agent":"curl/8.1.2"},"time":"2026-10-26T09:00:00Z"}"#;
        let entry = parse_traefik_access_log(json_line).expect("should parse json");
        assert_eq!(entry.client_ip, "203.0.113.195");
        assert_eq!(entry.status_code, 502);
        assert_eq!(entry.duration_ms, 124);
        assert_eq!(entry.router_name.as_deref(), Some("web-prod@docker"));
        assert_eq!(entry.host.as_deref(), Some("todo.apps.example.dev"));
        assert_eq!(entry.user_agent, "curl/8.1.2");
    }

    #[test]
    fn check_error_rate_threshold() {
        // Less than min requests: no alert
        assert_eq!(check_error_rate(10, 5, 20, 0.05), None);
        // Under threshold: no alert
        assert_eq!(check_error_rate(100, 3, 20, 0.05), None);
        // Over threshold: fires alert with calculated rate
        assert_eq!(check_error_rate(100, 6, 20, 0.05), Some(0.06));
    }
}
