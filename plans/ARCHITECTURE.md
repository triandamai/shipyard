# Shipyard SaaS/PaaS — Architecture & Implementation Status

This document describes what is actually built, what is stubbed, and what gaps remain. It is written for a developer who already knows Rust and has read the codebase. References to file paths are relative to the repo root.

---

## 1. System Overview

Shipyard is a container deployment platform. In self-hosted (single-tenant) mode it runs on one machine and manages Docker Swarm services on that machine's local daemon. In SaaS/PaaS mode the same binary adds:

- **Billing**: Stripe subscriptions gate access to dedicated infrastructure.
- **Compute provisioning**: When an org upgrades, a VM is created on a cloud provider and tracked through a state machine until Docker is reachable on it.
- **Deployment routing**: The `DeploymentEngine` checks whether a service has a node assignment and, if so, opens a remote Docker connection to that node instead of using the local socket.

The split is:

| Layer | Name | What runs there |
|---|---|---|
| Control plane | Shipyard Manager | Rust API, PostgreSQL, MQTT broker (rmqtt), Traefik reverse proxy |
| Data plane | Tenant Node | Docker daemon, WireGuard (installed but not configured), user containers |

Free-tier orgs run on the Manager's local Docker daemon (the same machine). Paid-tier orgs get a dedicated VM provisioned on Hetzner (default) or DigitalOcean.

---

## 2. Control Plane (Shipyard Manager)

Runs on the operator's VPS. All components are expected to co-locate on one machine for a single-region deployment.

| Component | Role |
|---|---|
| `shipyard-api` (Rust, Axum) | REST API + webhook handlers + background workers |
| PostgreSQL | Primary datastore for all state |
| rmqtt | MQTT broker; the API acts as the only internal publisher; browser clients subscribe over WebSocket |
| Traefik | Reverse proxy; handles TLS termination for all deployed services, uses Let's Encrypt |

The API binary (`crates/api/src/main.rs`) owns the process boundary. It connects to Docker, MQTT, and Postgres on startup, spawns all background workers, then serves HTTP.

`AppState` (defined in `crates/api/src/main.rs`) carries:

```rust
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: sqlx::PgPool,
    pub docker: Arc<dyn DockerEngine>,   // local daemon connection
    pub mqtt: Arc<MqttPublisher>,
    pub oauth_states: OAuthStates,
    pub redis: Option<redis::aio::ConnectionManager>,
    pub http_client: reqwest::Client,    // reused for all outbound HTTP (Stripe, cloud APIs)
    pub auth_limiter: SharedRateLimiter,
    pub swarm_sync_trigger: Arc<Notify>,
}
```

Redis is optional. When absent, refresh-token revocation (logout) does not work — stolen tokens remain valid until expiry.

---

## 3. Data Plane (Tenant VMs)

Each paid-tier org gets a dedicated VM. What runs on it after cloud-init:

- **Docker daemon** — listening on both Unix socket and `tcp://0.0.0.0:2375` (unauthenticated, no TLS).
- **WireGuard** — installed by cloud-init but not configured with a peer. The WG interface does not exist on the VM in the current implementation.
- **User containers** — deployed via remote Docker API calls from the Manager.

The cloud-init script (`provisioning/mod.rs::build_cloud_init`) does:

1. `apt-get install docker.io wireguard curl`
2. Writes `/etc/docker/daemon.json` to add `tcp://0.0.0.0:2375` alongside the Unix socket.
3. Drops a systemd override to suppress Docker's default `-H fd://` so the daemon reads `daemon.json` cleanly.
4. Restarts Docker.

It does **not** generate WireGuard keys, does not configure a WG interface, and does not call home to the Manager. The Manager finds the VM by polling the cloud provider API until status is `running`, then pings `http://<public_ip>:2375/_ping` directly.

---

## 4. Tenant Onboarding Flow

Step-by-step from account creation to first deployment on a paid node:

1. **User signs up** — `POST /api/auth/register`. Standard JWT auth.
2. **Org created** — `POST /api/orgs`. The new org has no `org_billing` row yet.
3. **Billing page loads** — `GET /api/orgs/:org_id/billing` (`billing/mod.rs::get_org_billing`). If no `org_billing` row exists, one is auto-inserted with `tier = 'free'` and `sub_status = 'active'`. Free orgs can deploy immediately to the Manager's local Docker.
4. **Upgrade clicked** — `POST /api/orgs/:org_id/billing/checkout` (`billing/mod.rs::create_checkout_session`). Creates a Stripe Checkout Session with `metadata.org_id` and `metadata.tier`. Returns the Stripe-hosted checkout URL. Frontend redirects there.
5. **Payment completes** — Stripe calls `POST /api/billing/webhooks` with `checkout.session.completed`.
6. **Webhook handler** (`billing/mod.rs::stripe_webhook`):
   - Verifies `Stripe-Signature` header (HMAC-SHA256 over `timestamp.body`).
   - Upserts `org_billing` with `stripe_customer_id`, `stripe_sub_id`, new `tier`, `sub_status = 'active'`.
   - Looks up the org name to build a node name (e.g. `acme-node-1`).
   - Inserts a `compute_nodes` row with `status = 'provisioning'` if no provisioning row already exists for the org.
7. **ProvisioningWorker picks it up** — next tick (≤30 s) calls `create_pending_vms`, which calls `HetznerProvider::create_vm` and moves the node to `cloud_init_running`.
8. **Node advances through the state machine** (see §5) until `active`.
9. **Services are auto-assigned** — on transition to `active`, all existing unassigned org services get a `service_node_assignments` row pointing to the new node.
10. **MQTT event published** — `platform/orgs/<org_id>/nodes` with event type `node.active`. The billing page reacts without a page refresh.
11. **Next deployment** routes to the node via remote Docker on `public_ip:2375`.

---

## 5. Provisioning State Machine

Defined as a Postgres enum `node_status` in `crates/db/migrations/20250101000028_saas_billing_and_nodes.sql`. The worker lives in `crates/api/src/provisioning/mod.rs` and runs its `tick()` every 30 seconds.

### States

| State | Meaning |
|---|---|
| `provisioning` | Row created; cloud API call pending or in flight |
| `cloud_init_running` | VM created on provider; cloud-init executing |
| `wireguard_joined` | VM API reports `running` status (name is misleading — WireGuard is not actually checked) |
| `active` | Docker daemon reachable at `public_ip:2375`; accepting deployments |
| `degraded` | Previously active; missed Docker heartbeat for >2 minutes |
| `failed` | Provisioning timed out (>15 min) or all 3 VM creation attempts failed |
| `stopped` | Intentionally halted by billing cancellation; VM deletion queued |

### Transitions (what the code actually does)

```
provisioning ──► cloud_init_running ──► wireguard_joined ──► active
                                                               │
                                                               ▼
                                                           degraded
                                                               │
                                              (heartbeat ok)   │  (heartbeat ok)
                                                               ▲
```

| Transition | Trigger | Code |
|---|---|---|
| `provisioning → cloud_init_running` | `create_pending_vms`: provider API `create_vm` returns a `VmDetails` with `public_ip` | Sets `provider_vm_id`, `public_ip`, advances status |
| `cloud_init_running → wireguard_joined` | `advance_booting_nodes`: cloud provider `get_vm_status` returns `VmStatus::Running` | Comment in code: "stub: real WireGuard check TBD" |
| `wireguard_joined → active` | `advance_to_active`: HTTP GET to `http://<public_ip>:2375/_ping` returns 2xx | Also auto-assigns all unassigned org services and publishes MQTT |
| `active → degraded` | `check_heartbeats`: `/_ping` fails on a node whose `last_heartbeat_at` is >2 min old | Sets `status = 'degraded'` only if currently `active` |
| `degraded → active` | `check_heartbeats`: `/_ping` succeeds again | Updates `last_heartbeat_at`, sets `status = 'active'` |
| `active/degraded → stopped` | `customer.subscription.deleted` webhook | Sets `status = 'stopped'` via raw SQL in `billing/mod.rs` |
| `stopped → VM deleted` | `delete_stopped_vms`: node has been `stopped` for >24 h and has a `provider_vm_id` | Calls `provider.delete_vm`, then nulls `provider_vm_id`, `public_ip`, `ip_address`, TLS columns |
| any → `failed` | `timeout_stuck_nodes`: node in `provisioning`, `cloud_init_running`, or `wireguard_joined` and `created_at < NOW() - 15 min` | Sets `provision_error = 'Provisioning timed out after 15 minutes'` |

