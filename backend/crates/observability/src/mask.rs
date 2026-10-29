//! Sensitive-data masking. Every line passes through [`Masker`] before it is
//! stored; [`MaskedText`] and [`MaskedJson`] can only be produced here, which
//! makes "nothing unmasked is ever written" a property of the types.
//!
//! A masked span is replaced by `••• [kind]` (for example `••• [token]`); the
//! UI renders that marker as a chip. Key names and separators are kept so a
//! reader still sees *what* was hidden (`password=••• [secret]`).

use regex::Regex;
use serde::{Serialize, Serializer};
use serde_json::Value;

/// Glyph that starts every mask marker.
pub const MASK_GLYPH: &str = "•••";

/// Render the marker that replaces a masked span.
pub fn marker(kind: &str) -> String {
    format!("{MASK_GLYPH} [{kind}]")
}

/// Text that has been through the masker.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MaskedText(String);

impl MaskedText {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn into_string(self) -> String {
        self.0
    }
    /// Text read back from the store, which only ever holds masked text.
    pub fn from_store(s: String) -> Self {
        Self(s)
    }
}

impl Serialize for MaskedText {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.0)
    }
}

/// JSON (structured log fields) that has been through the masker.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MaskedJson(Value);

impl MaskedJson {
    pub fn as_value(&self) -> &Value {
        &self.0
    }
    pub fn into_value(self) -> Value {
        self.0
    }
    pub fn from_store(v: Value) -> Self {
        Self(v)
    }
}

impl Serialize for MaskedJson {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(s)
    }
}

/// An extra pattern (org-defined later). The whole match is masked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRule {
    pub kind: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MaskConfig {
    /// Also mask e-mail addresses (off by default: they are often useful).
    pub mask_emails: bool,
    pub extra: Vec<CustomRule>,
}

#[derive(Debug, thiserror::Error)]
pub enum MaskError {
    #[error("invalid mask pattern for `{kind}`: {source}")]
    InvalidPattern {
        kind: String,
        #[source]
        source: regex::Error,
    },
}

/// Key names whose value is a credential. Shared by the `key=value` text rule
/// and the JSON key rule.
const SENSITIVE_KEY: &str = r"(?:password|passwd|pwd|secret|token|api[_-]?key|private[_-]?key|access[_-]?key)";

/// One masking rule. When `group` is set only that capture group is hidden,
/// so the surrounding context (key, scheme, user) stays readable.
struct Rule {
    kind: String,
    re: Regex,
    group: Option<usize>,
    /// Extra validation on the matched span (Luhn for cards).
    check: Option<fn(&str) -> bool>,
}

