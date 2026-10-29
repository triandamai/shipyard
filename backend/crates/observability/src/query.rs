//! Log query model shared by admin and org endpoints.
//!
//! The scope is decided by the server: [`Scope::Org`] forces `org_id` and
//! ignores any `org_id` the client sends, so an org request can never widen
//! to other orgs or to platform lines.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::parse::valid_trace_id;
use crate::types::{LogLevel, LogSource, StoredLogEntry};

pub const MAX_RANGE_DAYS: i64 = 90;
pub const DEFAULT_RANGE_HOURS: i64 = 24;
pub const DEFAULT_LIMIT: u32 = 200;
pub const MAX_LIMIT: u32 = 1000;
pub const MAX_KEYWORD_LEN: usize = 256;

/// Columns selected for a stored log row, in the order the store decodes them.
pub const LOG_COLUMNS: &str = "id, ts, org_id, project_id, service_id, deployment_id, source, \
container_id, replica, stream, level, trace_id, span_id, message, fields";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Platform staff: every row, optional `org_id` filter.
    Admin,
    /// An organization: only rows with this `org_id`.
    Org(Uuid),
}

/// Raw query-string parameters (`GET …/logs?…`).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct LogQueryParams {
    /// Comma-separated sources (`traefik,backend`).
    pub source: Option<String>,
    /// Comma-separated levels (`warn,error`).
    pub level: Option<String>,
    /// Keyword (case-insensitive substring of the masked message).
    pub q: Option<String>,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub trace_id: Option<String>,
    /// Honoured for [`Scope::Admin`] only.
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
    pub deployment_id: Option<Uuid>,
    pub replica: Option<i32>,
    pub container_id: Option<String>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QueryError {
    #[error("`from` must be before `to`")]
    InvalidRange,
    #[error("range is longer than {MAX_RANGE_DAYS} days")]
    RangeTooLarge,
    #[error("invalid cursor")]
    BadCursor,
    #[error("unknown source `{0}`")]
    UnknownSource(String),
    #[error("unknown level `{0}`")]
    UnknownLevel(String),
    #[error("invalid trace id")]
    BadTraceId,
    #[error("keyword longer than {MAX_KEYWORD_LEN} characters")]
    KeywordTooLong,
    #[error("unknown bucket `{0}` (use hour or day)")]
    UnknownBucket(String),
}

/// Position after the last row of a page (rows are ordered `ts DESC, id DESC`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub ts: DateTime<Utc>,
    pub id: i64,
}

impl Cursor {
    pub fn encode(&self) -> String {
        URL_SAFE_NO_PAD.encode(format!("{}:{}", self.ts.timestamp_micros(), self.id))
    }

    pub fn decode(s: &str) -> Result<Self, QueryError> {
        let bytes = URL_SAFE_NO_PAD.decode(s).map_err(|_| QueryError::BadCursor)?;
        let text = String::from_utf8(bytes).map_err(|_| QueryError::BadCursor)?;
        let (micros, id) = text.split_once(':').ok_or(QueryError::BadCursor)?;
        let micros: i64 = micros.parse().map_err(|_| QueryError::BadCursor)?;
        let id: i64 = id.parse().map_err(|_| QueryError::BadCursor)?;
        let ts = DateTime::<Utc>::from_timestamp_micros(micros).ok_or(QueryError::BadCursor)?;
        Ok(Self { ts, id })
    }
}

/// A bind value for the generated SQL (kept free of sqlx types so the
/// builder is testable without a database).
#[derive(Debug, Clone, PartialEq)]
pub enum Bind {
    Uuid(Uuid),
    Ts(DateTime<Utc>),
    Text(String),
    TextArray(Vec<String>),
    SmallIntArray(Vec<i16>),
    Int(i32),
    BigInt(i64),
}

#[derive(Debug, Clone, PartialEq)]
pub struct SqlQuery {
    pub sql: String,
    pub binds: Vec<Bind>,
}

