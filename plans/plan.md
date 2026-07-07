# PaaS Platform — Zero to Production Plan
**Stack:** Rust + Bollard · PostgreSQL · MQTT · Traefik · Docker Swarm · SvelteKit + SvelteFlow

---

## Overview

This document is a complete, ordered implementation plan broken into **Phases** and **Milestones**. Each milestone maps to one or more coding-agent tasks. Follow the order; later phases build on earlier ones.

---

## Phase 0 — Repository & Project Scaffold

### Milestone 0.1 — Monorepo Structure

```
/
├── backend/                  # Rust workspace
│   ├── Cargo.toml            # workspace root
│   ├── crates/
│   │   ├── api/              # HTTP API (axum)
│   │   ├── engine/           # core deployment engine
│   │   ├── docker/           # bollard wrapper & swarm logic
│   │   ├── docker_worker/    # ★ NEW: Docker event listener & reconciler
│   │   ├── mqtt_broker/      # rumqttd or connection to external broker
│   │   ├── traefik/          # traefik config generator
│   │   ├── git/              # git clone / webhook logic
│   │   ├── db/               # sqlx migrations & query layer
│   │   └── common/           # shared types, errors, config
├── frontend/                 # SvelteKit app
│   ├── src/
│   │   ├── lib/
│   │   │   ├── stores/       # state management (svelte stores)
│   │   │   ├── mqtt/         # mqtt client + event bus
│   │   │   ├── api/          # typed fetch client
│   │   │   ├── components/   # shared UI components
│   │   │   └── flows/        # SvelteFlow node types
│   │   └── routes/           # SvelteKit file-based routing
├── infra/
│   ├── docker-compose.dev.yml
│   ├── traefik/
│   │   └── traefik.yml
│   └── postgres/
│       └── init.sql
├── scripts/
│   ├── install.sh            # platform installer (setup wizard entry)
│   └── dev.sh
└── docs/
```

**Tasks:**
- [ ] Init Rust workspace with all crates listed above
- [ ] Init SvelteKit project with TypeScript, Tailwind, shadcn-svelte, SvelteFlow
- [ ] Set up `docker-compose.dev.yml` with postgres, MQTT broker (EMQX or Mosquitto), Traefik, and the backend
- [ ] Configure environment variable schema (`config.toml` + `.env`)

---

## Phase 1 — Core Infrastructure Layer

### Milestone 1.1 — Database Schema (PostgreSQL + sqlx)

Design all tables upfront. Use UUID primary keys. Use `sqlx` with compile-time checked queries.

**Tables:**

```sql
-- Auth & Org
users (id, email, password_hash, created_at, updated_at)
organizations (id, name, slug, created_at)
org_members (id, org_id, user_id, role ENUM('owner','admin','member','viewer'), created_at)
invitations (id, org_id, email, role, token, expires_at, accepted_at)

-- Projects
projects (id, org_id, name, slug, directory_path, node_positions JSONB, created_at, updated_at)

-- Services / Apps
services (id, project_id, name, slug, type ENUM('git','docker','docker_compose','manual','static','database'), directory_path, status, replicas, created_at, updated_at)
service_envs (id, service_id, key, value_encrypted, is_secret, created_at)
volumes (id, service_id, name, mount_path, driver, size_mb, created_at)
networks (id, project_id, name, driver, subnet, created_at)
service_networks (service_id, network_id)
domains (id, service_id, hostname, tls_enabled, traefik_router_name, created_at)

-- ★ NEW: Containers (replicas) — reconciled from Docker events
containers (
  id UUID PRIMARY KEY,
  service_id UUID REFERENCES services(id) ON DELETE CASCADE,
  docker_container_id TEXT NOT NULL UNIQUE,  -- Docker task/container ID
  docker_task_id TEXT,                        -- Swarm task ID
  node_id TEXT,                               -- Swarm node this ran on
  replica_index INT,                          -- 0-based replica slot
  status ENUM('pending','preparing','running','complete','failed','shutdown','rejected','orphan'),
  status_message TEXT,                        -- Docker status detail / error
  image TEXT NOT NULL,
  started_at TIMESTAMPTZ,
  finished_at TIMESTAMPTZ,
  exit_code INT,
  created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
  updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)

-- ★ NEW: Docker event audit log — raw events from the Docker socket
docker_events (
  id BIGSERIAL PRIMARY KEY,
  event_type TEXT NOT NULL,      -- 'container', 'service', 'network', 'volume', 'node'
  action TEXT NOT NULL,          -- 'create', 'start', 'die', 'destroy', 'update', etc.
  actor_id TEXT NOT NULL,        -- Docker object ID
  actor_attributes JSONB,        -- all labels / attributes from the event
  scope TEXT,                    -- 'local' | 'swarm'
  raw JSONB NOT NULL,            -- full raw event payload for replay/debugging
  received_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
)

-- Deployments
deployments (id, service_id, triggered_by, source_ref, status ENUM('pending','running','success','failed','cancelled'), created_at, finished_at)
deployment_steps (id, deployment_id, name, status, order_index, started_at, finished_at)
deployment_logs (id, deployment_id, step_id, level ENUM('debug','info','warn','error'), message, timestamp)

-- Topology (computed/cached)
topology_edges (id, project_id, source_node_id, target_node_id, edge_type, created_at)
```

**Tasks:**
- [ ] Create all migrations in `crates/db/migrations/`
- [ ] Generate sqlx types with `sqlx-cli`
- [ ] Write repository trait for each domain entity (UserRepo, ProjectRepo, ServiceRepo, DeploymentRepo, ContainerRepo, DockerEventRepo)
- [ ] Implement postgres repositories
- [ ] Add index on `containers(service_id)`, `containers(docker_container_id)`, `docker_events(actor_id)`, `docker_events(received_at)`

### Milestone 1.2 — Docker / Swarm Engine (`crates/docker`)

Wrap bollard with a clean internal API.

**Interfaces to implement:**

```rust
pub trait DockerEngine: Send + Sync {
    async fn create_service(&self, spec: ServiceSpec) -> Result<String>;
    async fn update_service(&self, id: &str, spec: ServiceSpec) -> Result<()>;
    async fn remove_service(&self, id: &str) -> Result<()>;
    async fn scale_service(&self, id: &str, replicas: u64) -> Result<()>;
    async fn list_tasks(&self, service_id: &str) -> Result<Vec<TaskInfo>>;
    async fn container_logs(&self, container_id: &str, opts: LogOpts) -> Result<LogStream>;
    async fn create_network(&self, name: &str, driver: &str) -> Result<String>;
    async fn remove_network(&self, id: &str) -> Result<()>;
    async fn create_volume(&self, name: &str, driver: &str) -> Result<()>;
    async fn remove_volume(&self, name: &str) -> Result<()>;
    async fn pull_image(&self, image: &str, tag: &str) -> Result<PullStream>;
    async fn build_image(&self, context_path: &str, tag: &str) -> Result<BuildStream>;
    async fn init_swarm(&self, advertise_addr: &str) -> Result<()>;
    async fn swarm_info(&self) -> Result<SwarmInfo>;
    // ★ NEW
    async fn event_stream(&self, filters: EventFilters) -> Result<DockerEventStream>;
    async fn inspect_task(&self, task_id: &str) -> Result<TaskDetail>;
    async fn inspect_container(&self, container_id: &str) -> Result<ContainerDetail>;
}
```

**Tasks:**
- [ ] Implement `BollardDockerEngine` using bollard async client
- [ ] Implement log streaming with timestamp parsing and level detection
- [ ] Implement image build streaming (parse Docker build output per-step)
- [ ] Implement swarm service spec builder (mounts, env, labels, networks, replicas, resource limits)
- [ ] Unit tests with a real local Docker socket (integration tests)

### Milestone 1.3 — Traefik Config Generator (`crates/traefik`)

Traefik is driven via its Docker provider (labels on swarm services) + optional dynamic file config.

**Tasks:**
- [ ] Write label generator: given a `Domain` + `ServiceId`, produce correct Traefik labels:
  - `traefik.enable=true`
  - `traefik.http.routers.<name>.rule=Host(\`hostname\`)`
  - `traefik.http.routers.<name>.entrypoints=websecure`
  - `traefik.http.routers.<name>.tls.certresolver=letsencrypt`
  - `traefik.http.services.<name>.loadbalancer.server.port=<port>`
- [ ] TLS support via Let's Encrypt (ACME) configuration
- [ ] Wildcard domain support
- [ ] Health check label injection
- [ ] Strip prefix middleware support

### Milestone 1.4 — MQTT Layer (`crates/mqtt_broker`)

Use `rumqttc` as the async MQTT client. All real-time events (logs, deployment status, container metrics) flow through MQTT.

**Topic Schema:**

```
platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/status
platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/deployments/{deployment_id}/steps/{step_id}/log
platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/deployments/{deployment_id}/status
platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/logs/{replica_id}
platform/orgs/{org_id}/projects/{project_id}/topology
platform/system/health
# ★ NEW — Docker event worker publishes to these:
platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/containers
platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/containers/{container_id}/status
platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/replicas/count
platform/docker/events/raw          # debug/audit only, not subscribed by UI
```

**Payload format (JSON):**
```json
{
  "event": "deployment.log",
  "timestamp": "2025-01-01T00:00:00Z",
  "level": "info",
  "message": "...",
  "meta": {}
}
```

**Tasks:**
- [ ] MQTT client wrapper with auto-reconnect
- [ ] Publisher service (fire-and-forget for log lines, QoS 0; status changes QoS 1)
- [ ] Subscription manager (subscribe per org/project/service scope)
- [ ] Message serialization with `serde_json`
- [ ] Bridge: Docker log stream → MQTT topic publisher

---

### Milestone 1.5 — ★ Docker Event Worker (`crates/docker_worker`)