/// Built-in rules in priority order: when two rules match at the same
/// position the earlier one wins.
fn builtin_rules(mask_emails: bool) -> Vec<(&'static str, String, Option<usize>, Option<fn(&str) -> bool>)> {
    let mut rules: Vec<(&'static str, String, Option<usize>, Option<fn(&str) -> bool>)> = vec![
        (
            "private_key",
            r"-----BEGIN [A-Z ]*PRIVATE KEY-----[\s\S]*?-----END [A-Z ]*PRIVATE KEY-----".into(),
            None,
            None,
        ),
        // scheme://user:PASSWORD@host
        ("password", r"\b[a-zA-Z][a-zA-Z0-9+.-]*://[^\s:/@]+:([^\s@/]+)@".into(), Some(1), None),
        ("jwt", r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+".into(), None, None),
        ("token", r"(?i)\bbearer\s+([A-Za-z0-9._~+/=-]{8,})".into(), Some(1), None),
        // Shipyard keys: ship_<64 hex>, ship_int_<hex>, whsec_<hex>. Alphanumeric
        // bodies only, so identifiers like `ship_container_logs` are left alone.
        ("token", r"\b(?:ship_int_|whsec_|ship_)[A-Za-z0-9]{16,}\b".into(), None, None),
        ("aws_key", r"\b(?:AKIA|ASIA)[A-Z0-9]{16}\b".into(), None, None),
        ("gcp_key", r"\bAIza[0-9A-Za-z_-]{35}".into(), None, None),
        (
            "secret",
            format!(r#"(?i)\b[\w.-]*?{SENSITIVE_KEY}[\w.-]*"?\s*[=:]\s*"?([^\s"'&,;}}]+)"#),
            Some(1),
            None,
        ),
        // Card-like: 13–19 digits (optionally space/dash grouped) starting with
        // a real issuer digit, and passing Luhn. Epoch millis start with 1.
        ("card", r"\b[2-6](?:[ -]?\d){12,18}\b".into(), None, Some(luhn_valid)),
    ];
    if mask_emails {
        rules.push(("email", r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b".into(), None, None));
    }
    rules
}

fn luhn_valid(s: &str) -> bool {
    let digits: Vec<u32> = s.chars().filter_map(|c| c.to_digit(10)).collect();
    if !(13..=19).contains(&digits.len()) {
        return false;
    }
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let x = d * 2;
                if x > 9 { x - 9 } else { x }
            } else {
                d
            }
        })
        .sum();
    sum % 10 == 0
}

pub struct Masker {
    rules: Vec<Rule>,
    sensitive_key: Regex,
}

impl Default for Masker {
    fn default() -> Self {
        Self::new(MaskConfig::default()).expect("built-in mask rules compile")
    }
}

impl Masker {
    pub fn new(cfg: MaskConfig) -> Result<Self, MaskError> {
        let compile = |kind: &str, pattern: &str| {
            Regex::new(pattern).map_err(|source| MaskError::InvalidPattern {
                kind: kind.to_string(),
                source,
            })
        };
        let mut rules = Vec::new();
        for (kind, pattern, group, check) in builtin_rules(cfg.mask_emails) {
            rules.push(Rule { kind: kind.to_string(), re: compile(kind, &pattern)?, group, check });
        }
        for extra in &cfg.extra {
            rules.push(Rule {
                kind: extra.kind.clone(),
                re: compile(&extra.kind, &extra.pattern)?,
                group: None,
                check: None,
            });
        }
        let sensitive_key = compile(
            "sensitive_key",
            &format!(r"(?i)(?:{SENSITIVE_KEY}|authorization|cookie)"),
        )?;
        Ok(Self { rules, sensitive_key })
    }

    /// Mask every sensitive span in `input`.
    pub fn mask(&self, input: &str) -> MaskedText {
        MaskedText(self.mask_str(input))
    }

    /// Mask structured fields: values under credential-like keys are replaced
    /// entirely; every other string is masked like a line.
    pub fn mask_json(&self, v: Value) -> MaskedJson {
        MaskedJson(self.mask_value(v))
    }

    fn mask_value(&self, v: Value) -> Value {
        match v {
            Value::String(s) => Value::String(self.mask_str(&s)),
            Value::Array(items) => Value::Array(items.into_iter().map(|i| self.mask_value(i)).collect()),
            Value::Object(map) => Value::Object(
                map.into_iter()
                    .map(|(k, val)| {
                        let val = if self.sensitive_key.is_match(&k) && !val.is_null() {
                            Value::String(marker("secret"))
                        } else {
                            self.mask_value(val)
                        };
                        (k, val)
                    })
                    .collect(),
            ),
            other => other,
        }
    }

    fn mask_str(&self, input: &str) -> String {
        // (start, end, priority, kind)
        let mut spans: Vec<(usize, usize, usize, &str)> = Vec::new();
        for (prio, rule) in self.rules.iter().enumerate() {
            for caps in rule.re.captures_iter(input) {
                let Some(m) = caps.get(rule.group.unwrap_or(0)) else { continue };
                let text = m.as_str();
                // Already masked (idempotence) or rejected by the validator.
                if text.starts_with(MASK_GLYPH) || rule.check.is_some_and(|f| !f(text)) {
                    continue;
                }
                spans.push((m.start(), m.end(), prio, rule.kind.as_str()));
            }
        }
        if spans.is_empty() {
            return input.to_string();
        }
        spans.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)).then(b.1.cmp(&a.1)));

        let mut out = String::with_capacity(input.len());
        let mut pos = 0;
        for (start, end, _, kind) in spans {
            if start < pos {
                continue; // overlaps a span already masked
            }
            out.push_str(&input[pos..start]);
            out.push_str(&marker(kind));
            pos = end;
        }
        out.push_str(&input[pos..]);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn m(s: &str) -> String {
        Masker::default().mask(s).into_string()
    }

    const JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U";

    #[test]
    fn masks_table() {
        let jwt_line = format!("session {JWT} ok");
        let cases: Vec<(&str, String, String)> = vec![
            // (name, input, expected)
            ("bearer header", "Authorization: Bearer abcDEF123456789xyz".into(),
             "Authorization: Bearer ••• [token]".into()),
            ("bearer lowercase", "bearer abcdefgh12345678".into(), "bearer ••• [token]".into()),
            ("integration key", "using ship_int_9f8e7d6c5b4a3210 now".into(),
             "using ••• [token] now".into()),
            ("webhook secret", "secret is whsec_AbCdEf0123456789".into(),
             "secret is ••• [token]".into()),
            ("shipyard key", "key ship_0192f3a4b5c6d7e8f9a0b1c2d3e4f5a6".into(), "key ••• [token]".into()),
            ("identifier is not a key", "fn ship_container_logs_v2()".into(),
             "fn ship_container_logs_v2()".into()),
            ("jwt", jwt_line, "session ••• [jwt] ok".into()),
            ("password pair", "login password=hunter2 user=bob".into(),
             "login password=••• [secret] user=bob".into()),
            ("env-style pair", "DB_PASSWORD=s3cr3t!".into(), "DB_PASSWORD=••• [secret]".into()),
            ("token colon", "token: abc123def".into(), "token: ••• [secret]".into()),
            ("client_secret query", "GET /cb?code=1&client_secret=xyz789&x=2".into(),
             "GET /cb?code=1&client_secret=••• [secret]&x=2".into()),
            ("json pair", r#"{"password":"hunter2","user":"bob"}"#.into(),
             r#"{"password":"••• [secret]","user":"bob"}"#.into()),
            ("api key pair", "api_key=AbC123".into(), "api_key=••• [secret]".into()),
            ("postgres url", "connecting to postgres://app:Sup3rS3cret@db:5432/app".into(),
             "connecting to postgres://app:••• [password]@db:5432/app".into()),
            ("redis url", "redis://default:pw123@cache:6379".into(),
             "redis://default:••• [password]@cache:6379".into()),
            ("aws access key", "aws key AKIAIOSFODNN7EXAMPLE used".into(),
             "aws key ••• [aws_key] used".into()),
            ("gcp api key", "key=AIzaSyDdI0hCZtE6vySjMm-WEfRq3CPzqKqqsHI".into(),
             "key=••• [gcp_key]".into()),
            ("private key", "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBg\n-----END PRIVATE KEY-----".into(),
             "••• [private_key]".into()),
            ("rsa private key", "k=-----BEGIN RSA PRIVATE KEY-----abc-----END RSA PRIVATE KEY----- done".into(),
             "k=••• [private_key] done".into()),
            ("visa card", "paid with 4111 1111 1111 1111 today".into(),
             "paid with ••• [card] today".into()),
            ("mastercard dashed", "card 5555-5555-5555-4444".into(), "card ••• [card]".into()),
            ("amex", "amex 378282246310005".into(), "amex ••• [card]".into()),
        ];
        for (name, input, want) in cases {
            assert_eq!(m(&input), want, "case `{name}`");
        }
    }

    #[test]
    fn leaves_harmless_text_alone() {
        let cases = [
            "GET /api/health 200 3ms",
            "user bob logged in from 10.0.0.1",
            "request_id=7f3a2c order 4111 shipped", // short number
            "epoch_ms 1730200000000",                // 13 digits, not a card
            "invoice 4111111111111112",              // fails Luhn
            "contact ops@example.com",               // emails kept by default
            "ship_ it",                              // prefix without key body
            "the token was refreshed",               // word, not a pair
            "https://example.com/path?x=1",          // url without credentials
        ];
        for input in cases {
            assert_eq!(m(input), input, "should be unchanged: {input}");
        }
    }

    #[test]
    fn emails_are_masked_when_enabled() {
        let masker = Masker::new(MaskConfig { mask_emails: true, ..Default::default() }).unwrap();
        assert_eq!(
            masker.mask("contact ops@example.com now").as_str(),
            "contact ••• [email] now"
        );
    }

    #[test]
    fn connection_string_wins_over_email() {
        let masker = Masker::new(MaskConfig { mask_emails: true, ..Default::default() }).unwrap();
        assert_eq!(
            masker.mask("mysql://root:pw@db.example.com/x").as_str(),
            "mysql://root:••• [password]@db.example.com/x"
        );
    }

    #[test]
    fn masks_every_occurrence() {
        assert_eq!(
            m("a=ship_int_aaaaaaaa11112222 b=ship_int_bbbbbbbb33334444"),
            "a=••• [token] b=••• [token]"
        );
    }

    #[test]
    fn masking_is_idempotent() {
        let once = m("password=hunter2 Bearer abcdefgh12345678 postgres://u:p@h/db");
        assert_eq!(m(&once), once);
    }

    #[test]
    fn pair_value_that_is_a_jwt_is_masked_once() {
        assert_eq!(m(&format!("token={JWT}")), "token=••• [jwt]");
    }

    #[test]
    fn custom_rules_apply_and_bad_patterns_are_rejected() {
        let masker = Masker::new(MaskConfig {
            mask_emails: false,
            extra: vec![CustomRule { kind: "acct".into(), pattern: r"ACCT-\d{6}".into() }],
        })
        .unwrap();
        assert_eq!(masker.mask("ref ACCT-123456").as_str(), "ref ••• [acct]");

        let err = Masker::new(MaskConfig {
            mask_emails: false,
            extra: vec![CustomRule { kind: "bad".into(), pattern: "(".into() }],
        })
        .err()
        .expect("invalid pattern must fail");
        assert!(err.to_string().contains("bad"));
    }

    #[test]
    fn masks_json_values_and_sensitive_keys() {
        let masker = Masker::default();
        let out = masker
            .mask_json(json!({
                "user": "bob",
                "password": "hunter2",
                "headers": { "authorization": "Bearer abcdefgh12345678", "x-api-key": 12345 },
                "nested": [{ "client_secret": "s" }, "token ship_int_aaaaaaaa11112222"],
                "count": 3,
                "ok": true,
                "none": null
            }))
            .into_value();
        assert_eq!(
            out,
            json!({
                "user": "bob",
                "password": "••• [secret]",
                "headers": { "authorization": "••• [secret]", "x-api-key": "••• [secret]" },
                "nested": [{ "client_secret": "••• [secret]" }, "token ••• [token]"],
                "count": 3,
                "ok": true,
                "none": null
            })
        );
    }

    #[test]
    fn masked_types_serialize_transparently() {
        let t = Masker::default().mask("hi");
        assert_eq!(serde_json::to_string(&t).unwrap(), "\"hi\"");
        let j = Masker::default().mask_json(json!({"a": 1}));
        assert_eq!(serde_json::to_string(&j).unwrap(), r#"{"a":1}"#);
        assert_eq!(MaskedText::from_store("x".into()).as_str(), "x");
        assert_eq!(MaskedJson::from_store(json!(1)).as_value(), &json!(1));
    }
}