/// A validated log query.
#[derive(Debug, Clone, PartialEq)]
pub struct LogQuery {
    scope: Scope,
    org_id: Option<Uuid>,
    pub sources: Vec<LogSource>,
    pub levels: Vec<LogLevel>,
    pub q: Option<String>,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub trace_id: Option<String>,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
    pub deployment_id: Option<Uuid>,
    pub replica: Option<i32>,
    pub container_id: Option<String>,
    pub cursor: Option<Cursor>,
    pub limit: u32,
}

impl LogQuery {
    pub fn new(scope: Scope, p: LogQueryParams, now: DateTime<Utc>) -> Result<Self, QueryError> {
        let (from, to) = resolve_range(p.from, p.to, now)?;
        let org_id = match scope {
            Scope::Org(id) => Some(id),
            Scope::Admin => p.org_id,
        };
        let sources = parse_sources(&p.source)?;
        let levels = csv(&p.level)
            .into_iter()
            .map(|x| LogLevel::parse_name(x).ok_or_else(|| QueryError::UnknownLevel(x.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        let q = p.q.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
        if q.as_ref().is_some_and(|q| q.chars().count() > MAX_KEYWORD_LEN) {
            return Err(QueryError::KeywordTooLong);
        }
        let trace_id = match p.trace_id.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(t) => Some(valid_trace_id(t).ok_or(QueryError::BadTraceId)?),
            None => None,
        };
        let cursor = p.cursor.as_deref().map(Cursor::decode).transpose()?;
        Ok(Self {
            scope,
            org_id,
            sources,
            levels,
            q,
            from,
            to,
            trace_id,
            project_id: p.project_id,
            service_id: p.service_id,
            deployment_id: p.deployment_id,
            replica: p.replica,
            container_id: p.container_id,
            cursor,
            limit: p.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT),
        })
    }

    pub fn scope(&self) -> Scope {
        self.scope
    }

    /// The effective org filter (forced for org scope).
    pub fn org_id(&self) -> Option<Uuid> {
        self.org_id
    }

    /// SQL for one page; fetches `limit + 1` rows to detect a next page.
    pub fn to_sql(&self) -> SqlQuery {
        let mut w = Where::default();
        w.push("ts >= ", Bind::Ts(self.from));
        w.push("ts < ", Bind::Ts(self.to));
        if let Some(org) = self.org_id {
            w.push("org_id = ", Bind::Uuid(org));
        }
        if !self.sources.is_empty() {
            let v = self.sources.iter().map(|s| s.as_str().to_string()).collect();
            w.push_any("source", Bind::TextArray(v));
        }
        if !self.levels.is_empty() {
            let v = self.levels.iter().map(|l| l.as_i16()).collect();
            w.push_any("level", Bind::SmallIntArray(v));
        }
        if let Some(q) = &self.q {
            w.push("message ILIKE ", Bind::Text(format!("%{}%", escape_like(q))));
        }
        if let Some(t) = &self.trace_id {
            w.push("trace_id = ", Bind::Text(t.clone()));
        }
        for (col, id) in [
            ("project_id = ", self.project_id),
            ("service_id = ", self.service_id),
            ("deployment_id = ", self.deployment_id),
        ] {
            if let Some(id) = id {
                w.push(col, Bind::Uuid(id));
            }
        }
        if let Some(r) = self.replica {
            w.push("replica = ", Bind::Int(r));
        }
        if let Some(c) = &self.container_id {
            w.push("container_id = ", Bind::Text(c.clone()));
        }
        if let Some(c) = self.cursor {
            let (a, b) = (w.next(), w.next() + 1);
            w.clauses.push(format!("(ts, id) < (${a}, ${b})"));
            w.binds.push(Bind::Ts(c.ts));
            w.binds.push(Bind::BigInt(c.id));
        }
        SqlQuery {
            sql: format!(
                "SELECT {LOG_COLUMNS} FROM log_entries WHERE {} ORDER BY ts DESC, id DESC LIMIT {}",
                w.clauses.join(" AND "),
                self.limit + 1
            ),
            binds: w.binds,
        }
    }

    /// Same semantics as [`to_sql`](Self::to_sql), for in-memory stores.
    pub fn matches(&self, row: &StoredLogEntry) -> bool {
        let e = &row.entry;
        let eq = |want: Option<Uuid>, got: Option<Uuid>| want.is_none() || want == got;
        e.ts >= self.from
            && e.ts < self.to
            && eq(self.org_id, e.org_id)
            && (self.sources.is_empty() || self.sources.contains(&e.source))
            && (self.levels.is_empty() || self.levels.contains(&e.level))
            && self.q.as_ref().is_none_or(|q| {
                e.message.as_str().to_lowercase().contains(&q.to_lowercase())
            })
            && (self.trace_id.is_none() || self.trace_id == e.trace_id)
            && eq(self.project_id, e.project_id)
            && eq(self.service_id, e.service_id)
            && eq(self.deployment_id, e.deployment_id)
            && (self.replica.is_none() || self.replica == e.replica)
            && (self.container_id.is_none() || self.container_id == e.container_id)
            && self
                .cursor
                .is_none_or(|c| e.ts < c.ts || (e.ts == c.ts && row.id < c.id))
    }
}

/// Accumulates `AND`-joined clauses with numbered placeholders.
#[derive(Default)]
pub(crate) struct Where {
    pub clauses: Vec<String>,
    pub binds: Vec<Bind>,
}

impl Where {
    pub fn next(&self) -> usize {
        self.binds.len() + 1
    }
    /// `prefix` ends with the operator, e.g. `"ts >= "`.
    pub fn push(&mut self, prefix: &str, b: Bind) {
        self.clauses.push(format!("{prefix}${}", self.next()));
        self.binds.push(b);
    }
    pub fn push_any(&mut self, col: &str, b: Bind) {
        self.clauses.push(format!("{col} = ANY(${})", self.next()));
        self.binds.push(b);
    }
}

/// Escape `LIKE` metacharacters (backslash is Postgres' default escape).
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '\\' | '%' | '_') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Split a comma list, trimming blanks.
pub(crate) fn csv(s: &Option<String>) -> Vec<&str> {
    s.as_deref()
        .map(|s| s.split(',').map(str::trim).filter(|x| !x.is_empty()).collect())
        .unwrap_or_default()
}