This is a **long-running background task** started at platform boot. It opens a persistent stream to the Docker socket via bollard's `events()` API, listens to every event (container lifecycle, swarm task transitions, network/volume changes), reconciles state into Postgres, and publishes real-time MQTT events so the UI always reflects Docker's ground truth — even for changes that didn't originate from the platform (manual `docker` CLI commands, external tooling, node failures).

#### Architecture

```
Docker Socket (/var/run/docker.sock)
        │
        │  bollard::Docker::events() — async stream
        ▼
┌──────────────────────────────────────────────────┐
│             DockerEventWorker (tokio task)        │
│                                                  │
│  EventStream ──► EventRouter                     │
│                      │                           │
│          ┌───────────┼──────────────┐            │
│          ▼           ▼              ▼            │
│   ContainerHandler  ServiceHandler  NetworkHandler│
│   VolumeHandler     NodeHandler                  │
│          │                                       │
│          ▼                                       │
│   ┌─────────────┐    ┌──────────────┐           │
│   │  DB Writer  │    │ MQTT Publisher│           │
│   │  (sqlx)     │    │  (rumqttc)   │           │
│   └─────────────┘    └──────────────┘           │
└──────────────────────────────────────────────────┘
```

The worker is **stateless between restarts**: on startup it performs a full reconciliation pass (list all swarm tasks, compare to DB) before entering the event loop. This ensures no events are missed during downtime.

#### Event Types to Handle

**Container / Task events** (most critical):

| Docker Action | Platform Meaning | DB Operation | MQTT Publish |
|---|---|---|---|
| `task create` | New replica scheduled | INSERT containers | `containers` topic |
| `task start` / container `start` | Replica now running | UPDATE containers.status = running, started_at | `containers/{id}/status` |
| `container die` | Replica exited | UPDATE status, exit_code, finished_at | `containers/{id}/status` |
| `task complete` | Healthy shutdown | UPDATE status = complete | `containers/{id}/status` |
| `task failed` | Replica failed | UPDATE status = failed, status_message | `containers/{id}/status` |
| `task shutdown` | Graceful stop | UPDATE status = shutdown | `containers/{id}/status` |
| `task rejected` | Node rejected task | UPDATE status = rejected | `containers/{id}/status` |
| `container destroy` | Container removed | UPDATE status = orphan OR DELETE | `containers` topic (full list refresh) |

After each container event: **recount running replicas** for the parent service and UPDATE `services.status` + `services.replicas` accordingly. Publish to `services/{service_id}/status` and `services/{service_id}/replicas/count`.

**Service events** (swarm service object changes):

| Docker Action | Platform Meaning | DB Operation | MQTT Publish |
|---|---|---|---|
| `service create` | Swarm service registered | no-op (we created it) | — |
| `service update` | Spec changed externally | log to docker_events, warn | `services/{id}/status` |
| `service remove` | Service deleted externally | UPDATE services.status = stopped | `services/{id}/status` |

**Network events:**

| Docker Action | Platform Meaning |
|---|---|
| `network create` | Sync if not in DB (external) |
| `network destroy` | UPDATE networks, cascade topology |
| `network connect` | Log connection to service_networks if from a known container |
| `network disconnect` | Remove from service_networks |

**Volume events:**

| Docker Action | Platform Meaning |
|---|---|
| `volume create` | Sync if not in DB |
| `volume destroy` | UPDATE volumes, publish topology update |
| `volume mount` | Record which container is using it |
| `volume unmount` | Clear the mount record |

**Node events** (swarm node health):

| Docker Action | Platform Meaning | MQTT Publish |
|---|---|---|
| `node update` | Node available/drain/down | `platform/system/nodes` |

#### Implementation Details

```rust
// crates/docker_worker/src/lib.rs

pub struct DockerEventWorker {
    docker: Arc<dyn DockerEngine>,
    db: Arc<dyn DbPool>,
    mqtt: Arc<MqttPublisher>,
    label_prefix: String,  // e.g. "platform.service_id" — our label key on swarm services
}

impl DockerEventWorker {
    /// Called at startup before entering event loop.
    /// Lists all swarm tasks, reconciles with containers table.
    pub async fn reconcile_on_startup(&self) -> Result<()>;

    /// Main loop — never returns unless Docker socket disconnects.
    pub async fn run(&self) -> Result<()>;

    async fn handle_event(&self, event: DockerEvent) -> Result<()>;
    async fn handle_container_event(&self, action: &str, actor: &EventActor) -> Result<()>;
    async fn handle_service_event(&self, action: &str, actor: &EventActor) -> Result<()>;
    async fn handle_network_event(&self, action: &str, actor: &EventActor) -> Result<()>;
    async fn handle_volume_event(&self, action: &str, actor: &EventActor) -> Result<()>;
    async fn handle_node_event(&self, action: &str, actor: &EventActor) -> Result<()>;

    /// Resolve Docker container/task → platform service_id via label lookup.
    /// Swarm services are created with label `platform.service_id=<uuid>`.
    async fn resolve_service_id(&self, labels: &HashMap<String, String>) -> Option<Uuid>;

    /// Recompute running replica count for a service and push to DB + MQTT.
    async fn sync_service_replica_count(&self, service_id: Uuid) -> Result<()>;

    /// Topology has changed (container added/removed) — publish topology diff.
    async fn publish_topology_update(&self, project_id: Uuid) -> Result<()>;
}
```

**Label convention** — every swarm service spec MUST be created with these labels (enforced in `crates/engine`):
```
platform.service_id   = <service uuid>
platform.project_id   = <project uuid>
platform.org_id       = <org uuid>
platform.managed      = "true"
```
The worker uses these labels on every `actor_attributes` to correlate Docker objects back to platform entities. Events without `platform.managed=true` are stored in `docker_events` but otherwise ignored.

**Reconnect / backpressure strategy:**
- If the Docker socket stream drops: exponential backoff (1s, 2s, 4s … max 30s), then reconnect and run `reconcile_on_startup()` again before re-entering the event loop.
- Events are processed serially per `service_id` (use `tokio::sync::Mutex` keyed by service ID) to avoid race conditions when multiple tasks for the same service fire simultaneously.
- Raw events are INSERT-ed into `docker_events` table **before** processing, so nothing is lost even if a handler panics.

#### Startup Integration

The worker is spawned as a `tokio::task` when the platform boots, inside `main.rs`:

```rust
// backend/src/main.rs
let worker = DockerEventWorker::new(docker.clone(), db.clone(), mqtt.clone());
tokio::spawn(async move {
    loop {
        if let Err(e) = worker.run().await {
            tracing::error!("Docker event worker crashed: {e}. Restarting...");
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    }
});
```

**Tasks:**
- [ ] Create `crates/docker_worker` crate with `DockerEventWorker` struct
- [ ] Implement `reconcile_on_startup()`: list all swarm tasks via bollard, upsert into `containers` table, compute correct service statuses
- [ ] Implement `run()` event loop using `bollard::Docker::events()` async stream
- [ ] Implement all five event handlers (container, service, network, volume, node)
- [ ] Implement label-based service resolution (`resolve_service_id`)
- [ ] Implement `sync_service_replica_count` — update `services.replicas` and `services.status`
- [ ] Implement exponential backoff reconnect loop
- [ ] Implement `publish_topology_update` — diff topology and publish to MQTT
- [ ] Write `ContainerRepo` with upsert-by-docker-container-id and bulk-list-by-service
- [ ] Enforce label injection in `crates/engine` swarm service spec builder
- [ ] Integration test: spawn a swarm service with 3 replicas, assert 3 rows appear in `containers`, scale to 1, assert 2 rows removed/shutdown
- [ ] Integration test: kill a container manually (`docker rm -f`), assert `containers.status` becomes `failed` and `services.status` updates

Framework: **axum** with tower middleware.

### Milestone 2.1 — Auth & Session

- [ ] POST `/auth/register`
- [ ] POST `/auth/login` → JWT access token + refresh token
- [ ] POST `/auth/refresh`
- [ ] POST `/auth/logout`
- [ ] GET  `/auth/me`
- [ ] Middleware: JWT extractor, attach user to request state
- [ ] Password hashing: `argon2`
- [ ] JWT: `jsonwebtoken` crate

### Milestone 2.2 — Setup Wizard API

Called once on fresh install.

- [ ] GET  `/setup/status` → `{ initialized: bool }`
- [ ] POST `/setup/init` → create admin user, org, configure DB, initialize swarm, configure Traefik
- [ ] POST `/setup/check-docker` → verify Docker socket access
- [ ] POST `/setup/check-swarm` → verify or init swarm mode
- [ ] Middleware: block API if not initialized (except `/setup/*` and `/health`)

### Milestone 2.3 — Organizations

- [ ] GET    `/orgs` — list orgs for current user
- [ ] POST   `/orgs` — create org
- [ ] GET    `/orgs/:org_id`
- [ ] PUT    `/orgs/:org_id`
- [ ] DELETE `/orgs/:org_id`
- [ ] GET    `/orgs/:org_id/members`
- [ ] POST   `/orgs/:org_id/members/invite` — send invite email
- [ ] PUT    `/orgs/:org_id/members/:user_id/role`
- [ ] DELETE `/orgs/:org_id/members/:user_id`
- [ ] POST   `/orgs/:org_id/invitations/:token/accept`

### Milestone 2.4 — Projects

On creation: create directory `<base_path>/<org_id>/<project_id>/`

- [ ] GET    `/orgs/:org_id/projects`
- [ ] POST   `/orgs/:org_id/projects` → create dir, insert DB row
- [ ] GET    `/orgs/:org_id/projects/:project_id`
- [ ] PUT    `/orgs/:org_id/projects/:project_id`
- [ ] DELETE `/orgs/:org_id/projects/:project_id` → remove dir, services, swarm services

### Milestone 2.5 — Services / Apps

On creation: create directory `<base_path>/<org_id>/<project_id>/<service_id>/`

