//! Stats query model for log volume and error metrics over time.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::query::{csv, parse_sources, resolve_range, QueryError};
use crate::types::{LogLevel, LogSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StatBucket {
    Hour,
    Day,
}

impl StatBucket {
    pub fn as_str(&self) -> &'static str {
        match self {
            StatBucket::Hour => "hour",
            StatBucket::Day => "day",
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogStatsParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub bucket: Option<String>,
    pub source: Option<String>,
    pub level: Option<String>,
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogStatsQuery {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub bucket: StatBucket,
    pub sources: Vec<LogSource>,
    pub levels: Vec<LogLevel>,
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
}

impl LogStatsQuery {
    pub fn new(
        scope_org: Option<Uuid>,
        p: LogStatsParams,
        now: DateTime<Utc>,
    ) -> Result<Self, QueryError> {
        let (from, to) = resolve_range(p.from, p.to, now)?;
        let bucket = match p.bucket.as_deref().map(str::trim).map(str::to_ascii_lowercase) {
            None => {
                if to - from > Duration::hours(48) {
                    StatBucket::Day
                } else {
                    StatBucket::Hour
                }
            }
            Some(ref b) if b == "hour" || b == "1h" => StatBucket::Hour,
            Some(ref b) if b == "day" || b == "1d" || b == "24h" => StatBucket::Day,
            Some(other) => return Err(QueryError::UnknownBucket(other)),
        };

        let sources = parse_sources(&p.source)?;
        let levels = csv(&p.level)
            .into_iter()
            .map(|x| LogLevel::parse_name(x).ok_or_else(|| QueryError::UnknownLevel(x.to_string())))
            .collect::<Result<Vec<_>, _>>()?;

        let org_id = scope_org.or(p.org_id);

        Ok(Self {
            from,
            to,
            bucket,
            sources,
            levels,
            org_id,
            project_id: p.project_id,
            service_id: p.service_id,
        })
    }
}

/// A bucketed count of log lines for a specific source and level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogCountPoint {
    pub bucket_start: DateTime<Utc>,
    pub source: LogSource,
    pub level: LogLevel,
    pub count: u64,
}

/// Aggregated stats response.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LogStatsResult {
    pub total: u64,
    pub errors: u64,
    pub points: Vec<LogCountPoint>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap()
    }

    #[test]
    fn defaults_and_bucket_selection() {
        let q = LogStatsQuery::new(None, LogStatsParams::default(), now()).unwrap();
        assert_eq!(q.bucket, StatBucket::Hour); // 24h default range -> Hour
        assert_eq!(q.from, now() - Duration::hours(24));
        assert_eq!(q.to, now());

        // Range > 48h defaults to Day
        let q_long = LogStatsQuery::new(
            None,
            LogStatsParams {
                from: Some(now() - Duration::days(7)),
                to: Some(now()),
                ..Default::default()
            },
            now(),
        )
        .unwrap();
        assert_eq!(q_long.bucket, StatBucket::Day);
    }

    #[test]
    fn explicit_bucket_parsing() {
        for b in ["hour", "1h", "HOUR"] {
            let q = LogStatsQuery::new(
                None,
                LogStatsParams { bucket: Some(b.into()), ..Default::default() },
                now(),
            )
            .unwrap();
            assert_eq!(q.bucket, StatBucket::Hour);
        }

        for b in ["day", "1d", "24h", "DAY"] {
            let q = LogStatsQuery::new(
                None,
                LogStatsParams { bucket: Some(b.into()), ..Default::default() },
                now(),
            )
            .unwrap();
            assert_eq!(q.bucket, StatBucket::Day);
        }

        let err = LogStatsQuery::new(
            None,
            LogStatsParams { bucket: Some("minute".into()), ..Default::default() },
            now(),
        )
        .unwrap_err();
        assert_eq!(err, QueryError::UnknownBucket("minute".into()));
    }

    #[test]
    fn forces_scope_org() {
        let org1 = Uuid::from_u128(1);
        let org2 = Uuid::from_u128(2);
        let q = LogStatsQuery::new(
            Some(org1),
            LogStatsParams { org_id: Some(org2), ..Default::default() },
            now(),
        )
        .unwrap();
        assert_eq!(q.org_id, Some(org1));
    }
}