pub(crate) fn parse_sources(s: &Option<String>) -> Result<Vec<LogSource>, QueryError> {
    csv(s)
        .into_iter()
        .map(|x| x.parse().map_err(|_| QueryError::UnknownSource(x.to_string())))
        .collect()
}

/// Validate `[from, to)`; defaults to the last `DEFAULT_RANGE_HOURS`.
pub(crate) fn resolve_range(
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Result<(DateTime<Utc>, DateTime<Utc>), QueryError> {
    let to = to.unwrap_or(now);
    let from = from.unwrap_or(to - Duration::hours(DEFAULT_RANGE_HOURS));
    if from >= to {
        return Err(QueryError::InvalidRange);
    }
    if to - from > Duration::days(MAX_RANGE_DAYS) {
        return Err(QueryError::RangeTooLarge);
    }
    Ok((from, to))
}

/// One page of results.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogPage {
    pub entries: Vec<StoredLogEntry>,
    pub next_cursor: Option<String>,
}

impl LogPage {
    /// Build a page from `limit + 1` fetched rows.
    pub fn from_rows(mut rows: Vec<StoredLogEntry>, limit: u32) -> Self {
        let limit = limit as usize;
        let next_cursor = if rows.len() > limit {
            rows.truncate(limit);
            rows.last().map(|r| Cursor { ts: r.entry.ts, id: r.id }.encode())
        } else {
            None
        };
        Self { entries: rows, next_cursor }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::{MaskedText};
    use crate::types::{LogEntry, Stream};
    use chrono::TimeZone;

    const ORG: Uuid = Uuid::from_u128(0xA);
    const OTHER: Uuid = Uuid::from_u128(0xB);
    const TID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap()
    }

