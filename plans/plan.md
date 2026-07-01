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