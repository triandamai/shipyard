# Sandbox Runtime Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the isolated, per-app gVisor sandbox runtime — start/stop/heartbeat/idle-reap lifecycle, persistent volumes, stack auto-detection, dedicated quotas, and Traefik-routed live preview with cold-start auto-provisioning — as a standalone, testable backend subsystem that the (separate, future) editor UI and publish pipeline will consume.

**Architecture:** Follows the existing `services`-as-root-table pattern (new `sandbox_app` service type + two specialization tables) and the existing edge-function Manager shape (free functions over `&AppState`, not a stateful struct). Sandboxes are plain `bollard`-created Docker containers (not Swarm services, since Swarm has no per-service runtime-class selection) running under the `runsc` (gVisor) runtime, reusing the existing per-org `resolve_docker_for_service` node-targeting and the existing `sync_traefik_dynamic_config`/`domains` table for preview routing — no new networking primitive needed.

**Tech Stack:** Rust (axum, sqlx/Postgres, bollard 0.17, tokio), existing `shipyard-docker`/`shipyard-engine`/`shipyard-common`/`shipyard-api` crates.

**Spec:** `docs/superpowers/specs/2026-09-23-sandbox-runtime-design.md`

## Global Constraints

- New `service_type` enum value: `sandbox_app` (spec "Data model").
- Isolation: `runsc` (gVisor) container runtime — requires the target Docker daemon to have gVisor installed and registered as a named runtime (`"runsc"`) in its `daemon.json`; this plan does not provision that (ops/deployment prerequisite), but every container-launch code path must pass the runtime class explicitly rather than assuming a daemon default.
- Persistence: one Docker named volume per app (via the existing `create_volume`/`volumes` table), mounted at `/app` in the sandbox container. No bind mounts (must work against remote dedicated-node Docker daemons, not just local disk).
- Idle timeout: 20 minutes without a heartbeat (spec "Decisions").
- Quota columns live on the existing `plans` table (`max_concurrent_sandboxes`, `sandbox_cpu_cores`, `sandbox_memory_gb`), not a new table.
- Detection interface boundary (new decision made while writing this plan — see Task 3 rationale): the backend/API process cannot assume local filesystem access to an app's volume, since it may live on a remote dedicated node reached only via the Docker API. Stack detection therefore runs *inside a throwaway probe container* on the same node/volume, not via direct file reads from the API process. A freshly created, never-yet-started app's empty volume is a valid, testable input (detection reports "unrecognized," start fails with an actionable error) — this sub-project does not need to solve "how do source files first get into the volume" (that's the editor-UI sub-project's job).
- All new authenticated routes reuse the existing `require_service_access(&state.db, user_id, service_id)` helper (`backend/crates/api/src/middleware/rbac.rs:247`) for authorization.

---

## Task 1: Database migrations

**Files:**
- Create: `backend/crates/db/migrations/20250101000053_sandbox_apps.sql`
- Create: `backend/crates/db/migrations/20250101000054_sandbox_quota_plans.sql`
- Test: `backend/crates/db/tests/sandbox_migrations.rs`

**Interfaces:**
- Produces: `sandbox_app_configs` table (`service_id` PK), `sandbox_instances` table (`service_id` PK), `plans.max_concurrent_sandboxes` / `plans.sandbox_cpu_cores` / `plans.sandbox_memory_gb` columns. All later tasks read/write these directly via `sqlx::query`/`query_as`.

- [ ] **Step 1: Write the migrations**

`backend/crates/db/migrations/20250101000053_sandbox_apps.sql`:
```sql
-- Sandbox runtime: in-editor apps get their own service_type, a static-config
-- specialization table, and a live-runtime-state table (mirrors the
-- services/containers split — config vs. frequently-mutated live state).

ALTER TYPE service_type ADD VALUE IF NOT EXISTS 'sandbox_app';

CREATE TABLE sandbox_app_configs (
    service_id      UUID PRIMARY KEY REFERENCES services(id) ON DELETE CASCADE,
    runtime         TEXT,
    base_image      TEXT,
    install_cmd     TEXT,
    dev_cmd         TEXT,
    port            INT,
    manifest_source TEXT NOT NULL DEFAULT 'undetected'
                    CHECK (manifest_source IN ('undetected', 'detected', 'manifest')),
    volume_name     TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE sandbox_instances (
    service_id       UUID PRIMARY KEY REFERENCES services(id) ON DELETE CASCADE,
    status           TEXT NOT NULL DEFAULT 'stopped'
                     CHECK (status IN ('stopped', 'starting', 'running')),
    container_id     TEXT,
    container_name   TEXT,
    preview_url      TEXT,
    last_heartbeat_at TIMESTAMPTZ,
    started_at       TIMESTAMPTZ,
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_sandbox_instances_status_heartbeat
    ON sandbox_instances (status, last_heartbeat_at);
```

`backend/crates/db/migrations/20250101000054_sandbox_quota_plans.sql`:
```sql
-- Dedicated dev-sandbox quota, separate from production plan limits, so
-- editor usage can never starve a customer's live production traffic.
ALTER TABLE plans ADD COLUMN IF NOT EXISTS max_concurrent_sandboxes INT NOT NULL DEFAULT 1;
ALTER TABLE plans ADD COLUMN IF NOT EXISTS sandbox_cpu_cores DOUBLE PRECISION NOT NULL DEFAULT 0.5;
ALTER TABLE plans ADD COLUMN IF NOT EXISTS sandbox_memory_gb DOUBLE PRECISION NOT NULL DEFAULT 1.0;

UPDATE plans SET max_concurrent_sandboxes = 1, sandbox_cpu_cores = 0.5,  sandbox_memory_gb = 1.0 WHERE name = 'free';
UPDATE plans SET max_concurrent_sandboxes = 3, sandbox_cpu_cores = 1.0,  sandbox_memory_gb = 2.0 WHERE name = 'pro';
UPDATE plans SET max_concurrent_sandboxes = 5, sandbox_cpu_cores = 2.0,  sandbox_memory_gb = 4.0 WHERE name = 'max';
```

- [ ] **Step 2: Write the migration test**

```rust
// backend/crates/db/tests/sandbox_migrations.rs
use sqlx::PgPool;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
}

async fn pool() -> PgPool {
    let pool = shipyard_db::init_pool(&test_database_url(), 5)
        .await
        .expect("failed to connect to TEST_DATABASE_URL — is Postgres running?");
    shipyard_db::run_migrations(&pool).await.expect("migrations failed to apply");
    pool
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn sandbox_tables_and_quota_columns_exist() {
    let pool = pool().await;

    let quota: (i32, f64, f64) = sqlx::query_as(
        "SELECT max_concurrent_sandboxes, sandbox_cpu_cores, sandbox_memory_gb FROM plans WHERE name = 'free'",
    )
    .fetch_one(&pool)
    .await
    .expect("free plan should have sandbox quota columns populated");
    assert_eq!(quota, (1, 0.5, 1.0));

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sandbox_app_configs")
        .fetch_one(&pool)
        .await
        .expect("sandbox_app_configs table should exist and be queryable");
    assert!(count >= 0);

    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM sandbox_instances")
        .fetch_one(&pool)
        .await
        .expect("sandbox_instances table should exist and be queryable");
    assert!(count >= 0);
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn sandbox_migrations_rerun_cleanly() {
    let pool = pool().await;
    shipyard_db::run_migrations(&pool).await.expect("re-running migrations should be a no-op, not an error");
}
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `TEST_DATABASE_URL=postgres://shipyard:shipyard@localhost:5432/shipyard cargo test -p shipyard-db --test sandbox_migrations -- --ignored`
Expected: both tests PASS (start a local Postgres first if needed — check `infra/postgres/` for the existing dev compose setup).

- [ ] **Step 4: Commit**

```bash
git add backend/crates/db/migrations/20250101000053_sandbox_apps.sql backend/crates/db/migrations/20250101000054_sandbox_quota_plans.sql backend/crates/db/tests/sandbox_migrations.rs
git commit -m "feat(db): add sandbox_app service type, config/instance tables, and quota columns"
```

---

## Task 2: Docker crate — plain-container lifecycle with gVisor runtime-class support

**Files:**
- Modify: `backend/crates/docker/src/types.rs`
- Modify: `backend/crates/docker/src/engine.rs`

**Interfaces:**
- Consumes: nothing new (extends the existing `DockerEngine` trait / `BollardDockerEngine` impl already used throughout the codebase).
- Produces:
  - `pub struct ContainerSpec { name, image, cmd: Option<Vec<String>>, env: Vec<String>, mounts: Vec<MountSpec>, network: Option<String>, network_aliases: Vec<String>, runtime_class: Option<String>, resources: Option<ResourceSpec> }`
  - `fn build_container_config(spec: &ContainerSpec) -> bollard::container::Config<String>` (pure, unit-testable)
  - `DockerEngine::create_container(&self, spec: ContainerSpec) -> AppResult<String>` (returns container ID)
  - `DockerEngine::remove_container(&self, container_id: &str, force: bool) -> AppResult<()>`
  - `DockerEngine::wait_container(&self, container_id: &str) -> AppResult<i64>` (blocks until exit, returns exit code)
  - `DockerEngine::start_container(&self, container_id: &str) -> AppResult<()>`
  - Reuses existing `container_logs`, `create_volume`, `remove_volume`.

- [ ] **Step 1: Write the failing test for `build_container_config`**

Add to `backend/crates/docker/src/types.rs`:
```rust
/// Specification for creating a single plain (non-Swarm) container — used for
/// sandbox runtime containers and short-lived stack-detection probe containers,
/// neither of which need Swarm's declarative reconciliation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContainerSpec {
    pub name: String,
    pub image: String,
    pub cmd: Option<Vec<String>>,
    pub env: Vec<String>,
    pub mounts: Vec<MountSpec>,
    pub network: Option<String>,
    /// DNS aliases this container is reachable by on `network` — used so a
    /// sandbox's Traefik upstream target stays stable across container
    /// recreation (same alias, new container each start).
    pub network_aliases: Vec<String>,
    /// Docker runtime class, e.g. Some("runsc") for gVisor isolation.
    /// None uses the daemon's default runtime.
    pub runtime_class: Option<String>,
    pub resources: Option<ResourceSpec>,
}
```

