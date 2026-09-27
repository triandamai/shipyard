# Cloudflare DNS Integration — Design Spec

**Date:** 2026-09-27
**Status:** Approved for implementation planning
**Scope:** Automatic DNS record management via Cloudflare for (1) regular per-service custom domains and (2) the sandbox runtime's platform-wide preview base domain. Both are additive to the existing domain flow — no existing behavior changes for operators/orgs who don't connect Cloudflare.

## Context

Today, adding a domain in Shipyard (`POST /services/:service_id/domains`) only ever does two things: writes a `domains` row, and regenerates the Traefik dynamic config file for that service (`sync_traefik_dynamic_config`, `backend/crates/api/src/resources/mod.rs`). It never touches DNS — the operator or org is expected to point DNS at the server manually, and Shipyard offers a `check-dns` endpoint to tell them whether it's resolved yet. The same is true for the sandbox runtime's preview URLs (`preview-<id>.<preview_base_domain>`): no DNS record is ever created per sandbox — the design already relies on the operator pointing **one wildcard DNS record** (`*.<preview_base_domain>`) at the server, once, when the feature is configured (confirmed via `ensure_preview_domain` in `backend/crates/api/src/sandbox_runtime/manager.rs`, which only ever upserts a `domains` row and re-syncs Traefik config — never DNS).

This sub-project automates both of these manual DNS steps via the Cloudflare API, without changing anything else about how domains or sandbox previews work.

Shipyard is multi-tenant: different organizations can own custom domains in entirely different DNS providers/accounts than the platform operator's. So this integration is **per-organization** — each org optionally connects its own Cloudflare API token, the same way orgs already connect git providers (`git_providers` table, `backend/crates/api/src/git_providers/mod.rs`) for their own source repos. An org with no connection, or whose connection doesn't cover a given hostname's zone, sees no change at all — domain creation falls back to exactly today's manual-DNS behavior.

The one platform-wide piece — the sandbox preview base domain's wildcard record — is not owned by any single org by nature (it's a platform setting, not a service). It's handled by letting a superadmin designate which org's *already-connected* Cloudflare account owns that domain's zone, then triggering a one-time sync. This follows the codebase's existing pattern where some settings pages live under an org-scoped URL (`/orgs/:orgSlug/settings/...`) but actually read/write genuinely platform-wide config, gated by "owner or superadmin" rather than true org-scoping (e.g. the existing Traefik and SMTP settings pages, `backend/crates/api/src/settings/mod.rs`).

## Decisions

### Scope: per-organization Cloudflare connections, not platform-wide

Each org may connect **one** Cloudflare API token (`UNIQUE (org_id)` — YAGNI on multiple accounts per org). This mirrors the existing `git_providers` per-org credential pattern rather than the platform-wide `AppConfig`/env-var pattern used for things like the artifact registry's S3 credentials, because a Cloudflare token is inherently scoped to whichever domains that org's Cloudflare account manages — there is no single platform-wide Cloudflare account that could cover every org's arbitrary custom domains.

### A dedicated crate for the Cloudflare API client

`backend/crates/cloudflare` (`shipyard-cloudflare`), following the same modularity convention as `shipyard-docker` (wraps bollard for the Docker API) and `shipyard-engine`. This crate knows nothing about Shipyard's domains, orgs, or database — it is a minimal, focused wrapper around exactly the Cloudflare API calls this feature needs: verify a token, list accounts, list zones, create/update/delete a DNS record. `crates/api` depends on it and owns all the orchestration (matching a hostname to a zone, deciding when to call it, storing the resulting ids) — the same dependency direction as `api → docker`.

### DNS-only records, never proxied

Every record Shipyard creates is created with Cloudflare's proxy ("orange cloud") **off** — a plain DNS answer, not routed through Cloudflare's edge. This is necessary, not a preference: a proxied record would very likely break Traefik's existing Let's Encrypt HTTP-01 certificate issuance, since Cloudflare's edge would intercept the ACME challenge instead of passing it through to Traefik. Keeping records DNS-only preserves today's TLS behavior exactly — Cloudflare only ever acts as a DNS host for this feature, never a CDN/proxy layer. All records are type `A`, pointed at the server's public IP, TTL 300.

### Resolving the server's public IP

