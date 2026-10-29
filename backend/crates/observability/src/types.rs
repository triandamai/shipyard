//! Core log types shared by the collector, the stores and the API.

use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::mask::{MaskedJson, MaskedText};

/// Severity of a log line. Stored as `SMALLINT` (`as_i16`), ordered so that
/// `level >= LogLevel::Error` means "error or worse".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
    Fatal,
}

impl LogLevel {
    pub const ALL: [LogLevel; 6] = [
        LogLevel::Trace,
        LogLevel::Debug,
        LogLevel::Info,
        LogLevel::Warn,
        LogLevel::Error,
        LogLevel::Fatal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LogLevel::Trace => "trace",
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
            LogLevel::Fatal => "fatal",
        }
    }

    pub fn as_i16(self) -> i16 {
        self as i16
    }

    pub fn from_i16(v: i16) -> Option<Self> {
        Self::ALL.get(usize::try_from(v).ok()?).copied()
    }

    /// Parse a level name as written by common loggers (case-insensitive).
    /// Covers pino/bunyan, zap, slog (`INFO+2` style offsets are reduced to
    /// their base), tracing, log4j, Python logging and syslog names.
    pub fn parse_name(s: &str) -> Option<Self> {
        let s = s.trim();
        // slog writes levels between the named ones as `WARN+2` / `INFO-4`.
        let base = s.split(['+', '-']).next().unwrap_or(s);
        let lvl = match base.to_ascii_lowercase().as_str() {
            "trace" | "trc" | "verbose" | "finest" => LogLevel::Trace,
            "debug" | "dbg" | "fine" => LogLevel::Debug,
            "info" | "inf" | "information" | "informational" | "notice" => LogLevel::Info,
            "warn" | "wrn" | "warning" => LogLevel::Warn,
            "error" | "err" | "eror" | "severe" | "dpanic" => LogLevel::Error,
            "fatal" | "ftl" | "panic" | "critical" | "crit" | "alert" | "emerg" | "emergency" => {
                LogLevel::Fatal
            }
            _ => return None,
        };
        Some(lvl)
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LogLevel {
    type Err = UnknownValue;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse_name(s).ok_or_else(|| UnknownValue(s.to_string()))
    }
}

/// Output stream of a container line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

impl Stream {
    pub fn as_str(self) -> &'static str {
        match self {
            Stream::Stdout => "stdout",
            Stream::Stderr => "stderr",
        }
    }
}

impl FromStr for Stream {
    type Err = UnknownValue;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stdout" => Ok(Stream::Stdout),
            "stderr" => Ok(Stream::Stderr),
            _ => Err(UnknownValue(s.to_string())),
        }
    }
}

/// Where a line came from. `App` is every tenant container; the others are
/// the components of Shipyard's own stack.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogSource {
    App,
    Traefik,
    Postgres,
    Backend,
    Frontend,
    Collector,
}

impl LogSource {
    pub const ALL: [LogSource; 6] = [
        LogSource::App,
        LogSource::Traefik,
        LogSource::Postgres,
        LogSource::Backend,
        LogSource::Frontend,
        LogSource::Collector,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LogSource::App => "app",
            LogSource::Traefik => "traefik",
            LogSource::Postgres => "postgres",
            LogSource::Backend => "backend",
            LogSource::Frontend => "frontend",
            LogSource::Collector => "collector",
        }
    }
}

impl fmt::Display for LogSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for LogSource {
    type Err = UnknownValue;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim().to_ascii_lowercase();
        // The admin UI calls the Postgres tab "database".
        let s = if s == "database" { "postgres".to_string() } else { s };
        Self::ALL
            .into_iter()
            .find(|v| v.as_str() == s)
            .ok_or(UnknownValue(s))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown value `{0}`")]
pub struct UnknownValue(pub String);

/// Identity of a log line's origin, derived from container labels (or fixed
/// for platform components). `org_id = None` means a platform component.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogContext {
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
    pub deployment_id: Option<Uuid>,
    pub source: Option<LogSource>,
    pub container_id: Option<String>,
    pub replica: Option<i32>,
}

impl LogContext {
    pub fn source(&self) -> LogSource {
        self.source.unwrap_or(LogSource::App)
    }
}

/// A log line ready to be stored. `message` and `fields` can only be built by
/// the [`Masker`](crate::mask::Masker), so an unmasked entry cannot exist.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LogEntry {
    pub ts: DateTime<Utc>,
    pub org_id: Option<Uuid>,
    pub project_id: Option<Uuid>,
    pub service_id: Option<Uuid>,
    pub deployment_id: Option<Uuid>,
    pub source: LogSource,
    pub container_id: Option<String>,
    pub replica: Option<i32>,
    pub stream: Stream,
    pub level: LogLevel,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    pub message: MaskedText,
    pub fields: Option<MaskedJson>,
}

/// A stored line with its store-assigned id (used for cursors).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StoredLogEntry {
    pub id: i64,
    #[serde(flatten)]
    pub entry: LogEntry,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn level_round_trips_through_i16_and_is_ordered() {
        for l in LogLevel::ALL {
            assert_eq!(LogLevel::from_i16(l.as_i16()), Some(l));
            assert_eq!(l.to_string().parse::<LogLevel>().unwrap(), l);
        }
        assert!(LogLevel::Error > LogLevel::Warn);
        assert!(LogLevel::Fatal > LogLevel::Error);
        assert_eq!(LogLevel::from_i16(-1), None);
        assert_eq!(LogLevel::from_i16(6), None);
    }

    #[test]
    fn level_names_from_common_loggers() {
        let cases = [
            ("INFO", LogLevel::Info),
            ("Warning", LogLevel::Warn),
            ("WARN+2", LogLevel::Warn),
            ("INFO-4", LogLevel::Info),
            ("err", LogLevel::Error),
            ("dpanic", LogLevel::Error),
            ("panic", LogLevel::Fatal),
            ("CRITICAL", LogLevel::Fatal),
            ("notice", LogLevel::Info),
            ("verbose", LogLevel::Trace),
            ("dbg", LogLevel::Debug),
            (" emerg ", LogLevel::Fatal),
        ];
        for (input, want) in cases {
            assert_eq!(LogLevel::parse_name(input), Some(want), "{input}");
        }
        assert_eq!(LogLevel::parse_name("loud"), None);
        assert!("nope".parse::<LogLevel>().is_err());
    }

    #[test]
    fn level_serializes_lowercase() {
        assert_eq!(serde_json::to_string(&LogLevel::Warn).unwrap(), "\"warn\"");
    }

    #[test]
    fn stream_parses() {
        assert_eq!("STDERR".parse::<Stream>().unwrap(), Stream::Stderr);
        assert_eq!("stdout".parse::<Stream>().unwrap(), Stream::Stdout);
        assert_eq!(Stream::Stderr.as_str(), "stderr");
        assert!("stdin".parse::<Stream>().is_err());
    }

    #[test]
    fn source_parses_including_database_alias() {
        for s in LogSource::ALL {
            assert_eq!(s.as_str().parse::<LogSource>().unwrap(), s);
            assert_eq!(s.to_string(), s.as_str());
        }
        assert_eq!("Database".parse::<LogSource>().unwrap(), LogSource::Postgres);
        assert_eq!(
            "mystery".parse::<LogSource>(),
            Err(UnknownValue("mystery".into()))
        );
    }

    #[test]
    fn context_defaults_to_app_source() {
        assert_eq!(LogContext::default().source(), LogSource::App);
        let ctx = LogContext { source: Some(LogSource::Traefik), ..Default::default() };
        assert_eq!(ctx.source(), LogSource::Traefik);
    }
}
