//! Line parsing: level, trace context and message extraction.
//!
//! Order of evidence (never guessing from words in free text):
//! 1. JSON object lines: `level` / `severity` / `lvl` (names or pino/bunyan
//!    and slog numbers), message from `msg` / `message` / `fields.message`.
//! 2. logfmt lines: `level=` / `lvl=` / `severity=`.
//! 3. The stream: `stderr` → `warn`, `stdout` → `info`.

use serde_json::{Map, Value};

use crate::types::{LogLevel, Stream};

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedLine {
    pub level: LogLevel,
    pub message: String,
    pub trace_id: Option<String>,
    pub span_id: Option<String>,
    /// Remaining structured fields of a JSON line (message key removed).
    pub fields: Option<Value>,
}

const LEVEL_KEYS: [&str; 3] = ["level", "severity", "lvl"];
const MESSAGE_KEYS: [&str; 2] = ["msg", "message"];
const TRACE_KEYS: [&str; 3] = ["trace_id", "traceId", "trace.id"];
const SPAN_KEYS: [&str; 3] = ["span_id", "spanId", "span.id"];

pub fn parse_line(raw: &str, stream: Stream) -> ParsedLine {
    let fallback = match stream {
        Stream::Stderr => LogLevel::Warn,
        Stream::Stdout => LogLevel::Info,
    };
    let trimmed = raw.trim();

    if trimmed.starts_with('{') {
        if let Ok(Value::Object(obj)) = serde_json::from_str::<Value>(trimmed) {
            return parse_json(raw, obj, fallback);
        }
    }

    let pairs = logfmt_pairs(trimmed);
    let get = |keys: &[&str]| {
        pairs
            .iter()
            .find(|(k, _)| keys.contains(&k.as_str()))
            .map(|(_, v)| v.as_str())
    };
    let level = get(&LEVEL_KEYS).and_then(LogLevel::parse_name).unwrap_or(fallback);
    let (trace_id, span_id) = trace_context(
        get(&TRACE_KEYS),
        get(&SPAN_KEYS),
        get(&["traceparent"]),
    );
    ParsedLine { level, message: raw.to_string(), trace_id, span_id, fields: None }
}

fn parse_json(raw: &str, mut obj: Map<String, Value>, fallback: LogLevel) -> ParsedLine {
    let level = LEVEL_KEYS
        .iter()
        .find_map(|k| obj.get(*k))
        .and_then(level_from_json)
        .unwrap_or(fallback);

    // Trace context: top level, then tracing-subscriber's `fields` / `span`.
    let (mut trace_id, mut span_id) = (None, None);
    for scope in [None, Some("fields"), Some("span")] {
        let map = match scope {
            None => Some(&obj),
            Some(k) => obj.get(k).and_then(Value::as_object),
        };
        let Some(map) = map else { continue };
        let s = |keys: &[&str]| keys.iter().find_map(|k| map.get(*k)).and_then(Value::as_str);
        let (t, sp) = trace_context(s(&TRACE_KEYS), s(&SPAN_KEYS), s(&["traceparent"]));
        if t.is_some() {
            trace_id = t;
            span_id = sp;
            break;
        }
    }

    let mut message = MESSAGE_KEYS.iter().find_map(|k| obj.remove(*k));
    if message.is_none() {
        if let Some(Value::Object(fields)) = obj.get_mut("fields") {
            message = fields.remove("message");
            if fields.is_empty() {
                obj.remove("fields");
            }
        }
    }
    let message = match message {
        Some(Value::String(s)) => s,
        Some(other) => other.to_string(),
        None => raw.to_string(),
    };
    let fields = (!obj.is_empty()).then_some(Value::Object(obj));
    ParsedLine { level, message, trace_id, span_id, fields }
}

/// Resolve trace/span ids from explicit ids or a W3C `traceparent`.
fn trace_context(
    trace: Option<&str>,
    span: Option<&str>,
    traceparent: Option<&str>,
) -> (Option<String>, Option<String>) {
    if let Some(t) = trace.and_then(valid_trace_id) {
        return (Some(t), span.and_then(valid_span_id));
    }
    match traceparent.and_then(parse_traceparent) {
        Some((t, s)) => (Some(t), Some(s)),
        None => (None, None),
    }
}

pub fn level_from_json(v: &Value) -> Option<LogLevel> {
    match v {
        Value::String(s) => LogLevel::parse_name(s)
            .or_else(|| s.trim().parse::<i64>().ok().and_then(level_from_number)),
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f.round() as i64))
            .and_then(level_from_number),
        _ => None,
    }
}

/// Numeric levels: pino/bunyan use 10..60; Go's slog uses -4/0/4/8.
pub fn level_from_number(n: i64) -> Option<LogLevel> {
    let lvl = match n {
        60.. => LogLevel::Fatal,
        50..=59 => LogLevel::Error,
        40..=49 => LogLevel::Warn,
        30..=39 => LogLevel::Info,
        20..=29 => LogLevel::Debug,
        10..=19 => LogLevel::Trace,
        8..=9 => LogLevel::Error,
        4..=7 => LogLevel::Warn,
        0..=3 => LogLevel::Info,
        _ => LogLevel::Debug,
    };
    Some(lvl)
}