- [ ] GET    `/projects/:project_id/services`
- [ ] POST   `/projects/:project_id/services`
- [ ] GET    `/projects/:project_id/services/:service_id`
- [ ] PUT    `/projects/:project_id/services/:service_id`
- [ ] DELETE `/projects/:project_id/services/:service_id`
- [ ] GET    `/projects/:project_id/services/:service_id/env`
- [ ] PUT    `/projects/:project_id/services/:service_id/env` (bulk update)
- [ ] POST   `/projects/:project_id/services/:service_id/env` (single)
- [ ] DELETE `/projects/:project_id/services/:service_id/env/:key`

### Milestone 2.6 — Domains, Volumes, Networks

**Domains:**
- [ ] GET    `/services/:service_id/domains`
- [ ] POST   `/services/:service_id/domains` → generate Traefik labels, update swarm service
- [ ] DELETE `/services/:service_id/domains/:domain_id`

**Volumes:**
- [ ] GET    `/services/:service_id/volumes`
- [ ] POST   `/services/:service_id/volumes` → create Docker volume, attach to service
- [ ] DELETE `/services/:service_id/volumes/:volume_id`

**Networks:**
- [ ] GET    `/projects/:project_id/networks`
- [ ] POST   `/projects/:project_id/networks` → create Docker network
- [ ] POST   `/projects/:project_id/networks/:network_id/attach` → attach service
- [ ] DELETE `/projects/:project_id/networks/:network_id`

### Milestone 2.7 — Deployments

**Deployment engine flow (per service type):**

```
Trigger
  └─ Step 1: Validate config
  └─ Step 2 (git): Clone / pull repo → project/service dir
  └─ Step 2 (docker): Pull image
  └─ Step 2 (compose): Parse compose file
  └─ Step 3: Build image (if Dockerfile / git with Dockerfile)
  └─ Step 4: Push to registry (if applicable)
  └─ Step 5: Apply env vars
  └─ Step 6: Configure volumes
  └─ Step 7: Configure networks
  └─ Step 8: Configure domains (Traefik labels)
  └─ Step 9: Create/update swarm service
  └─ Step 10: Health check
  └─ Step 11: Mark deployment success/failed
```

Each step publishes status + logs to MQTT.

- [ ] POST `/services/:service_id/deploy` — trigger deployment
- [ ] GET  `/services/:service_id/deployments` — list with pagination
- [ ] GET  `/services/:service_id/deployments/:deployment_id`
- [ ] GET  `/services/:service_id/deployments/:deployment_id/steps`
- [ ] GET  `/services/:service_id/deployments/:deployment_id/logs` — with query params: `?level=info&from=<timestamp>&follow=true`
- [ ] POST `/services/:service_id/deployments/:deployment_id/cancel`
- [ ] POST `/services/:service_id/start`
- [ ] POST `/services/:service_id/stop`
- [ ] POST `/services/:service_id/restart`
- [ ] POST `/services/:service_id/redeploy` — redeploy last successful deployment

### Milestone 2.8 — Logs API

- [ ] GET `/services/:service_id/logs` — `?replica=<task_id>&from=<timestamp>&follow=true&level=<level>`
- [ ] GET `/services/:service_id/replicas` — list running tasks/replicas with IDs
- [ ] SSE endpoint: `/services/:service_id/logs/stream` — Server-Sent Events for real-time log follow

### Milestone 2.8b — ★ Containers API

Expose the `containers` table (populated by the Docker Event Worker) via REST so the UI can query current and historical replica state.

- [ ] GET `/services/:service_id/containers` — list all containers for a service (filter by `?status=running|failed|all`)

  Response:
  ```json
  {
    "data": [
      {
        "id": "uuid",
        "docker_container_id": "abc123def456",
        "docker_task_id": "task789",
        "node_id": "node1",
        "replica_index": 0,
        "status": "running",
        "status_message": null,
        "image": "myapp:latest",
        "started_at": "2025-01-15T14:32:00Z",
        "finished_at": null,
        "exit_code": null
      }
    ]
  }
  ```

- [ ] GET `/services/:service_id/containers/:container_id` — single container detail
- [ ] GET `/services/:service_id/containers/:container_id/logs` — log stream for a specific container (proxies to Docker log API)
- [ ] GET `/projects/:project_id/containers` — all containers across a project (for topology)
- [ ] GET `/docker-events` — paginated audit log of raw Docker events (admin only, `?actor_id=&type=&from=&to=`)

### Milestone 2.9 — Topology API

Compute and return graph data for a project.

- [ ] GET `/projects/:project_id/topology`

Response:
```json
{
  "nodes": [
    { "id": "svc_1", "type": "service", "data": { "name": "api", "status": "running" } },
    { "id": "net_1", "type": "network", "data": { "name": "backend-net" } },
    { "id": "vol_1", "type": "volume",  "data": { "name": "postgres-data" } },
    { "id": "dom_1", "type": "domain",  "data": { "hostname": "api.example.com" } }
  ],
  "edges": [
    { "id": "e1", "source": "svc_1", "target": "net_1", "type": "network" },
    { "id": "e2", "source": "svc_1", "target": "vol_1", "type": "volume" },
    { "id": "e3", "source": "dom_1", "target": "svc_1", "type": "domain" }
  ]
}
```

MQTT publishes topology update whenever any resource changes.

### Milestone 2.10 — Git Integration (`crates/git`)

- [ ] Clone repo to `<project_dir>/<service_dir>/` using `git2` crate
- [ ] Pull latest on redeploy
- [ ] Parse branches, tags, commits
- [ ] Webhook receiver: POST `/webhooks/github`, POST `/webhooks/gitlab`, POST `/webhooks/gitea`
- [ ] Auto-deploy on push to configured branch

### Milestone 2.11 — Middleware & Cross-cutting

- [ ] RBAC middleware: extract role from `org_members`, enforce per-route
- [ ] Rate limiting (tower governor)
- [ ] Request tracing (tracing + tower-http)
- [ ] Audit log: every mutating action written to `audit_logs` table
- [ ] Error types: structured `AppError` → JSON response
- [ ] Pagination helper: cursor-based for logs, offset for lists

---

## Phase 3 — Frontend (`frontend/`)

### Milestone 3.1 — Project Setup & Design System

- [ ] Install: `shadcn-svelte`, `@xyflow/svelte` (SvelteFlow), `tailwindcss`, `lucide-svelte`
- [ ] Configure Tailwind with dark theme as default (discord-like dark sidebar)
- [ ] Define CSS variables for color tokens:
  - `--bg-base`: #0d0d0f (near-black canvas)
  - `--bg-surface`: #1a1a1f (sidebar, cards)
  - `--bg-elevated`: #242429 (popups, hover)
  - `--accent`: #7c6af7 (purple — primary actions)
  - `--accent-green`: #22c55e (running status)
  - `--accent-red`: #ef4444 (error/stopped)
  - `--accent-yellow`: #f59e0b (building/warning)
  - `--border`: #2e2e35
  - `--text-primary`: #e8e8ed
  - `--text-muted`: #8888a0
- [ ] Typography: `Inter` for UI, `JetBrains Mono` for logs and code
- [ ] Global layout: `AppShell` with left sidebar + main content area

### Milestone 3.2 — State Management (`src/lib/stores/`)

All stores use Svelte writable/derived. No component-local state for business logic.

```
stores/
├── auth.store.ts        # user, token, org memberships
├── org.store.ts         # active org, members
├── project.store.ts     # projects list, active project
├── service.store.ts     # services, active service
├── container.store.ts   # ★ NEW: containers/replicas per service, keyed by service_id
├── deployment.store.ts  # deployments, active deployment, steps
├── log.store.ts         # log buffer, level filter, timestamp cursor, follow flag
├── topology.store.ts    # nodes[], edges[] for SvelteFlow
├── ui.store.ts          # sidebar collapse, active slide panels stack
└── mqtt.store.ts        # connection status, subscriptions
```

Each store exports typed actions (functions that call API then update store).

### Milestone 3.3 — MQTT Event Bus (`src/lib/mqtt/`)

```
mqtt/
├── client.ts            # mqtt.js connection, auto-reconnect
├── eventBus.ts          # typed EventEmitter (mitt)
├── subscriptions.ts     # subscribe per org/project/service scope
└── handlers/
    ├── deployment.handler.ts  → updates deployment.store & log.store
    ├── service.handler.ts     → updates service.store status
    ├── container.handler.ts   → ★ NEW: updates container.store, triggers replica count badge refresh
    ├── topology.handler.ts    → updates topology.store
    └── log.handler.ts         → appends to log.store ring buffer
```

**Mount in root layout** (`+layout.svelte`): connect MQTT once, pass context down. Never connect in individual components.

```svelte
<!-- src/routes/+layout.svelte -->
<script>
  import { onMount } from 'svelte';
  import { initMqtt } from '$lib/mqtt/client';
  import { mqttStore } from '$lib/stores/mqtt.store';

  onMount(() => {
    initMqtt({ onEvent: (topic, payload) => eventBus.emit(topic, payload) });
  });
</script>
```

#### ★ Container Store (`src/lib/stores/container.store.ts`)

```ts
// Keyed by service_id → array of containers (sorted by replica_index)
type ContainerMap = Record<string, Container[]>;

interface Container {
  id: string;
  dockerContainerId: string;
  dockerTaskId: string;
  nodeId: string;
  replicaIndex: number;
  status: 'pending' | 'preparing' | 'running' | 'complete' | 'failed' | 'shutdown' | 'rejected' | 'orphan';
  statusMessage: string | null;
  image: string;
  startedAt: string | null;
  finishedAt: string | null;
  exitCode: number | null;
}

// Store actions
containerStore.loadForService(serviceId)    // fetch GET /services/:id/containers
containerStore.handleMqttUpdate(payload)    // called by container.handler.ts
containerStore.getRunningCount(serviceId)   // derived: count where status='running'
containerStore.getForService(serviceId)     // returns Container[] or []
```

#### ★ Container MQTT Handler (`src/lib/mqtt/handlers/container.handler.ts`)

Subscribes to:
- `platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/containers` — full container list refresh
- `platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/containers/{container_id}/status` — single container status delta
- `platform/orgs/{org_id}/projects/{project_id}/services/{service_id}/replicas/count` — running replica count badge

