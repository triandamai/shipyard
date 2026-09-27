# Cloudflare DNS Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically create/delete Cloudflare DNS records when a domain is added/removed from a service, and let a superadmin sync a one-time wildcard DNS record for the sandbox runtime's preview base domain — both via per-organization Cloudflare API token connections, always best-effort so nothing about today's manual-DNS behavior ever breaks.

**Architecture:** A new, domain-agnostic `shipyard-cloudflare` crate wraps exactly the four Cloudflare API calls this feature needs (verify token, list zones/accounts, create/update/delete a DNS record). `crates/api` gets a new per-org `cloudflare_connections` table + CRUD module (mirroring the existing `git_providers` pattern exactly), two new nullable columns on `domains`, and one new step each in the existing `create_domain`/`delete_domain` handlers that's entirely additive — any failure anywhere in the Cloudflare path is logged and swallowed, never blocking the existing domain create/delete flow. The sandbox preview wildcard record is a separate, platform-wide concern (a superadmin picks which org's connection owns it, then triggers a sync), reusing the existing `system_config` settings mechanism, not a new one.

**Tech Stack:** Rust (axum, sqlx/Postgres, reqwest), SvelteKit 5 (runes), matching the existing Shipyard backend/frontend stack throughout.

**Spec:** `docs/superpowers/specs/2026-09-27-cloudflare-dns-design.md`

## Global Constraints

- Every Cloudflare-created DNS record is type `A`, `proxied: false` ("DNS only"), TTL `300` — never proxied, since a proxied record would break Traefik's existing Let's Encrypt HTTP-01 issuance.
- Cloudflare API calls are always best-effort: any failure (no connection, no matching zone, API error, network failure) is logged via `tracing::warn!` and swallowed — `create_domain`/`delete_domain` must succeed exactly as they do today regardless of Cloudflare's state.
- `cloudflare_connections` is one row per org (`UNIQUE (org_id)`), plaintext `api_token` column — matches the existing `git_providers.token` convention exactly; no new at-rest encryption is introduced by this plan.
- The `/orgs/:org_id/cloudflare` routes are gated by `shipyard:{org_id}:providers:write` OR `shipyard:{org_id}:settings:write` (write) / the `:read` equivalents (read) — the exact same OR-permission shape `git_providers` already uses, not a new permission string.
- The `/admin/sandbox/preview-dns*` routes are gated the same "owner or superadmin" way the existing `update_settings` handler in `backend/crates/api/src/settings/mod.rs` already is.
- No local cache/table of Cloudflare zones — always fetched live via `list_zones` when needed (domain creation, connection status display, preview-DNS sync).
- `shipyard-cloudflare` has zero knowledge of Shipyard's domains/orgs/services/DB — it only knows tokens, zones, accounts, and DNS records.

---

## Task 1: `shipyard-cloudflare` crate — API client

**Files:**
- Create: `backend/crates/cloudflare/Cargo.toml`
- Create: `backend/crates/cloudflare/src/lib.rs`
- Modify: `backend/Cargo.toml:2-14` (add `"crates/cloudflare"` to `members`)
- Modify: `backend/crates/common/src/error.rs` (add `AppError::Cloudflare` variant)

**Interfaces:**
- Produces: `shipyard_cloudflare::{CloudflareClient, Zone, Account, DnsRecord, CfError}` — `CloudflareClient::new(token)`, `.verify_token()`, `.list_accounts()`, `.list_zones()`, `.create_dns_record(zone_id, name, content)`, `.update_dns_record(zone_id, record_id, content)`, `.delete_dns_record(zone_id, record_id)`, all `async fn(...) -> shipyard_common::error::AppResult<T>`.
- Consumes: `shipyard_common::error::{AppError, AppResult}` (already exists).

- [ ] **Step 1: Add the `Cloudflare` error variant**

Read `backend/crates/common/src/error.rs` first to confirm it matches what's below (it should — this file hasn't changed recently). Find:
```rust
    #[error("Git error: {0}")]
    Git(String),
```
Add immediately after it:
```rust
    #[error("Git error: {0}")]
    Git(String),

    #[error("Cloudflare error: {0}")]
    Cloudflare(String),
```
Then find `error_code()`'s match (every variant is listed explicitly there, no wildcard arm — you must add one or the crate won't compile):
```rust
            AppError::Git(_) => "GIT_ERROR",
```
Add immediately after:
```rust
            AppError::Git(_) => "GIT_ERROR",
            AppError::Cloudflare(_) => "CLOUDFLARE_ERROR",
```
`status_code()` already has a `_ => 500` catch-all, so no change needed there (a Cloudflare error becomes a 500, same as `Docker`/`Mqtt`/`Git`).

- [ ] **Step 2: Create the crate scaffold**

`backend/crates/cloudflare/Cargo.toml`:
```toml
[package]
name = "shipyard-cloudflare"
version.workspace = true
edition.workspace = true

[dependencies]
shipyard-common = { path = "../common" }
reqwest = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
```

`backend/Cargo.toml` — find:
```toml
members = [
    "crates/api",
    "crates/common",
```
Add `"crates/cloudflare",` alphabetically between them:
```toml
members = [
    "crates/api",
    "crates/cloudflare",
    "crates/common",
```

- [ ] **Step 3: Write the failing tests for response parsing**

`backend/crates/cloudflare/src/lib.rs` — start the file with the types, the pure response-parsing function, and its tests (the network-calling methods come in Step 5; write tests for the parser first since it needs no network/mocking at all):

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd backend && cargo test -p shipyard-cloudflare`
Expected: 4 tests pass (the crate doesn't build any network-calling code yet, but `parse_cf_response` is fully self-contained and compiles/tests independently of the rest of the client).

- [ ] **Step 5: Add `CloudflareClient` and its read methods**

Append to `backend/crates/cloudflare/src/lib.rs`:
```rust
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
```

- [ ] **Step 6: Run the crate build to confirm it compiles**

Run: `cd backend && cargo build -p shipyard-cloudflare`
Expected: clean build.

- [ ] **Step 7: Add the DNS record write methods**

Append to `backend/crates/cloudflare/src/lib.rs`:
```rust
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

    /// PUT /zones/:zone_id/dns_records/:id — used only to update the
    /// sandbox preview wildcard record on a re-sync, never for regular
    /// per-service domains (those are create-once, delete-on-removal).
    pub async fn update_dns_record(&self, zone_id: &str, record_id: &str, content: &str) -> AppResult<DnsRecord> {
        let url = format!("{API_BASE}/zones/{zone_id}/dns_records/{record_id}");
        let existing_name = String::new(); // Cloudflare's PUT requires `name` in the body; caller always
                                            // has it, see the update_dns_record_with_name wrapper below.
        let _ = existing_name;
        unreachable!("use update_dns_record_with_name")
    }
}
```

**Stop — do not paste that last method as-is.** Cloudflare's `PUT /zones/:zone_id/dns_records/:id` requires the full record body (`type`, `name`, `content`) on every update, not just the changed field — there is no partial-update endpoint. Replace the `update_dns_record` stub above with this correct version instead (delete the stub entirely, including the `unreachable!`):
```rust
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
```

- [ ] **Step 8: Run the full crate build and test suite**

Run: `cd backend && cargo build -p shipyard-cloudflare && cargo test -p shipyard-cloudflare`
Expected: clean build, the same 4 tests from Step 3 still pass (no new tests needed for the write methods themselves — they're thin wrappers around `send_json`/`DnsRecordBody`, already covered structurally by `parse_cf_response`'s tests; the actual HTTP round-trip can only be verified against a real Cloudflare account, per this plan's final manual-verification task).

- [ ] **Step 9: Commit**

```bash
cd backend && git add Cargo.toml crates/cloudflare crates/common/src/error.rs
git commit -m "feat(cloudflare): add shipyard-cloudflare API client crate"
```

---

## Task 2: Database migration + models

**Files:**
- Create: `backend/crates/db/migrations/20250101000057_cloudflare_connections.sql`
- Modify: `backend/crates/db/src/models.rs`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces: `shipyard_db::models::CloudflareConnection { id, org_id, api_token, account_id, account_name, created_at }`; `shipyard_db::models::Domain` gains `cloudflare_zone_id: Option<String>`, `cloudflare_record_id: Option<String>`. Both consumed by Tasks 4-7.

- [ ] **Step 1: Write the migration**

`backend/crates/db/migrations/20250101000057_cloudflare_connections.sql`:
```sql
-- Per-org Cloudflare API token connection, mirroring git_providers'
-- shape (plaintext token column, one row per org).
CREATE TABLE IF NOT EXISTS cloudflare_connections (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    api_token    TEXT NOT NULL,
    account_id   TEXT NOT NULL,
    account_name TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id)
);