Reuses the exact same 3-tier resolution the existing `/admin/host-ip` endpoint already uses (`backend/crates/api/src/settings/mod.rs`: a configured `DOMAIN` env var if it's a literal IP, else a live lookup via `api4.ipify.org`, else `127.0.0.1`), factored into one shared function both call. No new IP-detection mechanism.

### Zone matching: longest-suffix match, fetched live, no local zone cache

When a domain is created, if the org has a Cloudflare connection, its zones are fetched live (`GET /zones`) and matched against the new hostname by **longest zone name that is a suffix of the hostname** (e.g. `api.staging.example.com` matches zone `example.com` in preference to any shorter/no match). If no zone matches, Cloudflare is skipped entirely for that domain — same effective behavior as an org with no connection at all. No local table caching zones: this is a low-frequency call (only on domain add, and when an org loads its Cloudflare settings page), so keeping a cache in sync isn't worth the complexity (YAGNI).

### Failure handling: Cloudflare is always best-effort, never blocking

If any Cloudflare call fails during `create_domain` (no matching zone, API error, a conflicting record already exists, network failure) the Shipyard-side domain creation still succeeds exactly as it does today — the domain row is created, Traefik config is synced, and `cloudflare_zone_id`/`cloudflare_record_id` simply stay `NULL`. The failure is logged server-side but never surfaced as an error to the end user for the domain-creation request itself. Symmetrically, if deleting the remote Cloudflare record fails during `delete_domain` (token revoked, record already gone, API error), the Shipyard-side deletion still proceeds — a user must always be able to remove a domain from Shipyard regardless of Cloudflare's state. This makes the feature strictly additive: it can only make DNS management easier, never introduce a new way for domain operations to fail. This also covers disconnection cleanly: if an org disconnects its Cloudflare connection and later deletes a domain that still has a stored `cloudflare_record_id`, the lookup for a connection to authenticate the delete simply finds none — the same code path as an org that never connected at all — so the Shipyard-side delete proceeds and the now-orphaned Cloudflare record is left for the org to clean up manually (documented in the settings UI's disconnect confirmation, not automated).

### Sandbox preview wildcard: manual, one-time sync — no background polling

A superadmin picks, once (via a new small settings UI), which org's Cloudflare connection owns the preview base domain's zone. A "Sync" button then creates (or updates, if already synced before) one wildcard `A` record `*.<preview_base_domain>` pointed at the server's IP, using the same zone-matching and record-creation logic as regular domains. The resulting `zone_id`/`record_id` are stored in the existing platform `system_config` table (`sandbox_preview_cloudflare_zone_id`, `sandbox_preview_cloudflare_org_id`, `sandbox_preview_cloudflare_record_id`) rather than a `domains` row, since this isn't tied to any single service. Sync is **operator-triggered only** — if the server's IP changes, an operator clicks Sync again. No scheduled job polls for IP drift (YAGNI; this is a rare event with an obvious one-click fix). No change to any sandbox runtime code — `ensure_preview_domain`, `start_sandbox`/`stop_sandbox`'s Traefik config sync, and the preview hostname format are all untouched; this only adds the DNS record that makes those already-correct preview URLs actually resolve.

## Architecture

### Backend: `shipyard-cloudflare` crate

```rust
pub struct CloudflareClient { /* holds the API token */ }

impl CloudflareClient {
    pub fn new(token: impl Into<String>) -> Self;
    pub async fn verify_token(&self) -> Result<(), CloudflareError>;
    pub async fn list_accounts(&self) -> Result<Vec<Account>, CloudflareError>;
    pub async fn list_zones(&self) -> Result<Vec<Zone>, CloudflareError>;
    pub async fn create_dns_record(&self, zone_id: &str, name: &str, content: &str) -> Result<DnsRecord, CloudflareError>;
    pub async fn update_dns_record(&self, zone_id: &str, record_id: &str, content: &str) -> Result<DnsRecord, CloudflareError>;
    pub async fn delete_dns_record(&self, zone_id: &str, record_id: &str) -> Result<(), CloudflareError>;
}
```
`create_dns_record`/`update_dns_record` always send `type: "A"`, `proxied: false`, `ttl: 300` — not caller-configurable in this first version (YAGNI beyond what this feature needs). Uses `reqwest`, matching the HTTP client already used elsewhere in this codebase (the ipify public-IP lookup, git provider identity round-trips).

### Backend: data model