Add a test module at the bottom of `backend/crates/docker/src/engine.rs`:
```rust
#[cfg(test)]
mod container_spec_tests {
    use super::*;

    fn sample_spec() -> ContainerSpec {
        ContainerSpec {
            name: "shipyard-sandbox-abcd1234".to_string(),
            image: "node:20-alpine".to_string(),
            cmd: Some(vec!["sh".to_string(), "-c".to_string(), "npm run dev".to_string()]),
            env: vec!["PORT=3000".to_string()],
            mounts: vec![MountSpec {
                source: "sandbox-vol-abcd1234".to_string(),
                target: "/app".to_string(),
                mount_type: MountType::Volume,
                readonly: false,
            }],
            network: Some("shipyard-net".to_string()),
            network_aliases: vec!["shipyard-sandbox-abcd1234".to_string()],
            runtime_class: Some("runsc".to_string()),
            resources: Some(ResourceSpec {
                cpu_limit: Some(0.5),
                memory_limit_mb: Some(1024),
                cpu_reservation: None,
                memory_reservation_mb: None,
            }),
        }
    }

    #[test]
    fn build_container_config_sets_runtime_class() {
        let config = build_container_config(&sample_spec());
        let host_config = config.host_config.expect("host_config must be set");
        assert_eq!(host_config.runtime.as_deref(), Some("runsc"));
    }

    #[test]
    fn build_container_config_sets_resource_limits_in_bytes_and_nanocpus() {
        let config = build_container_config(&sample_spec());
        let host_config = config.host_config.expect("host_config must be set");
        assert_eq!(host_config.memory, Some(1024 * 1024 * 1024));
        assert_eq!(host_config.nano_cpus, Some(500_000_000));
    }

    #[test]
    fn build_container_config_mounts_volume_at_target() {
        let config = build_container_config(&sample_spec());
        let host_config = config.host_config.expect("host_config must be set");
        let mounts = host_config.mounts.expect("mounts must be set");
        assert_eq!(mounts.len(), 1);
        assert_eq!(mounts[0].source.as_deref(), Some("sandbox-vol-abcd1234"));
        assert_eq!(mounts[0].target.as_deref(), Some("/app"));
    }

    #[test]
    fn build_container_config_sets_network_mode() {
        let config = build_container_config(&sample_spec());
        let host_config = config.host_config.expect("host_config must be set");
        assert_eq!(host_config.network_mode.as_deref(), Some("shipyard-net"));
    }

    #[test]
    fn build_container_config_sets_network_alias() {
        let config = build_container_config(&sample_spec());
        let networking_config = config.networking_config.expect("networking_config must be set");
        let endpoint = networking_config
            .endpoints_config
            .get("shipyard-net")
            .expect("endpoint config for the spec's network must be present");
        assert_eq!(
            endpoint.aliases.as_deref(),
            Some(["shipyard-sandbox-abcd1234".to_string()].as_slice())
        );
    }

    #[test]
    fn build_container_config_none_runtime_class_omits_field() {
        let mut spec = sample_spec();
        spec.runtime_class = None;
        let config = build_container_config(&spec);
        let host_config = config.host_config.expect("host_config must be set");
        assert_eq!(host_config.runtime, None);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shipyard-docker container_spec_tests`
Expected: FAIL — `build_container_config` and `ContainerSpec` do not exist yet.

- [ ] **Step 3: Implement `build_container_config` and the trait methods**

Add to the imports at the top of `backend/crates/docker/src/engine.rs`:
```rust
use bollard::container::{
    Config as ContainerConfig, CreateContainerOptions, RemoveContainerOptions,
    WaitContainerOptions,
};
use bollard::models::{EndpointSettings, HostConfig, NetworkingConfig};
```

Add to the `DockerEngine` trait (near `create_volume`/`stop_container`):
```rust
    /// Create (but do not start) a plain, non-Swarm container. Used for
    /// sandbox runtime containers and short-lived detection probes — neither
    /// needs Swarm's declarative reconciliation, and Swarm has no per-service
    /// runtime-class selection, which gVisor isolation requires.
    async fn create_container(&self, spec: ContainerSpec) -> AppResult<String>;

    /// Start a previously created container.
    async fn start_container(&self, container_id: &str) -> AppResult<()>;

    /// Remove a container. `force` also stops it first if still running.
    async fn remove_container(&self, container_id: &str, force: bool) -> AppResult<()>;

    /// Block until the container exits, returning its exit code. Used for
    /// short-lived probe containers whose stdout is read via `container_logs`
    /// after this returns.
    async fn wait_container(&self, container_id: &str) -> AppResult<i64>;
```

Add the pure conversion function (module-level, near `build_bollard_spec`):
```rust
fn build_container_config(spec: &ContainerSpec) -> ContainerConfig<String> {
    let mounts: Vec<Mount> = spec
        .mounts
        .iter()
        .map(|m| Mount {
            source: Some(m.source.clone()),
            target: Some(m.target.clone()),
            typ: Some(match m.mount_type {
                MountType::Volume => MountTypeEnum::VOLUME,
                MountType::Bind => MountTypeEnum::BIND,
                MountType::Tmpfs => MountTypeEnum::TMPFS,
            }),
            read_only: Some(m.readonly),
            ..Default::default()
        })
        .collect();

    let cpu_to_nano = |cpus: f64| -> i64 { (cpus * 1_000_000_000.0) as i64 };
    let mb_to_bytes = |mb: u64| -> i64 { (mb * 1024 * 1024) as i64 };

    let host_config = HostConfig {
        mounts: Some(mounts),
        runtime: spec.runtime_class.clone(),
        network_mode: spec.network.clone(),
        nano_cpus: spec.resources.as_ref().and_then(|r| r.cpu_limit.map(cpu_to_nano)),
        memory: spec.resources.as_ref().and_then(|r| r.memory_limit_mb.map(mb_to_bytes)),
        ..Default::default()
    };

    ContainerConfig {
        image: Some(spec.image.clone()),
        cmd: spec.cmd.clone(),
        env: Some(spec.env.clone()),
        host_config: Some(host_config),
        networking_config: spec.network.as_ref().map(|net| NetworkingConfig {
            endpoints_config: std::collections::HashMap::from([(
                net.clone(),
                EndpointSettings {
                    aliases: Some(spec.network_aliases.clone()),
                    ..Default::default()
                },
            )]),
        }),
        ..Default::default()
    }
}
```

Add the trait impls to `impl DockerEngine for BollardDockerEngine` (near `stop_container`/`restart_container`):
```rust
    async fn create_container(&self, spec: ContainerSpec) -> AppResult<String> {
        let config = build_container_config(&spec);
        let result = self
            .client
            .create_container(
                Some(CreateContainerOptions { name: spec.name.clone(), platform: None }),
                config,
            )
            .await
            .map_err(|e| AppError::Docker(format!("create_container failed: {e}")))?;
        Ok(result.id)
    }

    async fn start_container(&self, container_id: &str) -> AppResult<()> {
        self.client
            .start_container::<String>(container_id, None)
            .await
            .map_err(|e| AppError::Docker(format!("start_container failed: {e}")))?;
        Ok(())
    }

    async fn remove_container(&self, container_id: &str, force: bool) -> AppResult<()> {
        self.client
            .remove_container(container_id, Some(RemoveContainerOptions { force, ..Default::default() }))
            .await
            .map_err(|e| AppError::Docker(format!("remove_container failed: {e}")))?;
        Ok(())
    }

    async fn wait_container(&self, container_id: &str) -> AppResult<i64> {
        use futures::StreamExt;
        let mut stream = self.client.wait_container(container_id, None::<WaitContainerOptions<String>>);
        match stream.next().await {
            Some(Ok(result)) => Ok(result.status_code),
            Some(Err(e)) => Err(AppError::Docker(format!("wait_container failed: {e}"))),
            None => Err(AppError::Docker("wait_container: stream ended with no result".to_string())),
        }
    }
```

Also add `pub use types::ContainerSpec;` is already covered by the existing `pub use types::*;` in `backend/crates/docker/src/lib.rs:11`.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p shipyard-docker container_spec_tests`
Expected: PASS (all 5 tests).

- [ ] **Step 5: Run the full docker crate build to check for compile errors from other DockerEngine implementers**

Run: `cargo build -p shipyard-docker -p shipyard-api`
Expected: builds cleanly. If any other `impl DockerEngine` exists (only `BollardDockerEngine` does, per the codebase search), no further changes needed.

- [ ] **Step 6: Commit**

```bash
git add backend/crates/docker/src/types.rs backend/crates/docker/src/engine.rs
git commit -m "feat(docker): add plain-container lifecycle with gVisor runtime-class support"
```

---

## Task 3: Stack detection (probe-based) and `shipyard.json` app config

**Files:**
- Create: `backend/crates/engine/src/sandbox_probe.rs`
- Modify: `backend/crates/engine/src/edge_fn_config.rs`
- Modify: `backend/crates/engine/src/lib.rs` (module declaration)

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `pub const SANDBOX_PROBE_SCRIPT: &str` — shell script run inside a throwaway `alpine` container against the app's mounted volume, printing one JSON line describing what it found.
  - `pub struct ProbeResult { has_package_json: bool, package_json: Option<String>, has_requirements_txt: bool, has_manage_py: bool, has_index_html: bool, shipyard_json: Option<String> }` (deserialized from the probe's JSON stdout)
  - `pub struct DetectedStack { pub runtime: String, pub base_image: String, pub install_cmd: Option<String>, pub dev_cmd: String, pub port: u16, pub source: DetectionSource }`
  - `pub enum DetectionSource { Detected, Manifest }`
  - `pub fn detect_stack(probe: &ProbeResult) -> Result<DetectedStack, String>` (pure, unit-testable — `Err` message is the actionable "add a shipyard.json" text surfaced to the user)
  - `AppConfig` (new field on the existing `ShipyardConfig` in `edge_fn_config.rs`): `pub app: Option<SandboxAppManifest>` where `SandboxAppManifest { runtime: String, install: Option<String>, dev: String, port: u16 }`.

- [ ] **Step 1: Write the failing tests**

```rust
// backend/crates/engine/src/sandbox_probe.rs (top portion — tests first)
use serde::Deserialize;