-- Nullable: a domain not covered by any connected Cloudflare zone (or
-- whose org never connected Cloudflare at all) simply has both NULL,
-- and behaves exactly as it did before this feature existed.
ALTER TABLE domains ADD COLUMN IF NOT EXISTS cloudflare_zone_id TEXT;
ALTER TABLE domains ADD COLUMN IF NOT EXISTS cloudflare_record_id TEXT;
```

- [ ] **Step 2: Apply the migration and verify**

Run (against the live test Postgres this workspace already uses — check for a running `shipyard-test-postgres` container first with `docker ps --filter name=shipyard-test-postgres`; if absent, start one the same way this project's other plans have: `docker run -d --name shipyard-test-postgres -p 5433:5432 -e POSTGRES_USER=shipyard -e POSTGRES_PASSWORD=shipyard -e POSTGRES_DB=shipyard postgres:16-alpine` then wait a few seconds for it to accept connections):
```bash
cd backend && DATABASE_URL="postgres://shipyard:shipyard@localhost:5433/shipyard" cargo sqlx migrate run --source crates/db/migrations
```
Expected: migration `20250101000057` applies cleanly with no errors. If `cargo sqlx` isn't installed, `cargo install sqlx-cli --no-default-features --features postgres,rustls` first.

- [ ] **Step 3: Add the `CloudflareConnection` model**

`backend/crates/db/src/models.rs` — find the `// ─── Git Providers ──` section (near the `GitProvider` struct) and add a new section right after it:
```rust
// ─── Cloudflare ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct CloudflareConnection {
    pub id: Uuid,
    pub org_id: Uuid,
    pub api_token: String,
    pub account_id: String,
    pub account_name: Option<String>,
    pub created_at: DateTime<Utc>,
}
```

- [ ] **Step 4: Add the two new fields to `Domain`**

Find:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Domain {
    pub id: Uuid,
    pub service_id: Uuid,
    pub hostname: String,
    pub tls_enabled: bool,
    pub traefik_router_name: String,
    pub cert_provider: String,
    pub port: Option<i32>,
    pub created_at: DateTime<Utc>,
}
```
Replace with:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct Domain {
    pub id: Uuid,
    pub service_id: Uuid,
    pub hostname: String,
    pub tls_enabled: bool,
    pub traefik_router_name: String,
    pub cert_provider: String,
    pub port: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub cloudflare_zone_id: Option<String>,
    pub cloudflare_record_id: Option<String>,
}
```

- [ ] **Step 5: Run the workspace build to confirm nothing else broke**

Run: `cd backend && cargo build --workspace`
Expected: **this will fail** — every existing `SELECT ... FROM domains` and `INSERT INTO domains (...) RETURNING ...` in `resources/mod.rs` and `sandbox_runtime/manager.rs` lists columns explicitly (none use `SELECT *`), so they won't automatically pick up the two new columns, but since `sqlx::query_as::<_, Domain>` maps by column name against the struct's fields, an existing query that doesn't select the two new columns will fail to deserialize (`Domain` now has fields the query doesn't return). **This is expected and intentional** — Task 6 is where every existing `Domain`-returning query gets updated to also select the two new columns. Confirm the build fails with sqlx column-mismatch errors in `resources/mod.rs` and `sandbox_runtime/manager.rs` (not somewhere unexpected), then proceed — Task 6 fixes this.

- [ ] **Step 6: Commit**

```bash
cd backend && git add crates/db/migrations/20250101000057_cloudflare_connections.sql crates/db/src/models.rs
git commit -m "feat(db): add cloudflare_connections table and domains.cloudflare_* columns"
```

---

## Task 3: `cloudflare_connections` API module

**Files:**
- Create: `backend/crates/api/src/cloudflare_connections/mod.rs`
- Modify: `backend/crates/api/src/lib.rs`
- Modify: `backend/crates/api/src/routes.rs`
- Modify: `backend/crates/api/Cargo.toml`

**Interfaces:**
- Consumes: `shipyard_cloudflare::CloudflareClient` (Task 1), `shipyard_db::models::CloudflareConnection` (Task 2).
- Produces: `POST/GET /orgs/:org_id/cloudflare`, `DELETE /orgs/:org_id/cloudflare/:id` routes, consumed by the frontend in Task 7.

- [ ] **Step 1: Add the crate dependency**

`backend/crates/api/Cargo.toml` — find:
```toml
shipyard-git = { path = "../git" }
```
Add immediately after:
```toml
shipyard-git = { path = "../git" }
shipyard-cloudflare = { path = "../cloudflare" }
```

- [ ] **Step 2: Write the module**

`backend/crates/api/src/cloudflare_connections/mod.rs`:
```rust
use axum::{
    extract::{Path, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::{error::ApiAppError, middleware::rbac, AppState};
use shipyard_cloudflare::CloudflareClient;
use shipyard_common::error::AppError;
use shipyard_db::models::CloudflareConnection;

#[derive(Debug, Deserialize)]
pub struct ConnectCloudflareRequest {
    pub api_token: String,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/orgs/:org_id/cloudflare", get(get_connection).post(connect).delete(disconnect))
}

async fn require_read(state: &AppState, user_id: Uuid, org_id: Uuid) -> Result<(), ApiAppError> {
    let ok = rbac::require_permission(&state.db, user_id, org_id, &format!("shipyard:{org_id}:providers:read")).await.is_ok()
        || rbac::require_permission(&state.db, user_id, org_id, &format!("shipyard:{org_id}:settings:read")).await.is_ok();
    if ok { Ok(()) } else { Err(ApiAppError(AppError::Forbidden("providers:read or settings:read required".to_string()))) }
}

async fn require_write(state: &AppState, user_id: Uuid, org_id: Uuid) -> Result<(), ApiAppError> {
    let ok = rbac::require_permission(&state.db, user_id, org_id, &format!("shipyard:{org_id}:providers:write")).await.is_ok()
        || rbac::require_permission(&state.db, user_id, org_id, &format!("shipyard:{org_id}:settings:write")).await.is_ok();
    if ok { Ok(()) } else { Err(ApiAppError(AppError::Forbidden("providers:write or settings:write required".to_string()))) }
}

/// GET /orgs/:org_id/cloudflare — connection status plus a live zone list
/// (fetched fresh from Cloudflare every call; no local zone cache).
#[derive(serde::Serialize)]
struct ConnectionStatus {
    #[serde(flatten)]
    connection: Option<CloudflareConnection>,
    zones: Vec<shipyard_cloudflare::Zone>,
}

async fn get_connection(
    auth: crate::auth::AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<crate::ApiResponse<ConnectionStatus>>, ApiAppError> {
    require_read(&state, auth.user_id, org_id).await?;

    let connection = sqlx::query_as::<_, CloudflareConnection>(
        "SELECT id, org_id, api_token, account_id, account_name, created_at
         FROM cloudflare_connections WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let zones = match &connection {
        Some(c) => CloudflareClient::new(c.api_token.clone()).list_zones().await.unwrap_or_default(),
        None => Vec::new(),
    };

    Ok(Json(crate::ApiResponse::ok(ConnectionStatus { connection, zones })))
}

/// POST /orgs/:org_id/cloudflare — verifies the token, resolves the account
/// for display, then stores the connection. One per org (UNIQUE constraint);
/// connecting again replaces the existing one.
async fn connect(
    auth: crate::auth::AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
    Json(body): Json<ConnectCloudflareRequest>,
) -> Result<(StatusCode, Json<crate::ApiResponse<CloudflareConnection>>), ApiAppError> {
    require_write(&state, auth.user_id, org_id).await?;

    if body.api_token.is_empty() {
        return Err(ApiAppError(AppError::BadRequest("api_token is required".to_string())));
    }

    let client = CloudflareClient::new(body.api_token.clone());
    client.verify_token().await.map_err(|e| {
        ApiAppError(AppError::BadRequest(format!("Could not verify Cloudflare token: {e}")))
    })?;

    let accounts = client.list_accounts().await.map_err(ApiAppError)?;
    let account = accounts.into_iter().next().ok_or_else(|| {
        ApiAppError(AppError::BadRequest("This Cloudflare token has no accessible accounts".to_string()))
    })?;

    let connection_id = Uuid::now_v7();
    let connection = sqlx::query_as::<_, CloudflareConnection>(
        "INSERT INTO cloudflare_connections (id, org_id, api_token, account_id, account_name, created_at)
         VALUES ($1, $2, $3, $4, $5, NOW())
         ON CONFLICT (org_id) DO UPDATE
           SET api_token = EXCLUDED.api_token, account_id = EXCLUDED.account_id,
               account_name = EXCLUDED.account_name, created_at = NOW()
         RETURNING id, org_id, api_token, account_id, account_name, created_at",
    )
    .bind(connection_id)
    .bind(org_id)
    .bind(&body.api_token)
    .bind(&account.id)
    .bind(&account.name)
    .fetch_one(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok((StatusCode::CREATED, Json(crate::ApiResponse::ok(connection))))
}

/// DELETE /orgs/:org_id/cloudflare — disconnects. Does NOT delete any DNS
/// records this connection created; existing domains' cloudflare_record_id
/// simply becomes unreachable for future deletes (see the spec's
/// "Failure handling" section — a delete_domain call with no connection to
/// authenticate against just skips the Cloudflare step, same as an org that
/// never connected at all).
async fn disconnect(
    auth: crate::auth::AuthUser,
    Path(org_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<crate::ApiResponse<()>>, ApiAppError> {
    require_write(&state, auth.user_id, org_id).await?;

    sqlx::query("DELETE FROM cloudflare_connections WHERE org_id = $1")
        .bind(org_id)
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(crate::ApiResponse::ok(())))
}
```