Behavior:
- On full-list message: replace `containerStore[service_id]` entirely
- On single-status message: patch only that container's `status`, `statusMessage`, `finishedAt`, `exitCode` in the store
- On replica-count message: update `serviceStore[service_id].replicas` so the badge on the canvas node updates immediately
- After any update: call `topologyStore.refreshNode(serviceId)` so the canvas node re-renders with new status color/badge

### Milestone 3.4 — API Client (`src/lib/api/`)

```
api/
├── client.ts            # base fetch wrapper, token injection, error handling
├── auth.api.ts
├── orgs.api.ts
├── projects.api.ts
├── services.api.ts
├── deployments.api.ts
├── logs.api.ts
├── topology.api.ts
└── types.ts             # shared API types (mirror Rust types)
```

### Milestone 3.5 — App Shell & Sidebar

```
Layout:
┌────────────────────────────────────────────┐
│ [Icon bar 48px] │ [Context panel 220px]   │ [Main content]          │
│                 │                          │                         │
│ • Home          │ Projects list            │  Canvas / Detail view   │
│ • Projects      │ ├─ project-alpha         │                         │
│ • Settings      │ └─ project-beta          │                         │
│                 │                          │                         │
│ [User avatar]   │ [+ New Project]          │                         │
└────────────────────────────────────────────┘
```

**Components:**
- [ ] `IconSidebar.svelte` — fixed 48px left, icons with tooltip
- [ ] `ContextPanel.svelte` — 220px, shows org/project/service tree
- [ ] `TopBar.svelte` — breadcrumbs, org switcher, user menu

### Milestone 3.6 — Slide-over Panel System

A stack-based panel system. Multiple panels can stack (each slides 380px from right, offset by 40px).

```svelte
<!-- SlidePanel.svelte -->
<!-- Props: title, width?, zIndex computed from stack position -->
<!-- Panel stack managed by ui.store panelStack[] -->
```

**Panel types to implement:**
- `ServiceDetailPanel`
- `DeploymentPanel`
- `LogViewerPanel`
- `AddResourcePanel` (the "Add new resource" flow)
- `EnvEditorPanel`
- `DomainPanel`
- `MemberPanel`

**Store API:**
```ts
uiStore.pushPanel({ component: ServiceDetailPanel, props: { serviceId } })
uiStore.popPanel()
uiStore.clearPanels()
```

### Milestone 3.7 — Project Canvas (SvelteFlow)

Route: `/orgs/[orgId]/projects/[projectId]`

```
Canvas:
┌──────────────────────────────────────────────────────┐
│  [Toolbar: fit view | zoom | minimap toggle]         │
│                                                      │
│   ┌─────────┐     ┌──────────┐     ┌────────────┐  │
│   │ 🌐 nginx │────▶│ ⚙️ api   │────▶│ 🗄️ postgres│  │
│   │ domain  │     │ service  │     │  database  │  │
│   └─────────┘     └──────────┘     └────────────┘  │
│                         │                           │
│                    ┌────▼─────┐                     │
│                    │ 📦 redis │                     │
│                    │  cache   │                     │
│                    └──────────┘                     │
│                                                     │
│   [+ Add new resource]  (floating bottom-left)      │
└──────────────────────────────────────────────────────┘
```

**SvelteFlow Node Types:**

```
nodes/
├── ServiceNode.svelte     # shows name, status dot, replica count
├── DatabaseNode.svelte    # db icon, engine label
├── VolumeNode.svelte      # disk icon, size
├── NetworkNode.svelte     # cloud icon
├── DomainNode.svelte      # globe icon, hostname
└── AddResourceNode.svelte # dashed border, + icon (always present)
```

**Edge Types:**
- `NetworkEdge` (dashed blue) — service ↔ network
- `VolumeEdge` (dashed orange) — service ↔ volume
- `DomainEdge` (solid purple) — domain → service

**Node click** → push corresponding `SlidePanel`

**Tasks:**
- [ ] Load topology from API on mount, subscribe to MQTT topology updates
- [ ] Auto-layout using dagre (top-to-bottom)
- [ ] Node status reflects live service status (MQTT updates)
- [ ] "Add new resource" floating button opens `AddResourcePanel`

### Milestone 3.8 — Add Resource Panel

Triggered by "+ Add new resource" button.

```
Panel shows a grid of resource type cards:
┌─────────────────────────────────────────────┐
│ Add New Resource                        [×] │
├─────────────────────────────────────────────┤
│  ┌──────────┐ ┌──────────┐ ┌──────────┐   │
│  │🐙 Docker  │ │📦 Git    │ │🗄️ Database│   │
│  │ Image    │ │ Repo     │ │          │   │
│  └──────────┘ └──────────┘ └──────────┘   │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐   │
│  │📄 Compose │ │📁 Volume  │ │🔗 Network │   │
│  │          │ │          │ │          │   │
│  └──────────┘ └──────────┘ └──────────┘   │
│  ┌──────────┐                              │
│  │📋 Template│                              │
│  └──────────┘                              │
└─────────────────────────────────────────────┘
```

Selecting a type pushes a new panel on top with the creation form.

### Milestone 3.9 — Service Detail Panel

```
┌────────────────────────────────────────┐
│ ← api-service               [···]  [×] │
│ ● Running  •  3 replicas               │
├────────────────────────────────────────┤
│ [Overview] [Deployments] [Logs] [Env]  │
│ [Domains] [Volumes] [Replicas] [Settings] │
├────────────────────────────────────────┤
│ (tab content)                          │
│                                        │
│ [▶ Deploy] [⏹ Stop] [↺ Redeploy]      │
└────────────────────────────────────────┘
```

#### ★ Replicas Tab

Shows live container list populated from `container.store`, updated in real-time via MQTT without page reload.

```
Replicas (3 running)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

● replica-0   abc123def   node-1   Running   since 2h ago   [View Logs]
● replica-1   def456ghi   node-2   Running   since 2h ago   [View Logs]
● replica-2   ghi789jkl   node-1   Running   since 1h ago   [View Logs]

(auto-updates when Docker event worker detects changes)
```

Status dot colors:
- `running` → green `●`
- `pending` / `preparing` → yellow `◌` (pulsing)
- `failed` / `rejected` → red `●`
- `shutdown` / `complete` → gray `●`

"View Logs" for a replica → sets `logStore.activeReplicaId` and switches to Logs tab, auto-subscribing to that replica's MQTT log topic.

### Milestone 3.10 — Deployment Detail View

Inside the Deployments tab of the service panel.

```
Deployment #42  ● Running     2025-01-15 14:32:00
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

▼ ✅ Validate config               (0.2s)
▼ ✅ Clone repository              (3.1s)
  > Cloning into /data/proj1/api...
  > remote: Counting objects: 1,234
  > Checked out commit abc1234
▶ 🔄 Build image                  (running...)
▶ ⬜ Create swarm service
▶ ⬜ Health check

[Cancel Deployment]
```

**Tasks:**
- [ ] Collapsible steps (click to expand log lines)
- [ ] Real-time log streaming per step via MQTT
- [ ] Status icons: pending ⬜, running 🔄, success ✅, failed ❌
- [ ] Timestamp on each log line, level badge

### Milestone 3.11 — Log Viewer

```
┌─────────────────────────────────────────────────────────────────┐
│ Logs: api-service                                               │
│ Replica: [all ▼]  Level: [all ▼]  [📌 Jump to time] [⟳ Follow]│
├─────────────────────────────────────────────────────────────────┤
│ 14:32:01.123 INFO  Server listening on :8080                    │
│ 14:32:02.441 DEBUG Processing request GET /health               │
│ 14:32:05.001 WARN  High memory usage: 82%                       │
│ 14:32:09.887 ERROR Database connection timeout                  │
├─────────────────────────────────────────────────────────────────┤
│ [▼ Follow]  [↑ Jump to top]  [↓ Jump to bottom]               │
└─────────────────────────────────────────────────────────────────┘
```

**Tasks:**
- [ ] Virtual scroll list (only render visible log lines) — use `svelte-virtual-list`
- [ ] Level filter (debug/info/warn/error) → client-side filter on log buffer
- [ ] Replica selector → re-subscribe MQTT topic for chosen replica
- [ ] "Follow" mode: auto-scroll to bottom on new lines; pause on manual scroll up
- [ ] Timestamp navigation: date-time picker → request logs from that point
- [ ] Color coding per level (debug=gray, info=blue, warn=yellow, error=red)
- [ ] Ring buffer: keep max 10,000 lines in memory; virtualize rendering

### Milestone 3.12 — Env Manager

```
┌──────────────────────────────────────────┐
│ Environment Variables          [+ Add]  │
├──────────────────────────────────────────┤
│ KEY              │ VALUE      │ Secret   │
│ DATABASE_URL     │ postgres:// │ 🔒 hide │
│ PORT             │ 8080       │         │
│ NODE_ENV         │ production │         │
├──────────────────────────────────────────┤
│ [Bulk edit (raw)]       [Save & Redeploy]│
└──────────────────────────────────────────┘
```

**Tasks:**
- [ ] Inline edit cells, mark as secret (masked display)
- [ ] Bulk raw edit mode (textarea of KEY=VALUE)
- [ ] Save sends PUT to API then optionally triggers redeploy

### Milestone 3.13 — Routes

```
/                              → redirect to /setup or /login
/setup                         → Setup Wizard (multi-step)
/login
/register
/orgs/[orgId]/                 → org dashboard
/orgs/[orgId]/projects         → projects list
/orgs/[orgId]/projects/[projectId]  → Canvas view
/orgs/[orgId]/settings         → org settings
/orgs/[orgId]/members          → member management
/settings                      → user settings
```

---

## Phase 4 — Setup Wizard

### Milestone 4.1 — Backend

- [ ] State machine: `UNINITIALISED → DOCKER_CHECK → SWARM_INIT → ADMIN_CREATE → DONE`
- [ ] Persist wizard state to DB `system_config` table
- [ ] Block all non-setup routes until state = `DONE`
- [ ] API routes: `/setup/status`, `/setup/step/:step`