**What is stubbed in the `cloud_init_running → wireguard_joined` transition**: The state is named `wireguard_joined` but the code simply checks that the cloud provider API reports the VM as `running`. There is no WireGuard peer check. The `wireguard_joined` state currently means "VM OS is up, cloud-init presumably finished."

VM creation is capped at `provision_attempts < 3`. After 3 failed `create_vm` calls the node stays in `provisioning` with `provision_error` set and will be swept to `failed` by the timeout.

---

## 6. Deployment Routing

`DeploymentEngine` is in `crates/engine/src/lib.rs`. It is instantiated per-deployment (not shared state).

### Node resolution

`resolve_docker_for_service(service_id)` runs before every deploy:

```sql
SELECT cn.ip_address, cn.status::text
FROM service_node_assignments sna
JOIN compute_nodes cn ON cn.id = sna.node_id
WHERE sna.service_id = $1
```

Decision logic:

| Condition | Docker engine used |
|---|---|
| No assignment row | Local docker (`self.docker`, the Manager's socket) |
| Assignment exists, `status != 'active'` | Local docker (falls back, logs at debug) |
| Assignment exists, `status = 'active'`, `ip_address` is empty | Local docker (falls back) |
| Assignment exists, `status = 'active'`, `ip_address` non-empty | `BollardDockerEngine::with_http("tcp://<ip>:2375")` |
| `with_http` constructor fails | Local docker (falls back, logs warning) |

**Critical note**: The query joins on `cn.ip_address`, not `cn.public_ip`. In the current provisioning flow, `ip_address` is **always NULL**. The `ip_address` column is intended for the WireGuard overlay IP (which is never set). `public_ip` is what actually holds the node's reachable IP. This means deployments **never route to tenant nodes** under the current code — they always fall back to local Docker.

To make routing work today, either:
- Change the query to use `public_ip` instead of `ip_address`, or
- Add a step in `advance_to_active` that copies `public_ip` into `ip_address`.

The `ip_address` column is the correct long-term field (WireGuard overlay IP), which is why it was named differently from `public_ip`. But until WireGuard is wired up, one of the above workarounds is needed.

---

## 7. Billing Lifecycle

### Stripe events handled (`billing/mod.rs::stripe_webhook`)

| Stripe event | What the code does |
|---|---|
| `checkout.session.completed` | Upserts `org_billing` (customer ID, sub ID, tier, `sub_status = 'active'`); inserts `compute_nodes` row in `provisioning` state (idempotent — skips if one already exists) |
| `customer.subscription.updated` | Updates `stripe_sub_id`, `sub_status`, `current_period_end` on `org_billing` matched by `stripe_customer_id` |
| `customer.subscription.deleted` | Sets `sub_status = 'canceled'`; sets `status = 'stopped'` on all `active` or `degraded` nodes for the org |
| `invoice.payment_failed` | Sets `sub_status = 'past_due'` on `org_billing` |

### What is not implemented

- `customer.subscription.updated` does not handle tier changes (upgrade/downgrade). It only updates the renewal date and status.
- `invoice.payment_failed` does not block new deployments after a grace period. The `sub_status` column is written but nothing reads it to gate deploys.
- There is no reconciliation worker. If a checkout webhook is delivered but provisioning silently fails (e.g. Hetzner API down), there is no retry loop that checks "paid org with no active node."
- The Stripe price IDs are read from `STRIPE_PRICE_PRO` / `STRIPE_PRICE_MAX` environment variables, not from `AppConfig`. This is noted with a "should come from config eventually" comment in the code.

### Signature verification

The webhook handler verifies `Stripe-Signature` using HMAC-SHA256 over `<timestamp>.<body>`. Verification is skipped only when `stripe_webhook_secret` is not configured. In production this must be set.

---

## 8. Free Tier vs Paid Tier

### What changes in the code

| Behaviour | Free tier | Paid tier |
|---|---|---|
| `org_billing.tier` | `free` | `pro` or `max` |
| Node assigned to service | None (no row in `service_node_assignments`) | Row inserted when node goes `active` |
| `resolve_docker_for_service` result | Local Docker (Manager's daemon) | Remote Docker (tenant VM — currently broken, see §6) |
| VM provisioned | No | Yes, on `checkout.session.completed` |

### What the plan specified but the code does not enforce

- CPU/memory/process cgroup limits on free-tier containers. Nothing in the deploy path sets Docker `--memory`, `--pids-limit`, or CPU quota for free orgs. Free and paid orgs run with identical resource constraints on the local daemon.
- Network isolation between free-tier orgs. Each org's containers are on a Docker network named by service ID, but there is no enforcement that prevents cross-org DNS resolution if containers are on the `platform_proxy` Traefik network.
- Replica limits (1 for free, 5 for pro, 20 for max). The deploy path does not check tier before accepting a replica count.
- Subdomain-only domain restriction for free tier. Custom domain configuration is not gated by tier.

---

## 9. Network Architecture

### Intended design (from plan)

- WireGuard overlay `10.100.0.0/16`; each org gets a `/30` subnet.
- Manager is `10.100.0.1`; tenant nodes get overlay IPs like `10.100.1.1`.
- `ip_address` on `compute_nodes` stores the WireGuard overlay IP.
- Docker daemon on tenant nodes listens on the WireGuard interface only (not public internet).
- `DeploymentEngine` connects to `tcp://<wireguard-ip>:2375`.

### Actual current state

- `ip_address` is always NULL. No WireGuard peer configuration is generated or pushed.
- Docker daemon on tenant nodes listens on `tcp://0.0.0.0:2375` — all interfaces, no TLS, no auth. This is exposed on the public internet.
- The provisioning worker connects via `public_ip` for the Docker ping health check.
- `resolve_docker_for_service` looks up `cn.ip_address`, which is NULL, so routing always falls back to local Docker.

**This is the single biggest gap between plan and implementation.** A tenant node is provisioned and reachable, but no deployment is actually sent to it.

### TLS columns

`compute_nodes` has `tls_ca_cert`, `tls_client_cert`, `tls_client_key` columns. These are never written to in the current code. They exist for the planned TLS-secured Docker socket approach. On subscription cancellation (`delete_stopped_vms`) these columns are nulled out — the only place they are referenced in code.

---

## 10. Cloud Provider Abstraction

### Trait (`compute/mod.rs`)

```rust
#[async_trait]
pub trait ComputeProvider: Send + Sync {
    async fn create_vm(&self, opts: &CreateVmOptions<'_>) -> Result<VmDetails, ProviderError>;
    async fn get_vm_status(&self, vm_id: &str) -> Result<VmStatus, ProviderError>;
    async fn delete_vm(&self, vm_id: &str) -> Result<(), ProviderError>;
}
```

`VmStatus` variants: `Running { public_ip }`, `Initializing`, `Off`, `Unknown(String)`.

### Implementations

| Provider | Status | Notes |
|---|---|---|
| `HetznerProvider` | Fully implemented | Uses `https://api.hetzner.cloud/v1/servers`. Hardcodes image `ubuntu-24.04`, server type `cpx21` (2 vCPU, 4 GB RAM). Auth via Bearer token. |
| `DigitalOceanProvider` | Fully implemented | Uses `https://api.digitalocean.com/v2/droplets`. Image `ubuntu-24-04-x64`. Auth via Bearer token. Tags VMs with `shipyard-tenant`. |

**Limitation**: `ProvisioningWorker::new` always instantiates `HetznerProvider`, regardless of `AppConfig::default_cloud_provider`. `DigitalOceanProvider` is implemented but never used by the worker. `delete_stopped_vms` also uses `self.provider` (Hetzner only) with a comment noting this limitation.

### Adding a new provider

1. Implement `ComputeProvider` for the new struct in `crates/api/src/compute/mod.rs`.
2. Update `ProvisioningWorker::new` to select the provider based on `config.default_cloud_provider` (or per-node `provider` column).
3. Map the provider's status strings to `VmStatus` variants in `get_vm_status`.
4. Add config fields for the API key / region / size under `AppConfig`.

---

## 11. Background Workers

All spawned in `crates/api/src/main.rs::async_main`. Listed in spawn order:

| Worker | Interval | What it does |
|---|---|---|
| Docker Event Worker | Event-driven (bollard stream) with exponential backoff restart (1 s → 30 s max) | Streams Docker events from local socket; updates `containers` table; reconciles DB on startup |
| docker_events retention | 24 h | `DELETE FROM docker_events WHERE received_at < NOW() - INTERVAL '7 days'` |
| Deployment scheduler | 5 s when `max_parallel_deployments > 0`; 60 s when disabled | Promotes `queued` deployments to `running` when below the configured concurrency cap |
| Swarm task sync | 10 s poll + immediate wake on `swarm_sync_trigger` notify | `sync_swarm_tasks`: polls Docker Swarm for task state across all nodes; fills the gap for worker-node containers that don't send events to the local socket |
| Provisioning worker | 30 s | Runs `tick()`: timeout stuck nodes → create VMs → advance booting → advance to active → check heartbeats → delete stopped VMs |
| Static site log cleaner | 24 h | Scans `{data_dir}/static/logs/` for `access-YYYY-MM-DD.log` files older than 7 days and deletes them |
| Rate limiter cleanup | 15 s | Calls `retain_recent()` + `shrink_to_fit()` on both the main rate limiter and the auth rate limiter to prevent unbounded memory growth |

---

## 12. What Is Fully Implemented

- **Auth**: JWT access + refresh tokens, rate-limited login/register endpoints, Redis-backed token revocation (when Redis is configured). `crates/api/src/auth/`
- **Organizations**: CRUD, member management, role-based access (member vs admin). `crates/api/src/orgs/`
- **Projects and services**: Full CRUD with git branch tracking, environment variables (encrypted at rest), volume mounts, domain configuration. `crates/api/src/services/`
- **Docker Swarm deployments**: `DeploymentEngine` deploys services as Swarm tasks, supports registry pull and git build. `crates/engine/src/`
- **Static site deployments**: File upload or git-based build, served by shared nginx container. `crates/engine/src/static_site.rs`
- **SSR deployments**: Git-triggered builds with Node.js/other runtimes. `crates/engine/src/ssr.rs`
- **Webhooks**: GitHub, GitLab, Bitbucket push → auto-deploy with signature verification. `crates/api/src/webhooks/`
- **Git OAuth**: GitHub, GitLab, Bitbucket OAuth flows for linking provider accounts. `crates/api/src/git_providers/`
- **MQTT event bus**: All deploy events published; browser clients authenticate with their JWT as MQTT password (verified by `/internal/mqtt/auth`). `crates/mqtt/`
- **Stripe checkout session creation**: `POST /api/orgs/:org_id/billing/checkout` is implemented and calls Stripe API. `billing/mod.rs`
- **Stripe webhook handling**: All four relevant events are handled (see §7). Signature verification is implemented.
- **VM provisioning**: Hetzner Cloud VMs are actually created via the real API; state machine advances to `active` once Docker responds on the public IP. `provisioning/mod.rs`
- **VM deletion**: Stopped nodes are deleted via cloud API after 24 h grace period.
- **Node heartbeat monitoring**: Active nodes are pinged every 30 s and marked `degraded` on miss.
- **Node CRUD API**: `GET/POST /api/orgs/:org_id/nodes`, `GET/DELETE /api/orgs/:org_id/nodes/:node_id`. `nodes/mod.rs`
- **Node migration endpoint**: `POST /api/orgs/:org_id/nodes/:node_id/migrate` re-deploys all services assigned to a node. `nodes/mod.rs`
- **Service auto-assignment**: When a node goes `active`, all unassigned org services get a `service_node_assignments` row.
- **Billing page (frontend)**: Displays current tier, subscription status, nodes list, upgrade button. `frontend/src/routes/orgs/[orgSlug]/billing/+page.svelte`
- **Health check and Prometheus metrics**: `/health` and `/metrics` endpoints. `main.rs`

---

## 13. What Is Stubbed / Incomplete

**`wireguard_joined` state does not check WireGuard.**
Transition from `cloud_init_running → wireguard_joined` fires when the cloud provider API reports the VM is `running`. The state name implies a WireGuard peer handshake was confirmed; no such check exists. The comment in `advance_booting_nodes` says "stub: real WireGuard check TBD."

**Docker port 2375 is unauthenticated and public.**
The cloud-init script binds Docker to `0.0.0.0:2375` with no TLS and no auth. Anyone who knows the public IP can access the Docker API. The `tls_ca_cert`/`tls_client_cert`/`tls_client_key` columns exist for the TLS-secured model described in the plan but are never written.

**Free → Pro service migration with zero downtime.**
`POST /nodes/:id/migrate` exists and fires re-deploys for all assigned services, but it does not: wait for health checks on the new node, atomically swap the `service_node_assignments` row, update Traefik to point at the new IP, or drain the old containers. It is a best-effort fire-and-forget redeploy only.

**Traefik route update on node switch.**
When a service moves to a new node, the Traefik dynamic config needs to point to the new IP. There is no code that updates Traefik config as part of node migration.

**Free-tier pids-limit not set.**
Memory (512 MB) and CPU (0.5 vCPU) limits are applied at deploy time via `ResourceSpec`. The `--pids-limit` flag (fork-bomb prevention) is not set on any container; the Docker Swarm spec currently has no pids-limit field in the `ResourceSpec` type.

---

## 14. Known Gaps (Not Yet Implemented)

These items appear in the plan (`plans/saas-paas-architecture-plan.md`) with no corresponding code:

- **WireGuard overlay network**: No peer configuration is generated or pushed to VMs. The Manager has no WireGuard interface. Routing between Manager and tenant VMs is over public internet. The `ip_address` column (overlay IP) is always NULL; routing uses `public_ip` via `COALESCE(cn.ip_address, cn.public_ip, '')` in `resolve_docker_for_service`.
- **TLS-secured Docker socket**: The plan describes generating a CA cert on the Manager, signing a server cert during cloud-init, and storing the client cert in `compute_nodes`. None of this exists. Docker is plain HTTP on port 2375 with no authentication.
- **Max tier: multiple nodes per org**: The plan allows up to 5 nodes on Max. The schema supports it (`compute_nodes` is 1:many with `organizations`), but provisioning always creates exactly one node per checkout, and the UI shows a single node.
- **Region selection**: `AppConfig` has `hetzner_region` and `do_region` as fixed operator-level settings. Users cannot choose their region.
- **Auto-scaling on Max tier**: Not started.
- **Shared sandbox for Free tier as a separate VM**: Currently free-tier traffic runs on the Manager's own Docker daemon. The plan envisions a separate shared sandbox VM with cgroup isolation. Not implemented.

### Fixed since initial doc was written

- ~~**Deployment routing broken**~~: `resolve_docker_for_service` now uses `COALESCE(cn.ip_address, cn.public_ip, '')` — deployments route to tenant nodes correctly when `public_ip` is set.
- ~~**`DigitalOceanProvider` dead code**~~: `ProvisioningWorker` now uses `HashMap<String, Box<dyn ComputeProvider>>` keyed by provider name. Both Hetzner and DO are registered when the respective API keys are set.
- ~~**Tier not updated on subscription change**~~: `customer.subscription.updated` handler now maps `items.data[0].price.id` against `config.stripe_price_pro`/`stripe_price_max` and updates `org_billing.tier`.
- ~~**No deployment gating**~~: `trigger_deploy`, `redeploy_service`, and `rollback_deployment` now call `check_billing_allows_deploy()` — returns 403 if subscription is `canceled` or `past_due` beyond the 7-day grace window.
- ~~**No billing reconciliation worker**~~: `ProvisioningWorker::run_reconciliation()` runs every 5 min, detects paid orgs with no live node, and re-queues provisioning within the 30-min retry window.
- ~~**No container drain on cancellation**~~: `drain_stopped_nodes()` runs every 30 s tick, scales swarm services to 0 on stopped-node Docker daemons (best-effort), and removes `service_node_assignments` rows so nodes can be deleted.
- ~~**Free-tier no resource limits**~~: `DeploymentEngine` now queries `org_billing.tier` before building `ServiceSpec`. Free-tier deployments are capped at 0.5 vCPU and 512 MB.
- ~~**`STRIPE_PRICE_PRO`/`STRIPE_PRICE_MAX` env-var bypass**~~: Both are now `AppConfig` fields (`stripe_price_pro`, `stripe_price_max`) loaded via the standard `SHIPYARD__` prefix.
- ~~**Provider region ignored**~~: Billing webhook now uses `config.do_region` when `default_cloud_provider = "digitalocean"` and `config.hetzner_region` otherwise.

---

## 15. Configuration Reference

All config is loaded via the `config` crate using `SHIPYARD__` prefix for environment variables (double underscore as separator). A `config.toml` file is also supported. Docker secrets are read from `/run/secrets/` for the four sensitive values.

### Required for SaaS mode (in addition to core API config)

| Env var | Config key | Default | Notes |
|---|---|---|---|
| `SHIPYARD__STRIPE_SECRET_KEY` | `stripe_secret_key` | None | `sk_live_...` or `sk_test_...`. Required for checkout. |
| `SHIPYARD__STRIPE_WEBHOOK_SECRET` | `stripe_webhook_secret` | None | `whsec_...`. Required for webhook signature verification. |
| `STRIPE_PRICE_PRO` | *(env only, not in config)* | *(none)* | Stripe price ID for Pro tier. |
| `STRIPE_PRICE_MAX` | *(env only, not in config)* | *(none)* | Stripe price ID for Max tier. |
| `SHIPYARD__HETZNER_API_KEY` | `hetzner_api_key` | None | Required if `default_cloud_provider = hetzner`. |
| `SHIPYARD__HETZNER_SERVER_TYPE` | `hetzner_server_type` | `cpx21` | Hetzner server type for new VMs. |
| `SHIPYARD__HETZNER_REGION` | `hetzner_region` | `eu-central` | Hetzner datacenter location. |
| `SHIPYARD__DEFAULT_CLOUD_PROVIDER` | `default_cloud_provider` | `hetzner` | Only `hetzner` is wired up in the worker. |
| `SHIPYARD__DO_API_KEY` | `do_api_key` | None | DigitalOcean API token (currently unused by worker). |
| `SHIPYARD__DO_SIZE` | `do_size` | `s-2vcpu-4gb` | DO droplet size (currently unused). |
| `SHIPYARD__DO_REGION` | `do_region` | `fra1` | DO region (currently unused). |

### Core API config (always required)

| Env var | Config key | Default | Notes |
|---|---|---|---|
| `SHIPYARD__DATABASE__URL` | `database.url` | `postgres://shipyard:shipyard@localhost:5432/shipyard` | Or `/run/secrets/shipyard_db_url` |
| `SHIPYARD__AUTH__JWT_SECRET` | `auth.jwt_secret` | `change-me-in-production` | **API refuses to start with default value** |
| `SHIPYARD__AUTH__SECRET_KEY` | `auth.secret_key` | 64 zeros | **API refuses to start with default value**. 64-char hex string. Used for encrypting env vars. |
| `SHIPYARD__AUTH__ACCESS_TOKEN_EXPIRY` | `auth.access_token_expiry` | `3600` | Seconds |
| `SHIPYARD__AUTH__REFRESH_TOKEN_EXPIRY` | `auth.refresh_token_expiry` | `604800` | Seconds |
| `SHIPYARD__SERVER__HOST` | `server.host` | `0.0.0.0` | |
| `SHIPYARD__SERVER__PORT` | `server.port` | `3001` | |
| `SHIPYARD__SERVER__CORS_ALLOWED_ORIGINS` | `server.cors_allowed_origins` | `["*"]` | Restrict in production |
| `SHIPYARD__MQTT__HOST` | `mqtt.host` | `localhost` | |
| `SHIPYARD__MQTT__PORT` | `mqtt.port` | `1883` | |
| `SHIPYARD__MQTT__USERNAME` | `mqtt.username` | None | |
| `SHIPYARD__MQTT__PASSWORD` | `mqtt.password` | None | |
| `SHIPYARD__DOCKER__SOCKET_PATH` | `docker.socket_path` | None (uses default) | |
| `SHIPYARD__DOCKER__LABEL_PREFIX` | `docker.label_prefix` | `platform` | |
| `SHIPYARD__DOCKER__PORT_PROXY` | `docker.port_proxy` | `false` | Set `true` on macOS Docker Desktop |
| `SHIPYARD__TRAEFIK__NETWORK` | `traefik.network` | `platform_proxy` | |
| `SHIPYARD__TRAEFIK__CERT_RESOLVER` | `traefik.cert_resolver` | `letsencrypt` | |
| `SHIPYARD__REDIS__URL` | `redis.url` | None | Optional; disables token revocation if absent |
| `SHIPYARD__DATA_DIR` | `data_dir` | `/opt/shipyard/data` | |
| `SHIPYARD__APP_URL` | `app_url` | `http://localhost:5173` | Frontend URL; used for invite links |
| `SHIPYARD__SMTP__ENABLED` | `smtp.enabled` | `false` | |

Docker secrets (override config): `/run/secrets/shipyard_db_url`, `/run/secrets/shipyard_jwt_secret`, `/run/secrets/shipyard_secret_key`, `/run/secrets/shipyard_smtp_password`.

---

## 16. Database Schema (SaaS Tables)

Migration: `crates/db/migrations/20250101000028_saas_billing_and_nodes.sql`

### `org_billing`

One row per organization. Auto-created with `tier = 'free'` on first `GET /billing` request.

| Column | Type | Notes |
|---|---|---|
| `org_id` | UUID PK | FK → `organizations(id)` CASCADE |
| `stripe_customer_id` | TEXT UNIQUE | Set on `checkout.session.completed` |
| `stripe_sub_id` | TEXT UNIQUE | Set on `checkout.session.completed` |
| `tier` | `subscription_tier` enum | `free`, `pro`, `max` |
| `sub_status` | TEXT | `active`, `past_due`, `canceled`. Not a Postgres enum. |
| `current_period_end` | TIMESTAMPTZ | Updated by `customer.subscription.updated` |
| `updated_at` | TIMESTAMPTZ | |

### `compute_nodes`

One or more rows per paid org. Each row represents one cloud VM.

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `org_id` | UUID | FK → `organizations(id)` CASCADE |
| `name` | TEXT | e.g. `acme-node-1`. Set by webhook handler from org name. |
| `provider` | TEXT | `hetzner` or `digitalocean`. Informational only — worker always uses Hetzner. |
| `provider_vm_id` | TEXT | Cloud provider's server/droplet ID. Set after VM creation. |
| `region` | TEXT | Datacenter region. Set from `AppConfig::hetzner_region` at provisioning time. |
| `ip_address` | TEXT | **Always NULL currently.** Intended for WireGuard overlay IP. |
| `public_ip` | TEXT | Set when VM is created. Used for health checks and Docker ping. |
| `status` | `node_status` enum | See §5 |
| `last_heartbeat_at` | TIMESTAMPTZ | Updated by `check_heartbeats` every 30 s cycle when reachable |
| `cpu_cores` | INT | Set to 2 on auto-provision. Default 1. |
| `ram_mb` | INT | Set to 4096 on auto-provision. Default 1024. |
| `tls_ca_cert` | TEXT | Never written. Reserved for TLS-secured Docker socket. |
| `tls_client_cert` | TEXT | Never written. Reserved. |
| `tls_client_key` | TEXT | Never written. Reserved. |
| `provision_error` | TEXT | Last error from a failed provisioning step |
| `provision_attempts` | INT | Incremented before each `create_vm` call. Capped at 3. |
| `created_at` | TIMESTAMPTZ | |
| `updated_at` | TIMESTAMPTZ | |

Indexes: `idx_compute_nodes_org` on `(org_id)`, `idx_compute_nodes_status` on `(status)`.

### `service_node_assignments`

Maps services to their compute node. One row per service (primary key is `service_id`).

| Column | Type | Notes |
|---|---|---|
| `service_id` | UUID PK | FK → `services(id)` CASCADE |
| `node_id` | UUID | FK → `compute_nodes(id)` RESTRICT (cannot delete node while services are assigned) |
| `assigned_at` | TIMESTAMPTZ | |

Index: `idx_sna_node` on `(node_id)`.

The `PRIMARY KEY (service_id)` constraint means each service can only be assigned to one node at a time. Moving a service to a different node is an `UPDATE` (or DELETE + INSERT), not an INSERT. The RESTRICT FK on `node_id` prevents deleting a node that still has services; callers must reassign or delete services first.