- [ ] **Step 3: Register the module**

`backend/crates/api/src/lib.rs` — find:
```rust
pub mod git_providers;
```
Add immediately after:
```rust
pub mod git_providers;
pub mod cloudflare_connections;
```

- [ ] **Step 4: Register the routes**

`backend/crates/api/src/routes.rs` — find:
```rust
.merge(git_providers::routes())
```
Add immediately after:
```rust
.merge(git_providers::routes())
.merge(cloudflare_connections::routes())
```

- [ ] **Step 5: Run the workspace build**

Run: `cd backend && cargo build --workspace`
Expected: `shipyard-api` now builds against the new crate and module (the Task 2 Step 5 `Domain`-column-mismatch failures are unrelated and still present — expected until Task 6; confirm no NEW failures beyond those).

- [ ] **Step 6: Commit**

```bash
cd backend && git add crates/api/Cargo.toml crates/api/src/lib.rs crates/api/src/routes.rs crates/api/src/cloudflare_connections
git commit -m "feat(api): add per-org Cloudflare connection CRUD endpoints"
```

---

## Task 4: Zone matching + shared host-IP resolver

**Files:**
- Modify: `backend/crates/api/src/resources/mod.rs`
- Modify: `backend/crates/api/src/settings/mod.rs`

**Interfaces:**
- Consumes: `shipyard_cloudflare::Zone` (Task 1).
- Produces: `fn longest_matching_zone(hostname: &str, zones: &[Zone]) -> Option<&Zone>` in `resources/mod.rs`, consumed by Task 5. `pub(crate) async fn resolve_host_ip() -> String` in `settings/mod.rs`, consumed by Task 5 and Task 6.

- [ ] **Step 1: Write the failing test for zone matching**

`backend/crates/api/src/resources/mod.rs` — add near the bottom of the file, in a new `#[cfg(test)] mod tests` block (check first whether one already exists in this file; if it does, add these tests inside it instead of creating a second block):
```rust
#[cfg(test)]
mod cloudflare_zone_matching_tests {
    use super::*;
    use shipyard_cloudflare::Zone;

    fn zone(id: &str, name: &str) -> Zone {
        Zone { id: id.to_string(), name: name.to_string() }
    }

    #[test]
    fn matches_the_longest_zone_name_that_is_a_suffix_of_the_hostname() {
        let zones = vec![zone("1", "example.com"), zone("2", "staging.example.com")];
        let m = longest_matching_zone("api.staging.example.com", &zones).unwrap();
        assert_eq!(m.id, "2", "the more specific zone must win");
    }

    #[test]
    fn matches_an_exact_hostname_equal_to_the_zone_apex() {
        let zones = vec![zone("1", "example.com")];
        let m = longest_matching_zone("example.com", &zones).unwrap();
        assert_eq!(m.id, "1");
    }

    #[test]
    fn returns_none_when_no_zone_covers_the_hostname() {
        let zones = vec![zone("1", "example.com")];
        assert!(longest_matching_zone("totally-different.org", &zones).is_none());
    }

    #[test]
    fn does_not_match_a_different_domain_that_merely_shares_a_suffix_string() {
        // "notexample.com" ends with "example.com" as a raw string, but is a
        // completely different domain — must not match.
        let zones = vec![zone("1", "example.com")];
        assert!(longest_matching_zone("notexample.com", &zones).is_none());
    }

    #[test]
    fn matching_is_case_insensitive() {
        let zones = vec![zone("1", "Example.com")];
        let m = longest_matching_zone("API.EXAMPLE.COM", &zones).unwrap();
        assert_eq!(m.id, "1");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p shipyard-api cloudflare_zone_matching -- --test-threads=1`
Expected: FAIL — `longest_matching_zone` is not defined yet, and `shipyard_cloudflare` is not yet a dependency of `crates/api`.

- [ ] **Step 3: Add the `shipyard-cloudflare` dependency to `crates/api`**

Already added in Task 3 Step 1 — confirm it's present (`grep shipyard-cloudflare backend/crates/api/Cargo.toml`); if this task is somehow run before Task 3, add it now the same way.

- [ ] **Step 4: Implement `longest_matching_zone`**

`backend/crates/api/src/resources/mod.rs` — add near the top, right after the existing `is_convenience_domain`/`hostname_to_router_name` helper functions:
```rust
/// Finds the zone (if any) that covers `hostname` — the longest zone name
/// that is either exactly `hostname` or a dot-separated suffix of it (e.g.
/// zone "example.com" covers "api.example.com" but not "notexample.com").
/// Longest match wins so a more specific zone (e.g. "staging.example.com")
/// is preferred over a broader one (e.g. "example.com") when an org has
/// both connected.
fn longest_matching_zone<'a>(hostname: &str, zones: &'a [shipyard_cloudflare::Zone]) -> Option<&'a shipyard_cloudflare::Zone> {
    let hostname = hostname.trim_end_matches('.').to_lowercase();
    zones
        .iter()
        .filter(|z| {
            let zone_name = z.name.to_lowercase();
            hostname == zone_name || hostname.ends_with(&format!(".{zone_name}"))
        })
        .max_by_key(|z| z.name.len())
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd backend && cargo test -p shipyard-api cloudflare_zone_matching -- --test-threads=1`
Expected: PASS, all 5 tests.

- [ ] **Step 6: Refactor `get_host_ip`'s IP resolution into a shared function**

Read `backend/crates/api/src/settings/mod.rs`'s current `get_host_ip`/`fetch_public_ip`/`is_loopback_or_private` first (around lines 1413-1470) to confirm they match what's below before editing.