    fn q(scope: Scope, p: LogQueryParams) -> LogQuery {
        LogQuery::new(scope, p, now()).unwrap()
    }

    fn row(id: i64, org: Option<Uuid>, mins_ago: i64) -> StoredLogEntry {
        StoredLogEntry {
            id,
            entry: LogEntry {
                ts: now() - Duration::minutes(mins_ago),
                org_id: org,
                project_id: Some(Uuid::from_u128(1)),
                service_id: Some(Uuid::from_u128(2)),
                deployment_id: Some(Uuid::from_u128(3)),
                source: LogSource::App,
                container_id: Some("c1".into()),
                replica: Some(1),
                stream: Stream::Stdout,
                level: LogLevel::Info,
                trace_id: None,
                span_id: None,
                message: MaskedText::from_store("Hello World".into()),
                fields: None,
            },
        }
    }

    #[test]
    fn defaults() {
        let lq = q(Scope::Admin, LogQueryParams::default());
        assert_eq!(lq.to, now());
        assert_eq!(lq.from, now() - Duration::hours(24));
        assert_eq!(lq.limit, DEFAULT_LIMIT);
        assert_eq!(lq.org_id(), None);
        assert_eq!(lq.scope(), Scope::Admin);
        assert!(lq.sources.is_empty() && lq.levels.is_empty());
    }

    #[test]
    fn org_scope_forces_org_and_ignores_client_org() {
        let lq = q(Scope::Org(ORG), LogQueryParams { org_id: Some(OTHER), ..Default::default() });
        assert_eq!(lq.org_id(), Some(ORG));
        let sql = lq.to_sql();
        assert!(sql.sql.contains("org_id = $"), "{}", sql.sql);
        assert!(sql.binds.contains(&Bind::Uuid(ORG)));
        assert!(!sql.binds.contains(&Bind::Uuid(OTHER)));
    }

    #[test]
    fn admin_may_filter_by_org() {
        let lq = q(Scope::Admin, LogQueryParams { org_id: Some(OTHER), ..Default::default() });
        assert_eq!(lq.org_id(), Some(OTHER));
    }

    #[test]
    fn validation_errors() {
        let bad = |p: LogQueryParams| LogQuery::new(Scope::Admin, p, now()).unwrap_err();
        assert_eq!(
            bad(LogQueryParams { from: Some(now()), to: Some(now()), ..Default::default() }),
            QueryError::InvalidRange
        );
        assert_eq!(
            bad(LogQueryParams { from: Some(now() - Duration::days(91)), ..Default::default() }),
            QueryError::RangeTooLarge
        );
        assert_eq!(
            bad(LogQueryParams { source: Some("app,nope".into()), ..Default::default() }),
            QueryError::UnknownSource("nope".into())
        );
        assert_eq!(
            bad(LogQueryParams { level: Some("loud".into()), ..Default::default() }),
            QueryError::UnknownLevel("loud".into())
        );
        assert_eq!(
            bad(LogQueryParams { trace_id: Some("xyz".into()), ..Default::default() }),
            QueryError::BadTraceId
        );
        assert_eq!(
            bad(LogQueryParams { cursor: Some("!!".into()), ..Default::default() }),
            QueryError::BadCursor
        );
        assert_eq!(
            bad(LogQueryParams { q: Some("x".repeat(MAX_KEYWORD_LEN + 1)), ..Default::default() }),
            QueryError::KeywordTooLong
        );
        // exactly 90 days is allowed
        assert!(LogQuery::new(
            Scope::Admin,
            LogQueryParams { from: Some(now() - Duration::days(90)), ..Default::default() },
            now()
        )
        .is_ok());
    }