/// `00-<32 hex trace>-<16 hex span>-<2 hex flags>`
pub fn parse_traceparent(s: &str) -> Option<(String, String)> {
    let parts: Vec<&str> = s.trim().split('-').collect();
    if parts.len() != 4 || parts[0].len() != 2 || parts[3].len() != 2 {
        return None;
    }
    Some((valid_trace_id(parts[1])?, valid_span_id(parts[2])?))
}

fn valid_hex_id(s: &str, len: usize) -> Option<String> {
    let s = s.trim();
    let ok = s.len() == len
        && s.bytes().all(|b| b.is_ascii_hexdigit())
        && s.bytes().any(|b| b != b'0');
    ok.then(|| s.to_ascii_lowercase())
}

pub fn valid_trace_id(s: &str) -> Option<String> {
    valid_hex_id(s, 32)
}

pub fn valid_span_id(s: &str) -> Option<String> {
    valid_hex_id(s, 16)
}

/// Parse `key=value` pairs (values may be double-quoted with `\"` escapes).
/// Bare words are skipped.
pub fn logfmt_pairs(s: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();
    loop {
        while chars.next_if(|c| c.is_whitespace()).is_some() {}
        if chars.peek().is_none() {
            break;
        }
        let mut key = String::new();
        while let Some(c) = chars.next_if(|c| !c.is_whitespace() && *c != '=') {
            key.push(c);
        }
        if chars.next_if_eq(&'=').is_none() || key.is_empty() {
            // bare word (or `=value` without key): skip to next whitespace
            while chars.next_if(|c| !c.is_whitespace()).is_some() {}
            continue;
        }
        let mut value = String::new();
        if chars.next_if_eq(&'"').is_some() {
            while let Some(c) = chars.next() {
                match c {
                    '\\' => {
                        if let Some(n) = chars.next() {
                            value.push(n);
                        }
                    }
                    '"' => break,
                    c => value.push(c),
                }
            }
        } else {
            while let Some(c) = chars.next_if(|c| !c.is_whitespace()) {
                value.push(c);
            }
        }
        out.push((key, value));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TID: &str = "4bf92f3577b34da6a3ce929d0e0e4736";
    const SID: &str = "00f067aa0ba902b7";

    fn level(raw: &str, stream: Stream) -> LogLevel {
        parse_line(raw, stream).level
    }

    #[test]
    fn json_level_keys_and_names() {
        let cases = [
            (r#"{"level":"error","msg":"boom"}"#, LogLevel::Error),
            (r#"{"severity":"WARNING","message":"careful"}"#, LogLevel::Warn),
            (r#"{"lvl":"dbug"}"#, LogLevel::Info), // unknown name → stream fallback
            (r#"{"lvl":"debug"}"#, LogLevel::Debug),
            (r#"{"level":"INFO","fields":{"message":"tracing"}}"#, LogLevel::Info),
            (r#"{"level":"WARN+2","msg":"slog offset"}"#, LogLevel::Warn),
            (r#"{"level":"dpanic"}"#, LogLevel::Error), // zap
        ];
        for (raw, want) in cases {
            assert_eq!(level(raw, Stream::Stdout), want, "{raw}");
        }
    }

    #[test]
    fn json_numeric_levels() {
        // pino / bunyan
        for (n, want) in [
            (10, LogLevel::Trace),
            (20, LogLevel::Debug),
            (30, LogLevel::Info),
            (40, LogLevel::Warn),
            (50, LogLevel::Error),
            (60, LogLevel::Fatal),
        ] {
            assert_eq!(level_from_number(n), Some(want), "pino {n}");
            assert_eq!(level(&format!(r#"{{"level":{n},"msg":"x"}}"#), Stream::Stdout), want);
        }
        // slog
        for (n, want) in [(-4, LogLevel::Debug), (0, LogLevel::Info), (4, LogLevel::Warn), (8, LogLevel::Error)] {
            assert_eq!(level_from_number(n), Some(want), "slog {n}");
        }
        assert_eq!(level_from_number(-8), Some(LogLevel::Debug));
        assert_eq!(level_from_number(1000), Some(LogLevel::Fatal));
        assert_eq!(level_from_json(&json!(true)), None);
        assert_eq!(level_from_json(&json!("50")), Some(LogLevel::Error)); // numeric string
        assert_eq!(level_from_json(&json!(30.0)), Some(LogLevel::Info));
    }

    #[test]
    fn json_message_and_fields() {
        let p = parse_line(r#"{"level":"info","msg":"hello","user":"bob"}"#, Stream::Stdout);
        assert_eq!(p.message, "hello");
        assert_eq!(p.fields, Some(json!({"level":"info","user":"bob"})));

        let p = parse_line(r#"{"level":"INFO","fields":{"message":"from tracing","n":1}}"#, Stream::Stdout);
        assert_eq!(p.message, "from tracing");
        assert_eq!(p.fields, Some(json!({"level":"INFO","fields":{"n":1}})));

        // No message key: the whole line is the message, fields kept.
        let raw = r#"{"level":"warn","event":"x"}"#;
        let p = parse_line(raw, Stream::Stdout);
        assert_eq!(p.message, raw);
        assert_eq!(p.fields, Some(json!({"level":"warn","event":"x"})));

        // Non-string message values are rendered as JSON text.
        let p = parse_line(r#"{"msg":{"a":1}}"#, Stream::Stdout);
        assert_eq!(p.message, r#"{"a":1}"#);
    }

    #[test]
    fn non_object_json_is_plain_text() {
        let p = parse_line("[1,2,3]", Stream::Stdout);
        assert_eq!(p.message, "[1,2,3]");
        assert_eq!(p.fields, None);
        assert_eq!(p.level, LogLevel::Info);
    }

    #[test]
    fn logfmt_levels() {
        assert_eq!(level("time=1 level=error msg=\"db down\"", Stream::Stdout), LogLevel::Error);
        assert_eq!(level("lvl=warn msg=slow", Stream::Stdout), LogLevel::Warn);
        assert_eq!(level("severity=\"debug\" x=1", Stream::Stderr), LogLevel::Debug);
        let p = parse_line("level=info msg=\"hi there\"", Stream::Stdout);
        assert_eq!(p.message, "level=info msg=\"hi there\"", "logfmt keeps the raw line");
        assert_eq!(p.fields, None);
    }

    #[test]
    fn logfmt_pairs_parse_quotes_and_escapes() {
        assert_eq!(
            logfmt_pairs(r#"a=1 b="two words" c="esc \"q\"" flag d="#),
            vec![
                ("a".into(), "1".into()),
                ("b".into(), "two words".into()),
                ("c".into(), "esc \"q\"".into()),
                ("d".into(), "".into()),
            ]
        );
        assert!(logfmt_pairs("just words here").is_empty());
    }

    #[test]
    fn stream_fallback_never_guesses_from_words() {
        assert_eq!(level("ERROR: something failed", Stream::Stdout), LogLevel::Info);
        assert_eq!(level("all good", Stream::Stderr), LogLevel::Warn);
        assert_eq!(level("", Stream::Stdout), LogLevel::Info);
    }

    #[test]
    fn explicit_level_beats_stream() {
        assert_eq!(level(r#"{"level":"info","msg":"x"}"#, Stream::Stderr), LogLevel::Info);
        assert_eq!(level("level=debug", Stream::Stderr), LogLevel::Debug);
    }

    #[test]
    fn trace_ids_from_json_keys() {
        for raw in [
            format!(r#"{{"trace_id":"{TID}","span_id":"{SID}"}}"#),
            format!(r#"{{"traceId":"{}","spanId":"{SID}"}}"#, TID.to_uppercase()),
            format!(r#"{{"traceparent":"00-{TID}-{SID}-01"}}"#),
            format!(r#"{{"fields":{{"trace_id":"{TID}","span_id":"{SID}"}}}}"#),
            format!(r#"{{"span":{{"trace_id":"{TID}","span_id":"{SID}"}}}}"#),
        ] {
            let p = parse_line(&raw, Stream::Stdout);
            assert_eq!(p.trace_id.as_deref(), Some(TID), "{raw}");
            assert_eq!(p.span_id.as_deref(), Some(SID), "{raw}");
        }
    }

    #[test]
    fn trace_ids_from_logfmt() {
        let p = parse_line(&format!("level=info trace_id={TID} span_id={SID}"), Stream::Stdout);
        assert_eq!(p.trace_id.as_deref(), Some(TID));
        assert_eq!(p.span_id.as_deref(), Some(SID));
        let p = parse_line(&format!("traceparent=00-{TID}-{SID}-01"), Stream::Stdout);
        assert_eq!(p.trace_id.as_deref(), Some(TID));
    }

    #[test]
    fn invalid_trace_ids_are_dropped() {
        assert_eq!(valid_trace_id("00000000000000000000000000000000"), None);
        assert_eq!(valid_trace_id("xyz"), None);
        assert_eq!(valid_trace_id(&format!("{TID}0")), None);
        assert_eq!(valid_span_id("0000000000000000"), None);
        assert_eq!(valid_span_id("00f067aa0ba902bz"), None);
        assert_eq!(parse_traceparent("00-abc-def-01"), None);
        assert_eq!(parse_traceparent(&format!("00-{TID}-{SID}")), None);
        assert_eq!(
            parse_traceparent(&format!(" 00-{TID}-{SID}-01 ")),
            Some((TID.to_string(), SID.to_string()))
        );
        let p = parse_line(r#"{"trace_id":"nope"}"#, Stream::Stdout);
        assert_eq!(p.trace_id, None);
    }
}