Find:
```rust
async fn get_host_ip(
    _auth: AuthUser,
) -> Json<ApiResponse<HostIpResponse>> {
    // 1. Prefer the DOMAIN env var if it looks like an IPv4 address
    if let Ok(domain) = std::env::var("DOMAIN") {
        let trimmed = domain.trim().to_string();
        if trimmed.parse::<std::net::IpAddr>().is_ok() {
            let is_public = !is_loopback_or_private(&trimmed);
            return Json(ApiResponse::ok(HostIpResponse { ip: trimmed, is_public }));
        }
    }

    // 2. Try to detect the public IP via external service (3 s timeout)
    let detected = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        fetch_public_ip(),
    )
    .await
    .ok()
    .flatten();

    if let Some(ip) = detected {
        let is_public = !is_loopback_or_private(&ip);
        return Json(ApiResponse::ok(HostIpResponse { ip, is_public }));
    }

    // 3. Fall back to localhost
    Json(ApiResponse::ok(HostIpResponse {
        ip: "127.0.0.1".to_string(),
        is_public: false,
    }))
}
```
Replace with:
```rust
/// The same 3-tier IP resolution `get_host_ip` exposes over HTTP, factored
/// out so the Cloudflare DNS-record creation path (resources/mod.rs,
/// Task 5/6 of the Cloudflare DNS plan) can resolve the same address
/// without duplicating this logic or making an HTTP round-trip to itself.
pub(crate) async fn resolve_host_ip() -> String {
    // 1. Prefer the DOMAIN env var if it looks like an IPv4 address
    if let Ok(domain) = std::env::var("DOMAIN") {
        let trimmed = domain.trim().to_string();
        if trimmed.parse::<std::net::IpAddr>().is_ok() {
            return trimmed;
        }
    }

    // 2. Try to detect the public IP via external service (3 s timeout)
    let detected = tokio::time::timeout(
        std::time::Duration::from_secs(3),
        fetch_public_ip(),
    )
    .await
    .ok()
    .flatten();

    if let Some(ip) = detected {
        return ip;
    }

    // 3. Fall back to localhost
    "127.0.0.1".to_string()
}

async fn get_host_ip(
    _auth: AuthUser,
) -> Json<ApiResponse<HostIpResponse>> {
    let ip = resolve_host_ip().await;
    let is_public = !is_loopback_or_private(&ip);
    Json(ApiResponse::ok(HostIpResponse { ip, is_public }))
}
```

- [ ] **Step 7: Run the workspace build**

Run: `cd backend && cargo build --workspace`
Expected: same pre-existing `Domain`-column-mismatch failures as before (unrelated, fixed in Task 6), no new failures.

- [ ] **Step 8: Commit**

```bash
cd backend && git add crates/api/src/resources/mod.rs crates/api/src/settings/mod.rs
git commit -m "feat(api): add Cloudflare zone matching and a shared host-IP resolver"
```

---

## Task 5: Wire Cloudflare sync into `create_domain` / `delete_domain`

**Files:**
- Modify: `backend/crates/api/src/resources/mod.rs`

**Interfaces:**
- Consumes: `longest_matching_zone` (Task 4), `crate::settings::resolve_host_ip()` (Task 4), `shipyard_cloudflare::CloudflareClient` (Task 1), `shipyard_db::models::CloudflareConnection` (Task 2), `Domain.cloudflare_zone_id`/`cloudflare_record_id` (Task 2).
- Produces: every existing `Domain`-returning query in this file now selects the two new columns, resolving the Task 2/Step 5 build failure.

- [ ] **Step 1: Update every `Domain`-returning query to select the new columns**

Read the current file first — there are four queries returning `Domain` rows: `list_domains`, `create_domain`'s `INSERT ... RETURNING`, and (in `sandbox_runtime/manager.rs`, a different file, out of scope for this task but check it isn't accidentally also selecting `Domain` — it isn't; `ensure_preview_domain` doesn't `RETURNING` a `Domain`, it just does a bare `INSERT ... ON CONFLICT`, so it's unaffected). In `resources/mod.rs`:

Find (`list_domains`):
```rust
    let domains = sqlx::query_as::<_, Domain>(
        "SELECT id, service_id, hostname, tls_enabled, traefik_router_name, cert_provider, port, created_at
         FROM domains
         WHERE service_id = $1
         ORDER BY created_at ASC",
    )
```
Replace with:
```rust
    let domains = sqlx::query_as::<_, Domain>(
        "SELECT id, service_id, hostname, tls_enabled, traefik_router_name, cert_provider, port, created_at, cloudflare_zone_id, cloudflare_record_id
         FROM domains
         WHERE service_id = $1
         ORDER BY created_at ASC",
    )
```