### Milestone 4.2 — Frontend Wizard

Multi-step flow, full-screen:

```
Step 1: Welcome + system requirements check (Docker ✅, Disk ✅, Ports ✅)
Step 2: Configure Docker / Swarm
Step 3: Configure domain & Traefik
Step 4: Create admin account
Step 5: Create first organization
Step 6: Done → redirect to canvas
```

---

## Phase 5 — RBAC & Security

### Milestone 5.1 — Roles & Permissions

```
owner  → all operations on org
admin  → all except delete org, manage billing
member → deploy, manage services, view logs
viewer → read-only: view services, logs (no env secrets)
```

**Implementation:**
- [ ] Permission enum + `can(user, action, resource)` function in Rust
- [ ] Middleware that checks permission on every route
- [ ] UI: hide/disable actions based on user role (from auth store)

### Milestone 5.2 — Secrets

- [ ] Env var values encrypted at rest using `AES-256-GCM`
- [ ] Encryption key from env `SECRET_KEY` (set during setup wizard)
- [ ] Secrets only decrypted when injecting into swarm service spec
- [ ] API never returns plaintext secret values after initial set

---

## Phase 6 — Git & Webhooks

### Milestone 6.1 — Git Operations

- [ ] Clone with progress → publish to MQTT deployment step log
- [ ] Support: HTTP clone (username/password), SSH clone (keypair per service)
- [ ] Store deploy key per service in DB (encrypted)
- [ ] Branch/tag picker in UI (service settings)
- [ ] `git pull --rebase` on redeploy (if already cloned)

### Milestone 6.2 — Webhooks

- [ ] POST `/webhooks/github/:service_id/:token`
- [ ] POST `/webhooks/gitlab/:service_id/:token`
- [ ] Verify HMAC signature
- [ ] Trigger deployment if push is to configured branch
- [ ] Show webhook URL in service settings UI

---

## Phase 7 — Templates & Docker Compose

### Milestone 7.1 — Templates

Pre-defined service templates (postgres, redis, nginx, node, etc.)

```json
{
  "id": "postgres-15",
  "name": "PostgreSQL 15",
  "type": "database",
  "image": "postgres:15-alpine",
  "env": [
    { "key": "POSTGRES_USER", "default": "postgres", "required": true },
    { "key": "POSTGRES_PASSWORD", "required": true, "secret": true },
    { "key": "POSTGRES_DB", "default": "app", "required": true }
  ],
  "volumes": [{ "mount": "/var/lib/postgresql/data" }],
  "ports": [5432]
}
```

- [ ] Template registry in DB
- [ ] GET `/templates` — list available templates
- [ ] Seed initial templates on setup

### Milestone 7.2 — Docker Compose Support

- [ ] Parse `docker-compose.yml` using `serde_yaml`
- [ ] Map each compose service to a swarm service
- [ ] Handle compose `depends_on`, `networks`, `volumes`
- [ ] Display each compose service as a separate node on canvas

---

## Phase 8 — Installer Script

### Milestone 8.1 — `install.sh`

Single-command install for users:

```bash
curl -sSL https://get.yourplatform.dev | bash
```

**Script steps:**
1. Detect OS (Ubuntu/Debian/CentOS)
2. Install Docker if not present
3. Init Docker Swarm if not in swarm mode
4. Pull platform images
5. Write `docker-compose.yml` to `/opt/platform/`
6. Start platform via `docker stack deploy`
7. Print: "Visit http://YOUR_IP:3000 to complete setup"

---

## Phase 9 — Observability & Production Hardening

### Milestone 9.1 — Health & Metrics

- [ ] GET `/health` → `{ status: ok, db: ok, docker: ok, mqtt: ok }`
- [ ] Prometheus metrics endpoint: `GET /metrics`
  - Deployment count by status
  - Active service count
  - MQTT message throughput
- [ ] Structured logging (tracing-subscriber, JSON format)

### Milestone 9.2 — Production Config

- [ ] TLS on the platform API itself (via Traefik)
- [ ] Rate limiting on API and webhook endpoints
- [ ] Docker secrets for DB password, secret key
- [ ] Backup script: pg_dump → S3 or local (configurable)
- [ ] Rolling update support: `--update-parallelism`, `--update-delay` on swarm services

### Milestone 9.3 — E2E Tests

- [ ] Playwright tests for wizard flow, canvas interactions, deployment panel
- [ ] Rust integration tests: deploy a real Nginx container via API
- [ ] CI pipeline: GitHub Actions (lint → test → build → docker push)

---

## Implementation Order for Coding Agent

Execute strictly in this order. Do not begin a milestone until the previous one compiles and passes tests.

```
0.1 → Repo scaffold
1.1 → DB schema + migrations
1.2 → Docker/Bollard engine
1.3 → Traefik label generator
1.4 → MQTT layer
2.1 → Auth API
2.2 → Setup wizard API
2.3 → Orgs API
2.4 → Projects API
2.5 → Services API
2.6 → Domains, Volumes, Networks API
2.7 → Deployment engine
2.8 → Logs API
2.9 → Topology API
2.10 → Git integration
2.11 → Middleware (RBAC, rate limit, tracing)
3.1 → Frontend scaffold + design system
3.2 → State stores
3.3 → MQTT event bus (frontend)
3.4 → API client (frontend)
3.5 → App shell + sidebar
3.6 → Slide-over panel system
3.7 → Project canvas (SvelteFlow)
3.8 → Add resource panel
3.9 → Service detail panel
3.10 → Deployment detail view
3.11 → Log viewer
3.12 → Env manager
3.13 → Routes + navigation
4.1 → Setup wizard backend
4.2 → Setup wizard frontend
5.1 → RBAC
5.2 → Secrets encryption
6.1 → Git operations
6.2 → Webhooks
7.1 → Templates
7.2 → Docker Compose support
8.1 → Installer script
9.1 → Health + metrics
9.2 → Production hardening
9.3 → E2E tests
```

---

## Key Dependencies

### Rust (Cargo.toml)
```toml
axum = "0.7"
tokio = { version = "1", features = ["full"] }
bollard = "0.17"
sqlx = { version = "0.7", features = ["postgres", "runtime-tokio", "uuid", "time"] }
rumqttc = "0.24"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
jsonwebtoken = "9"
argon2 = "0.5"
git2 = "0.18"
uuid = { version = "1", features = ["v4"] }
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["json"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
aes-gcm = "0.10"
```

### Frontend (package.json)
```json
{
  "dependencies": {
    "@xyflow/svelte": "^0.1",
    "shadcn-svelte": "latest",
    "mqtt": "^5",
    "mitt": "^3",
    "lucide-svelte": "latest",
    "svelte-virtual-list": "^3",
    "date-fns": "^3"
  }
}
```

---

## Phase 6 — Static Site Hosting

> **Goal:** Allow users to deploy zero-runtime static sites (HTML/CSS/JS/assets) with no per-site memory cost. Sites can be sourced from git (with a build step) or uploaded directly as a pre-built artifact.

### Design Principles

1. **One shared nginx container serves all static sites** — no per-site Swarm task, no idle process overhead (~12 MB total regardless of site count).
2. **Two deployment sources** — `git` (build on deploy) or `upload` (pre-built artifact via API/CLI). Both paths share the same publish + config-reload tail.
3. **`deploy.json` in repo/zip root** — optional per-project config that overrides build settings and drives runtime nginx behavior (redirects, rewrites, custom headers, SPA mode).
4. **Symlink-based atomic versioning** — each deploy writes to a versioned directory; a single `readlink`-swap makes it live. Rollback = swap symlink + reload.

---

### Milestone 6.1 — Database Schema

**Migration: `20250101000020_static_service.sql`**

```sql
-- Extend the service_type enum
ALTER TYPE service_type ADD VALUE IF NOT EXISTS 'static';

-- Per-service static config (defaults, overridable per-deploy via deploy.json)
CREATE TABLE static_site_configs (
    service_id       UUID PRIMARY KEY REFERENCES services(id) ON DELETE CASCADE,
    -- Deployment source: 'git' or 'upload'
    source           TEXT NOT NULL DEFAULT 'git',
    -- Build settings (used when source = 'git'; overridable by deploy.json)
    build_command    TEXT NOT NULL DEFAULT 'npm run build',
    output_dir       TEXT NOT NULL DEFAULT 'dist',
    node_version     TEXT NOT NULL DEFAULT '20',
    install_command  TEXT NOT NULL DEFAULT 'npm ci',
    -- Framework hint for build image selection and SPA default
    -- 'vite' | 'sveltekit-static' | 'nextjs-export' | 'hugo' | 'jekyll' | 'custom'
    framework        TEXT NOT NULL DEFAULT 'custom',
    -- Effective runtime config merged from last deploy.json (nullable = not yet deployed)
    deploy_config    JSONB,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Per-deployment artifact path (which versioned directory is live)
-- Stored in the existing deployments.deployed_image column repurposed as deployed_artifact.
-- No schema change needed; the column is already TEXT.
```

---

### Milestone 6.2 — `deploy.json` Config Format

The file lives at the root of the git repo or the root of the uploaded zip. It is optional; omitting it uses `static_site_configs` defaults and sensible nginx defaults.

