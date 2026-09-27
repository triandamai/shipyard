use serde::{de::DeserializeOwned, Deserialize, Serialize};
use shipyard_common::error::{AppError, AppResult};

const API_BASE: &str = "https://api.cloudflare.com/client/v4";

#[derive(Debug, Clone, Deserialize)]
pub struct Zone {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Account {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DnsRecord {
    pub id: String,
}

#[derive(Debug, Deserialize)]
struct CfEnvelope<T> {
    success: bool,
    #[serde(default)]
    errors: Vec<CfApiError>,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct CfApiError {
    code: i64,
    message: String,
}

/// Parses a Cloudflare API JSON response body. Cloudflare wraps every
/// response (success or failure) in `{ success, errors, result }` — this
/// unwraps that envelope into either the deserialized result or an
/// `AppError::Cloudflare` built from the `errors` array, independent of any
/// actual network call so it's fully unit-testable with static strings.
fn parse_cf_response<T: DeserializeOwned>(body: &str) -> AppResult<T> {
    let envelope: CfEnvelope<T> = serde_json::from_str(body)
        .map_err(|e| AppError::Cloudflare(format!("invalid response from Cloudflare: {e}")))?;

    if envelope.success {
        envelope
            .result
            .ok_or_else(|| AppError::Cloudflare("Cloudflare returned success with no result".to_string()))
    } else {
        let msg = envelope
            .errors
            .iter()
            .map(|e| format!("{} ({})", e.message, e.code))
            .collect::<Vec<_>>()
            .join("; ");
        Err(AppError::Cloudflare(if msg.is_empty() {
            "Cloudflare API call failed".to_string()
        } else {
            msg
        }))
    }
}

#[cfg(test)]
mod parse_cf_response_tests {
    use super::*;

    #[derive(Debug, Deserialize, PartialEq)]
    struct Thing {
        id: String,
    }

    #[test]
    fn parses_a_successful_response_into_its_result() {
        let body = r#"{"success":true,"errors":[],"result":{"id":"abc123"}}"#;
        let thing: Thing = parse_cf_response(body).expect("must parse");
        assert_eq!(thing, Thing { id: "abc123".to_string() });
    }

    #[test]
    fn maps_a_failed_response_to_a_cloudflare_error_with_the_real_message() {
        let body = r#"{"success":false,"errors":[{"code":1003,"message":"Invalid zone identifier"}],"result":null}"#;
        let err = parse_cf_response::<Thing>(body).unwrap_err();
        match err {
            AppError::Cloudflare(msg) => {
                assert!(msg.contains("Invalid zone identifier"), "got: {msg}");
                assert!(msg.contains("1003"), "got: {msg}");
            }
            other => panic!("expected AppError::Cloudflare, got {other:?}"),
        }
    }

    #[test]
    fn maps_a_failed_response_with_no_errors_array_entries_to_a_generic_message() {
        let body = r#"{"success":false,"errors":[],"result":null}"#;
        let err = parse_cf_response::<Thing>(body).unwrap_err();
        match err {
            AppError::Cloudflare(msg) => assert_eq!(msg, "Cloudflare API call failed"),
            other => panic!("expected AppError::Cloudflare, got {other:?}"),
        }
    }

    #[test]
    fn rejects_malformed_json_as_a_cloudflare_error_not_a_panic() {
        let err = parse_cf_response::<Thing>("not json at all").unwrap_err();
        assert!(matches!(err, AppError::Cloudflare(_)));
    }
}

pub struct CloudflareClient {
    token: String,
    client: reqwest::Client,
}

impl CloudflareClient {
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            client: reqwest::Client::new(),
        }
    }

    fn authed(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.bearer_auth(&self.token)
    }

    async fn get<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        let url = format!("{API_BASE}{path}");
        let resp = self
            .authed(self.client.get(&url))
            .send()
            .await
            .map_err(|e| AppError::Cloudflare(format!("request to {url} failed: {e}")))?;
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::Cloudflare(format!("reading response from {url} failed: {e}")))?;
        parse_cf_response(&body)
    }

    /// GET /user/tokens/verify — confirms the token is valid and active.
    /// Discards the actual result payload; only success/failure matters here.
    pub async fn verify_token(&self) -> AppResult<()> {
        self.get::<serde_json::Value>("/user/tokens/verify").await?;
        Ok(())
    }

    /// GET /accounts — the token-verify endpoint doesn't return account
    /// details, so resolving which Cloudflare account(s) a token can act on
    /// needs this separate call.
    pub async fn list_accounts(&self) -> AppResult<Vec<Account>> {
        self.get("/accounts").await
    }

    /// GET /zones — every zone (domain) this token can manage.
    pub async fn list_zones(&self) -> AppResult<Vec<Zone>> {
        self.get("/zones").await
    }
}

#[derive(Debug, Serialize)]
struct DnsRecordBody<'a> {
    #[serde(rename = "type")]
    record_type: &'a str,
    name: &'a str,
    content: &'a str,
    ttl: u32,
    proxied: bool,
}

impl<'a> DnsRecordBody<'a> {
    /// Every record this crate ever writes is a DNS-only (never proxied) `A`
    /// record with a 300s TTL — see the plan's Global Constraints for why
    /// "never proxied" matters (it would break Traefik's own ACME flow).
    fn a_record(name: &'a str, content: &'a str) -> Self {
        Self { record_type: "A", name, content, ttl: 300, proxied: false }
    }
}

impl CloudflareClient {
    async fn send_json<T: DeserializeOwned>(
        &self,
        req: reqwest::RequestBuilder,
    ) -> AppResult<T> {
        let resp = self
            .authed(req)
            .send()
            .await
            .map_err(|e| AppError::Cloudflare(format!("request failed: {e}")))?;
        let body = resp
            .text()
            .await
            .map_err(|e| AppError::Cloudflare(format!("reading response failed: {e}")))?;
        parse_cf_response(&body)
    }

    /// POST /zones/:zone_id/dns_records
    pub async fn create_dns_record(&self, zone_id: &str, name: &str, content: &str) -> AppResult<DnsRecord> {
        let url = format!("{API_BASE}/zones/{zone_id}/dns_records");
        let body = DnsRecordBody::a_record(name, content);
        self.send_json(self.client.post(&url).json(&body)).await
    }

    /// PUT /zones/:zone_id/dns_records/:id — Cloudflare requires the full
    /// record body on every update, not just the changed field, so this
    /// takes `name` too even though it's normally unchanged across a
    /// re-sync (only `content`, the target IP, actually changes in
    /// practice — see the sandbox preview-DNS sync in Task 6).
    pub async fn update_dns_record(&self, zone_id: &str, record_id: &str, name: &str, content: &str) -> AppResult<DnsRecord> {
        let url = format!("{API_BASE}/zones/{zone_id}/dns_records/{record_id}");
        let body = DnsRecordBody::a_record(name, content);
        self.send_json(self.client.put(&url).json(&body)).await
    }

    /// DELETE /zones/:zone_id/dns_records/:id
    pub async fn delete_dns_record(&self, zone_id: &str, record_id: &str) -> AppResult<()> {
        let url = format!("{API_BASE}/zones/{zone_id}/dns_records/{record_id}");
        self.send_json::<serde_json::Value>(self.client.delete(&url)).await?;
        Ok(())
    }
}