    #[test]
    fn parses_lists_limit_keyword_and_trace() {
        let lq = q(
            Scope::Admin,
            LogQueryParams {
                source: Some("traefik, database,,".into()),
                level: Some("WARN,error".into()),
                q: Some("  timeout ".into()),
                trace_id: Some(TID.to_uppercase()),
                limit: Some(5000),
                ..Default::default()
            },
        );
        assert_eq!(lq.sources, vec![LogSource::Traefik, LogSource::Postgres]);
        assert_eq!(lq.levels, vec![LogLevel::Warn, LogLevel::Error]);
        assert_eq!(lq.q.as_deref(), Some("timeout"));
        assert_eq!(lq.trace_id.as_deref(), Some(TID));
        assert_eq!(lq.limit, MAX_LIMIT);

        let lq = q(Scope::Admin, LogQueryParams { q: Some("   ".into()), limit: Some(0), ..Default::default() });
        assert_eq!(lq.q, None);
        assert_eq!(lq.limit, 1);
    }

    #[test]
    fn cursor_round_trip() {
        let c = Cursor { ts: now() + Duration::microseconds(123), id: 42 };
        assert_eq!(Cursor::decode(&c.encode()).unwrap(), c);
        assert_eq!(Cursor::decode("bm9wZQ"), Err(QueryError::BadCursor)); // "nope"
        assert_eq!(Cursor::decode("MTox"), Ok(Cursor { ts: Utc.timestamp_micros(1).unwrap(), id: 1 })); // "1:1"
        assert_eq!(Cursor::decode("eDox"), Err(QueryError::BadCursor)); // "x:1"
    }

    #[test]
    fn sql_minimal() {
        let lq = q(Scope::Admin, LogQueryParams::default());
        let sql = lq.to_sql();
        assert_eq!(
            sql.sql,
            format!(
                "SELECT {LOG_COLUMNS} FROM log_entries WHERE ts >= $1 AND ts < $2 \
                 ORDER BY ts DESC, id DESC LIMIT 201"
            )
        );
        assert_eq!(sql.binds, vec![Bind::Ts(lq.from), Bind::Ts(lq.to)]);
    }

    #[test]
    fn sql_all_filters() {
        let cursor = Cursor { ts: now() - Duration::minutes(5), id: 9 };
        let lq = q(
            Scope::Org(ORG),
            LogQueryParams {
                source: Some("app".into()),
                level: Some("error,fatal".into()),
                q: Some("50%_off\\".into()),
                trace_id: Some(TID.into()),
                project_id: Some(Uuid::from_u128(1)),
                service_id: Some(Uuid::from_u128(2)),
                deployment_id: Some(Uuid::from_u128(3)),
                replica: Some(2),
                container_id: Some("c1".into()),
                cursor: Some(cursor.encode()),
                limit: Some(10),
                ..Default::default()
            },
        );
        let sql = lq.to_sql();
        assert_eq!(
            sql.sql,
            format!(
                "SELECT {LOG_COLUMNS} FROM log_entries WHERE ts >= $1 AND ts < $2 \
                 AND org_id = $3 AND source = ANY($4) AND level = ANY($5) \
                 AND message ILIKE $6 AND trace_id = $7 AND project_id = $8 \
                 AND service_id = $9 AND deployment_id = $10 AND replica = $11 \
                 AND container_id = $12 AND (ts, id) < ($13, $14) \
                 ORDER BY ts DESC, id DESC LIMIT 11"
            )
        );
        assert_eq!(
            sql.binds,
            vec![
                Bind::Ts(lq.from),
                Bind::Ts(lq.to),
                Bind::Uuid(ORG),
                Bind::TextArray(vec!["app".into()]),
                Bind::SmallIntArray(vec![4, 5]),
                Bind::Text("%50\\%\\_off\\\\%".into()),
                Bind::Text(TID.into()),
                Bind::Uuid(Uuid::from_u128(1)),
                Bind::Uuid(Uuid::from_u128(2)),
                Bind::Uuid(Uuid::from_u128(3)),
                Bind::Int(2),
                Bind::Text("c1".into()),
                Bind::Ts(cursor.ts),
                Bind::BigInt(9),
            ]
        );
    }