```sql
CREATE TABLE cloudflare_connections (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
    api_token    TEXT NOT NULL,
    account_id   TEXT NOT NULL,
    account_name TEXT,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (org_id)
);

ALTER TABLE domains
    ADD COLUMN cloudflare_zone_id   TEXT,
    ADD COLUMN cloudflare_record_id TEXT;
```
`api_token` is stored as plaintext, matching the existing `git_providers.token` convention — no new at-rest encryption is introduced by this feature (consistent with today's baseline, not a regression).

New `system_config` keys (reusing the existing settings mechanism in `backend/crates/api/src/settings/mod.rs`, no new storage mechanism): `sandbox_preview_cloudflare_org_id`, `sandbox_preview_cloudflare_zone_id`, `sandbox_preview_cloudflare_record_id`.

### Backend: `create_domain` / `delete_domain` integration

In `backend/crates/api/src/resources/mod.rs`:

- `create_domain`, after the existing `INSERT INTO domains` and before `sync_traefik_dynamic_config`: resolve the domain's org (`service → project → org`), look up its `cloudflare_connections` row. If present, list zones, find the longest-suffix match, resolve the server's public IP, call `create_dns_record`. On success, `UPDATE domains SET cloudflare_zone_id = $1, cloudflare_record_id = $2 WHERE id = $3`. Any failure at any step is logged and swallowed — falls through to the existing Traefik sync exactly as today.
- `delete_domain`, before the existing DB delete: if the domain row has a `cloudflare_record_id`, call `delete_dns_record`. Failure is logged and swallowed — the Shipyard-side delete always proceeds.

### Backend: new routes

```
POST   /orgs/:org_id/cloudflare              connect (body: { api_token })
GET    /orgs/:org_id/cloudflare              connection status + live zone list
DELETE /orgs/:org_id/cloudflare              disconnect

GET    /admin/sandbox/preview-dns            current owner org + last sync status
PUT    /admin/sandbox/preview-dns            set owner org (body: { org_id })
POST   /admin/sandbox/preview-dns/sync       trigger the wildcard record create-or-update
```
The `/orgs/:org_id/cloudflare` routes are gated the same way `git_providers` routes are: `shipyard:{org_id}:providers:write` OR `shipyard:{org_id}:settings:write`. The `/admin/sandbox/preview-dns*` routes are gated the same "owner or superadmin" way the existing Traefik/SMTP settings endpoints are.

`POST /orgs/:org_id/cloudflare` calls `verify_token` then `list_accounts` (Cloudflare's token-verify response doesn't include account details, a separate call is needed) to resolve `account_id`/`account_name` for display, mirroring how `create_git_provider` does a best-effort identity round-trip on connect.

### Frontend

- **New org settings tab, "Cloudflare"** (`orgs/[orgSlug]/settings/cloudflare/+page.svelte`), modeled directly on the existing Providers page (`.../settings/providers/+page.svelte`): not-connected state shows a token input + Connect button; connected state shows the resolved account name, a read-only list of the token's zones, and a Disconnect button. Added to the settings layout's `tabs` array the same way Providers is.
- **New platform settings section, "Sandbox Preview DNS"**: a read-only display of the current `preview_base_domain` (still env-var-configured, not made editable by this feature — out of scope), a dropdown of orgs that have a Cloudflare connection, and a Sync button with last-synced status. Placed alongside the existing (currently tab-bar-unlinked) Traefik/SMTP settings pages, gated the same way.
- **Domains list**: a small "Cloudflare-managed" badge on domains where `cloudflare_record_id` is set. Cosmetic, not required for the feature to function.
- `DomainAddPanel.svelte` requires **no changes** — the Cloudflare sync happens transparently server-side when a domain is created.

## Testing

- **Backend unit tests**: the longest-suffix zone-matching function (pure, easily tested with a table of hostname/zone-list/expected-match cases including "no match"), and the shared public-IP-resolution helper's 3-tier fallback logic (already partially covered by refactoring the existing `/admin/host-ip` logic into a testable function rather than leaving it inline).
- **`shipyard-cloudflare` crate**: unit tests against a mocked HTTP layer (or a lightweight local test server) for request shape (correct method/path/body for each of the four calls) and error mapping (Cloudflare's error-response envelope → `CloudflareError`), independent of a real Cloudflare account.
- **Integration**: no automated test can safely exercise a real Cloudflare account's DNS records in CI. Manual verification (this plan's final task): connect a real Cloudflare account to a test org, add a domain under a zone that account manages, confirm the DNS record appears in the Cloudflare dashboard and `cloudflare_record_id` is stored; delete the domain, confirm the record disappears from Cloudflare; repeat for the sandbox preview wildcard sync.