```jsonc
// deploy.json
{
  "$schema": "https://shipyard.app/schema/deploy-v1.json",

  // ── Build (applies when source = 'git'; ignored for uploads) ───────────────
  "build": {
    "command": "npm run build",       // overrides static_site_configs.build_command
    "output": "dist",                 // overrides static_site_configs.output_dir
    "node_version": "20",             // overrides static_site_configs.node_version
    "install_command": "npm ci",      // overrides static_site_configs.install_command
    "env": {                          // injected into build container only (not runtime)
      "VITE_API_URL": "https://api.example.com"
    }
  },

  // ── SPA mode ───────────────────────────────────────────────────────────────
  // When true: unknown paths fall back to /index.html (React/Vue/Svelte router)
  "spa": true,

  // ── Custom error pages (path relative to output dir) ──────────────────────
  "error_pages": {
    "404": "404.html",
    "500": "50x.html"
  },

  // ── Redirects (evaluated before rewrites) ─────────────────────────────────
  // status defaults to 301 for permanent, 302 for temporary
  "redirects": [
    { "src": "/old-page",       "dest": "/new-page",   "status": 301 },
    { "src": "/docs/(.*)",      "dest": "/help/$1",    "status": 302 }
  ],

  // ── Rewrites (internal, URL stays the same in browser) ────────────────────
  // Processed after redirects; no status field (always 200 internal)
  "rewrites": [
    { "src": "/app/(.*)", "dest": "/index.html" }
  ],

  // ── Custom response headers ────────────────────────────────────────────────
  // src is a glob; applied to all matching paths
  "headers": [
    {
      "src": "/assets/(.*)",
      "values": {
        "Cache-Control": "public, max-age=31536000, immutable"
      }
    },
    {
      "src": "/(.*)\\.html",
      "values": {
        "Cache-Control": "no-cache, must-revalidate",
        "X-Frame-Options": "SAMEORIGIN"
      }
    }
  ]
}
```

**Parsing rules:**
- `deploy.json` is parsed at the start of Step 3 (`publish_files`) in the static pipeline.
- Build keys (`build.*`) override `static_site_configs` for this deployment only — they are not written back to the table.
- Runtime keys (`spa`, `error_pages`, `redirects`, `rewrites`, `headers`) are stored in `static_site_configs.deploy_config` JSONB and used to regenerate `site.conf`.
- Unknown top-level keys are ignored (forward-compatible).
- Invalid JSON → deployment fails at Step 3 with a clear error message.
- The parsed runtime config is also stored on the deployment record (`deployed_image` column stores the artifact path; add `deploy_config_snapshot` JSONB to `deployments` table if rollback needs to restore config too).

---

### Milestone 6.3 — Deployment Sources

Static services have two `source` modes. Both paths share Steps 4–7 (publish, configure, reload, finalize).

#### Source A: Git + Build

```
Step 0  validate_static      — assert git_repo_url set; build_command non-empty
Step 1  clone_or_pull        — git fetch into data_dir/repos/<service_id>/
                               (reuses existing GitService; incremental pull on re-deploy)
Step 2  run_build            — ephemeral build container (exits after build)
Step 3  parse_deploy_json    — read deploy.json from repo root; merge with DB config
Step 4  publish_files        — copy output_dir → data_dir/static/<service_id>/<deploy_id>/
Step 5  write_nginx_conf     — render site.conf from merged config
Step 6  atomic_swap          — update symlink: public → <deploy_id>/; reload nginx
Step 7  finalize             — set service status 'running', record artifact path
```

**Ephemeral build container (Step 2):**

The build does NOT run on the host. We spawn a one-shot Docker container:

```rust
// Pseudo-spec for the build container
ServiceSpec {
    name: format!("shipyard-build-{deployment_id}"),
    image: build_image_for_framework(framework, node_version),
    // e.g. "node:20-alpine" for vite/sveltekit/nextjs
    // e.g. "klakegg/hugo:0.120-alpine" for hugo
    // e.g. "ruby:3.3-alpine" for jekyll
    command: vec![
        "sh", "-c",
        &format!("{install_command} && {build_command}")
    ],
    mounts: vec![
        // repo dir mounted read-only
        MountSpec { source: repo_path, target: "/app", read_only: true, .. },
        // output collected via a shared tmpfs or second bind-mount
        MountSpec { source: build_output_path, target: "/output", .. },
    ],
    working_dir: Some("/app".into()),
    env_vars: build_env_vars,  // from deploy.json build.env
    restart_policy: RestartPolicy::None,  // run once then exit
    replicas: 1,
}
// Wait for task to reach Terminated/Complete state
// On exit code != 0: fail deployment with build log output
// Stream stdout/stderr to MQTT (same as existing deployment log streaming)
```

After the container exits, copy `/output/<output_dir>` to the versioned publish directory. The build container is then removed (`docker service rm`).

#### Source B: Direct Upload

No git clone, no build step. User uploads a pre-built artifact.

```
Step 0  validate_upload      — assert content-type is zip or tar.gz; size ≤ limit
Step 1  stream_to_disk       — write multipart body directly to
                               data_dir/uploads/<service_id>/<deploy_id>.zip
                               (streaming — never fully buffered in RAM)
Step 2  extract              — unzip into data_dir/tmp/<deploy_id>/
Step 3  parse_deploy_json    — read deploy.json from archive root if present
Step 4  publish_files        — move extracted dir → data_dir/static/<service_id>/<deploy_id>/
Step 5  write_nginx_conf     — same as git path
Step 6  atomic_swap          — same as git path
Step 7  finalize             — same as git path
```

**Upload API endpoint:**

```
POST /projects/:project_id/services/:service_id/static/upload
Content-Type: multipart/form-data

Fields:
  artifact   — the zip or tar.gz file (required)
  message    — optional deployment message shown in history
```

Response: same envelope as `trigger_deploy` — returns `{ deployment_id }` immediately; status tracked via MQTT.

**Streaming upload — memory efficiency:**

Axum's `Multipart` extractor yields `Field` chunks lazily. Write each chunk directly to a temp file:

```rust
async fn upload_static_artifact(
    ...
    mut multipart: Multipart,
) -> Result<...> {
    while let Some(field) = multipart.next_field().await? {
        if field.name() == Some("artifact") {
            let dest = format!("{uploads_dir}/{deploy_id}.zip");
            let mut file = tokio::fs::File::create(&dest).await?;
            // Stream chunks — max RAM usage = one chunk (~64 KB), not the whole file
            while let Some(chunk) = field.chunk().await? {
                file.write_all(&chunk).await?;
            }
        }
    }
    // spawn deployment task, return deploy_id
}
```

This means a 500 MB upload uses ~64 KB of RAM at any point, not 500 MB.

**Size limit:** configurable in `AppConfig.static_server.max_upload_mb` (default: 256 MB). Enforced with a `ContentLengthLimit` middleware layer.

---

### Milestone 6.4 — Atomic Versioning & Rollback

```
data_dir/static/<service_id>/
├── current -> ./<deploy_id_3>/          ← symlink, updated atomically
├── <deploy_id_1>/                        ← previous version (kept for rollback)
│   ├── site.conf                         ← nginx config snapshot
│   └── public/                           ← built files
├── <deploy_id_2>/
│   ├── site.conf
│   └── public/
└── <deploy_id_3>/                        ← current live version
    ├── site.conf
    └── public/
```

**nginx serves from symlink:**
```nginx
root /var/www/static/<service_id>/current/public;
```

**Atomic swap (Step 6):**
```rust
// ln -sfn <deploy_id>/ current   — atomic on Linux (rename syscall)
std::os::unix::fs::symlink(&new_version_path, &tmp_link)?;
std::fs::rename(&tmp_link, &current_link)?;  // atomic rename
// Then reload nginx
```

**Rollback:** Set the symlink to a previous `<deploy_id>/` directory + reload nginx. No file copy needed. Executes in <500ms.

**Retention policy:** Keep the last N versions (configurable, default 5). The finalize step deletes the oldest beyond the limit. Tracked in `deployments` table — `deployed_image` stores the path.

---

### Milestone 6.5 — Shared nginx Config Generation

**Master nginx.conf** (written once at `shipyard-static` startup via init container or volume):
```nginx
user nginx;
worker_processes auto;
pid /tmp/nginx.pid;

events {
    worker_connections 1024;
    use epoll;
    multi_accept on;
}

http {
    include       /etc/nginx/mime.types;
    default_type  application/octet-stream;

    sendfile        on;
    tcp_nopush      on;
    tcp_nodelay     on;
    keepalive_timeout 65;

    gzip on;
    gzip_vary on;
    gzip_proxied any;
    gzip_comp_level 6;
    gzip_types text/plain text/css application/json application/javascript
               text/xml application/xml image/svg+xml font/woff2;

    # One include per deployed static site — nginx hot-reloads when signalled
    include /var/www/static/*/site.conf;
}
```

**Generated `site.conf`** (one per service, written at deploy time from `deploy.json` + domain config):
```nginx
server {
    listen 80;
    server_name {domains};           # space-separated; populated from service domains table

    root /var/www/static/{service_id}/current/public;
    index index.html index.htm;

    # ── Error pages ──────────────────────────────────────────────────────────
    error_page 404 /404.html;
    error_page 500 502 503 504 /50x.html;

    # ── Redirects (from deploy.json redirects[]) ─────────────────────────────
    # Each redirect entry becomes a location block with return directive
    location = /old-page {
        return 301 /new-page;
    }

    # ── SPA fallback / rewrites (from deploy.json) ───────────────────────────
    location / {
        try_files $uri $uri/ /index.html;    # omitted when spa=false
    }

    # ── Asset caching ─────────────────────────────────────────────────────────
    location ~* \.(js|css|woff2|woff|ttf|png|jpg|jpeg|gif|ico|svg|webp)$ {
        expires 1y;
        add_header Cache-Control "public, immutable";
        access_log off;
    }

    # ── Custom headers (from deploy.json headers[]) ───────────────────────────
    # Injected per-location or globally depending on src glob
    add_header X-Frame-Options "SAMEORIGIN" always;
    add_header X-Content-Type-Options "nosniff" always;
    add_header Referrer-Policy "strict-origin-when-cross-origin" always;
}
```

**Config generation is done in Rust** — a `render_nginx_site_conf(config: &SiteConfig) -> String` function in the engine crate. No templating library needed; build the string programmatically (the output is small and deterministic).

---

### Milestone 6.6 — `shipyard-static` Infrastructure