Find (`create_domain`'s insert):
```rust
    let domain = sqlx::query_as::<_, Domain>(
        "INSERT INTO domains (id, service_id, hostname, tls_enabled, traefik_router_name, cert_provider, port, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
         RETURNING id, service_id, hostname, tls_enabled, traefik_router_name, cert_provider, port, created_at",
    )
```
Replace with:
```rust
    let domain = sqlx::query_as::<_, Domain>(
        "INSERT INTO domains (id, service_id, hostname, tls_enabled, traefik_router_name, cert_provider, port, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
         RETURNING id, service_id, hostname, tls_enabled, traefik_router_name, cert_provider, port, created_at, cloudflare_zone_id, cloudflare_record_id",
    )
```
(The two new columns default to `NULL` since they're not in the `INSERT`'s column/value list — correct, they get filled in by Step 3 below after this insert succeeds.)

- [ ] **Step 2: Run the build to confirm the column-mismatch failures are gone**

Run: `cd backend && cargo build --workspace`
Expected: clean build — the Task 2/Step 5 failures are now resolved.

- [ ] **Step 3: Write the best-effort Cloudflare sync helper for domain creation**

Add this function to `backend/crates/api/src/resources/mod.rs`, near `sync_traefik_dynamic_config`:
```rust
/// Best-effort: if the domain's org has a Cloudflare connection whose zones
/// cover `hostname`, creates a DNS-only A record pointed at this server and
/// returns its (zone_id, record_id). Returns None for every "nothing to do
/// or something went wrong" case — no connection, no matching zone, or any
/// Cloudflare API failure — logging a warning but never returning an `Err`,
/// since a Cloudflare hiccup must never block domain creation (see the
/// plan's Global Constraints).
async fn try_create_cloudflare_record(db: &sqlx::PgPool, service_id: Uuid, hostname: &str) -> Option<(String, String)> {
    let org_id: Uuid = sqlx::query_scalar(
        "SELECT p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()?;

    let connection = sqlx::query_as::<_, shipyard_db::models::CloudflareConnection>(
        "SELECT id, org_id, api_token, account_id, account_name, created_at FROM cloudflare_connections WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()?;

    let client = shipyard_cloudflare::CloudflareClient::new(connection.api_token);
    let zones = match client.list_zones().await {
        Ok(z) => z,
        Err(e) => {
            tracing::warn!(hostname, "Cloudflare list_zones failed while creating domain: {e}");
            return None;
        }
    };

    let zone = longest_matching_zone(hostname, &zones)?;
    let ip = crate::settings::resolve_host_ip().await;

    match client.create_dns_record(&zone.id, hostname, &ip).await {
        Ok(record) => Some((zone.id.clone(), record.id)),
        Err(e) => {
            tracing::warn!(hostname, zone = %zone.name, "Cloudflare create_dns_record failed: {e}");
            None
        }
    }
}
```

- [ ] **Step 4: Call it from `create_domain`**

Find, in `create_domain`, right after the `INSERT ... RETURNING` block and before `sync_traefik_dynamic_config`:
```rust
    // Write Traefik file-provider config (no-op when dynamic_config_dir is unset)
    sync_traefik_dynamic_config(
```
Replace with:
```rust
    // Best-effort Cloudflare DNS record — never blocks domain creation on failure.
    if let Some((zone_id, record_id)) = try_create_cloudflare_record(&state.db, service_id, &body.hostname).await {
        sqlx::query("UPDATE domains SET cloudflare_zone_id = $1, cloudflare_record_id = $2 WHERE id = $3")
            .bind(&zone_id)
            .bind(&record_id)
            .bind(domain.id)
            .execute(&state.db)
            .await
            .ok();
    }

    // Write Traefik file-provider config (no-op when dynamic_config_dir is unset)
    sync_traefik_dynamic_config(
```

- [ ] **Step 5: Write the best-effort delete helper**

Add this function right after `try_create_cloudflare_record`:
```rust
/// Best-effort: deletes the given Cloudflare DNS record if the domain's org
/// still has a connection (an org that disconnected simply has none to
/// authenticate the delete with — same code path as never having connected
/// at all, per the spec). Never returns an error; a failure here must never
/// block deleting the domain from Shipyard.
async fn try_delete_cloudflare_record(db: &sqlx::PgPool, service_id: Uuid, zone_id: &str, record_id: &str) {
    let org_id: Option<Uuid> = sqlx::query_scalar(
        "SELECT p.org_id FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten();

    let Some(org_id) = org_id else { return };

    let token: Option<String> = sqlx::query_scalar("SELECT api_token FROM cloudflare_connections WHERE org_id = $1")
        .bind(org_id)
        .fetch_optional(db)
        .await
        .ok()
        .flatten();

    let Some(token) = token else { return };

    let client = shipyard_cloudflare::CloudflareClient::new(token);
    if let Err(e) = client.delete_dns_record(zone_id, record_id).await {
        tracing::warn!(zone_id, record_id, "Cloudflare delete_dns_record failed: {e}");
    }
}
```

- [ ] **Step 6: Wire it into `delete_domain`, fetching the Cloudflare fields before the row disappears**

Read the current `delete_domain` first. Find:
```rust
    require_service_permission(&state.db, auth_user.user_id, service_id, "domain:write").await.map_err(ApiAppError)?;
    let rows_affected = sqlx::query(
        "DELETE FROM domains WHERE id = $1 AND service_id = $2",
    )
    .bind(domain_id)
    .bind(service_id)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
    .rows_affected();

    if rows_affected == 0 {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Domain '{}' not found",
            domain_id
        ))));
    }
```
Replace with (switching to `RETURNING` so the Cloudflare fields are captured atomically with the delete, before the row is gone):
```rust
    require_service_permission(&state.db, auth_user.user_id, service_id, "domain:write").await.map_err(ApiAppError)?;
    let deleted: Option<(Option<String>, Option<String>)> = sqlx::query_as(
        "DELETE FROM domains WHERE id = $1 AND service_id = $2 RETURNING cloudflare_zone_id, cloudflare_record_id",
    )
    .bind(domain_id)
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let Some((cloudflare_zone_id, cloudflare_record_id)) = deleted else {
        return Err(ApiAppError(AppError::NotFound(format!(
            "Domain '{}' not found",
            domain_id
        ))));
    };

    if let (Some(zone_id), Some(record_id)) = (cloudflare_zone_id, cloudflare_record_id) {
        try_delete_cloudflare_record(&state.db, service_id, &zone_id, &record_id).await;
    }
```

- [ ] **Step 7: Run the workspace build and full test suite**

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build; test count is the pre-existing baseline plus the 5 zone-matching tests from Task 4 plus the 4 parser tests from Task 1 (report the exact before/after counts you see).

- [ ] **Step 8: Commit**

```bash
cd backend && git add crates/api/src/resources/mod.rs
git commit -m "feat(api): create/delete Cloudflare DNS records on domain create/delete"
```

---

## Task 6: Sandbox preview DNS platform settings

**Files:**
- Modify: `backend/crates/api/src/settings/mod.rs`

**Interfaces:**
- Consumes: `shipyard_cloudflare::CloudflareClient` (Task 1), `longest_matching_zone` — **note:** this function lives in `resources/mod.rs` (Task 4); since it's a private `fn` there, either mark it `pub(crate)` (do this now — change `fn longest_matching_zone` to `pub(crate) fn longest_matching_zone` in `resources/mod.rs`) so `settings/mod.rs` can call `crate::resources::longest_matching_zone(...)`, or duplicate the ~8-line function locally. Mark it `pub(crate)` — duplicating matching logic for the same behavior is the wrong call here.
- Produces: `GET/PUT /admin/sandbox/preview-dns`, `POST /admin/sandbox/preview-dns/sync`, consumed by the frontend in Task 7.

- [ ] **Step 1: Make `longest_matching_zone` crate-visible**

`backend/crates/api/src/resources/mod.rs` — find:
```rust
fn longest_matching_zone<'a>(hostname: &str, zones: &'a [shipyard_cloudflare::Zone]) -> Option<&'a shipyard_cloudflare::Zone> {
```
Replace with:
```rust
pub(crate) fn longest_matching_zone<'a>(hostname: &str, zones: &'a [shipyard_cloudflare::Zone]) -> Option<&'a shipyard_cloudflare::Zone> {
```

- [ ] **Step 2: Write the failing test for the settings payload shape**

`backend/crates/api/src/settings/mod.rs` — add near any existing test module in this file (check first; if none exists, add a new `#[cfg(test)] mod sandbox_preview_dns_tests` near the bottom):
```rust
#[cfg(test)]
mod sandbox_preview_dns_tests {
    use super::*;

    #[test]
    fn preview_dns_status_serializes_with_null_fields_when_never_configured() {
        let status = SandboxPreviewDnsStatus {
            preview_base_domain: "shipyard-apps.dev".to_string(),
            owner_org_id: None,
            cloudflare_zone_id: None,
            cloudflare_record_id: None,
        };
        let json = serde_json::to_value(&status).unwrap();
        assert_eq!(json["preview_base_domain"], "shipyard-apps.dev");
        assert!(json["owner_org_id"].is_null());
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd backend && cargo test -p shipyard-api sandbox_preview_dns -- --test-threads=1`
Expected: FAIL — `SandboxPreviewDnsStatus` doesn't exist yet.

- [ ] **Step 4: Implement the types and handlers**

Add to `backend/crates/api/src/settings/mod.rs`, near the other settings structs/handlers:
```rust
#[derive(Debug, Serialize)]
struct SandboxPreviewDnsStatus {
    preview_base_domain: String,
    owner_org_id: Option<Uuid>,
    cloudflare_zone_id: Option<String>,
    cloudflare_record_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SetPreviewDnsOwnerRequest {
    org_id: Uuid,
}

async fn require_owner_or_superadmin(db: &sqlx::PgPool, user_id: Uuid) -> Result<(), ApiAppError> {
    if superadmin_bypass(db, user_id).await {
        return Ok(());
    }
    let is_owner: Option<(bool,)> = sqlx::query_as::<_, (bool,)>(
        "SELECT TRUE FROM org_members WHERE user_id = $1 AND role = 'owner' LIMIT 1",
    )
    .bind(user_id)
    .fetch_optional(db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    if is_owner.is_none() {
        return Err(ApiAppError(AppError::Forbidden(
            "Only platform owners or superadmins can manage sandbox preview DNS".to_string(),
        )));
    }
    Ok(())
}

async fn get_sandbox_preview_dns(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SandboxPreviewDnsStatus>>, ApiAppError> {
    require_owner_or_superadmin(&state.db, auth.user_id).await?;

    let rows: Vec<(String, Value)> = sqlx::query_as(
        "SELECT key, value FROM system_config WHERE key = ANY($1)",
    )
    .bind([
        "sandbox_preview_cloudflare_org_id",
        "sandbox_preview_cloudflare_zone_id",
        "sandbox_preview_cloudflare_record_id",
    ].as_slice())
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let mut map: std::collections::HashMap<String, String> = rows
        .into_iter()
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s.to_string())))
        .collect();

    Ok(Json(ApiResponse::ok(SandboxPreviewDnsStatus {
        preview_base_domain: state.config.sandbox.preview_base_domain.clone(),
        owner_org_id: map.remove("sandbox_preview_cloudflare_org_id").and_then(|s| s.parse().ok()),
        cloudflare_zone_id: map.remove("sandbox_preview_cloudflare_zone_id"),
        cloudflare_record_id: map.remove("sandbox_preview_cloudflare_record_id"),
    })))
}

async fn set_sandbox_preview_dns_owner(
    auth: AuthUser,
    State(state): State<AppState>,
    Json(body): Json<SetPreviewDnsOwnerRequest>,
) -> Result<Json<ApiResponse<()>>, ApiAppError> {
    require_owner_or_superadmin(&state.db, auth.user_id).await?;

    sqlx::query(
        "INSERT INTO system_config (key, value, updated_at) VALUES ('sandbox_preview_cloudflare_org_id', $1, NOW())
         ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
    )
    .bind(Value::String(body.org_id.to_string()))
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    Ok(Json(ApiResponse::ok(())))
}

/// POST /admin/sandbox/preview-dns/sync — creates the wildcard record on
/// the first sync, updates it (in case the server's IP changed) on every
/// sync after that. Manually triggered only — no background polling.
async fn sync_sandbox_preview_dns(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SandboxPreviewDnsStatus>>, ApiAppError> {
    require_owner_or_superadmin(&state.db, auth.user_id).await?;

    let owner_org_id: Option<String> = sqlx::query_scalar(
        "SELECT value #>> '{}' FROM system_config WHERE key = 'sandbox_preview_cloudflare_org_id'",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    let owner_org_id: Uuid = owner_org_id
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| ApiAppError(AppError::BadRequest("No DNS-owner org set yet — call PUT /admin/sandbox/preview-dns first".to_string())))?;

    let token: String = sqlx::query_scalar("SELECT api_token FROM cloudflare_connections WHERE org_id = $1")
        .bind(owner_org_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| ApiAppError(AppError::BadRequest("The DNS-owner org has no Cloudflare connection".to_string())))?;

    let client = CloudflareClient::new(token);
    let zones = client.list_zones().await.map_err(ApiAppError)?;
    let preview_base_domain = state.config.sandbox.preview_base_domain.clone();
    let zone = crate::resources::longest_matching_zone(&preview_base_domain, &zones)
        .ok_or_else(|| ApiAppError(AppError::BadRequest(format!(
            "No zone in the DNS-owner org's Cloudflare account covers '{preview_base_domain}'"
        ))))?
        .clone();

    let wildcard_name = format!("*.{preview_base_domain}");
    let ip = resolve_host_ip().await;

    let existing_record_id: Option<String> = sqlx::query_scalar(
        "SELECT value #>> '{}' FROM system_config WHERE key = 'sandbox_preview_cloudflare_record_id'",
    )
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let record_id = match existing_record_id {
        Some(id) => {
            client.update_dns_record(&zone.id, &id, &wildcard_name, &ip).await.map_err(ApiAppError)?;
            id
        }
        None => {
            let record = client.create_dns_record(&zone.id, &wildcard_name, &ip).await.map_err(ApiAppError)?;
            record.id
        }
    };

    for (key, value) in [
        ("sandbox_preview_cloudflare_zone_id", zone.id.clone()),
        ("sandbox_preview_cloudflare_record_id", record_id.clone()),
    ] {
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, NOW())
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value, updated_at = NOW()",
        )
        .bind(key)
        .bind(Value::String(value))
        .execute(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    }

    Ok(Json(ApiResponse::ok(SandboxPreviewDnsStatus {
        preview_base_domain,
        owner_org_id: Some(owner_org_id),
        cloudflare_zone_id: Some(zone.id),
        cloudflare_record_id: Some(record_id),
    })))
}
```

**Note on the `system_config` scalar reads above:** this file's existing `load_settings` reads multiple keys via `query_as::<_, (String, Value)>` and filters with `.as_str()`; the handlers above instead read one key at a time via `query_scalar` with the Postgres `#>> '{}'` JSONB-to-text operator (equivalent to `.as_str()` but at the SQL level) since each needs just one value, not a batch — both approaches already coexist in this codebase's query style, this isn't introducing a new one.

- [ ] **Step 5: Register the routes**

Find `pub fn routes() -> Router<AppState> {` in `backend/crates/api/src/settings/mod.rs` and add these three lines to the chain of `.route(...)` calls (match the existing formatting style — one `.route(...)` per line):
```rust
        .route("/admin/sandbox/preview-dns", get(get_sandbox_preview_dns).put(set_sandbox_preview_dns_owner))
        .route("/admin/sandbox/preview-dns/sync", post(sync_sandbox_preview_dns))
```

- [ ] **Step 6: Run the test to verify it passes, then the full workspace build/test suite**

Run: `cd backend && cargo test -p shipyard-api sandbox_preview_dns -- --test-threads=1`
Expected: PASS.

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build, all tests pass (baseline + 5 zone-matching + 4 parser + 1 new settings-shape test).

- [ ] **Step 7: Commit**

```bash
cd backend && git add crates/api/src/resources/mod.rs crates/api/src/settings/mod.rs
git commit -m "feat(api): sandbox preview base domain wildcard DNS sync"
```

---

## Task 7: Frontend types + API client methods

**Files:**
- Modify: `frontend/src/lib/api/types.ts`
- Modify: `frontend/src/lib/api/client.ts`

**Interfaces:**
- Consumes: the backend response shapes from Tasks 3, 5, 6.
- Produces: `CloudflareConnection`, `CloudflareZone`, `SandboxPreviewDnsStatus` types; `api.{getCloudflareConnection,connectCloudflare,disconnectCloudflare,getSandboxPreviewDns,setSandboxPreviewDnsOwner,syncSandboxPreviewDns}` methods, consumed by Task 8.

- [ ] **Step 1: Add the types**

`frontend/src/lib/api/types.ts` — find the `GitProvider` interface and add immediately after it:
```ts
export interface CloudflareConnection {
	id: string;
	org_id: string;
	api_token: string;
	account_id: string;
	account_name: string | null;
	created_at: string;
}

export interface CloudflareZone {
	id: string;
	name: string;
}

export interface CloudflareConnectionStatus {
	id?: string;
	org_id?: string;
	api_token?: string;
	account_id?: string;
	account_name?: string | null;
	created_at?: string;
	zones: CloudflareZone[];
}

export interface SandboxPreviewDnsStatus {
	preview_base_domain: string;
	owner_org_id: string | null;
	cloudflare_zone_id: string | null;
	cloudflare_record_id: string | null;
}
```
(`CloudflareConnectionStatus`'s connection fields are all optional and flattened, matching the backend's `#[serde(flatten)]` on `Option<CloudflareConnection>` — when there's no connection, none of those keys are present in the JSON at all, only `zones: []`.)

Find the `Domain` interface (mirrors the backend's `Domain` struct) and add the two new fields:
```ts
export interface Domain {
	id: string;
	service_id: string;
	hostname: string;
	tls_enabled: boolean;
	traefik_router_name: string;
	cert_provider: string;
	port: number | null;
	created_at: string;
	cloudflare_zone_id: string | null;
	cloudflare_record_id: string | null;
}
```
(Read the actual current `Domain` interface first — if its field list differs slightly from this description, keep its existing fields and only add the two new ones at the end.)

- [ ] **Step 2: Add the client methods**

`frontend/src/lib/api/client.ts` — find the `// ─── Git Providers ──` section and add a new section immediately after it:
```ts
	// ─── Cloudflare ─────────────────────────────────────────────────────────────
	async getCloudflareConnection(orgId: string): Promise<ApiResponse<import('./types').CloudflareConnectionStatus>> {
		return this.get(`/orgs/${orgId}/cloudflare`);
	}

	async connectCloudflare(orgId: string, apiToken: string): Promise<ApiResponse<import('./types').CloudflareConnection>> {
		return this.post(`/orgs/${orgId}/cloudflare`, { api_token: apiToken });
	}

	async disconnectCloudflare(orgId: string): Promise<ApiResponse<null>> {
		return this.delete(`/orgs/${orgId}/cloudflare`);
	}

	async getSandboxPreviewDns(): Promise<ApiResponse<import('./types').SandboxPreviewDnsStatus>> {
		return this.get(`/admin/sandbox/preview-dns`);
	}

	async setSandboxPreviewDnsOwner(orgId: string): Promise<ApiResponse<null>> {
		return this.put(`/admin/sandbox/preview-dns`, { org_id: orgId });
	}

	async syncSandboxPreviewDns(): Promise<ApiResponse<import('./types').SandboxPreviewDnsStatus>> {
		return this.post(`/admin/sandbox/preview-dns/sync`, {});
	}
```
(Confirm `this.put`/`this.post`/`this.get`/`this.delete` are the actual method names this class already uses elsewhere — `listGitProviders`/`createGitProvider`/`deleteGitProvider` just above use `this.get`/`this.post`/`this.delete`; check whether a plain `this.put` helper already exists too — e.g. `saveWebhookSecret` in the Providers page calls `api.put<PlatformSettings>('/settings', ...)` directly, confirming `api.put` is a real, existing public method.)

- [ ] **Step 3: Type-check**

Run: `cd frontend && npm run check`
Expected: no new errors versus whatever the current baseline is (check the count before and after your edit).

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/api/types.ts frontend/src/lib/api/client.ts
git commit -m "feat(frontend): add Cloudflare types and API client methods"
```

---

## Task 8: Frontend Cloudflare settings page

**Files:**
- Create: `frontend/src/routes/orgs/[orgSlug]/settings/cloudflare/+page.svelte`
- Modify: `frontend/src/routes/orgs/[orgSlug]/settings/+layout.svelte`

**Interfaces:**
- Consumes: `api.{getCloudflareConnection,connectCloudflare,disconnectCloudflare,getSandboxPreviewDns,setSandboxPreviewDnsOwner,syncSandboxPreviewDns}` (Task 7).
- Produces: a reachable settings page, linked from the tab bar.

- [ ] **Step 1: Add the settings tab**

Read the current `frontend/src/routes/orgs/[orgSlug]/settings/+layout.svelte` first. Find:
```svelte
	import { Settings2, Users, KeyRound, Rocket, ShieldCheck, GitBranch } from '@lucide/svelte';
```
Replace with:
```svelte
	import { Settings2, Users, KeyRound, Rocket, ShieldCheck, GitBranch, Cloud } from '@lucide/svelte';
```

Find:
```svelte
	const SETTINGS_SUFFIXES = [
		'settings:read','settings:write','members:read','members:invite','members:manage',
		'providers:read','providers:write',
```
Replace with (no new permission suffix needed — Cloudflare reuses `providers:*`/`settings:*`, already listed):
```svelte
	const SETTINGS_SUFFIXES = [
		'settings:read','settings:write','members:read','members:invite','members:manage',
		'providers:read','providers:write',
```
(No change needed here — this step exists only to confirm `providers:read`/`providers:write` are already present, which they are; skip editing this block.)

Find:
```svelte
		{ label: 'Providers',   href: (slug: string) => `/orgs/${slug}/settings/providers`,   icon: GitBranch,   badge: 'admin' },
```
Add immediately after:
```svelte
		{ label: 'Providers',   href: (slug: string) => `/orgs/${slug}/settings/providers`,   icon: GitBranch,   badge: 'admin' },
		{ label: 'Cloudflare',  href: (slug: string) => `/orgs/${slug}/settings/cloudflare`,  icon: Cloud,       badge: 'admin' },
```

- [ ] **Step 2: Write the page**

`frontend/src/routes/orgs/[orgSlug]/settings/cloudflare/+page.svelte`:
```svelte
<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm, isAdminRole } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { Cloud, Loader2, AlertCircle, Trash2, RefreshCw, Check } from '@lucide/svelte';
	import type { CloudflareConnectionStatus, SandboxPreviewDnsStatus, Organization } from '$lib/api/types';

	let orgId   = $derived($orgStore.activeOrg?.id ?? '');
	let myRole  = $derived($orgStore.myMembership?.role ?? null);
	let myPerms = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let isAdmin = $derived(isAdminRole(myRole));

	let canRead  = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'providers', 'read'))  || can(myRole, myPerms, perm(orgId, 'settings', 'read')));
	let canWrite = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'providers', 'write')) || can(myRole, myPerms, perm(orgId, 'settings', 'write')));

	let loading    = $state(true);
	let status     = $state<CloudflareConnectionStatus | null>(null);
	let tokenInput = $state('');
	let connecting = $state(false);
	let errorMsg   = $state('');

	async function loadStatus() {
		if (!orgId) return;
		const res = await api.getCloudflareConnection(orgId);
		if (res.data) status = res.data;
	}

	async function connect(e: Event) {
		e.preventDefault();
		if (!orgId || !tokenInput.trim()) return;
		connecting = true; errorMsg = '';
		const res = await api.connectCloudflare(orgId, tokenInput.trim());
		connecting = false;
		if (res.error) {
			errorMsg = res.error.message;
		} else {
			tokenInput = '';
			await loadStatus();
		}
	}

	async function disconnect() {
		if (!orgId) return;
		if (!confirm('Disconnect this Cloudflare account? Existing DNS records it created will NOT be deleted automatically.')) return;
		await api.disconnectCloudflare(orgId);
		await loadStatus();
	}

	// ── Platform: sandbox preview DNS (owner/superadmin only; backend enforces) ──
	let previewDns   = $state<SandboxPreviewDnsStatus | null>(null);
	let orgs         = $state<Organization[]>([]);
	let selectedOwnerOrgId = $state('');
	let savingOwner  = $state(false);
	let syncing      = $state(false);
	let syncMsg      = $state('');

	async function loadPreviewDns() {
		const res = await api.getSandboxPreviewDns();
		if (res.data) {
			previewDns = res.data;
			selectedOwnerOrgId = res.data.owner_org_id ?? '';
		}
	}

	async function saveOwner() {
		if (!selectedOwnerOrgId) return;
		savingOwner = true;
		await api.setSandboxPreviewDnsOwner(selectedOwnerOrgId);
		savingOwner = false;
		await loadPreviewDns();
	}

	async function syncNow() {
		syncing = true; syncMsg = '';
		const res = await api.syncSandboxPreviewDns();
		syncing = false;
		if (res.error) {
			syncMsg = `✗ ${res.error.message}`;
		} else {
			syncMsg = '✓ Synced';
			await loadPreviewDns();
		}
	}

	onMount(async () => {
		await loadStatus();
		// Best-effort: only owners/superadmins can actually read this; a
		// Forbidden response here is expected and silently ignored for
		// everyone else, matching how this page's per-org section already
		// only shows write controls to those with write permission.
		await loadPreviewDns();
		loading = false;
	});

	$effect(() => {
		if (orgId) loadStatus();
	});
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canRead}
	message="You need the 'View providers' or 'View settings' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if loading}
	<div class="loading"><div class="spinner"></div><span>Loading…</span></div>
{:else if canRead}

