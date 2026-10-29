//! Ingest pipeline: raw container line → masked [`LogEntry`].
//!
//! This is the only constructor of `LogEntry` used by the collector, so every
//! stored line has been parsed and masked here.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::mask::Masker;
use crate::types::{LogContext, LogEntry, Stream};

/// Lines longer than this are cut (on a char boundary) and flagged with
/// `fields._truncated = true`.
pub const MAX_MESSAGE_BYTES: usize = 16 * 1024;

pub fn build_entry(
    ctx: &LogContext,
    stream: Stream,
    ts: DateTime<Utc>,
    raw: &str,
    masker: &Masker,
) -> LogEntry {
    let line = strip_ansi(raw.trim_end_matches(['\r', '\n']));
    let parsed = crate::parse::parse_line(&line, stream);

    let mut message = parsed.message;
    let mut fields = parsed.fields;
    if message.len() > MAX_MESSAGE_BYTES {
        let mut cut = MAX_MESSAGE_BYTES;
        while !message.is_char_boundary(cut) {
            cut -= 1;
        }
        message.truncate(cut);
        let mut obj = match fields {
            Some(Value::Object(o)) => o,
            _ => serde_json::Map::new(),
        };
        obj.insert("_truncated".into(), Value::Bool(true));
        fields = Some(Value::Object(obj));
    }

    LogEntry {
        ts,
        org_id: ctx.org_id,
        project_id: ctx.project_id,
        service_id: ctx.service_id,
        deployment_id: ctx.deployment_id,
        source: ctx.source(),
        container_id: ctx.container_id.clone(),
        replica: ctx.replica,
        stream,
        level: parsed.level,
        trace_id: parsed.trace_id,
        span_id: parsed.span_id,
        message: masker.mask(&message),
        fields: fields.map(|f| masker.mask_json(f)),
    }
}

/// Split Docker's `timestamps=true` prefix (`2024-05-01T10:00:00.123456789Z msg`).
pub fn split_docker_timestamp(line: &str) -> (Option<DateTime<Utc>>, &str) {
    let Some((head, rest)) = line.split_once(' ') else {
        return (None, line);
    };
    match DateTime::parse_from_rfc3339(head) {
        Ok(ts) => (Some(ts.with_timezone(&Utc)), rest),
        Err(_) => (None, line),
    }
}