    #[test]
    fn matches_mirrors_sql_filters() {
        let lq = q(Scope::Org(ORG), LogQueryParams::default());
        assert!(lq.matches(&row(1, Some(ORG), 5)));
        assert!(!lq.matches(&row(1, Some(OTHER), 5)), "other org");
        assert!(!lq.matches(&row(1, None, 5)), "platform line");
        assert!(!lq.matches(&row(1, Some(ORG), 60 * 25)), "out of range");

        let admin = q(Scope::Admin, LogQueryParams::default());
        assert!(admin.matches(&row(1, None, 5)));
        assert!(admin.matches(&row(1, Some(OTHER), 5)));

        let kw = q(Scope::Admin, LogQueryParams { q: Some("hello world".into()), ..Default::default() });
        assert!(kw.matches(&row(1, None, 5)));
        let kw = q(Scope::Admin, LogQueryParams { q: Some("bye".into()), ..Default::default() });
        assert!(!kw.matches(&row(1, None, 5)));

        let lvl = q(Scope::Admin, LogQueryParams { level: Some("error".into()), ..Default::default() });
        assert!(!lvl.matches(&row(1, None, 5)));
        let src = q(Scope::Admin, LogQueryParams { source: Some("traefik".into()), ..Default::default() });
        assert!(!src.matches(&row(1, None, 5)));

        let mut traced = row(1, None, 5);
        traced.entry.trace_id = Some(TID.into());
        let tq = q(Scope::Admin, LogQueryParams { trace_id: Some(TID.into()), ..Default::default() });
        assert!(tq.matches(&traced));
        assert!(!tq.matches(&row(1, None, 5)));

        let ids = |p: LogQueryParams| q(Scope::Admin, p).matches(&row(1, None, 5));
        assert!(!ids(LogQueryParams { project_id: Some(OTHER), ..Default::default() }));
        assert!(!ids(LogQueryParams { service_id: Some(OTHER), ..Default::default() }));
        assert!(!ids(LogQueryParams { deployment_id: Some(OTHER), ..Default::default() }));
        assert!(!ids(LogQueryParams { replica: Some(9), ..Default::default() }));
        assert!(!ids(LogQueryParams { container_id: Some("c9".into()), ..Default::default() }));
        assert!(ids(LogQueryParams { replica: Some(1), container_id: Some("c1".into()), ..Default::default() }));

        // cursor: strictly after (older than) the cursor position
        let r = row(5, None, 5);
        let at = Cursor { ts: r.entry.ts, id: 5 };
        let cq = q(Scope::Admin, LogQueryParams { cursor: Some(at.encode()), ..Default::default() });
        assert!(!cq.matches(&r));
        assert!(cq.matches(&row(4, None, 5)));
        assert!(cq.matches(&row(9, None, 6)));
    }

    #[test]
    fn page_from_rows() {
        let rows: Vec<_> = (0..3).map(|i| row(10 - i, None, i)).collect();
        let page = LogPage::from_rows(rows.clone(), 2);
        assert_eq!(page.entries.len(), 2);
        let next = Cursor::decode(page.next_cursor.as_deref().unwrap()).unwrap();
        assert_eq!(next, Cursor { ts: rows[1].entry.ts, id: rows[1].id });

        let page = LogPage::from_rows(rows, 3);
        assert_eq!(page.entries.len(), 3);
        assert_eq!(page.next_cursor, None);
    }
}
