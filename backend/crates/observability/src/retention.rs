//! Retention rules and partition management for logs and metrics.

use chrono::{Datelike, DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

pub const DEFAULT_RETENTION_DAYS: u32 = 90;
pub const VISITOR_HASH_RETENTION_DAYS: u32 = 8;
pub const RAW_RESOURCE_RETENTION_HOURS: u32 = 48;
pub const ROLLUP_RESOURCE_RETENTION_DAYS: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetentionPolicy {
    /// Days to keep log entries. Default 90.
    pub retention_days: u32,
    /// Days to keep visitor hashes. Default 8.
    pub visitor_hash_retention_days: u32,
    /// Hours to keep raw (1-minute) resource samples. Default 48.
    pub raw_resource_retention_hours: u32,
    /// Days to keep rolled up (5m/1h) resource samples. Default 8.
    pub rollup_resource_retention_days: u32,
    /// Optional storage limit in gigabytes; if exceeded, oldest partitions are dropped first.
    pub max_storage_gb: Option<u64>,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            retention_days: DEFAULT_RETENTION_DAYS,
            visitor_hash_retention_days: VISITOR_HASH_RETENTION_DAYS,
            raw_resource_retention_hours: RAW_RESOURCE_RETENTION_HOURS,
            rollup_resource_retention_days: ROLLUP_RESOURCE_RETENTION_DAYS,
            max_storage_gb: None,
        }
    }
}

impl RetentionPolicy {
    /// Earliest timestamp for log entries that should be kept.
    pub fn log_cutoff(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - Duration::days(self.retention_days as i64)
    }

    /// Earliest timestamp for visitor hashes that should be kept.
    pub fn visitor_hash_cutoff(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - Duration::days(self.visitor_hash_retention_days as i64)
    }

    /// Earliest timestamp for raw 1m container resource samples.
    pub fn raw_resource_cutoff(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - Duration::hours(self.raw_resource_retention_hours as i64)
    }

    /// Earliest timestamp for rolled up container resource samples.
    pub fn rollup_resource_cutoff(&self, now: DateTime<Utc>) -> DateTime<Utc> {
        now - Duration::days(self.rollup_resource_retention_days as i64)
    }
}

/// Partition table name for a given UTC date: `log_entries_yYYYYmMMdDD`.
pub fn partition_name_for_date(date: NaiveDate) -> String {
    format!("log_entries_y{}m{:02}d{:02}", date.year(), date.month(), date.day())
}

/// Start (inclusive) and end (exclusive) timestamps for a date's partition.
pub fn partition_bounds(date: NaiveDate) -> (DateTime<Utc>, DateTime<Utc>) {
    let start = date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    let next_date = date.succ_opt().unwrap_or(date);
    let end = next_date.and_hms_opt(0, 0, 0).unwrap().and_utc();
    (start, end)
}

/// Dates for which partitions should exist: `[today - 1, today + advance_days]`.
pub fn required_partition_dates(today: NaiveDate, advance_days: u32) -> Vec<NaiveDate> {
    let mut dates = Vec::new();
    if let Some(yesterday) = today.pred_opt() {
        dates.push(yesterday);
    }
    dates.push(today);
    let mut current = today;
    for _ in 0..advance_days {
        if let Some(next) = current.succ_opt() {
            dates.push(next);
            current = next;
        }
    }
    dates
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn partition_name_formatting() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        assert_eq!(partition_name_for_date(d), "log_entries_y2026m10d29");
    }

    #[test]
    fn partition_bounds_are_day_aligned() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        let (start, end) = partition_bounds(d);
        assert_eq!(start, Utc.with_ymd_and_hms(2026, 10, 29, 0, 0, 0).unwrap());
        assert_eq!(end, Utc.with_ymd_and_hms(2026, 10, 30, 0, 0, 0).unwrap());
    }

    #[test]
    fn required_partitions_includes_yesterday_today_and_advance() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 29).unwrap();
        let dates = required_partition_dates(today, 2);
        assert_eq!(
            dates,
            vec![
                NaiveDate::from_ymd_opt(2026, 10, 28).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 29).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 30).unwrap(),
                NaiveDate::from_ymd_opt(2026, 10, 31).unwrap(),
            ]
        );
    }

    #[test]
    fn cutoffs_match_policy() {
        let p = RetentionPolicy::default();
        let now = Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap();
        assert_eq!(p.log_cutoff(now), now - Duration::days(90));
        assert_eq!(p.visitor_hash_cutoff(now), now - Duration::days(8));
        assert_eq!(p.raw_resource_cutoff(now), now - Duration::hours(48));
        assert_eq!(p.rollup_resource_cutoff(now), now - Duration::days(8));
    }
}
