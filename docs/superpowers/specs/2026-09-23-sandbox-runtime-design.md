# Sandbox Runtime — Design Spec

**Date:** 2026-09-23
**Status:** Approved for implementation planning
**Scope:** Sub-project 1 of 4 for the "in-app code editor with live preview and one-button publish" feature. This spec covers only the sandbox runtime — the isolated, per-app execution environment and its lifecycle/API/networking. The code editor UI, HMR/editor-side preview UX, and the publish pipeline are separate sub-projects that will consume this runtime's API and are out of scope here.

## Context

Shipyard is a Rust + SvelteKit container orchestration platform. This feature lets a customer create an "app," edit its code in a built-in editor, see a live preview of it running, and publish it to production with one action. The apps being edited can be **any stack** (Node, Python, static, etc.), and the customers running them are **Shipyard's own multi-tenant SaaS customers** — i.e., mutually untrusted tenants sharing infrastructure. This is the hard case: it directly inherits a gap already flagged in `plans/saas-paas-architecture-plan.md` — the dedicated-VM-per-org isolation story for paid tiers is largely unimplemented, and free-tier isolation is explicitly "best-effort for MVP." This sub-project closes that gap for the sandbox use case specifically, rather than deferring it.

No existing plan document designs an in-browser editor, a per-app dev sandbox, or a live-preview mechanism. The closest existing analog is the edge-function runtime (`plans/edge-functions-plan.md`): one Deno container per org running short-lived V8 isolates with hard resource caps, hot-reloaded via manifest re-fetch. That pattern informed some choices below (stack auto-detection, hot-reload-without-restart) but does not solve arbitrary-stack, filesystem-needing, subprocess-needing execution, which this feature requires.

## Decisions

### Isolation technology: gVisor (`runsc`)

Chosen over Firecracker microVMs and Kata Containers because both alternatives require nested virtualization/KVM on the host, which is not guaranteed on the VPS-class hosts Shipyard runs on, and both require building a substantial new orchestration layer from scratch (jailer, custom rootfs/kernel images, snapshot management for Firecracker; QEMU-based OCI runtime integration for Kata). gVisor is a drop-in `runsc` runtime class for the existing Docker/containerd + Traefik infrastructure — integration is mostly configuration, not new infrastructure. It intercepts syscalls in a user-space kernel, giving materially stronger isolation than default `runc` without a hardware VM boundary. This is the same boundary Google Cloud Run uses to run untrusted multi-tenant customer code in production, which matches this use case.

### Persistence: persistent volume per app

Each app gets its own volume (reusing the existing `volumes` table/mechanism — no new persistence primitive), created once at app creation and mounted into whichever sandbox container is currently running for that app. Sandboxes are ephemeral (start on demand, stop on idle/explicit signal); the volume is not. This avoids repeating `npm install`/`pip install` from scratch on every editor session, at the cost of one volume per app to manage (acceptable — this mirrors how volumes already work for regular services).

### Stack detection: auto-detect with manifest override

Primary path reuses the existing `FunctionDetector` pattern from edge functions: inspect the app's files (`package.json`, `requirements.txt`, etc.) to pick a base image and infer install/dev commands. An optional `shipyard.json` manifest (`{ "runtime": "...", "install": "...", "dev": "..." }`) can be added to override detection or handle stacks detection can't identify. If detection is ambiguous and no manifest is present, sandbox start fails with an error naming the missing manifest as the fix — no silent guessing.

### Resource quotas: dedicated dev-sandbox quota on the `plans` table

Sandbox resource limits are a **separate quota dimension from production limits**, so a customer's editor usage can never starve their live production traffic. This extends the existing `plans` table (which already defines `cpu_cores`/`memory_gb`/etc. per tier for production) with three new columns: `max_concurrent_sandboxes`, `sandbox_cpu_cores`, `sandbox_memory_gb`. No new quota table — same pattern as the existing tier-limits columns.

### Teardown triggers: idle timeout (source of truth) + explicit stop + tab-close signal

Three triggers, all converging on the same teardown path (stop container, deregister Traefik route, keep volume):
1. **Idle timeout** (20 minutes without a heartbeat) — the reliable backstop. This is the source of truth for "is this sandbox actually in use," since the other two signals can be missed (browser crash, network drop). The reaper runs on a fixed interval (e.g. every minute) checking `last_heartbeat_at` against this threshold.
2. **Explicit "stop" button** in the editor UI.
3. **Tab close / navigate away** — best-effort `navigator.sendBeacon` call to the stop endpoint; not relied upon alone.

### Cold preview links: auto-start on visit

Visiting a preview URL while its sandbox is stopped (e.g. a shared link, not opened via the editor) triggers an auto-start: the placeholder page calls the start endpoint unauthenticated (preview links must work for people without editor access), then polls until the container is up and reloads. The start endpoint is idempotent (a concurrent call while already starting/running is a no-op returning current state) and still enforces the owning org's `max_concurrent_sandboxes` — an anonymous visitor can trigger a start but never bypass or inflate the owner's quota. A full quota shows "this app is busy, try again shortly" instead of starting.

## Architecture

Four pieces, reusing existing infrastructure wherever it fits:

- **Sandbox Manager** (new backend module, same shape as the existing edge-function Manager) — owns the full lifecycle: start, stop, heartbeat, idle-reap, quota checks. The only component that talks to the container runtime and to Traefik's dynamic config.
- **gVisor-backed sandbox containers** — one per currently-running app, launched with the `runsc` runtime class, the app's volume mounted, and a base image chosen by stack detection.
- **Per-app persistent volume** — a `volumes` row per app (see Decisions above), survives sandbox stop/start.
- **Traefik dynamic route per app** — `preview-<app-slug>.shipyard-apps.dev` routes to the currently-running sandbox's dev-server port when running; serves a placeholder ("waking up..." / cold-start) page otherwise. Must proxy WebSocket upgrades (not just HTTP) so each stack's dev-server HMR works through the preview URL unmodified — this is what gives live preview its "live" behavior while editing, for free, since the sandbox runs the app's real dev server.

**Flow:** open editor → Manager checks quota → finds/creates volume → resolves runtime (detection or manifest) → starts gVisor container with volume mounted, runs install+dev command → registers Traefik route → returns preview URL. Editor sends a periodic heartbeat while open; idle timeout, explicit stop, or tab-close each trigger the same teardown path.

## Data model

Follows the existing `services`-as-root-table specialization pattern exactly (same shape as static sites / edge function groups — see `db_schema_rules` conventions already established in this codebase):

- **New `service_type` value: `'sandbox_app'`.** Every app-in-progress gets a `services` row from creation, giving it `service_envs`, `topology_edges`, etc. for free and letting it transition into a real deployed service type later (publish sub-project) without inventing a separate resource concept.
- **`sandbox_app_configs(service_id PK REFERENCES services(id) ON DELETE CASCADE)`** — static config: `runtime`, `install_cmd`, `dev_cmd`, `manifest_source` (`'detected'` | `'manifest'`). Same 1:1 specialization pattern as `static_site_configs`.
- **File storage** — no new table. An app's persistent volume is a `volumes` row with `service_id` pointing at it, exactly like any other service's attached volume.
- **`sandbox_instances(service_id PK REFERENCES services(id) ON DELETE CASCADE)`** — live, frequently-mutated runtime state, split out from static config the same way `containers` is split from `services`: `status` (`'stopped'` | `'starting'` | `'running'`), `container_id`, `host_node_id`, `preview_url`, `last_heartbeat_at`, `started_at`.
- **`plans` table** gains `max_concurrent_sandboxes INT`, `sandbox_cpu_cores INT`, `sandbox_memory_gb INT` — extends the existing tier-limits row shape (`free`/`pro`/`max`) rather than a new quota table.

**Deferred, not decided here:** whether `sandbox_app` nodes appear in the project topology graph (like edge function groups do). This belongs to the editor-UI sub-project's design.

**Migration checklist** (per existing convention):
1. `ALTER TYPE service_type ADD VALUE IF NOT EXISTS 'sandbox_app'`
2. Create `sandbox_app_configs`
3. Create `sandbox_instances`
4. `ALTER TABLE plans ADD COLUMN ...` (the three quota columns), backfill sane defaults per existing tier row
5. Topology: exclude `'sandbox_app'` from the generic service query for now (no node renderer yet — deferred per above)

## Lifecycle & API

All endpoints authorized against existing project/org membership checks except where noted.

- **`POST /apps/:id/sandbox/start`** — Manager checks the org's `plans.max_concurrent_sandboxes` against the current count of `sandbox_instances WHERE status='running'` for that org. Under quota: ensures the volume exists, resolves runtime from `sandbox_app_configs` (populating it via detection/manifest on first start), launches the `runsc` container with volume mounted and resource caps from `plans.sandbox_cpu_cores`/`sandbox_memory_gb`, writes the Traefik dynamic config file, writes `sandbox_instances` (`'starting'` → `'running'`), returns the preview URL. Over quota: `409` with an actionable message. Idempotent while already starting/running. Unauthenticated calls (from a cold preview-link visit) are permitted but still subject to the same quota check.
- **`POST /apps/:id/sandbox/heartbeat`** — editor calls on an interval while open; updates `last_heartbeat_at`.
- **`POST /apps/:id/sandbox/stop`** — explicit stop (button, or best-effort beacon on tab close): stops the container, deletes the Traefik config file, sets `status='stopped'`. Volume untouched.
- **Idle reaper** (background periodic task) — scans `sandbox_instances` for `status='running'` rows past the idle threshold since `last_heartbeat_at`, tears them down via the same path as explicit stop.
- **Self-healing** — if a sandbox container dies unexpectedly (crash, OOM under its resource cap) without a stop call, the next heartbeat or start attempt detects the stale `status='running'` row with a dead container, tears it down, and starts fresh rather than surfacing a confusing "already running" error.

## Networking

- Traefik dynamic config file per app, named `sbx-{service_id[..8]}.yml` (mirrors the existing `efg-{group_id[..8]}.yml` naming convention for edge functions), written by the Manager on start and deleted on stop.
- Route must proxy WebSocket upgrades in addition to HTTP, so dev-server HMR (Vite/webpack/etc.) works transparently through the preview URL.
- When no config file exists for an app (sandbox stopped), Traefik falls through to a placeholder responder that serves the cold-start page described in Decisions above.

## Testing

- **Backend unit tests**: Manager's quota-check logic (at limit, over limit, idempotent concurrent start), and the idle-reaper's stale-container self-heal path (dead container, stale `'running'` row).
- **Integration test**: full start → heartbeat → idle-timeout-without-heartbeat → auto-stop cycle against a real `runsc`-backed test container, verifying the Traefik config file's appearance/disappearance.
- **Manual verification**: spin up a real sandbox against a host with `runsc` installed and confirm the live preview URL serves a running dev server with working HMR, before considering the sandbox runtime done — this is infrastructure that's awkward to fully fake in automated tests.