<div class="cf-page">
	<section class="settings-section">
		<div class="section-header">
			<div class="section-icon"><Cloud size={16} /></div>
			<div style="flex: 1;">
				<h2 class="section-title">Cloudflare</h2>
				<p class="section-desc">Connect a Cloudflare account to automatically create DNS records when you add a domain to a service.</p>
			</div>
		</div>

		{#if errorMsg}
			<div class="error-banner" style="margin: 0 20px 12px;">
				<AlertCircle size={14} /><span>{errorMsg}</span>
			</div>
		{/if}

		{#if status?.account_id}
			<div class="cf-connected">
				<div class="cf-connected-main">
					<span class="cf-account-name">{status.account_name || status.account_id}</span>
					<span class="cf-zone-count">{status.zones.length} zone{status.zones.length === 1 ? '' : 's'} available</span>
				</div>
				{#if canWrite}
					<button class="cf-btn disconnect" onclick={disconnect}><Trash2 size={13} /> Disconnect</button>
				{/if}
			</div>
			{#if status.zones.length > 0}
				<ul class="cf-zone-list">
					{#each status.zones as zone (zone.id)}
						<li>{zone.name}</li>
					{/each}
				</ul>
			{/if}
		{:else if canWrite}
			<form class="cf-connect-form" onsubmit={connect}>
				<div class="field">
					<span class="field-label">Cloudflare API Token</span>
					<input class="field-input font-mono" type="password" placeholder="Paste an API token with Zone:DNS:Edit permission…" bind:value={tokenInput} autocomplete="off" />
					<p class="field-hint">Create one at <a href="https://dash.cloudflare.com/profile/api-tokens" target="_blank" rel="noopener noreferrer" style="color: var(--accent); text-decoration: underline;">dash.cloudflare.com/profile/api-tokens</a> with "Zone / DNS / Edit" permission for the zones you want Shipyard to manage.</p>
				</div>
				<div class="save-bar">
					<button type="submit" class="cf-btn connect" disabled={connecting}>
						{#if connecting}<Loader2 size={12} class="spin" /> Connecting…{:else}Connect{/if}
					</button>
				</div>
			</form>
		{:else}
			<div class="empty-state">No Cloudflare account connected.</div>
		{/if}
	</section>

	{#if previewDns}
		<section class="settings-section">
			<div class="section-header">
				<div class="section-icon" style="background: rgba(139,92,246,0.1); color: #8B5CF6;"><RefreshCw size={16} /></div>
				<div style="flex: 1;">
					<h2 class="section-title">Sandbox Preview DNS</h2>
					<p class="section-desc">Platform-wide: one wildcard DNS record for <code>*.{previewDns.preview_base_domain}</code>, owned by whichever org's Cloudflare connection covers that zone. Only visible/usable by platform owners or superadmins.</p>
				</div>
			</div>
			<div class="fields">
				<div class="field">
					<span class="field-label">DNS-owner organization</span>
					<input class="field-input font-mono" placeholder="org id" bind:value={selectedOwnerOrgId} />
					<p class="field-hint">Paste the id of the org whose Cloudflare connection covers <code>{previewDns.preview_base_domain}</code>'s zone, then Save.</p>
				</div>
				<div class="save-bar" style="justify-content: flex-start; gap: 8px;">
					<button class="cf-btn connect" onclick={saveOwner} disabled={savingOwner || !selectedOwnerOrgId}>
						{#if savingOwner}<Loader2 size={12} class="spin" /> Saving…{:else}Save Owner{/if}
					</button>
					<button class="cf-btn connect" onclick={syncNow} disabled={syncing || !previewDns.owner_org_id}>
						{#if syncing}<Loader2 size={12} class="spin" /> Syncing…{:else}<RefreshCw size={13} /> Sync DNS record{/if}
					</button>
					{#if syncMsg}<span class="sync-msg">{syncMsg}</span>{/if}
				</div>
				{#if previewDns.cloudflare_record_id}
					<p class="field-hint"><Check size={12} style="display:inline;vertical-align:-2px;" /> Last synced record: <code>{previewDns.cloudflare_record_id}</code></p>
				{/if}
			</div>
		</section>
	{/if}
</div>

{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }
	.spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.8s linear infinite; }

	.cf-page { display: flex; flex-direction: column; gap: 20px; }
	.settings-section { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; }
	.section-header { display: flex; gap: 14px; padding: 18px 20px; align-items: center; border-bottom: 1px solid var(--border); background: var(--bg-elevated); }
	.section-icon { width: 32px; height: 32px; border-radius: var(--radius-md); background: rgba(37,99,235,0.1); color: var(--accent); display: flex; align-items: center; justify-content: center; flex-shrink: 0; margin-top: 1px; }
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }
	.section-desc code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 11px; }

	.cf-connected { display: flex; align-items: center; gap: 14px; padding: 14px 20px; }
	.cf-connected-main { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
	.cf-account-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.cf-zone-count { font-size: 12px; color: var(--text-dim); }
	.cf-zone-list { list-style: none; margin: 0; padding: 0 20px 16px; display: flex; flex-direction: column; gap: 4px; }
	.cf-zone-list li { font-size: 12px; font-family: var(--font-mono); color: var(--text-secondary); }

	.cf-connect-form { padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; }
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
	.field-input { background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px; font-family: var(--font-sans); padding: 8px 10px; outline: none; width: 100%; box-sizing: border-box; }
	.field-input.font-mono { font-family: var(--font-mono); }
	.field-input:focus { border-color: var(--accent); }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; margin: 2px 0 0; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }

	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }
	.save-bar { display: flex; justify-content: flex-end; align-items: center; }
	.sync-msg { font-size: 12px; color: var(--text-muted); }

	.empty-state { padding: 30px; text-align: center; color: var(--text-muted); font-size: 13px; }
	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	.cf-btn { font-size: 12px; font-weight: 500; padding: 5px 12px; border-radius: var(--radius-sm); cursor: pointer; transition: all var(--transition-fast); border: 1px solid transparent; display: inline-flex; align-items: center; gap: 5px; }
	.cf-btn.connect { background: var(--accent); color: white; border-color: var(--accent); }
	.cf-btn.connect:hover:not(:disabled) { opacity: 0.85; }
	.cf-btn.connect:disabled { opacity: 0.5; cursor: default; }
	.cf-btn.disconnect { background: transparent; color: #EF4444; border-color: rgba(239,68,68,0.4); }
	.cf-btn.disconnect:hover { background: rgba(239,68,68,0.08); }
</style>
```

- [ ] **Step 3: Type-check**

Run: `cd frontend && npm run check`
Expected: no new errors versus the pre-existing baseline. If `Organization` isn't exported from `$lib/api/types` under that exact name, check the actual type name used elsewhere in this codebase for an org object and use that instead — do not invent a new type.

- [ ] **Step 4: Commit**

```bash
git add "frontend/src/routes/orgs/[orgSlug]/settings/cloudflare/+page.svelte" "frontend/src/routes/orgs/[orgSlug]/settings/+layout.svelte"
git commit -m "feat(frontend): add the Cloudflare settings page"
```

---

## Task 9: Domain badge + manual end-to-end verification

**Files:**
- Modify: wherever this codebase currently renders a service's domain list in a detail panel (search for a component consuming `Domain[]` and rendering `hostname` — likely `frontend/src/lib/panels/ServiceDetailPanel.svelte` or a dedicated domains sub-component; read the actual current file before editing, this plan doesn't assume its exact name).

**Interfaces:**
- Consumes: `Domain.cloudflare_record_id` (Task 7).

- [ ] **Step 1: Find the domain list rendering**

Run: `grep -rln "hostname" frontend/src/lib/panels frontend/src/lib/components | xargs grep -l "domain"`
Read whichever file(s) this turns up that render a list of a service's domains (showing `hostname`, a delete button, etc.).

- [ ] **Step 2: Add the badge**

In that component's markup, wherever a single domain's hostname is rendered, add a small conditional badge right next to it:
```svelte
{#if domain.cloudflare_record_id}
	<span class="cf-managed-badge" title="DNS record managed by Cloudflare">Cloudflare</span>
{/if}
```
Add a matching style block entry (adjust to the component's existing style conventions — check what CSS custom properties/badge patterns it already uses elsewhere in the same file, e.g. `.meta-chip` from `ServiceNode.svelte` is a good reference for a small pill-shaped badge):
```css
.cf-managed-badge {
	font-size: 10px;
	font-weight: 500;
	padding: 1px 6px;
	border-radius: 100px;
	background: rgba(37,99,235,0.1);
	color: var(--accent);
	border: 1px solid transparent;
}
```

- [ ] **Step 3: Type-check**

Run: `cd frontend && npm run check`
Expected: no new errors.

- [ ] **Step 4: Commit**

```bash
git add <the file(s) you edited in Step 2>
git commit -m "feat(frontend): show a Cloudflare-managed badge on synced domains"
```

- [ ] **Step 5: Manual end-to-end verification**

Requires a real Cloudflare account with at least one zone, and this backend running with real network access (this is the same class of gap this project's other plans have flagged for anything needing a live external service — document the outcome rather than skip it silently):

1. In an org's Settings → Cloudflare page, connect a real Cloudflare API token (scoped to `Zone:DNS:Edit` for at least one test zone). Confirm the connected account name and zone list appear.
2. Add a domain to a service under that zone (e.g. `test123.yourzone.com`). Confirm in the Cloudflare dashboard that an `A` record for that exact hostname now exists, DNS-only (not proxied), pointed at this server's IP. Confirm the Shipyard domain list shows the "Cloudflare" badge.
3. Delete that domain from Shipyard. Confirm the record disappears from the Cloudflare dashboard.
4. Add a domain hostname that does NOT fall under any connected zone (e.g. a completely different apex domain). Confirm domain creation still succeeds in Shipyard (no Cloudflare record created, no badge, no error shown to the user) — this is the "always best-effort" behavior working correctly.
5. As a platform owner/superadmin, go to the Sandbox Preview DNS section, set the DNS-owner org to the one just connected (using a zone that covers your actual `SHIPYARD__SANDBOX__PREVIEW_BASE_DOMAIN`), and click Sync. Confirm a wildcard `A` record (`*.<preview_base_domain>`) appears in the Cloudflare dashboard. Click Sync again and confirm it updates the existing record rather than creating a duplicate.
6. Create a sandbox app and open its editor; confirm the preview URL (`preview-<id>.<preview_base_domain>`) now actually resolves (it wouldn't have before this feature, since no DNS record existed at all until step 5).