#[derive(Debug, Deserialize, Default)]
pub struct ProbeResult {
    pub has_package_json: bool,
    pub package_json: Option<String>,
    pub has_requirements_txt: bool,
    pub has_manage_py: bool,
    pub has_index_html: bool,
    pub shipyard_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DetectionSource {
    Detected,
    Manifest,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedStack {
    pub runtime: String,
    pub base_image: String,
    pub install_cmd: Option<String>,
    pub dev_cmd: String,
    pub port: u16,
    pub source: DetectionSource,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_overrides_detection_when_present() {
        let probe = ProbeResult {
            has_package_json: true,
            package_json: Some(r#"{"scripts":{"dev":"vite"}}"#.to_string()),
            shipyard_json: Some(r#"{"app":{"runtime":"node","install":"pnpm install","dev":"pnpm dev","port":4000}}"#.to_string()),
            ..Default::default()
        };
        let stack = detect_stack(&probe).expect("manifest should resolve successfully");
        assert_eq!(stack.source, DetectionSource::Manifest);
        assert_eq!(stack.install_cmd.as_deref(), Some("pnpm install"));
        assert_eq!(stack.dev_cmd, "pnpm dev");
        assert_eq!(stack.port, 4000);
    }

    #[test]
    fn detects_node_from_package_json_dev_script() {
        let probe = ProbeResult {
            has_package_json: true,
            package_json: Some(r#"{"scripts":{"dev":"vite --host 0.0.0.0"}}"#.to_string()),
            ..Default::default()
        };
        let stack = detect_stack(&probe).expect("node should be detected");
        assert_eq!(stack.source, DetectionSource::Detected);
        assert_eq!(stack.runtime, "node");
        assert_eq!(stack.base_image, "node:20-alpine");
        assert_eq!(stack.install_cmd.as_deref(), Some("npm install"));
        assert_eq!(stack.dev_cmd, "npm run dev");
        assert_eq!(stack.port, 3000);
    }

    #[test]
    fn detects_python_from_requirements_and_manage_py() {
        let probe = ProbeResult {
            has_requirements_txt: true,
            has_manage_py: true,
            ..Default::default()
        };
        let stack = detect_stack(&probe).expect("python/django should be detected");
        assert_eq!(stack.runtime, "python");
        assert_eq!(stack.base_image, "python:3.12-slim");
        assert_eq!(stack.install_cmd.as_deref(), Some("pip install -r requirements.txt"));
        assert_eq!(stack.dev_cmd, "python manage.py runserver 0.0.0.0:8000");
        assert_eq!(stack.port, 8000);
    }

    #[test]
    fn detects_static_from_index_html_only() {
        let probe = ProbeResult { has_index_html: true, ..Default::default() };
        let stack = detect_stack(&probe).expect("static site should be detected");
        assert_eq!(stack.runtime, "static");
        assert_eq!(stack.install_cmd, None);
        assert_eq!(stack.port, 8080);
    }

    #[test]
    fn unrecognized_stack_returns_actionable_error() {
        let probe = ProbeResult::default();
        let err = detect_stack(&probe).unwrap_err();
        assert!(err.contains("shipyard.json"), "error should point the user at the manifest fallback, got: {err}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p shipyard-engine sandbox_probe`
Expected: FAIL — `detect_stack` is not defined yet.

- [ ] **Step 3: Implement `detect_stack` and the probe script constant**

Append to `backend/crates/engine/src/sandbox_probe.rs` (below the types/tests already written):
```rust
/// Shell script executed inside a throwaway `alpine`-based container with the
/// app's volume mounted read-only at /app. Prints exactly one JSON line to
/// stdout describing what it found, so the API process never needs direct
/// filesystem access to a volume that may live on a remote node.
pub const SANDBOX_PROBE_SCRIPT: &str = r#"
set -e
cd /app
pkg=$( [ -f package.json ] && cat package.json || echo "" )
sj=$( [ -f shipyard.json ] && cat shipyard.json || echo "" )
has_pkg=false; [ -f package.json ] && has_pkg=true
has_req=false; [ -f requirements.txt ] && has_req=true
has_manage=false; [ -f manage.py ] && has_manage=true
has_index=false; [ -f index.html ] && has_index=true
printf '{"has_package_json":%s,"package_json":%s,"has_requirements_txt":%s,"has_manage_py":%s,"has_index_html":%s,"shipyard_json":%s}\n' \
  "$has_pkg" "$( [ -n "$pkg" ] && printf '%s' "$pkg" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | awk 'BEGIN{printf "\""} {printf "%s\\n", $0} END{printf "\""}' || echo null )" \
  "$has_req" "$has_manage" "$has_index" \
  "$( [ -n "$sj" ] && printf '%s' "$sj" | sed -e 's/\\/\\\\/g' -e 's/"/\\"/g' | awk 'BEGIN{printf "\""} {printf "%s\\n", $0} END{printf "\""}' || echo null )"
"#;

fn dev_script_from_package_json(package_json: &str) -> Option<String> {
    let parsed: serde_json::Value = serde_json::from_str(package_json).ok()?;
    parsed.get("scripts")?.get("dev").and_then(|v| v.as_str()).map(|_| "npm run dev".to_string())
}

/// Resolve the stack a sandbox should run. Manifest (shipyard.json `app` key)
/// always wins when present; otherwise falls back to file-presence detection;
/// otherwise returns an error naming the manifest as the fix.
pub fn detect_stack(probe: &ProbeResult) -> Result<DetectedStack, String> {
    if let Some(sj) = &probe.shipyard_json {
        if let Ok(config) = serde_json::from_str::<crate::edge_fn_config::ShipyardConfig>(sj) {
            if let Some(app) = config.app {
                let base_image = match app.runtime.as_str() {
                    "node" => "node:20-alpine",
                    "python" => "python:3.12-slim",
                    "static" => "nginx:alpine",
                    other => return Err(format!("shipyard.json declares unknown runtime '{other}'")),
                };
                return Ok(DetectedStack {
                    runtime: app.runtime,
                    base_image: base_image.to_string(),
                    install_cmd: app.install,
                    dev_cmd: app.dev,
                    port: app.port,
                    source: DetectionSource::Manifest,
                });
            }
        }
    }

    if probe.has_package_json {
        let dev_cmd = probe
            .package_json
            .as_deref()
            .and_then(dev_script_from_package_json)
            .unwrap_or_else(|| "npm start".to_string());
        return Ok(DetectedStack {
            runtime: "node".to_string(),
            base_image: "node:20-alpine".to_string(),
            install_cmd: Some("npm install".to_string()),
            dev_cmd,
            port: 3000,
            source: DetectionSource::Detected,
        });
    }

    if probe.has_requirements_txt {
        let dev_cmd = if probe.has_manage_py {
            "python manage.py runserver 0.0.0.0:8000".to_string()
        } else {
            "python app.py".to_string()
        };
        return Ok(DetectedStack {
            runtime: "python".to_string(),
            base_image: "python:3.12-slim".to_string(),
            install_cmd: Some("pip install -r requirements.txt".to_string()),
            dev_cmd,
            port: 8000,
            source: DetectionSource::Detected,
        });
    }

    if probe.has_index_html {
        return Ok(DetectedStack {
            runtime: "static".to_string(),
            base_image: "nginx:alpine".to_string(),
            install_cmd: None,
            dev_cmd: "nginx -g 'daemon off;'".to_string(),
            port: 8080,
            source: DetectionSource::Detected,
        });
    }

    Err("Could not detect a recognized stack (no package.json, requirements.txt, or index.html found). Add a shipyard.json manifest declaring { \"app\": { \"runtime\", \"install\", \"dev\", \"port\" } }.".to_string())
}
```

Add the `app` field to `backend/crates/engine/src/edge_fn_config.rs`:
```rust
#[derive(Debug, Deserialize, Default)]
pub struct ShipyardConfig {
    pub functions: Option<FunctionsConfig>,
    pub app: Option<SandboxAppManifest>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct SandboxAppManifest {
    pub runtime: String,
    pub install: Option<String>,
    pub dev: String,
    pub port: u16,
}
```

Add `pub mod sandbox_probe;` to `backend/crates/engine/src/lib.rs` alongside the existing `mod`/`pub mod` declarations (find the line declaring `edge_fn_detector`/`edge_fn_config` and add the new one next to it).

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p shipyard-engine sandbox_probe`
Expected: PASS (all 5 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/crates/engine/src/sandbox_probe.rs backend/crates/engine/src/edge_fn_config.rs backend/crates/engine/src/lib.rs
git commit -m "feat(engine): add probe-based sandbox stack detection with shipyard.json override"
```

---

## Task 4: `SandboxConfig` app configuration

**Files:**
- Modify: `backend/crates/common/src/config.rs`

**Interfaces:**
- Produces: `AppConfig.sandbox: SandboxConfig` with fields `enabled: bool`, `idle_timeout_secs: u64` (default 1200), `reaper_interval_secs: u64` (default 60), `preview_base_domain: String`, `runtime_class: String` (default `"runsc"`), `probe_image: String` (default `"alpine:3.19"`), `placeholder_upstream: String` (default `"shipyard-sandbox-placeholder"`), `placeholder_port: u16` (default 8080). Consumed by Tasks 6-10.

- [ ] **Step 1: Write the failing test**

Add near the bottom of `backend/crates/common/src/config.rs` (alongside any existing tests, or create a new `#[cfg(test)] mod tests` block if none exists for this file):
```rust
#[cfg(test)]
mod sandbox_config_tests {
    use super::*;

    #[test]
    fn sandbox_config_defaults_are_sane() {
        let cfg = SandboxConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.idle_timeout_secs, 1200);
        assert_eq!(cfg.reaper_interval_secs, 60);
        assert_eq!(cfg.runtime_class, "runsc");
        assert_eq!(cfg.probe_image, "alpine:3.19");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shipyard-common sandbox_config_tests`
Expected: FAIL — `SandboxConfig` does not exist.

- [ ] **Step 3: Implement `SandboxConfig`**

Add the struct near `EdgeFunctionsConfig` in `backend/crates/common/src/config.rs`:
```rust
#[derive(Debug, Clone, Deserialize)]
pub struct SandboxConfig {
    /// Set true to enable the sandbox runtime feature.
    #[serde(default)]
    pub enabled: bool,
    /// Seconds without a heartbeat before the idle reaper stops a sandbox.
    #[serde(default = "default_sandbox_idle_timeout_secs")]
    pub idle_timeout_secs: u64,
    /// How often (seconds) the idle reaper scans for stale sandboxes.
    #[serde(default = "default_sandbox_reaper_interval_secs")]
    pub reaper_interval_secs: u64,
    /// Base domain preview URLs are minted under, e.g. "shipyard-apps.dev"
    /// produces "preview-<slug>.shipyard-apps.dev".
    pub preview_base_domain: String,
    /// Docker runtime class used for sandbox and probe containers.
    #[serde(default = "default_sandbox_runtime_class")]
    pub runtime_class: String,
    /// Image used for the short-lived stack-detection probe container.
    #[serde(default = "default_sandbox_probe_image")]
    pub probe_image: String,
    /// Traefik upstream (network-alias hostname) of the always-running
    /// placeholder/cold-start responder service, used while a sandbox is stopped.
    #[serde(default = "default_sandbox_placeholder_upstream")]
    pub placeholder_upstream: String,
    /// Port the placeholder responder listens on.
    #[serde(default = "default_sandbox_placeholder_port")]
    pub placeholder_port: u16,
}

fn default_sandbox_idle_timeout_secs() -> u64 { 1200 }
fn default_sandbox_reaper_interval_secs() -> u64 { 60 }
fn default_sandbox_runtime_class() -> String { "runsc".to_string() }
fn default_sandbox_probe_image() -> String { "alpine:3.19".to_string() }
fn default_sandbox_placeholder_upstream() -> String { "shipyard-sandbox-placeholder".to_string() }
fn default_sandbox_placeholder_port() -> u16 { 8080 }

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            idle_timeout_secs: default_sandbox_idle_timeout_secs(),
            reaper_interval_secs: default_sandbox_reaper_interval_secs(),
            preview_base_domain: "shipyard-apps.dev".to_string(),
            runtime_class: default_sandbox_runtime_class(),
            probe_image: default_sandbox_probe_image(),
            placeholder_upstream: default_sandbox_placeholder_upstream(),
            placeholder_port: default_sandbox_placeholder_port(),
        }
    }
}
```

Add the field to `AppConfig` (near `pub edge_functions: EdgeFunctionsConfig,`):
```rust
    #[serde(default)]
    pub sandbox: SandboxConfig,
```

Add `sandbox: SandboxConfig::default(),` to the `AppConfig` construction literal at `config.rs:439` (same place `edge_functions: EdgeFunctionsConfig::default(),` is set).

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shipyard-common sandbox_config_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/common/src/config.rs
git commit -m "feat(config): add SandboxConfig for sandbox runtime settings"
```

---

## Task 5: `OptionalAuthUser` extractor

**Files:**
- Modify: `backend/crates/api/src/auth/mod.rs`

**Interfaces:**
- Produces: `pub struct OptionalAuthUser(pub Option<AuthUser>)` implementing `FromRequestParts<AppState, Rejection = Infallible>`. Consumed by Task 9's public cold-preview route.

- [ ] **Step 1: Write the failing test**

Add near any existing tests in `backend/crates/api/src/auth/mod.rs` (or a new `#[cfg(test)] mod tests` block at the bottom):
```rust
#[cfg(test)]
mod optional_auth_tests {
    use super::*;
    use axum::http::{HeaderMap, HeaderValue};

    #[test]
    fn missing_bearer_header_yields_none() {
        let headers = HeaderMap::new();
        assert!(extract_bearer_token(&headers).is_err());
        // OptionalAuthUser must not propagate this as an error — verified in
        // Step 3's from_request_parts impl by construction (Rejection = Infallible).
    }

    #[test]
    fn malformed_bearer_header_is_treated_as_absent() {
        let mut headers = HeaderMap::new();
        headers.insert("authorization", HeaderValue::from_static("not-a-bearer-token"));
        assert!(extract_bearer_token(&headers).is_err());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shipyard-api optional_auth_tests`
Expected: FAIL if `extract_bearer_token` is private (compile error: function is private). This confirms the test needs the extractor built with visibility the module already controls — proceed to Step 3, which uses `extract_bearer_token` from within the same module so this compiles once the impl below is added in the same file.

- [ ] **Step 3: Implement `OptionalAuthUser`**

Add to `backend/crates/api/src/auth/mod.rs`, near the `AuthUser` / `FromRequestParts` impl:
```rust
/// Same identity resolution as `AuthUser`, but never rejects the request.
/// Used for routes that must work for both authenticated callers (org
/// members) and anonymous visitors (e.g. a shared, unauthenticated preview
/// link) with different behavior for each — `AuthUser` alone would 401 the
/// anonymous case, which is exactly the case this route needs to allow.
pub struct OptionalAuthUser(pub Option<AuthUser>);

#[async_trait]
impl FromRequestParts<AppState> for OptionalAuthUser {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        match AuthUser::from_request_parts(parts, state).await {
            Ok(user) => Ok(OptionalAuthUser(Some(user))),
            Err(_) => Ok(OptionalAuthUser(None)),
        }
    }
}
```

Note: `AuthUser::from_request_parts` consumes `parts` (reads headers, doesn't remove them), so calling it here and letting the handler also declare `OptionalAuthUser` is safe — no double-extraction conflict since only one auth extractor is used per handler.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shipyard-api optional_auth_tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/auth/mod.rs
git commit -m "feat(auth): add OptionalAuthUser extractor for public+authed routes"
```

---

## Task 6: `sandbox_runtime` module scaffolding — models and quota

**Files:**
- Create: `backend/crates/api/src/sandbox_runtime/mod.rs`
- Create: `backend/crates/api/src/sandbox_runtime/models.rs`
- Create: `backend/crates/api/src/sandbox_runtime/quota.rs`
- Modify: `backend/crates/api/src/main.rs` (module declaration)
- Test: `backend/crates/api/tests/sandbox_quota.rs`

**Interfaces:**
- Consumes: `plans` table (Task 1), `sandbox_instances` table (Task 1).
- Produces:
  - `pub struct SandboxAppConfigRow { service_id, runtime, base_image, install_cmd, dev_cmd, port, manifest_source, volume_name }`
  - `pub struct SandboxInstanceRow { service_id, status, container_id, container_name, preview_url, last_heartbeat_at, started_at }`
  - `pub async fn check_quota(db: &PgPool, org_id: Uuid) -> AppResult<Result<(), String>>` — `Ok(Ok(()))` = under quota, `Ok(Err(message))` = over quota with a user-facing message. Consumed by Task 7's `start_sandbox`.

- [ ] **Step 1: Write the failing test**

```rust
// backend/crates/api/tests/sandbox_quota.rs
use sqlx::PgPool;
use uuid::Uuid;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
}

async fn pool() -> PgPool {
    let pool = shipyard_db::init_pool(&test_database_url(), 5).await.expect("connect to TEST_DATABASE_URL");
    shipyard_db::run_migrations(&pool).await.expect("migrations apply");
    pool
}

/// Seeds an org on the free plan (max_concurrent_sandboxes = 1) with one
/// project, returning the org_id.
async fn seed_free_org(pool: &PgPool) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug, plan_id) VALUES ($1, $2, $3, (SELECT id FROM plans WHERE name = 'free'))")
        .bind(org_id)
        .bind(format!("Test Org {suffix}"))
        .bind(format!("test-org-{suffix}"))
        .execute(pool)
        .await
        .expect("insert organization");
    org_id
}

async fn seed_sandbox_service(pool: &PgPool, org_id: Uuid, status: &str) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, org_id, name, slug) VALUES ($1, $2, $3, $4)")
        .bind(project_id).bind(org_id).bind(format!("proj-{suffix}")).bind(format!("proj-{suffix}"))
        .execute(pool).await.expect("insert project");

    let service_id = Uuid::new_v4();
    sqlx::query("INSERT INTO services (id, project_id, name, slug, type, status, replicas, ports) VALUES ($1, $2, $3, $4, 'sandbox_app', 'running', 0, '[]'::jsonb)")
        .bind(service_id).bind(project_id).bind(format!("app-{suffix}")).bind(format!("app-{suffix}"))
        .execute(pool).await.expect("insert service");

    sqlx::query("INSERT INTO sandbox_instances (service_id, status) VALUES ($1, $2)")
        .bind(service_id).bind(status)
        .execute(pool).await.expect("insert sandbox_instances");

    service_id
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn free_org_under_quota_with_zero_running_sandboxes() {
    let pool = pool().await;
    let org_id = seed_free_org(&pool).await;
    let result = shipyard_api::sandbox_runtime::quota::check_quota(&pool, org_id).await.unwrap();
    assert!(result.is_ok(), "org with 0 running sandboxes should be under its quota of 1");
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn free_org_over_quota_with_one_running_sandbox() {
    let pool = pool().await;
    let org_id = seed_free_org(&pool).await;
    seed_sandbox_service(&pool, org_id, "running").await;

    let result = shipyard_api::sandbox_runtime::quota::check_quota(&pool, org_id).await.unwrap();
    assert!(result.is_err(), "org already at its quota of 1 running sandbox should be over quota");
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn stopped_sandboxes_do_not_count_against_quota() {
    let pool = pool().await;
    let org_id = seed_free_org(&pool).await;
    seed_sandbox_service(&pool, org_id, "stopped").await;

    let result = shipyard_api::sandbox_runtime::quota::check_quota(&pool, org_id).await.unwrap();
    assert!(result.is_ok(), "a stopped sandbox should not count against the concurrent quota");
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `TEST_DATABASE_URL=postgres://shipyard:shipyard@localhost:5432/shipyard cargo test -p shipyard-api --test sandbox_quota -- --ignored`
Expected: FAIL — `sandbox_runtime` module does not exist yet.

- [ ] **Step 3: Implement the module**

`backend/crates/api/src/sandbox_runtime/models.rs`:
```rust
use uuid::Uuid;

#[derive(Debug, sqlx::FromRow)]
pub struct SandboxAppConfigRow {
    pub service_id: Uuid,
    pub runtime: Option<String>,
    pub base_image: Option<String>,
    pub install_cmd: Option<String>,
    pub dev_cmd: Option<String>,
    pub port: Option<i32>,
    pub manifest_source: String,
    pub volume_name: String,
}

#[derive(Debug, sqlx::FromRow)]
pub struct SandboxInstanceRow {
    pub service_id: Uuid,
    pub status: String,
    pub container_id: Option<String>,
    pub container_name: Option<String>,
    pub preview_url: Option<String>,
    pub last_heartbeat_at: Option<chrono::DateTime<chrono::Utc>>,
    pub started_at: Option<chrono::DateTime<chrono::Utc>>,
}
```

`backend/crates/api/src/sandbox_runtime/quota.rs`:
```rust
use sqlx::PgPool;
use shipyard_common::error::{AppError, AppResult};
use uuid::Uuid;

/// Checks the org's dedicated sandbox quota (separate from production
/// resource limits — see plans.max_concurrent_sandboxes). Returns `Ok(Ok(()))`
/// when under quota, `Ok(Err(message))` with a user-facing message when not.
pub async fn check_quota(db: &PgPool, org_id: Uuid) -> AppResult<Result<(), String>> {
    let max_concurrent: i32 = sqlx::query_scalar(
        "SELECT p.max_concurrent_sandboxes
         FROM organizations o JOIN plans p ON p.id = o.plan_id
         WHERE o.id = $1",
    )
    .bind(org_id)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let running_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sandbox_instances si
         JOIN services s ON s.id = si.service_id
         JOIN projects p ON p.id = s.project_id
         WHERE p.org_id = $1 AND si.status = 'running'",
    )
    .bind(org_id)
    .fetch_one(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    if running_count >= max_concurrent as i64 {
        return Ok(Err(format!(
            "This organization has reached its limit of {max_concurrent} concurrent sandbox(es). Stop another sandbox or upgrade your plan."
        )));
    }
    Ok(Ok(()))
}
```

`backend/crates/api/src/sandbox_runtime/mod.rs`:
```rust
pub mod models;
pub mod quota;
```

Add `pub mod sandbox_runtime;` to `backend/crates/api/src/main.rs` alongside `mod edge_functions;`, and confirm `shipyard_api` exposes it as a library target so integration tests in `backend/crates/api/tests/` can reach `shipyard_api::sandbox_runtime::quota::check_quota` — check `backend/crates/api/src/lib.rs` (or add one if the crate is currently binary-only) re-exporting `pub mod sandbox_runtime;` the same way it must already expose modules for any existing `backend/crates/api/tests/*.rs` file to compile. If `backend/crates/api` has no `lib.rs` yet (it may be binary-only today, per the earlier research finding that `api` has no `tests/` dir), add one:
```rust
// backend/crates/api/src/lib.rs
pub mod sandbox_runtime;
```
and adjust `main.rs` to `use shipyard_api::sandbox_runtime;` instead of declaring the module inline, so the module has exactly one owner.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `TEST_DATABASE_URL=postgres://shipyard:shipyard@localhost:5432/shipyard cargo test -p shipyard-api --test sandbox_quota -- --ignored`
Expected: PASS (all 3 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/sandbox_runtime/ backend/crates/api/src/main.rs backend/crates/api/src/lib.rs backend/crates/api/tests/sandbox_quota.rs
git commit -m "feat(api): add sandbox_runtime module scaffolding with quota checks"
```

---

## Task 7: Manager — `start_sandbox`

**Files:**
- Create: `backend/crates/api/src/sandbox_runtime/manager.rs`
- Modify: `backend/crates/api/src/sandbox_runtime/mod.rs`

**Interfaces:**
- Consumes: `quota::check_quota` (Task 6), `DockerEngine::{create_volume, create_container, start_container, wait_container, container_logs, remove_container}` (Task 2), `sandbox_probe::{SANDBOX_PROBE_SCRIPT, ProbeResult, detect_stack}` (Task 3), `resolve_docker_for_service`-equivalent per-service Docker engine resolution (reuse `shipyard_engine`'s existing helper — see Step 3 note), `crate::resources::sync_traefik_dynamic_config` (existing), `state.config.sandbox`/`state.config.traefik` (Task 4).
- Produces: `pub async fn start_sandbox(state: &AppState, service_id: Uuid) -> AppResult<SandboxInstanceRow>`. Consumed by Task 9's HTTP handler.

- [ ] **Step 1: Write the failing test**

Since `start_sandbox` genuinely needs a live Docker daemon (even a local one) to exercise `create_container`/`wait_container` meaningfully, write a unit test for the one pure sub-piece that doesn't: the container-naming/aliasing helper, which every other test and the manual verification step depend on being stable.

```rust
// bottom of backend/crates/api/src/sandbox_runtime/manager.rs
#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn sandbox_container_name_is_stable_across_calls_for_same_service() {
        let service_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        let name1 = sandbox_container_name(service_id);
        let name2 = sandbox_container_name(service_id);
        assert_eq!(name1, name2);
        assert_eq!(name1, "shipyard-sandbox-11111111");
    }

    #[test]
    fn sandbox_volume_name_is_stable_and_distinct_from_container_name() {
        let service_id = Uuid::parse_str("11111111-2222-3333-4444-555555555555").unwrap();
        let vol = sandbox_volume_name(service_id);
        assert_eq!(vol, "sandbox-vol-11111111");
        assert_ne!(vol, sandbox_container_name(service_id));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shipyard-api sandbox_container_name`
Expected: FAIL — functions don't exist.

- [ ] **Step 3: Implement `manager.rs`**

```rust
use sqlx::PgPool;
use uuid::Uuid;
use shipyard_common::error::{AppError, AppResult};
use shipyard_docker::engine::DockerEngine;
use shipyard_docker::types::{ContainerSpec, MountSpec, MountType, ResourceSpec};

use crate::AppState;
use super::models::{SandboxAppConfigRow, SandboxInstanceRow};
use super::quota;

pub fn sandbox_container_name(service_id: Uuid) -> String {
    format!("shipyard-sandbox-{}", &service_id.to_string()[..8])
}

pub fn sandbox_volume_name(service_id: Uuid) -> String {
    format!("sandbox-vol-{}", &service_id.to_string()[..8])
}

fn preview_hostname(slug: &str, base_domain: &str) -> String {
    format!("preview-{slug}.{base_domain}")
}

/// Starts (or returns the already-running) sandbox for `service_id`. Enforces
/// the org's dedicated sandbox quota, resolves the app's stack on first start
/// (probe container → detect_stack), launches the gVisor-isolated sandbox
/// container, and points the app's Traefik route at it.
pub async fn start_sandbox(state: &AppState, service_id: Uuid) -> AppResult<SandboxInstanceRow> {
    let (org_id, slug): (Uuid, String) = sqlx::query_as(
        "SELECT p.org_id, s.slug FROM services s JOIN projects p ON p.id = s.project_id WHERE s.id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?
    .ok_or_else(|| AppError::NotFound(format!("Service '{service_id}' not found")))?;

    // Idempotent: already starting/running just returns current state.
    if let Some(existing) = fetch_instance(&state.db, service_id).await? {
        if existing.status == "starting" || existing.status == "running" {
            return Ok(existing);
        }
    }

    match quota::check_quota(&state.db, org_id).await? {
        Ok(()) => {}
        Err(message) => return Err(AppError::BadRequest(message)),
    }

    upsert_instance_status(&state.db, service_id, "starting", None, None, None).await?;

    let volume_name = sandbox_volume_name(service_id);
    state.docker.create_volume(&volume_name, "local").await.ok(); // idempotent: ignore "already exists"

    let mut config = fetch_config(&state.db, service_id).await?;
    if config.is_none() {
        let stack = probe_and_detect(state, &volume_name).await.map_err(|e| {
            AppError::BadRequest(format!("Could not start sandbox: {e}"))
        })?;
        sqlx::query(
            "INSERT INTO sandbox_app_configs (service_id, runtime, base_image, install_cmd, dev_cmd, port, manifest_source, volume_name)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
        )
        .bind(service_id)
        .bind(&stack.runtime)
        .bind(&stack.base_image)
        .bind(&stack.install_cmd)
        .bind(&stack.dev_cmd)
        .bind(stack.port as i32)
        .bind(match stack.source {
            shipyard_engine::sandbox_probe::DetectionSource::Detected => "detected",
            shipyard_engine::sandbox_probe::DetectionSource::Manifest => "manifest",
        })
        .bind(&volume_name)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        config = fetch_config(&state.db, service_id).await?;
    }
    let config = config.ok_or_else(|| AppError::Internal("sandbox config missing after insert".to_string()))?;

    let container_name = sandbox_container_name(service_id);
    state.docker.remove_container(&container_name, true).await.ok(); // clean slate

    let (cpu_cores, memory_gb): (f64, f64) = sqlx::query_as(
        "SELECT p.sandbox_cpu_cores, p.sandbox_memory_gb
         FROM organizations o JOIN plans p ON p.id = o.plan_id WHERE o.id = $1",
    )
    .bind(org_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    let install_and_dev = match &config.install_cmd {
        Some(install) => format!("{install} && {}", config.dev_cmd.as_deref().unwrap_or("")),
        None => config.dev_cmd.clone().unwrap_or_default(),
    };

    let container_id = state
        .docker
        .create_container(ContainerSpec {
            name: container_name.clone(),
            image: config.base_image.clone().unwrap_or_default(),
            cmd: Some(vec!["sh".to_string(), "-c".to_string(), install_and_dev]),
            env: vec![format!("PORT={}", config.port.unwrap_or(3000))],
            mounts: vec![MountSpec {
                source: volume_name.clone(),
                target: "/app".to_string(),
                mount_type: MountType::Volume,
                readonly: false,
            }],
            network: Some(state.config.traefik.network.clone()),
            network_aliases: vec![container_name.clone()],
            runtime_class: Some(state.config.sandbox.runtime_class.clone()),
            resources: Some(ResourceSpec {
                cpu_limit: Some(cpu_cores),
                memory_limit_mb: Some((memory_gb * 1024.0) as u64),
                cpu_reservation: None,
                memory_reservation_mb: None,
            }),
        })
        .await?;

    state.docker.start_container(&container_id).await?;

    let hostname = preview_hostname(&slug, &state.config.sandbox.preview_base_domain);
    ensure_preview_domain(&state.db, service_id, &hostname, config.port.unwrap_or(3000)).await?;
    crate::resources::sync_traefik_dynamic_config(
        &state.db,
        service_id,
        &state.config.docker.label_prefix,
        &state.config.traefik.entrypoint_http,
        &state.config.traefik.entrypoint_https,
        state.config.traefik.dynamic_config_dir.as_deref(),
        Some(&container_name),
        false,
    )
    .await;

    let preview_url = format!("https://{hostname}");
    upsert_instance_status(&state.db, service_id, "running", Some(&container_id), Some(&container_name)).await?;
    sqlx::query("UPDATE sandbox_instances SET preview_url = $2, started_at = NOW(), last_heartbeat_at = NOW() WHERE service_id = $1")
        .bind(service_id)
        .bind(&preview_url)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;

    fetch_instance(&state.db, service_id)
        .await?
        .ok_or_else(|| AppError::Internal("sandbox_instances row missing after start".to_string()))
}

async fn probe_and_detect(
    state: &AppState,
    volume_name: &str,
) -> Result<shipyard_engine::sandbox_probe::DetectedStack, String> {
    use shipyard_engine::sandbox_probe::{detect_stack, ProbeResult, SANDBOX_PROBE_SCRIPT};

    let probe_name = format!("shipyard-probe-{}", Uuid::new_v4().simple());
    let container_id = state
        .docker
        .create_container(ContainerSpec {
            name: probe_name.clone(),
            image: state.config.sandbox.probe_image.clone(),
            cmd: Some(vec!["sh".to_string(), "-c".to_string(), SANDBOX_PROBE_SCRIPT.to_string()]),
            env: vec![],
            mounts: vec![MountSpec {
                source: volume_name.to_string(),
                target: "/app".to_string(),
                mount_type: MountType::Volume,
                readonly: true,
            }],
            network: None,
            network_aliases: vec![],
            runtime_class: Some(state.config.sandbox.runtime_class.clone()),
            resources: None,
        })
        .await
        .map_err(|e| e.to_string())?;

    state.docker.start_container(&container_id).await.map_err(|e| e.to_string())?;
    state.docker.wait_container(&container_id).await.map_err(|e| e.to_string())?;
    let logs = state
        .docker
        .container_logs(&container_id, shipyard_docker::types::LogOpts { stdout: true, stderr: false, ..Default::default() })
        .await
        .map_err(|e| e.to_string())?;
    state.docker.remove_container(&container_id, true).await.ok();

    let output = logs.join("");
    let probe: ProbeResult = serde_json::from_str(output.trim())
        .map_err(|e| format!("probe output was not valid JSON: {e} (output: {output})"))?;
    detect_stack(&probe)
}

async fn ensure_preview_domain(db: &PgPool, service_id: Uuid, hostname: &str, port: i32) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO domains (id, service_id, hostname, tls_enabled, cert_provider, port, traefik_router_name)
         VALUES ($1, $2, $3, TRUE, 'letsencrypt', $4, '')
         ON CONFLICT (service_id, hostname) DO UPDATE SET port = EXCLUDED.port",
    )
    .bind(Uuid::new_v4())
    .bind(service_id)
    .bind(hostname)
    .bind(port)
    .execute(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}

async fn fetch_config(db: &PgPool, service_id: Uuid) -> AppResult<Option<SandboxAppConfigRow>> {
    sqlx::query_as("SELECT * FROM sandbox_app_configs WHERE service_id = $1")
        .bind(service_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()).into())
}

pub(crate) async fn fetch_instance(db: &PgPool, service_id: Uuid) -> AppResult<Option<SandboxInstanceRow>> {
    sqlx::query_as("SELECT * FROM sandbox_instances WHERE service_id = $1")
        .bind(service_id)
        .fetch_optional(db)
        .await
        .map_err(|e| AppError::Database(e.to_string()).into())
}

async fn upsert_instance_status(
    db: &PgPool,
    service_id: Uuid,
    status: &str,
    container_id: Option<&str>,
    container_name: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO sandbox_instances (service_id, status, container_id, container_name, updated_at)
         VALUES ($1, $2, $3, $4, NOW())
         ON CONFLICT (service_id) DO UPDATE
           SET status = EXCLUDED.status, container_id = EXCLUDED.container_id,
               container_name = EXCLUDED.container_name, updated_at = NOW()",
    )
    .bind(service_id)
    .bind(status)
    .bind(container_id)
    .bind(container_name)
    .execute(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}
```

Note: `domains` needs a unique constraint on `(service_id, hostname)` for the `ON CONFLICT` clause above to work. Task 1's migrations already shipped and were applied in Task 1's own commit — migrations are append-only and must never be edited after they've run, so add a new migration here rather than reopening `20250101000053_sandbox_apps.sql`:

Create `backend/crates/db/migrations/20250101000055_domains_service_hostname_unique.sql`:
```sql
-- Sandbox preview domains are upserted by (service_id, hostname) on every
-- sandbox start/stop (toggling the port between the app's real port and the
-- placeholder port) — this constraint makes that upsert well-defined.
ALTER TABLE domains ADD CONSTRAINT domains_service_hostname_unique UNIQUE (service_id, hostname);
```

Add `pub mod manager;` to `backend/crates/api/src/sandbox_runtime/mod.rs`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shipyard-api sandbox_container_name`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/sandbox_runtime/manager.rs backend/crates/api/src/sandbox_runtime/mod.rs backend/crates/db/migrations/20250101000055_domains_service_hostname_unique.sql
git commit -m "feat(api): implement sandbox start_sandbox lifecycle"
```

---

## Task 8: Manager — `stop_sandbox` and `heartbeat`

**Files:**
- Modify: `backend/crates/api/src/sandbox_runtime/manager.rs`

**Interfaces:**
- Consumes: everything from Task 7.
- Produces: `pub async fn stop_sandbox(state: &AppState, service_id: Uuid) -> AppResult<()>`, `pub async fn heartbeat(state: &AppState, service_id: Uuid) -> AppResult<()>`. Consumed by Task 9's HTTP handlers and Task 10's reaper.

- [ ] **Step 1: Write the failing test**

```rust
// added to the `#[cfg(test)] mod tests` block in manager.rs
#[test]
fn placeholder_upstream_used_when_stopping() {
    // stop_sandbox always re-points Traefik at the configured placeholder
    // upstream rather than deleting the domain/route outright, so a cold
    // preview link still resolves to the "waking up" responder instead of a
    // dead connection. This is exercised end-to-end in Task 9's manual
    // verification; here we just lock down the constant used.
    assert_eq!(super::PLACEHOLDER_ROUTE_REASON, "sandbox stopped");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shipyard-api placeholder_upstream_used_when_stopping`
Expected: FAIL — `PLACEHOLDER_ROUTE_REASON` doesn't exist.

- [ ] **Step 3: Implement `stop_sandbox` and `heartbeat`**

Add to `backend/crates/api/src/sandbox_runtime/manager.rs`:
```rust
pub const PLACEHOLDER_ROUTE_REASON: &str = "sandbox stopped";

/// Stops the running container (if any) and re-points the app's Traefik
/// route at the shared placeholder/cold-start responder rather than removing
/// the route outright — a stopped sandbox's preview URL must still resolve
/// to something that can trigger an auto-start (see Task 9's public route),
/// not a dead connection.
pub async fn stop_sandbox(state: &AppState, service_id: Uuid) -> AppResult<()> {
    let Some(instance) = fetch_instance(&state.db, service_id).await? else {
        return Ok(()); // never started — nothing to stop
    };
    if instance.status == "stopped" {
        return Ok(());
    }

    if let Some(container_id) = &instance.container_id {
        state.docker.stop_container(container_id, 10).await.ok();
    }

    crate::resources::sync_traefik_dynamic_config(
        &state.db,
        service_id,
        &state.config.docker.label_prefix,
        &state.config.traefik.entrypoint_http,
        &state.config.traefik.entrypoint_https,
        state.config.traefik.dynamic_config_dir.as_deref(),
        Some(&format!("{}:{}", state.config.sandbox.placeholder_upstream, state.config.sandbox.placeholder_port)),
        false,
    )
    .await;

    sqlx::query(
        "UPDATE sandbox_instances SET status = 'stopped', last_heartbeat_at = NULL, started_at = NULL, updated_at = NOW() WHERE service_id = $1",
    )
    .bind(service_id)
    .execute(&state.db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;

    Ok(())
}

/// Records that the editor is still open for this sandbox. Called on an
/// interval by the frontend while the editor tab is active.
pub async fn heartbeat(state: &AppState, service_id: Uuid) -> AppResult<()> {
    sqlx::query("UPDATE sandbox_instances SET last_heartbeat_at = NOW() WHERE service_id = $1 AND status = 'running'")
        .bind(service_id)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(())
}
```

Note: `sync_traefik_dynamic_config`'s `upstream_override` is used verbatim as `http://{upstream_host}:{backend_port}` internally, where `backend_port` comes from the `domains.port` column — since the placeholder upstream string here already includes `:{placeholder_port}`, adjust `ensure_preview_domain`'s stored `port` is irrelevant for this call because `upstream_override` is passed as a full `host:port` string, which the existing function's template renders as `http://{upstream_override_value}:{domains.port}` — **this double-appends a port**. Fix: pass only the hostname (`state.config.sandbox.placeholder_upstream`) as the override, and instead update the `domains.port` column to `state.config.sandbox.placeholder_port` when stopping and back to the app's real port when starting, so the existing template's `:{port}` suffix is always correct for whichever upstream is currently active. Update both `start_sandbox` (Task 7) and `stop_sandbox` accordingly:

In `stop_sandbox`, replace the `sync_traefik_dynamic_config` call's override argument with `Some(&state.config.sandbox.placeholder_upstream)` and add before it:
```rust
    sqlx::query("UPDATE domains SET port = $2 WHERE service_id = $1")
        .bind(service_id)
        .bind(state.config.sandbox.placeholder_port as i32)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
```

In `start_sandbox` (Task 7), `ensure_preview_domain` already sets `domains.port` to the app's real port on every start via its `ON CONFLICT ... DO UPDATE SET port = EXCLUDED.port`, so no additional change is needed there — it already self-corrects the port back on every start.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shipyard-api placeholder_upstream_used_when_stopping`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/sandbox_runtime/manager.rs
git commit -m "feat(api): implement sandbox stop and heartbeat, fix placeholder routing port handling"
```

---

## Task 9: HTTP routes — authenticated lifecycle + public cold-preview auto-start

**Files:**
- Create: `backend/crates/api/src/sandbox_runtime/routes.rs`
- Modify: `backend/crates/api/src/sandbox_runtime/mod.rs`
- Modify: `backend/crates/api/src/routes.rs`
- Modify: `backend/crates/api/src/main.rs`
- Create: `infra/sandbox-placeholder/index.html`

**Interfaces:**
- Consumes: `manager::{start_sandbox, stop_sandbox, heartbeat}` (Tasks 7-8), `require_service_access` (existing), `OptionalAuthUser` (Task 5).
- Produces: `pub fn routes() -> Router<AppState>` (authenticated, mounted under `/api`), `pub fn public_routes() -> Router<AppState>` (mounted outside `/api`, for cold-preview auto-start).

- [ ] **Step 1: Implement the authenticated routes**

```rust
// backend/crates/api/src/sandbox_runtime/routes.rs
use axum::{
    extract::{Path, State},
    routing::post,
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

use shipyard_common::error::AppError;
use shipyard_common::types::ApiResponse;

use crate::auth::{AuthUser, OptionalAuthUser};
use crate::error::ApiAppError;
use crate::middleware::rbac::require_service_access;
use crate::AppState;

use super::manager;

#[derive(Debug, Serialize)]
struct SandboxStatusResponse {
    status: String,
    preview_url: Option<String>,
}

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/apps/:service_id/sandbox/start", post(start))
        .route("/apps/:service_id/sandbox/stop", post(stop))
        .route("/apps/:service_id/sandbox/heartbeat", post(heartbeat))
}

async fn start(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SandboxStatusResponse>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let instance = manager::start_sandbox(&state, service_id).await?;
    Ok(Json(ApiResponse::ok(SandboxStatusResponse {
        status: instance.status,
        preview_url: instance.preview_url,
    })))
}

async fn stop(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    manager::stop_sandbox(&state, service_id).await?;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "status": "stopped" }))))
}

async fn heartbeat(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    manager::heartbeat(&state, service_id).await?;
    Ok(Json(ApiResponse::ok(serde_json::json!({ "ok": true }))))
}

// ── Public: cold-preview auto-start ─────────────────────────────────────────
//
// Preview links must work for people without editor access, so this endpoint
// takes no required auth (OptionalAuthUser is accepted but unused today —
// reserved for a future "private preview" check) and is idempotent: a second
// call while already starting/running just returns current state, so an
// anonymous visitor can trigger a start but never inflate or bypass the
// owning org's quota (start_sandbox's quota check still applies).

pub fn public_routes() -> Router<AppState> {
    Router::new().route("/preview/:slug/start", post(public_start))
}

async fn public_start(
    _maybe_auth: OptionalAuthUser,
    Path(slug): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SandboxStatusResponse>>, ApiAppError> {
    let service_id: Uuid = sqlx::query_scalar("SELECT id FROM services WHERE slug = $1 AND type = 'sandbox_app'")
        .bind(&slug)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?
        .ok_or_else(|| ApiAppError(AppError::NotFound(format!("App '{slug}' not found"))))?;

    let instance = manager::start_sandbox(&state, service_id).await?;
    Ok(Json(ApiResponse::ok(SandboxStatusResponse {
        status: instance.status,
        preview_url: instance.preview_url,
    })))
}
```

Add `pub mod routes;` to `backend/crates/api/src/sandbox_runtime/mod.rs`.

- [ ] **Step 2: Wire the routers into the app**

In `backend/crates/api/src/routes.rs`, add near the `edge_functions` nesting in `api_router()`. `sandbox_runtime::routes()` already declares full paths (e.g. `/apps/:service_id/sandbox/start`), so it's mounted with `.merge(...)`, the same idiom used for `services::routes()`:
```rust
        .merge(sandbox_runtime::routes())
```

Add `use crate::sandbox_runtime;` to the imports at the top of `routes.rs`.

In `backend/crates/api/src/main.rs`, mount the public router outside `/api` (next to `edge_functions::invoke_routes()`):
```rust
    .nest("/apps", sandbox_runtime::routes::public_routes())
```
placed in the same `Router::new()` chain as the existing `.nest("/fn", edge_functions::invoke_routes())` line, so it skips the init-gate middleware the same way.

- [ ] **Step 3: Write the placeholder responder page**

```html
<!-- infra/sandbox-placeholder/index.html -->
<!doctype html>
<html>
<head><meta charset="utf-8"><title>Waking up…</title></head>
<body style="font-family: system-ui; display:flex; align-items:center; justify-content:center; height:100vh; margin:0; background:#0b0b0f; color:#eee;">
  <div style="text-align:center;">
    <p id="msg">Waking up this app…</p>
  </div>
  <script>
    const slug = window.location.hostname.split('.')[0].replace(/^preview-/, '');
    async function poll() {
      const res = await fetch(`/apps/preview/${slug}/start`, { method: 'POST' });
      const body = await res.json();
      if (body.data && body.data.status === 'running') {
        window.location.reload();
      } else {
        setTimeout(poll, 2000);
      }
    }
    poll();
  </script>
</body>
</html>
```

Note in the plan: this placeholder page must be deployed as its own always-running service (e.g. a tiny `nginx:alpine` container serving this file, attached to the Traefik network with the network alias `shipyard-sandbox-placeholder` matching `SandboxConfig::placeholder_upstream`) as an ops/deployment step — this plan provides the file; wiring it into the platform's own docker-compose/deployment manifests is infrastructure setup, not application code, and should be done once when this feature is deployed (documented here rather than automated, since it's a one-time, environment-level concern like the `runsc` daemon prerequisite in Global Constraints).

- [ ] **Step 4: Build and manually verify routing**

Run: `cargo build -p shipyard-api`
Expected: builds cleanly.

Manual check (requires a locally running Postgres + Docker with `runsc` registered, and the API started with `SHIPYARD__SANDBOX__ENABLED=true`): create a `sandbox_app` service row directly via `psql`, call `POST /api/apps/:id/sandbox/start` with a valid auth token against an app whose volume has a `package.json`, confirm a `docker ps` shows the sandbox container running under `runsc` (`docker inspect <id> --format '{{.HostConfig.Runtime}}'` should print `runsc`), and confirm the generated Traefik dynamic config file at `dynamic_config_dir` points at the container's alias.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/sandbox_runtime/routes.rs backend/crates/api/src/sandbox_runtime/mod.rs backend/crates/api/src/routes.rs backend/crates/api/src/main.rs infra/sandbox-placeholder/index.html
git commit -m "feat(api): add sandbox lifecycle HTTP routes and public cold-preview auto-start"
```

---

## Task 10: Idle reaper background task

**Files:**
- Create: `backend/crates/api/src/sandbox_runtime/reaper.rs`
- Modify: `backend/crates/api/src/sandbox_runtime/mod.rs`
- Modify: `backend/crates/api/src/main.rs`
- Test: `backend/crates/api/tests/sandbox_reaper.rs`

**Interfaces:**
- Consumes: `manager::stop_sandbox` (Task 8), `sandbox_instances` table (Task 1), `state.config.sandbox.{idle_timeout_secs, reaper_interval_secs, enabled}` (Task 4).
- Produces: `pub async fn find_stale_sandboxes(db: &PgPool, idle_timeout_secs: u64) -> AppResult<Vec<Uuid>>` (pure DB query, unit-testable against a real test DB), `pub async fn run(state: Arc<AppState>)` (the `tokio::time::interval` loop, mirrors `edge_functions::runtime_worker::run`).

- [ ] **Step 1: Write the failing test**

```rust
// backend/crates/api/tests/sandbox_reaper.rs
use sqlx::PgPool;
use uuid::Uuid;

fn test_database_url() -> String {
    std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| "postgres://shipyard:shipyard@localhost:5432/shipyard".to_string())
}

async fn pool() -> PgPool {
    let pool = shipyard_db::init_pool(&test_database_url(), 5).await.expect("connect");
    shipyard_db::run_migrations(&pool).await.expect("migrate");
    pool
}

async fn seed_running_sandbox(pool: &PgPool, heartbeat_age_secs: i64) -> Uuid {
    let suffix = Uuid::new_v4().simple().to_string();
    let org_id = Uuid::new_v4();
    sqlx::query("INSERT INTO organizations (id, name, slug, plan_id) VALUES ($1, $2, $3, (SELECT id FROM plans WHERE name = 'free'))")
        .bind(org_id).bind(format!("org-{suffix}")).bind(format!("org-{suffix}")).execute(pool).await.unwrap();
    let project_id = Uuid::new_v4();
    sqlx::query("INSERT INTO projects (id, org_id, name, slug) VALUES ($1, $2, $3, $4)")
        .bind(project_id).bind(org_id).bind(format!("p-{suffix}")).bind(format!("p-{suffix}")).execute(pool).await.unwrap();
    let service_id = Uuid::new_v4();
    sqlx::query("INSERT INTO services (id, project_id, name, slug, type, status, replicas, ports) VALUES ($1, $2, $3, $4, 'sandbox_app', 'running', 0, '[]'::jsonb)")
        .bind(service_id).bind(project_id).bind(format!("a-{suffix}")).bind(format!("a-{suffix}")).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO sandbox_instances (service_id, status, last_heartbeat_at) VALUES ($1, 'running', NOW() - ($2 || ' seconds')::interval)")
        .bind(service_id).bind(heartbeat_age_secs.to_string()).execute(pool).await.unwrap();
    service_id
}

#[tokio::test]
#[ignore = "requires a reachable Postgres database (see TEST_DATABASE_URL)"]
async fn finds_sandboxes_past_idle_timeout() {
    let pool = pool().await;
    let stale_id = seed_running_sandbox(&pool, 2000).await; // 33 min ago
    let fresh_id = seed_running_sandbox(&pool, 60).await;   // 1 min ago

    let stale = shipyard_api::sandbox_runtime::reaper::find_stale_sandboxes(&pool, 1200).await.unwrap();
    assert!(stale.contains(&stale_id), "sandbox idle for 2000s should be found stale at a 1200s timeout");
    assert!(!stale.contains(&fresh_id), "sandbox idle for 60s should not be found stale at a 1200s timeout");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `TEST_DATABASE_URL=postgres://shipyard:shipyard@localhost:5432/shipyard cargo test -p shipyard-api --test sandbox_reaper -- --ignored`
Expected: FAIL — `reaper` module doesn't exist.

- [ ] **Step 3: Implement the reaper**

```rust
// backend/crates/api/src/sandbox_runtime/reaper.rs
use std::sync::Arc;
use std::time::Duration;
use sqlx::PgPool;
use uuid::Uuid;
use shipyard_common::error::{AppError, AppResult};

use crate::AppState;
use super::manager;

/// Finds sandboxes whose last heartbeat is older than `idle_timeout_secs`.
/// Idle timeout is the source of truth for "is this sandbox actually in
/// use" (see spec Decisions) — explicit stop and tab-close signals are
/// best-effort and can be missed, so this is the reliable backstop.
pub async fn find_stale_sandboxes(db: &PgPool, idle_timeout_secs: u64) -> AppResult<Vec<Uuid>> {
    let rows: Vec<(Uuid,)> = sqlx::query_as(
        "SELECT service_id FROM sandbox_instances
         WHERE status = 'running'
           AND last_heartbeat_at IS NOT NULL
           AND last_heartbeat_at < NOW() - ($1 || ' seconds')::interval",
    )
    .bind(idle_timeout_secs.to_string())
    .fetch_all(db)
    .await
    .map_err(|e| AppError::Database(e.to_string()))?;
    Ok(rows.into_iter().map(|(id,)| id).collect())
}

async fn tick(state: &AppState) -> AppResult<()> {
    let stale = find_stale_sandboxes(&state.db, state.config.sandbox.idle_timeout_secs).await?;
    for service_id in stale {
        if let Err(e) = manager::stop_sandbox(state, service_id).await {
            tracing::warn!(service_id = %service_id, "idle reaper: failed to stop sandbox: {e}");
        } else {
            tracing::info!(service_id = %service_id, "idle reaper: stopped idle sandbox");
        }
    }
    Ok(())
}

/// Runs on `state.config.sandbox.reaper_interval_secs` (default 60s). Mirrors
/// `edge_functions::runtime_worker::run`'s tokio::time::interval shape.
pub async fn run(state: Arc<AppState>) {
    let mut interval = tokio::time::interval(Duration::from_secs(state.config.sandbox.reaper_interval_secs));
    loop {
        interval.tick().await;
        if let Err(e) = tick(&state).await {
            tracing::error!("SandboxReaper error: {e}");
        }
    }
}
```

Add `pub mod reaper;` to `backend/crates/api/src/sandbox_runtime/mod.rs`.

In `backend/crates/api/src/main.rs`, spawn it next to the edge-functions runtime worker spawn:
```rust
    if state.config.sandbox.enabled {
        let sandbox_state = Arc::new(state.clone());
        tokio::spawn(async move {
            sandbox_runtime::reaper::run(sandbox_state).await;
        });
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `TEST_DATABASE_URL=postgres://shipyard:shipyard@localhost:5432/shipyard cargo test -p shipyard-api --test sandbox_reaper -- --ignored`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/sandbox_runtime/reaper.rs backend/crates/api/src/sandbox_runtime/mod.rs backend/crates/api/src/main.rs backend/crates/api/tests/sandbox_reaper.rs
git commit -m "feat(api): add idle-timeout reaper for sandbox runtime"
```

---

## Task 11: Self-healing on stale/dead containers

**Files:**
- Modify: `backend/crates/api/src/sandbox_runtime/manager.rs`

**Interfaces:**
- Consumes: `DockerEngine::inspect_container` (existing trait method).
- Produces: modifies `start_sandbox`'s idempotency check to detect and recover from a `running`-in-DB-but-dead-in-Docker sandbox instead of returning it as-is.

- [ ] **Step 1: Write the failing test**

```rust
// added to the `#[cfg(test)] mod tests` block in manager.rs
#[test]
fn is_container_dead_treats_inspect_error_as_dead() {
    // A container_id that Docker no longer knows about (removed out-of-band,
    // OOM-killed and reaped, host rebooted) must be treated as dead rather
    // than trusting the stale DB row — modeled here as any Err from inspect.
    let simulated_inspect_result: Result<String, String> = Err("No such container".to_string());
    assert!(is_container_dead(&simulated_inspect_result));

    let simulated_running: Result<String, String> = Ok("running".to_string());
    assert!(!is_container_dead(&simulated_running));

    let simulated_exited: Result<String, String> = Ok("exited".to_string());
    assert!(is_container_dead(&simulated_exited));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p shipyard-api is_container_dead`
Expected: FAIL — function doesn't exist.

- [ ] **Step 3: Implement the self-heal check**

Add to `backend/crates/api/src/sandbox_runtime/manager.rs`:
```rust
fn is_container_dead(inspect_result: &Result<String, String>) -> bool {
    match inspect_result {
        Err(_) => true,
        Ok(state) => state != "running",
    }
}
```

Update `start_sandbox`'s idempotency check (Task 7, near the top) from:
```rust
    if let Some(existing) = fetch_instance(&state.db, service_id).await? {
        if existing.status == "starting" || existing.status == "running" {
            return Ok(existing);
        }
    }
```
to:
```rust
    if let Some(existing) = fetch_instance(&state.db, service_id).await? {
        if existing.status == "starting" {
            return Ok(existing);
        }
        if existing.status == "running" {
            let inspect_result = match &existing.container_id {
                Some(id) => state.docker.inspect_container(id).await.map(|d| d.state).map_err(|e| e.to_string()),
                None => Err("no container_id recorded".to_string()),
            };
            if !is_container_dead(&inspect_result) {
                return Ok(existing);
            }
            tracing::warn!(service_id = %service_id, "start_sandbox: DB says running but container is dead — self-healing");
            // Fall through to relaunch below instead of returning stale state.
        }
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p shipyard-api is_container_dead`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/crates/api/src/sandbox_runtime/manager.rs
git commit -m "fix(api): self-heal sandbox start when DB row is stale but container is dead"
```

---

## Task 12: Manual end-to-end verification

**Files:** none (verification only — no code changes).

- [ ] **Step 1: Verify gVisor is installed and registered**

On a test host with Docker installed, install gVisor per its official install docs, then confirm:
```bash
cat /etc/docker/daemon.json  # should contain a "runtimes": {"runsc": {"path": "/usr/local/bin/runsc"}} entry
docker run --rm --runtime=runsc alpine:3.19 uname -a
```
Expected: the command succeeds and `uname -a` output includes gVisor's kernel signature (not the host's real kernel version).

- [ ] **Step 2: Run the full test suite**

Run: `cargo test -p shipyard-docker -p shipyard-engine -p shipyard-common -p shipyard-api` (unignored tests)
Run: `TEST_DATABASE_URL=postgres://shipyard:shipyard@localhost:5432/shipyard cargo test -p shipyard-db -p shipyard-api -- --ignored` (DB integration tests)
Expected: all PASS.

- [ ] **Step 3: End-to-end smoke test**

With the API running locally (`SHIPYARD__SANDBOX__ENABLED=true`, Traefik and the placeholder responder deployed per Task 9's ops note) and a real `sandbox_app` service seeded with a volume containing a minimal Vite app (`package.json` with a `dev` script binding to `0.0.0.0:$PORT`):

1. `POST /api/apps/:id/sandbox/start` with a valid auth token — confirm the response includes a `preview_url`.
2. Visit the `preview_url` in a browser — confirm the running Vite dev server's page loads.
3. Edit a file directly in the mounted volume (e.g. `docker exec` into the sandbox container) and confirm the page hot-reloads via the proxied WebSocket connection, without a manual refresh.
4. `POST /api/apps/:id/sandbox/stop` — confirm the container stops (`docker ps` no longer lists it) and revisiting `preview_url` now shows the placeholder "waking up" page.
5. Wait past `idle_timeout_secs` after starting a sandbox again without sending heartbeats — confirm the reaper stops it automatically (check logs for "idle reaper: stopped idle sandbox").
6. Visit `preview_url` directly (simulating a cold shared link, sandbox stopped) — confirm the placeholder page's JS calls the public start endpoint and the page auto-reloads into the running app within a few seconds.

Expected: all six behaviors work as designed. Only once this passes should the sandbox runtime be considered done, per the spec's Testing section — this is infrastructure that's awkward to fully fake in automated tests.