**Added to `docker-compose.dev.yml`:**
```yaml
shipyard-static:
  image: nginx:1.27-alpine
  restart: unless-stopped
  volumes:
    # Master config (never changes after initial write)
    - ./static-server/nginx.conf:/etc/nginx/nginx.conf:ro
    # Sites bind-mount: both per-site configs and files are here
    - ${SHIPYARD_DATA_DIR:-/opt/shipyard/data}/static:/var/www/static
  labels:
    - "traefik.enable=true"
    # Catch-all router — lowest priority so app routes win
    - "traefik.http.routers.shipyard-static.rule=HostRegexp(`{host:.+}`)"
    - "traefik.http.routers.shipyard-static.priority=1"
    - "traefik.http.routers.shipyard-static.entrypoints=web,websecure"
    - "traefik.http.services.shipyard-static.loadbalancer.server.port=80"
  networks:
    - platform_proxy
  deploy:
    resources:
      limits:
        memory: 64M   # nginx + all file metadata; actual file data is on disk
        cpus: "0.25"
  healthcheck:
    test: ["CMD", "nginx", "-t"]
    interval: 30s
    timeout: 5s
    retries: 3
```

**Memory breakdown:**
| Component | RSS |
|---|---|
| nginx master process | ~3 MB |
| nginx worker (auto = 1 on a single-core host) | ~5–8 MB |
| Per additional static site | 0 MB (files on disk, config in worker cache) |
| **Total for 100 sites** | **~12 MB** |

Compare to container-per-site: 100 × ~8 MB = **800 MB**.

**Traefik routing for TLS / custom domains:**

For TLS, each custom domain still needs an explicit Traefik router so the cert resolver can issue per-domain certificates. When a user adds a domain to a static service, write a Traefik dynamic config file:

```yaml
# data_dir/traefik-dynamic/static-<service_id>-<domain_hash>.yml
http:
  routers:
    static-{service_id}-mysite:
      rule: "Host(`mysite.example.com`)"
      entryPoints: [websecure]
      service: shipyard-static
      tls:
        certResolver: letsencrypt
```

nginx handles the actual host-based file serving. Traefik only handles TLS + routing to the shared container.

---

### Milestone 6.7 — AppConfig Changes

```rust
// In AppConfig
pub static_server: StaticServerConfig,

#[derive(Debug, Clone, Deserialize)]
pub struct StaticServerConfig {
    /// Name of the shared nginx Swarm service / container.
    #[serde(default = "default_static_service_name")]
    pub service_name: String,

    /// Absolute path on the host where site dirs are stored.
    /// Must match the bind-mount in the static server compose service.
    /// Defaults to {data_dir}/static
    pub sites_dir: Option<String>,

    /// Max upload size in MB for direct-upload deployments.
    #[serde(default = "default_max_upload_mb")]
    pub max_upload_mb: u64,

    /// Number of past deployment versions to retain per site.
    #[serde(default = "default_retention_versions")]
    pub retention_versions: usize,
}

fn default_static_service_name() -> String { "shipyard-static".to_string() }
fn default_max_upload_mb() -> u64 { 256 }
fn default_retention_versions() -> usize { 5 }
```

---

### Milestone 6.8 — API Endpoints

```
# Static site config CRUD
GET    /projects/:pid/services/:sid/static/config
PUT    /projects/:pid/services/:sid/static/config

# Direct-upload deployment (returns deployment_id immediately)
POST   /projects/:pid/services/:sid/static/upload
  Content-Type: multipart/form-data
  Body: artifact (zip|tar.gz), message? (string)
  Response: { deployment_id: UUID }

# Reuse existing deployment endpoints for status/logs/rollback
POST   /projects/:pid/services/:sid/deploy          (git-based trigger)
GET    /projects/:pid/services/:sid/deployments
POST   /projects/:pid/services/:sid/deployments/:did/rollback
```

**RBAC:** upload endpoint requires `service:write` (same as `trigger_deploy`).

---

### Milestone 6.9 — Frontend Changes

**Service creation dialog — new "Static Site" type:**
- Tabs: `Git` / `Upload`
- Git tab: repo URL + branch + framework preset selector
- Upload tab: drag-and-drop or file picker; shows upload progress bar
- Framework presets auto-fill build command + output dir:

| Preset | Build command | Output dir |
|---|---|---|
| Vite | `npm run build` | `dist` |
| SvelteKit (static) | `npm run build` | `build` |
| Next.js (export) | `npm run build && npm run export` | `out` |
| Hugo | `hugo --minify` | `public` |
| Jekyll | `bundle exec jekyll build` | `_site` |
| Custom | (editable) | (editable) |

**Service detail page (static type):**
- No replicas/ports/resource-limits controls
- "Preview" button → opens the primary domain
- "Deploy" button → triggers git rebuild OR opens upload dialog
- Deployment history shows build steps differently from container services
- `deploy.json` viewer — shows parsed config in effect for the live deployment
- Rollback panel — shows previous N versions with timestamp + commit hash

**Deployment step names for static pipeline:**

| Step | Label shown in UI |
|---|---|
| 0 | Validate config |
| 1 | Clone / pull repository |
| 2 | Build (build container) |
| 3 | Read deploy.json |
| 4 | Publish files |
| 5 | Write nginx config |
| 6 | Go live |
| 7 | Finalize |

For upload source, steps 1 and 2 are replaced by "Stream upload" and "Extract archive".

---

### File System Layout (Static Sites)

```
data_dir/
├── repos/
│   └── <service_id>/                   ← git clone (incremental pull on re-deploy)
├── static/
│   ├── <service_id>/
│   │   ├── current -> ./<deploy_id_3>/ ← symlink (atomic swap on each deploy)
│   │   ├── <deploy_id_1>/              ← retained for rollback
│   │   │   ├── site.conf               ← nginx config snapshot for this version
│   │   │   └── public/                 ← built/uploaded files
│   │   ├── <deploy_id_2>/
│   │   └── <deploy_id_3>/
│   └── <service_id_2>/
│       └── ...
├── uploads/
│   └── <service_id>/
│       └── <deploy_id>.zip             ← raw upload (deleted after extract)
└── tmp/
    └── <deploy_id>/                    ← extraction scratch space (deleted after Step 4)
```

---

### Coding Agent Notes (Static Sites)

1. **Never load the entire artifact into RAM.** Use streaming multipart for upload, `tokio::fs::copy` for file publishing (kernel-managed, not userspace buffers).
2. **Symlink swap must be atomic.** Use `rename(2)` via `std::fs::rename` — not `remove + symlink`. On Linux this is guaranteed atomic. Write the new symlink to a temp name first, then rename over `current`.
3. **nginx reload is not a restart.** `nginx -s reload` sends SIGHUP, which causes workers to finish in-flight requests before swapping config. Zero downtime.
4. **deploy.json is advisory, not required.** A site with no `deploy.json` must deploy successfully with defaults. Never fail a deployment solely because `deploy.json` is absent.
5. **Build containers must not persist.** After Step 2 exits, immediately `docker service rm` the build service. Lingering build containers are a memory + disk leak.
6. **Security: validate paths in uploaded archives.** Zip-slip attack — reject any entry whose resolved path escapes the target directory. Use `std::path::Path::components()` to check for `..` segments before extracting.
7. **The `deploy.json` `build.env` values are injected into the build container only**, not into the published files or nginx config. Do not log or persist them.

---

### Milestone 6.10 — Topology Canvas Integration

Static sites create a specific representation challenge: they have no running container (no `containers` table rows), they are deployed across many projects and organizations, and they all converge on one shared infrastructure node (`shipyard-static`). The solution is two distinct topology views with different scopes.

---

#### 6.10.1 — Within-Project Canvas: `StaticSiteNode`

Static services appear in the project topology as a new node type — `static_site` — replacing `service` for `service_type = 'static'`. This node is visually and semantically different from a container service node.

**Visual differences from `ServiceNode`:**

| Field | ServiceNode | StaticSiteNode |
|---|---|---|
| Icon | `Container` | `Globe` |
| Status label | Running / Stopped / Failed | Live / Building / Failed / Not deployed |
| Status source | containers table | last deployment status |
| Replicas badge | ✓ shown | ✗ absent |
| Ports | ✓ shown | ✗ absent (domains instead) |
| New: Framework badge | ✗ | ✓ shown (Vite, Hugo, …) |
| New: Last deploy time | ✗ | ✓ shown (relative, e.g. "3h ago") |
| Right handle (source) | ✓ → containers | ✗ (no containers) |

**Node data payload** (added to topology backend query for `static` services):

```json
{
  "name": "my-site",
  "slug": "my-site",
  "type": "static",
  "status": "running",
  "framework": "vite",
  "source": "git",
  "last_deploy_status": "success",
  "last_deploy_at": "2025-07-01T12:00:00Z",
  "deployed_artifact": "/opt/shipyard/data/static/<svc_id>/<deploy_id>"
}
```

**Backend topology enrichment** — update the `ServiceRow` struct and query to LEFT JOIN static data for `static` services:

```sql
SELECT
    s.id, s.name, s.slug, s.status, s.replicas,
    s.type, s.ports, s.service_parent_id,
    ssc.framework, ssc.source,
    d.status  AS last_deploy_status,
    d.finished_at AS last_deploy_at,
    d.deployed_image AS deployed_artifact
FROM services s
LEFT JOIN static_site_configs ssc ON ssc.service_id = s.id
LEFT JOIN LATERAL (
    SELECT status, finished_at, deployed_image
    FROM deployments
    WHERE service_id = s.id
    ORDER BY created_at DESC
    LIMIT 1
) d ON true
WHERE s.project_id = $1
```

The node_type emitted is `"static_site"` when `s.type = 'static'`, and `"service"` otherwise.

**Auto-layout:** Static service slots have no container children, so `rootSlotHeight()` returns `max(1, domainCount) × Y_STEP + SVC_GAP`. The CONTAINER_X column stays empty for static services. No algorithm change needed — the existing pass-3 skips them naturally because `containersBySvc[staticSvcId]` is always empty.

**`nodeTypes` registration** in the canvas page:

```typescript
const nodeTypes: NodeTypes = {
  service:     ServiceNode    as any,
  static_site: StaticSiteNode as any,   // NEW
  network:     NetworkNode    as any,
  volume:      VolumeNode     as any,
  domain:      DomainNode     as any,
  container:   ContainerNode  as any,
};
```

**`handleNodeClick` for `static_site`** → pushes a `StaticSiteDetailPanel` (the static-specific service panel) instead of `ServiceDetailPanel`.

**MQTT live updates:** When a static deployment completes, the `deployment.success` / `deployment.failed` MQTT event triggers `topologyStore.refreshNode('svc_<id>', { data: { status: 'running', last_deploy_status: 'success', last_deploy_at: ... } })`. The status dot turns green in real time without a full topology reload.

---

#### 6.10.2 — Cross-Org / Cross-Project: Static Hub Topology

Static services from every project in every organization all route through the same `shipyard-static` nginx instance. Showing this dependency in the per-project canvas would be noisy and misleading (it's infrastructure, not a user-created resource). Instead, a dedicated **Static Hub** view exposes this relationship.

**Scope hierarchy:**

```
Platform Admin
└── Global Static Hub (/admin/topology/static)
    └── shows ALL static sites across ALL orgs → shipyard-static

Org Member / Admin
└── Org Static Hub (/orgs/:slug/topology/static)
    └── shows all static sites in this org, grouped by project → shipyard-static
```

**Hub topology shape (star / spoke model):**

```
[project-A-group]
  ├── [static-site-1]  ──┐
  └── [static-site-2]  ──┤
                         ├──→  [shipyard-static nginx]  ──→  [Internet / Traefik]
[project-B-group]        │
  └── [static-site-3]  ──┘
```

Node types used in the hub view:

| Node type | Description |
|---|---|
| `static_site` | Reuse same component from project canvas |
| `project_group` | New group node — visual container for sites in one project |
| `infra_static_server` | New node — represents the shared `shipyard-static` container |
| `domain` | Reuse existing domain node |

**New backend endpoint:**

```
GET /orgs/:org_id/topology/static
```

Returns:
```json
{
  "nodes": [
    { "id": "infra_static", "type": "infra_static_server", "data": { "service_name": "shipyard-static", "site_count": 12 } },
    { "id": "proj_<pid>",   "type": "project_group",       "data": { "name": "my-project", "slug": "my-project" } },
    { "id": "svc_<sid>",    "type": "static_site",         "data": { ... } },
    { "id": "dom_<did>",    "type": "domain",              "data": { ... } }
  ],
  "edges": [
    { "id": "e_svc_infra",  "source": "svc_<sid>",  "target": "infra_static", "type": "serves" },
    { "id": "e_dom_svc",    "source": "dom_<did>",  "target": "svc_<sid>",    "type": "domain"  },
    { "id": "e_svc_proj",   "source": "svc_<sid>",  "target": "proj_<pid>",   "type": "belongs" }
  ]
}
```

The SQL query for this endpoint:

```sql
SELECT
    s.id           AS service_id,
    s.name         AS service_name,
    s.status       AS service_status,
    s.project_id,
    p.name         AS project_name,
    p.slug         AS project_slug,
    ssc.framework,
    d.status       AS last_deploy_status,
    d.finished_at  AS last_deploy_at,
    dom.id         AS domain_id,
    dom.hostname,
    dom.tls_enabled
FROM services s
JOIN projects p        ON p.id = s.project_id
JOIN static_site_configs ssc ON ssc.service_id = s.id
LEFT JOIN LATERAL (
    SELECT status, finished_at FROM deployments
    WHERE service_id = s.id ORDER BY created_at DESC LIMIT 1
) d ON true
LEFT JOIN domains dom  ON dom.service_id = s.id
WHERE p.org_id = $1
  AND s.type   = 'static'
ORDER BY p.name, s.name
```

**Auto-layout for hub view:**

```
DOMAIN_X = -400  ← domain nodes (leftmost)
SITE_X   = -200  ← static site nodes
HUB_X    =  200  ← infra_static_server (center-right, all edges converge here)

Projects are stacked vertically:
  Project A group occupies rows 0..N_A
  Project B group starts at row N_A + 1
  etc.
```

`project_group` nodes use SvelteFlow's [sub-flow / group node](https://reactflow.dev/docs/concepts/sub-flows/) feature — they render as a labelled background rectangle that visually groups sites belonging to the same project. The group node's `extent` is set to `'parent'` for its children, keeping them inside the rectangle.

**`InfraStaticServerNode` component:**

```svelte
<!-- Shows: "shipyard-static", memory usage, total sites count, status indicator -->
<div class="infra-node">
  <Server size={14} />
  <span>shipyard-static</span>
  <span class="site-count">{data.site_count} sites</span>
  <span class="mem-badge">{data.memory_mb ?? '~12'} MB</span>
</div>
```

Styled distinctly (darker background, dashed border) to signal it is infrastructure rather than a user-created resource.

**`ProjectGroupNode` component:**

Uses SvelteFlow's built-in `group` node type (transparent fill + labelled border). No custom component needed — set `type: 'group'` and `style: 'background: rgba(99,102,241,0.04); border: 1px dashed ...'`.

---

#### 6.10.3 — Status Model for Static Services

Static services never have container rows, so `services.status` has a new semantic:

| `services.status` value | Meaning for static |
|---|---|
| `stopped` | Never deployed / explicitly taken offline |
| `running` | Files live and being served (symlink is valid) |
| `pending` | Deployment is currently running |
| `failed` | Last deployment failed; no live files |

The deployment engine's `step_finalize` sets `status = 'running'` on success and `status = 'failed'` on failure, exactly as for container services. No code change needed.

The `StaticSiteNode` maps status → display label:

```typescript
const STATIC_STATUS: Record<string, { label: string; color: 'green' | 'grey' | 'red' | 'yellow' }> = {
  running:  { label: 'Live',         color: 'green'  },
  pending:  { label: 'Building…',    color: 'yellow' },
  failed:   { label: 'Build failed', color: 'red'    },
  stopped:  { label: 'Not deployed', color: 'grey'   },
};
```

---

#### 6.10.4 — Summary of New Frontend Files

| File | Description |
|---|---|
| `src/lib/flows/StaticSiteNode.svelte` | New canvas node for `static_site` type |
| `src/lib/flows/InfraStaticServerNode.svelte` | Hub view — shared nginx node |
| `src/lib/flows/ProjectGroupNode.svelte` | Hub view — group container for one project |
| `src/lib/panels/StaticSiteDetailPanel.svelte` | Side panel for static service (replaces ServiceDetailPanel for `type=static`) |
| `src/routes/orgs/[orgSlug]/topology/static/+page.svelte` | Org-level Static Hub canvas page |
| `src/lib/stores/staticHubStore.ts` | Topology store variant scoped to hub view |

**`topology.store.ts` change:** Add `'static_site'` to the column layout logic. Static nodes sit in the `ROOT_SVC_X` column. The layout already handles them correctly (container column is simply empty), but add an explicit guard:

```typescript
if (n.type === 'static_site') {
  // place in service column; never extends into container or child-service columns
  rootSvcIds.push(n.id);
}
```

**Routing:** Add "Static Sites" link in the org sidebar that navigates to `/orgs/:slug/topology/static`.

---

#### 6.10.5 — Coding Agent Notes (Topology)

1. **Never show `shipyard-static` in the per-project canvas.** It is platform infrastructure, not a user resource. The per-project view shows static services exactly like regular services (just a different node component) — the shared nginx is abstracted away.
2. **`static_site` nodes have no right-side container handle.** Remove the `<Handle type="source" position={Position.Right} />` from `StaticSiteNode` — there are no replica containers to connect to.
3. **SvelteFlow group nodes require children to set `parentId`** on the flow node object. Set `parentId: 'proj_<project_id>'` for each static site node in the hub view so SvelteFlow renders them inside the group rectangle.
4. **The hub topology is read-only** — no drag-to-connect, no "Add Resource" button. Positions can still be saved per user for layout preference (use a separate `hub_positions` key in localStorage, not the project's `node_positions` DB column).
5. **RBAC for hub view**: `GET /orgs/:org_id/topology/static` requires `project:view` permission on at least one project in the org. Filter the result to only include projects the requesting user has access to — never leak static site names from projects outside the user's permissions.
6. **Static services produce zero MQTT container events.** The `deployment.success` MQTT event is the sole trigger for refreshing a static site node's status. Subscribe to `orgs/:org_id/projects/+/services/+/deployments/+/status` in the topology subscription and call `topologyStore.refreshNode()` on receipt.

---

## File System Layout (Runtime)

```
/opt/platform/data/
├── <org_id>/
│   └── <project_id>/
│       ├── .project.json         # project metadata cache
│       └── <service_id>/
│           ├── .service.json     # service metadata cache
│           ├── repo/             # git clone target (if type=git)
│           ├── build/            # build context
│           └── compose/          # compose files (if type=docker_compose)
```

---

## Notes for Coding Agent

1. **Never store Docker credentials, Git tokens, or env secrets in plaintext.** Always encrypt before DB insert.
2. **MQTT events are append-only.** Don't use MQTT for state — use it only for real-time deltas. Source of truth is always Postgres.
3. **SvelteFlow node positions** should be persisted to DB per project (add `node_positions` JSONB column to `projects`).
4. **Deployment logs** should use `deployment_logs` DB table as the durable store, and MQTT as the live stream. On reconnect, UI fetches DB history then subscribes MQTT for new lines.
5. **The slide panel stack** in `ui.store` should use `$effect` / reactive pattern to animate each panel's transform offset based on its index in the stack.
6. **Traefik must be in the same Docker network** as all deployed services. Name this network `platform_proxy` and attach all service swarm specs to it automatically.
7. **All API responses** use a consistent envelope: `{ data: T, error: null }` or `{ data: null, error: { code, message } }`.
8. **Log ring buffer** in frontend: cap at 10,000 entries. When full, drop from the top (oldest). Virtual list handles rendering.
```