/// Remove ANSI escape sequences (colors, cursor movement, OSC titles).
pub fn strip_ansi(s: &str) -> String {
    if !s.contains('\u{1b}') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters, then one final byte in 0x40..=0x7E.
            Some('[') => {
                for c in chars.by_ref() {
                    if ('\u{40}'..='\u{7e}').contains(&c) {
                        break;
                    }
                }
            }
            // OSC: until BEL or ESC \.
            Some(']') => {
                while let Some(c) = chars.next() {
                    if c == '\u{7}' {
                        break;
                    }
                    if c == '\u{1b}' {
                        chars.next_if_eq(&'\\');
                        break;
                    }
                }
            }
            _ => {} // two-char escape or trailing ESC
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{LogLevel, LogSource};
    use chrono::TimeZone;
    use serde_json::json;
    use uuid::Uuid;

    fn ctx() -> LogContext {
        LogContext {
            org_id: Some(Uuid::nil()),
            project_id: Some(Uuid::from_u128(1)),
            service_id: Some(Uuid::from_u128(2)),
            deployment_id: Some(Uuid::from_u128(3)),
            source: Some(LogSource::App),
            container_id: Some("abc123".into()),
            replica: Some(2),
        }
    }

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 10, 29, 12, 0, 0).unwrap()
    }

    #[test]
    fn builds_entry_with_context_and_parsed_fields() {
        let raw = r#"{"level":"error","msg":"db failed","trace_id":"4bf92f3577b34da6a3ce929d0e0e4736","user":"bob"}"#;
        let e = build_entry(&ctx(), Stream::Stdout, now(), raw, &Masker::default());
        assert_eq!(e.ts, now());
        assert_eq!(e.org_id, Some(Uuid::nil()));
        assert_eq!(e.project_id, Some(Uuid::from_u128(1)));
        assert_eq!(e.service_id, Some(Uuid::from_u128(2)));
        assert_eq!(e.deployment_id, Some(Uuid::from_u128(3)));
        assert_eq!(e.source, LogSource::App);
        assert_eq!(e.container_id.as_deref(), Some("abc123"));
        assert_eq!(e.replica, Some(2));
        assert_eq!(e.stream, Stream::Stdout);
        assert_eq!(e.level, LogLevel::Error);
        assert_eq!(e.trace_id.as_deref(), Some("4bf92f3577b34da6a3ce929d0e0e4736"));
        assert_eq!(e.message.as_str(), "db failed");
        assert_eq!(
            e.fields.unwrap().into_value(),
            json!({"level":"error","trace_id":"4bf92f3577b34da6a3ce929d0e0e4736","user":"bob"})
        );
    }

    #[test]
    fn secrets_never_reach_the_entry() {
        let secret = "hunter2-very-secret";
        let raw = format!(r#"{{"level":"info","msg":"login password={secret}","password":"{secret}","nested":{{"token":"{secret}"}}}}"#);
        let e = build_entry(&ctx(), Stream::Stdout, now(), &raw, &Masker::default());
        let stored = serde_json::to_string(&e).unwrap();
        assert!(!stored.contains(secret), "unmasked secret stored: {stored}");
        assert_eq!(e.message.as_str(), "login password=••• [secret]");

        let plain = format!("connect postgres://u:{secret}@db/x");
        let e = build_entry(&ctx(), Stream::Stderr, now(), &plain, &Masker::default());
        assert!(!serde_json::to_string(&e).unwrap().contains(secret));
        assert_eq!(e.level, LogLevel::Warn);
    }

    #[test]
    fn strips_ansi_and_trailing_newlines() {
        let e = build_entry(
            &ctx(),
            Stream::Stdout,
            now(),
            "\u{1b}[32mINFO\u{1b}[0m ready\r\n",
            &Masker::default(),
        );
        assert_eq!(e.message.as_str(), "INFO ready");
        assert_eq!(e.fields, None);
    }

    #[test]
    fn truncates_long_lines_on_char_boundary() {
        let raw = "é".repeat(MAX_MESSAGE_BYTES); // 2 bytes each
        let e = build_entry(&ctx(), Stream::Stdout, now(), &raw, &Masker::default());
        assert!(e.message.as_str().len() <= MAX_MESSAGE_BYTES);
        assert!(e.message.as_str().chars().all(|c| c == 'é'));
        assert_eq!(e.fields.unwrap().into_value(), json!({"_truncated": true}));
    }

    #[test]
    fn truncation_flag_merges_into_json_fields() {
        let raw = format!(r#"{{"level":"info","msg":"{}","k":1}}"#, "a".repeat(MAX_MESSAGE_BYTES + 10));
        let e = build_entry(&ctx(), Stream::Stdout, now(), &raw, &Masker::default());
        assert_eq!(e.message.as_str().len(), MAX_MESSAGE_BYTES);
        assert_eq!(e.fields.unwrap().into_value(), json!({"level":"info","k":1,"_truncated":true}));
    }

    #[test]
    fn platform_context_has_no_org() {
        let c = LogContext { source: Some(LogSource::Traefik), ..Default::default() };
        let e = build_entry(&c, Stream::Stdout, now(), "x", &Masker::default());
        assert_eq!(e.org_id, None);
        assert_eq!(e.source, LogSource::Traefik);
    }

    #[test]
    fn docker_timestamp_prefix() {
        let (ts, rest) = split_docker_timestamp("2026-10-29T12:00:00.123456789Z hello world");
        assert_eq!(ts.unwrap().timestamp_subsec_nanos(), 123_456_789);
        assert_eq!(rest, "hello world");

        let (ts, rest) = split_docker_timestamp("2026-10-29T12:00:00Z ");
        assert_eq!(ts, Some(now()));
        assert_eq!(rest, "");

        let (ts, rest) = split_docker_timestamp("not a timestamp here");
        assert_eq!(ts, None);
        assert_eq!(rest, "not a timestamp here");

        let (ts, rest) = split_docker_timestamp("nospace");
        assert_eq!(ts, None);
        assert_eq!(rest, "nospace");
    }

    #[test]
    fn strip_ansi_variants() {
        assert_eq!(strip_ansi("\u{1b}[1;31merr\u{1b}[0m"), "err");
        assert_eq!(strip_ansi("\u{1b}[2K\u{1b}[1Gline"), "line");
        assert_eq!(strip_ansi("\u{1b}]0;title\u{7}text"), "text");
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("trailing \u{1b}"), "trailing ");
    }
}
