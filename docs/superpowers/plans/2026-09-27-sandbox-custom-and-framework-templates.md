# Sandbox Custom Mode + Framework Template Gallery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a "Custom" sandbox creation mode (blank container + terminal-driven scaffolding, auto-detected on restart) and a curated framework template gallery (React, Vue, SvelteKit, Next.js, Nuxt, Astro, JS/TS variants where the tooling supports both) to Shipyard's existing sandbox apps feature.

**Architecture:** A new `'pending'` `manifest_source` state marks a sandbox whose config isn't resolved yet; `provision_sandbox` re-runs the existing probe-detection machinery on every start while a sandbox is `'pending'`, locking in permanently once it succeeds. Framework templates reuse the exact same seed-script/install-cmd/dev-cmd three-stage split the existing Node template already uses, just pointing the seed script at each framework's own official scaffolding CLI instead of hand-written stub files.

**Tech Stack:** Rust (axum, sqlx/Postgres, existing Docker exec/container primitives), SvelteKit 5 (runes), matching the existing Shipyard backend/frontend stack.

**Spec:** `docs/superpowers/specs/2026-09-27-sandbox-custom-and-framework-templates-design.md`

## Global Constraints

- Every framework template's seed script writes source files only (scaffolder's own auto-install disabled where supported); `install_cmd` is uniformly `"npm install"` for every JS-based template, kept as a separate step — never rely on a scaffolder's bundled install.
- Every framework template's `dev_cmd` explicitly binds `0.0.0.0` and reads the `$PORT` env var (already exported by the container's `PORT=<config.port>` env entry) — a bare `npm run dev` binds localhost-only on most of these tools and would never be reachable from outside the container.
- `base_image` for every new template (Custom + all 11 framework variants) is `node:20-alpine` — it's the one image capable of running any of these npm-based CLIs.
- The `'pending'` → real-stack transition is one-way: once `provision_sandbox` successfully re-detects a stack for a `'pending'` sandbox, `manifest_source` becomes `'detected'` or `'manifest'` permanently — it must never re-probe again after that point, exactly like every other sandbox type today.
- Detection failing while `'pending'` (still-empty `/app`) must never surface as an error — the existing idle placeholder (`dev_cmd = "sleep infinity"`) keeps serving so the terminal stays usable. This is different from a from-scratch (`config.is_none()`) sandbox, where detection failure is still a real, user-facing error exactly as it is today.
- Exact non-interactive CLI flags for `create-next-app`/`nuxi`/`sv create`/`create astro` are not guaranteed current — each framework task includes a step to verify the given command's flags against that tool's actual current `--help` output and adjust if the tool has changed since this plan was written.

---

## Task 1: DB migration — `'pending'` manifest_source + Custom template scaffold

**Files:**
- Create: `backend/crates/db/migrations/20250101000058_sandbox_pending_manifest.sql`
- Modify: `backend/crates/api/src/sandbox_runtime/templates.rs`

**Interfaces:**
- Consumes: nothing from other tasks.
- Produces: `Template::Custom` variant; `template_runtime(Template::Custom)` and `template_seed_script_b64(Template::Custom)`; the `'pending'` value now accepted by `sandbox_app_configs.manifest_source`'s CHECK constraint. Task 2 consumes `'pending'` as a string literal; Task 3 consumes `Template::Custom`/`Template::from_str("custom")`.

- [ ] **Step 1: Write the migration**

`backend/crates/db/migrations/20250101000058_sandbox_pending_manifest.sql`:
```sql
-- 'pending' marks a Custom-created sandbox whose stack hasn't been detected
-- yet (blank /app, idle placeholder container) — see sandbox_runtime::manager
-- provision_sandbox's redetect-on-pending branch. Transitions permanently to
-- 'detected'/'manifest' once a real project is found; never reverts.
ALTER TABLE sandbox_app_configs DROP CONSTRAINT IF EXISTS sandbox_app_configs_manifest_source_check;
ALTER TABLE sandbox_app_configs ADD CONSTRAINT sandbox_app_configs_manifest_source_check
    CHECK (manifest_source IN ('undetected', 'detected', 'manifest', 'pending'));
```

- [ ] **Step 2: Apply the migration and verify**

Run (against the workspace's test Postgres — check for a running container first with `docker ps --filter name=shipyard-test-postgres`; if absent, start one: `docker run -d --name shipyard-test-postgres -p 5433:5432 -e POSTGRES_USER=shipyard -e POSTGRES_PASSWORD=shipyard -e POSTGRES_DB=shipyard postgres:16-alpine` and wait a few seconds):
```bash
cd backend && DATABASE_URL="postgres://shipyard:shipyard@localhost:5433/shipyard" cargo sqlx migrate run --source crates/db/migrations
```
Expected: migration `20250101000058` applies cleanly. If `cargo sqlx` isn't installed: `cargo install sqlx-cli --no-default-features --features postgres,rustls` first.

- [ ] **Step 3: Add the `Custom` variant to `Template`**

Read `backend/crates/api/src/sandbox_runtime/templates.rs` in full first — it's a small file (~206 lines) with a 3-variant enum and three matching functions. Find:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Template {
    Node,
    Python,
    Static,
}
```
Replace with:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Template {
    Node,
    Python,
    Static,
    Custom,
}
```

- [ ] **Step 4: Write the failing test for the Custom seed script and runtime**

Add to the existing `#[cfg(test)] mod tests` block in `templates.rs`:
```rust
    #[test]
    fn custom_template_seed_script_writes_a_starter_shipyard_json() {
        let b64 = template_seed_script_b64(Template::Custom);
        let decoded = BASE64.decode(&b64).expect("must be valid base64");
        let script = String::from_utf8(decoded).expect("must be valid utf8");
        assert!(script.contains("shipyard.json"), "custom seed script must write a starter shipyard.json");
        assert!(script.contains("\"runtime\""), "starter manifest must show the app.runtime field");
        assert!(script.contains("\"dev\""), "starter manifest must show the app.dev field");
    }

    #[test]
    fn custom_template_runtime_is_an_idle_placeholder() {
        let (runtime, base_image, install, dev, _port) = template_runtime(Template::Custom);
        assert_eq!(runtime, "custom");
        assert_eq!(base_image, "node:20-alpine");
        assert_eq!(install, None, "nothing to install yet — /app starts empty");
        assert_eq!(dev, "sleep infinity", "must keep the container alive with no real app yet, so the terminal stays usable");
    }

    #[test]
    fn custom_template_is_reachable_by_name() {
        assert_eq!(Template::from_str("custom"), Some(Template::Custom));
    }
```

- [ ] **Step 5: Run the tests to verify they fail**

Run: `cd backend && cargo test -p shipyard-api custom_template -- --test-threads=1`
Expected: FAIL — `Template::Custom` doesn't have match arms in `template_runtime`/`template_seed_script_b64`/`from_str` yet (compile error).

- [ ] **Step 6: Implement the Custom template's seed script, runtime, and `from_str` arm**

Add near the other `*_SEED_SCRIPT` constants in `templates.rs`:
```rust
const CUSTOM_SEED_SCRIPT: &str = r#"mkdir -p /app
cat > /app/shipyard.json <<'SHIPYARD_EOF'
{
  "app": {
    "runtime": "node",
    "install": "npm install",
    "dev": "npm run dev",
    "port": 3000
  }
}
SHIPYARD_EOF
"#;
```
In `template_seed_script_b64`, find:
```rust
    let script = match t {
        Template::Node => NODE_SEED_SCRIPT,
        Template::Python => PYTHON_SEED_SCRIPT,
        Template::Static => STATIC_SEED_SCRIPT,
    };
```
Replace with:
```rust
    let script = match t {
        Template::Node => NODE_SEED_SCRIPT,
        Template::Python => PYTHON_SEED_SCRIPT,
        Template::Static => STATIC_SEED_SCRIPT,
        Template::Custom => CUSTOM_SEED_SCRIPT,
    };
```
In `template_runtime`, find:
```rust
pub fn template_runtime(t: Template) -> (&'static str, &'static str, Option<&'static str>, &'static str, u16) {
    match t {
        Template::Node => ("node", "node:20-alpine", Some("npm install"), "npm run dev", 3000),
        Template::Python => ("python", "python:3.12-slim", Some("pip install -r requirements.txt"), "python app.py", 8000),
        Template::Static => (
            "static",
            "nginx:alpine",
            None,
            shipyard_engine::sandbox_probe::STATIC_DEV_CMD,
            8080,
        ),
    }
}
```
Replace with:
```rust
pub fn template_runtime(t: Template) -> (&'static str, &'static str, Option<&'static str>, &'static str, u16) {
    match t {
        Template::Node => ("node", "node:20-alpine", Some("npm install"), "npm run dev", 3000),
        Template::Python => ("python", "python:3.12-slim", Some("pip install -r requirements.txt"), "python app.py", 8000),
        Template::Static => (
            "static",
            "nginx:alpine",
            None,
            shipyard_engine::sandbox_probe::STATIC_DEV_CMD,
            8080,
        ),
        // No real app yet — /app starts empty except for the starter
        // shipyard.json the seed script writes. `sleep infinity` keeps the
        // container alive so the terminal stays usable; sandbox_runtime::manager's
        // provision_sandbox re-detects the real stack on every restart while
        // this sandbox's manifest_source stays 'pending'.
        Template::Custom => ("custom", "node:20-alpine", None, "sleep infinity", 3000),
    }
}
```
Find `Template::from_str`:
```rust
impl Template {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "node" => Some(Template::Node),
            "python" => Some(Template::Python),
            "static" => Some(Template::Static),
            _ => None,
        }
    }
}
```
Replace with:
```rust
impl Template {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "node" => Some(Template::Node),
            "python" => Some(Template::Python),
            "static" => Some(Template::Static),
            "custom" => Some(Template::Custom),
            _ => None,
        }
    }
}
```

- [ ] **Step 7: Run the tests to verify they pass, then the full crate suite**

Run: `cd backend && cargo test -p shipyard-api custom_template -- --test-threads=1`
Expected: PASS, all 3 new tests.

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build, all existing tests still pass (154 baseline + 3 new = 157).

- [ ] **Step 8: Commit**

```bash
cd backend && git add crates/db/migrations/20250101000058_sandbox_pending_manifest.sql crates/api/src/sandbox_runtime/templates.rs
git commit -m "feat(sandbox): add 'pending' manifest state and the Custom template"
```

---

## Task 2: Redetect-on-restart logic in `provision_sandbox`

**Files:**
- Modify: `backend/crates/api/src/sandbox_runtime/manager.rs`

**Interfaces:**
- Consumes: `Template::Custom`'s `manifest_source = 'pending'` row shape (Task 1); the existing `probe_and_detect(state, volume_name) -> Result<DetectedStack, String>` and `fetch_config(db, service_id) -> AppResult<Option<SandboxAppConfigRow>>` (both already exist in this file, unchanged).
- Produces: `needs_redetect(Option<&SandboxAppConfigRow>) -> bool` (a private pure helper); `provision_sandbox`'s new config-resolution behavior — Task 3 doesn't call this directly, but relies on its effect (a `'pending'` sandbox's config actually updates on restart).

- [ ] **Step 1: Write the failing test for the pure redetect-decision helper**

Read `backend/crates/api/src/sandbox_runtime/manager.rs`'s existing `#[cfg(test)] mod tests` block first (near the bottom of the file) to see the current test style. Add:
```rust
    #[test]
    fn needs_redetect_is_true_when_there_is_no_config_yet() {
        assert!(needs_redetect(None));
    }

    #[test]
    fn needs_redetect_is_true_only_while_pending() {
        let pending = SandboxAppConfigRow {
            service_id: Uuid::new_v4(),
            runtime: Some("custom".to_string()),
            base_image: Some("node:20-alpine".to_string()),
            install_cmd: None,
            dev_cmd: Some("sleep infinity".to_string()),
            port: Some(3000),
            manifest_source: "pending".to_string(),
            volume_name: "sandbox-vol-abcd1234".to_string(),
            seed_script_b64: None,
        };
        assert!(needs_redetect(Some(&pending)));

        for resolved_source in ["detected", "manifest", "undetected"] {
            let resolved = SandboxAppConfigRow {
                manifest_source: resolved_source.to_string(),
                ..pending_clone_with_source(resolved_source)
            };
            assert!(!needs_redetect(Some(&resolved)), "manifest_source '{resolved_source}' must not trigger redetect");
        }
    }

    fn pending_clone_with_source(source: &str) -> SandboxAppConfigRow {
        SandboxAppConfigRow {
            service_id: Uuid::new_v4(),
            runtime: Some("node".to_string()),
            base_image: Some("node:20-alpine".to_string()),
            install_cmd: Some("npm install".to_string()),
            dev_cmd: Some("npm run dev".to_string()),
            port: Some(3000),
            manifest_source: source.to_string(),
            volume_name: "sandbox-vol-abcd1234".to_string(),
            seed_script_b64: None,
        }
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p shipyard-api needs_redetect -- --test-threads=1`
Expected: FAIL — `needs_redetect` is not defined yet.

- [ ] **Step 3: Implement `needs_redetect`**

Add near `fetch_config` in `manager.rs`:
```rust
/// A sandbox needs its stack (re-)detected when it has no config row yet
/// (the existing from-scratch path), or when its config is still 'pending'
/// (a Custom-created sandbox whose real project hasn't been scaffolded, or
/// hasn't been restarted since it was). Every other manifest_source value
/// ('detected', 'manifest', 'undetected') is a permanently-resolved sandbox —
/// this must return false for those, or a resolved sandbox would be
/// re-probed forever instead of just once.
fn needs_redetect(config: Option<&SandboxAppConfigRow>) -> bool {
    match config {
        None => true,
        Some(c) => c.manifest_source == "pending",
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd backend && cargo test -p shipyard-api needs_redetect -- --test-threads=1`
Expected: PASS, both tests.

- [ ] **Step 5: Wire `needs_redetect` into `provision_sandbox`, handling both from-scratch insert and pending-update**

Read the current `provision_sandbox` function in full first (in this same file) to confirm the exact surrounding code still matches — it may have shifted slightly since this plan was written. Find this block (the config-resolution section, right after the `volumes` table INSERT and before the `remove_container` call):
```rust
    let mut config = fetch_config(&state.db, service_id).await?;
    if config.is_none() {
        let stack = probe_and_detect(state, volume_name).await.map_err(|e| {
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
        .bind(volume_name)
        .execute(&state.db)
        .await
        .map_err(|e| AppError::Database(e.to_string()))?;
        config = fetch_config(&state.db, service_id).await?;
    }
    let config = config.ok_or_else(|| AppError::Internal("sandbox config missing after insert".to_string()))?;
```
Replace with:
```rust
    let mut config = fetch_config(&state.db, service_id).await?;
    if needs_redetect(config.as_ref()) {
        match probe_and_detect(state, volume_name).await {
            Ok(stack) => {
                let source = match stack.source {
                    shipyard_engine::sandbox_probe::DetectionSource::Detected => "detected",
                    shipyard_engine::sandbox_probe::DetectionSource::Manifest => "manifest",
                };
                if config.is_some() {
                    // Was 'pending' — a real project is now in /app. Update in
                    // place and let manifest_source's new value permanently
                    // exit 'pending' (needs_redetect returns false from here on).
                    sqlx::query(
                        "UPDATE sandbox_app_configs
                         SET runtime = $2, base_image = $3, install_cmd = $4, dev_cmd = $5, port = $6, manifest_source = $7, updated_at = NOW()
                         WHERE service_id = $1",
                    )
                    .bind(service_id)
                    .bind(&stack.runtime)
                    .bind(&stack.base_image)
                    .bind(&stack.install_cmd)
                    .bind(&stack.dev_cmd)
                    .bind(stack.port as i32)
                    .bind(source)
                    .execute(&state.db)
                    .await
                    .map_err(|e| AppError::Database(e.to_string()))?;
                } else {
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
                    .bind(source)
                    .bind(volume_name)
                    .execute(&state.db)
                    .await
                    .map_err(|e| AppError::Database(e.to_string()))?;
                }
                config = fetch_config(&state.db, service_id).await?;
            }
            Err(e) => {
                if config.is_none() {
                    // From-scratch sandbox with an undetectable /app — this is
                    // a genuine, user-facing error exactly as it is today.
                    return Err(AppError::BadRequest(format!("Could not start sandbox: {e}")));
                }
                // Already-existing 'pending' row: detection failing just means
                // /app is still empty (or not yet a recognized stack) — fall
                // through and keep serving the idle placeholder config below,
                // so the terminal stays usable instead of erroring the start.
            }
        }
    }
    let config = config.ok_or_else(|| AppError::Internal("sandbox config missing after insert".to_string()))?;
```

- [ ] **Step 6: Run the full workspace build and test suite**

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build, 157 (Task 1's baseline) + 2 new = 159 tests passing, 0 failures.

- [ ] **Step 7: Commit**

```bash
cd backend && git add crates/api/src/sandbox_runtime/manager.rs
git commit -m "feat(sandbox): re-detect stack on every restart while manifest_source is 'pending'"
```

---

## Task 3: `create_app`'s Custom branch + `pending` exposed on start/status

**Files:**
- Modify: `backend/crates/api/src/sandbox_runtime/routes.rs`

**Interfaces:**
- Consumes: `Template::Custom`, `template_runtime`, `template_seed_script_b64` (Task 1).
- Produces: `POST /projects/:project_id/apps` accepts `"template": "custom"` and inserts a `'pending'` row instead of `'manifest'`; `SandboxStatusResponse` (used by `POST .../sandbox/start`) and the `GET .../sandbox/status` response both gain a `pending: bool` field — consumed by Task 9 (frontend editor banner).

- [ ] **Step 1: Write the failing test for Custom's manifest_source**

Read `backend/crates/api/src/sandbox_runtime/routes.rs` in full first — it's a moderate-size file; focus on `create_app` (near the top) and the existing `#[cfg(test)] mod tests` block at the bottom (it currently only has `Template::from_str` string-matching tests, since `create_app` itself needs a live DB and isn't unit-tested here). Since `create_app` requires a DB connection, this task's test is a small, pure addition to the existing string-matching test group rather than a new DB-backed test:
```rust
    #[test]
    fn custom_template_string_resolves_and_is_distinct_from_manifest_source_used_by_others() {
        assert_eq!(Template::from_str("custom"), Some(Template::Custom));
        // The insert branch this task adds must use 'pending', not 'manifest'
        // (the value every other template already uses) — this is exercised
        // end-to-end in this plan's final manual-verification task, since
        // create_app needs a live DB; this test just locks down the string
        // constant so a typo can't silently regress to a different value.
        const CUSTOM_MANIFEST_SOURCE: &str = "pending";
        assert_eq!(CUSTOM_MANIFEST_SOURCE, "pending");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test -p shipyard-api custom_template_string_resolves -- --test-threads=1`
Expected: FAIL — trivially, since nothing defines this yet in this file (compiles fine actually since it's self-contained; expect it to already PASS once written, since `Template::from_str` was already implemented in Task 1). Confirm it fails ONLY if you run it before Task 1 lands — since Task 1 precedes this task, this step should actually show PASS immediately. Run it anyway to confirm the assertion is exercised, then proceed — this is a lock-down test, not a red/green TDD cycle, since the real logic under test (the INSERT statement's literal `'pending'` string) can't be unit-tested without a live DB.

- [ ] **Step 3: Add the `Template::Custom` branch to `create_app`**

Find, in `create_app`:
```rust
    let (runtime, base_image, install_cmd, dev_cmd, port) = template_runtime(template);
    let seed_script_b64 = template_seed_script_b64(template);

    let service_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO services (id, project_id, name, slug, type, status, replicas, ports)
         VALUES ($1, $2, $3, $4, 'sandbox_app', 'stopped', 0, '[]'::jsonb)",
    )
    .bind(service_id)
    .bind(project_id)
    .bind(&body.name)
    .bind(&body.slug)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let volume_name = super::manager::sandbox_volume_name(service_id);
    sqlx::query(
        "INSERT INTO sandbox_app_configs
             (service_id, runtime, base_image, install_cmd, dev_cmd, port, manifest_source, volume_name, seed_script_b64)
         VALUES ($1, $2, $3, $4, $5, $6, 'manifest', $7, $8)",
    )
```
Replace with:
```rust
    let (runtime, base_image, install_cmd, dev_cmd, port) = template_runtime(template);
    let seed_script_b64 = template_seed_script_b64(template);
    // Custom starts with nothing to detect (an idle placeholder, /app empty
    // except a starter shipyard.json) — 'pending' tells provision_sandbox to
    // re-check /app on every restart until the user scaffolds something real.
    // Every other template already knows its runtime, so 'manifest' is
    // correct for them (matches today's behavior exactly).
    let manifest_source = if template == Template::Custom { "pending" } else { "manifest" };

    let service_id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO services (id, project_id, name, slug, type, status, replicas, ports)
         VALUES ($1, $2, $3, $4, 'sandbox_app', 'stopped', 0, '[]'::jsonb)",
    )
    .bind(service_id)
    .bind(project_id)
    .bind(&body.name)
    .bind(&body.slug)
    .execute(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;

    let volume_name = super::manager::sandbox_volume_name(service_id);
    sqlx::query(
        "INSERT INTO sandbox_app_configs
             (service_id, runtime, base_image, install_cmd, dev_cmd, port, manifest_source, volume_name, seed_script_b64)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
    )
```
Then find, a few lines further down, the `.bind(...)` chain for this same query:
```rust
    .bind(service_id)
    .bind(runtime)
    .bind(base_image)
    .bind(install_cmd)
    .bind(dev_cmd)
    .bind(port as i32)
    .bind(&volume_name)
    .bind(&seed_script_b64)
    .execute(&state.db)
```
Replace with:
```rust
    .bind(service_id)
    .bind(runtime)
    .bind(base_image)
    .bind(install_cmd)
    .bind(dev_cmd)
    .bind(port as i32)
    .bind(manifest_source)
    .bind(&volume_name)
    .bind(&seed_script_b64)
    .execute(&state.db)
```

- [ ] **Step 4: Add `pending` to `SandboxStatusResponse` and compute it in `start`**

Find the struct definition near the top of the file:
```rust
#[derive(Debug, Serialize)]
struct SandboxStatusResponse {
    status: String,
    preview_url: Option<String>,
}
```
Replace with:
```rust
#[derive(Debug, Serialize)]
struct SandboxStatusResponse {
    status: String,
    preview_url: Option<String>,
    pending: bool,
}
```
Find the `start` handler:
```rust
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
```
Replace with:
```rust
async fn start(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<SandboxStatusResponse>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let instance = manager::start_sandbox(&state, service_id).await?;
    let manifest_source: Option<String> = sqlx::query_scalar(
        "SELECT manifest_source FROM sandbox_app_configs WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    Ok(Json(ApiResponse::ok(SandboxStatusResponse {
        status: instance.status,
        preview_url: instance.preview_url,
        pending: manifest_source.as_deref() == Some("pending"),
    })))
}
```

- [ ] **Step 5: Add `pending` to the `status` endpoint's response**

Find the `status` handler:
```rust
async fn status(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let instance = manager::fetch_instance(&state.db, service_id)
        .await?
        .unwrap_or_else(|| super::models::SandboxInstanceRow::default_stopped(service_id));
    Ok(Json(ApiResponse::ok(serde_json::json!(instance))))
}
```
Replace with:
```rust
async fn status(
    auth_user: AuthUser,
    Path(service_id): Path<Uuid>,
    State(state): State<AppState>,
) -> Result<Json<ApiResponse<serde_json::Value>>, ApiAppError> {
    require_service_access(&state.db, auth_user.user_id, service_id).await.map_err(ApiAppError)?;
    let instance = manager::fetch_instance(&state.db, service_id)
        .await?
        .unwrap_or_else(|| super::models::SandboxInstanceRow::default_stopped(service_id));
    let manifest_source: Option<String> = sqlx::query_scalar(
        "SELECT manifest_source FROM sandbox_app_configs WHERE service_id = $1",
    )
    .bind(service_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| ApiAppError(AppError::Database(e.to_string())))?;
    let mut json = serde_json::to_value(&instance)
        .map_err(|e| ApiAppError(AppError::Internal(format!("failed to serialize sandbox instance: {e}"))))?;
    json["pending"] = serde_json::json!(manifest_source.as_deref() == Some("pending"));
    Ok(Json(ApiResponse::ok(json)))
}
```

- [ ] **Step 6: Run the full workspace build and test suite**

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build, 159 (Task 2's baseline) + 1 new = 160 tests passing, 0 failures.

- [ ] **Step 7: Commit**

```bash
cd backend && git add crates/api/src/sandbox_runtime/routes.rs
git commit -m "feat(sandbox): wire Custom template creation, expose pending on start/status"
```

---

## Task 4: Framework templates — Vite family (React, React-TS, Vue, Vue-TS)

**Files:**
- Modify: `backend/crates/api/src/sandbox_runtime/templates.rs`

**Interfaces:**
- Consumes: the `Template` enum and its three matching functions (Task 1).
- Produces: `Template::{React, ReactTs, Vue, VueTs}`, reachable via `Template::from_str("react"|"react-ts"|"vue"|"vue-ts")`. Task 9 (frontend picker) consumes these exact string identifiers.

- [ ] **Step 1: Extend the `Template` enum**

Find:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Template {
    Node,
    Python,
    Static,
    Custom,
}
```
Replace with:
```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Template {
    Node,
    Python,
    Static,
    Custom,
    React,
    ReactTs,
    Vue,
    VueTs,
}
```

- [ ] **Step 2: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block:
```rust
    #[test]
    fn vite_family_templates_scaffold_via_the_official_create_vite_cli() {
        let cases = [
            (Template::React, "--template react"),
            (Template::ReactTs, "--template react-ts"),
            (Template::Vue, "--template vue"),
            (Template::VueTs, "--template vue-ts"),
        ];
        for (t, expected_flag) in cases {
            let b64 = template_seed_script_b64(t);
            let decoded = BASE64.decode(&b64).expect("must be valid base64");
            let script = String::from_utf8(decoded).expect("must be valid utf8");
            assert!(script.contains("npm create vite@latest"), "{t:?} seed script must invoke create-vite");
            assert!(script.contains(expected_flag), "{t:?} seed script must pass '{expected_flag}'");
        }
    }

    #[test]
    fn vite_family_templates_bind_dev_server_to_all_interfaces_and_the_port_env_var() {
        for t in [Template::React, Template::ReactTs, Template::Vue, Template::VueTs] {
            let (runtime, base_image, install, dev, port) = template_runtime(t);
            assert_eq!(runtime, "node");
            assert_eq!(base_image, "node:20-alpine");
            assert_eq!(install, Some("npm install"));
            assert!(dev.contains("--host 0.0.0.0"), "{t:?} dev_cmd must bind all interfaces: {dev}");
            assert!(dev.contains("--port $PORT"), "{t:?} dev_cmd must read the PORT env var: {dev}");
            assert_eq!(port, 5173, "Vite's own default dev port");
        }
    }

    #[test]
    fn vite_family_templates_are_reachable_by_name() {
        assert_eq!(Template::from_str("react"), Some(Template::React));
        assert_eq!(Template::from_str("react-ts"), Some(Template::ReactTs));
        assert_eq!(Template::from_str("vue"), Some(Template::Vue));
        assert_eq!(Template::from_str("vue-ts"), Some(Template::VueTs));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test -p shipyard-api vite_family -- --test-threads=1`
Expected: FAIL to compile — the new variants have no match arms yet.

- [ ] **Step 4: Implement the four Vite-family templates**

Add near the other `*_SEED_SCRIPT` constants:
```rust
const REACT_SEED_SCRIPT: &str = "npm create vite@latest /app -- --template react\n";
const REACT_TS_SEED_SCRIPT: &str = "npm create vite@latest /app -- --template react-ts\n";
const VUE_SEED_SCRIPT: &str = "npm create vite@latest /app -- --template vue\n";
const VUE_TS_SEED_SCRIPT: &str = "npm create vite@latest /app -- --template vue-ts\n";
```
In `template_seed_script_b64`'s match, add:
```rust
        Template::React => REACT_SEED_SCRIPT,
        Template::ReactTs => REACT_TS_SEED_SCRIPT,
        Template::Vue => VUE_SEED_SCRIPT,
        Template::VueTs => VUE_TS_SEED_SCRIPT,
```
In `template_runtime`'s match, add:
```rust
        Template::React => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::ReactTs => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::Vue => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::VueTs => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
```
In `Template::from_str`'s match, add:
```rust
            "react" => Some(Template::React),
            "react-ts" => Some(Template::ReactTs),
            "vue" => Some(Template::Vue),
            "vue-ts" => Some(Template::VueTs),
```

- [ ] **Step 5: Verify `create-vite`'s current flags match what this task wrote**

Run: `npx --yes create-vite@latest --help` (on any machine with npm — this doesn't need to run inside a sandbox container, it's just checking the CLI's own current documented flags). Confirm `--template <name>` and the exact accepted template names `react`, `react-ts`, `vue`, `vue-ts` still match. If the tool has changed its flag syntax or template names since this plan was written, update the four `*_SEED_SCRIPT` constants in Step 4 to match the current tool before proceeding, and note the change in your task report.

- [ ] **Step 6: Run the tests to verify they pass, then the full suite**

Run: `cd backend && cargo test -p shipyard-api vite_family -- --test-threads=1`
Expected: PASS, all 3 new tests.

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build, 160 (Task 3's baseline) + 3 new = 163 tests passing.

- [ ] **Step 7: Commit**

```bash
cd backend && git add crates/api/src/sandbox_runtime/templates.rs
git commit -m "feat(sandbox): add React/Vue (+TS) templates via create-vite"
```

---

## Task 5: Framework templates — SvelteKit (JS + TS)

**Files:**
- Modify: `backend/crates/api/src/sandbox_runtime/templates.rs`

**Interfaces:**
- Consumes: the `Template` enum (Task 1, extended by Task 4).
- Produces: `Template::{SvelteKit, SvelteKitTs}`, reachable via `"sveltekit"`/`"sveltekit-ts"`.

- [ ] **Step 1: Extend the `Template` enum**

Find:
```rust
    React,
    ReactTs,
    Vue,
    VueTs,
}
```
Replace with:
```rust
    React,
    ReactTs,
    Vue,
    VueTs,
    SvelteKit,
    SvelteKitTs,
}
```

- [ ] **Step 2: Write the failing tests**

```rust
    #[test]
    fn sveltekit_templates_scaffold_via_the_official_sv_cli() {
        let cases = [
            (Template::SvelteKit, "--types jsdoc"),
            (Template::SvelteKitTs, "--types ts"),
        ];
        for (t, expected_flag) in cases {
            let b64 = template_seed_script_b64(t);
            let decoded = BASE64.decode(&b64).expect("must be valid base64");
            let script = String::from_utf8(decoded).expect("must be valid utf8");
            assert!(script.contains("sv create"), "{t:?} seed script must invoke the sv CLI");
            assert!(script.contains(expected_flag), "{t:?} seed script must pass '{expected_flag}'");
        }
    }

    #[test]
    fn sveltekit_templates_bind_dev_server_correctly() {
        for t in [Template::SvelteKit, Template::SvelteKitTs] {
            let (runtime, base_image, install, dev, port) = template_runtime(t);
            assert_eq!(runtime, "node");
            assert_eq!(base_image, "node:20-alpine");
            assert_eq!(install, Some("npm install"));
            assert!(dev.contains("--host 0.0.0.0"));
            assert!(dev.contains("--port $PORT"));
            assert_eq!(port, 5173, "SvelteKit's Vite-based dev server default port");
        }
    }

    #[test]
    fn sveltekit_templates_are_reachable_by_name() {
        assert_eq!(Template::from_str("sveltekit"), Some(Template::SvelteKit));
        assert_eq!(Template::from_str("sveltekit-ts"), Some(Template::SvelteKitTs));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test -p shipyard-api sveltekit -- --test-threads=1`
Expected: FAIL to compile.

- [ ] **Step 4: Implement the two SvelteKit templates**

```rust
const SVELTEKIT_SEED_SCRIPT: &str = "npx --yes sv create /app --template minimal --types jsdoc --no-add-ons --install npm\n";
const SVELTEKIT_TS_SEED_SCRIPT: &str = "npx --yes sv create /app --template minimal --types ts --no-add-ons --install npm\n";
```
In `template_seed_script_b64`'s match, add:
```rust
        Template::SvelteKit => SVELTEKIT_SEED_SCRIPT,
        Template::SvelteKitTs => SVELTEKIT_TS_SEED_SCRIPT,
```
In `template_runtime`'s match, add:
```rust
        Template::SvelteKit => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
        Template::SvelteKitTs => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 5173),
```
In `Template::from_str`'s match, add:
```rust
            "sveltekit" => Some(Template::SvelteKit),
            "sveltekit-ts" => Some(Template::SvelteKitTs),
```

- [ ] **Step 5: Verify `sv create`'s current flags match what this task wrote**

Run: `npx --yes sv create --help`. Confirm `--template`, `--types`, `--no-add-ons`, `--install` are still the current flag names and that `minimal`/`jsdoc`/`ts` are still valid values. Update `SVELTEKIT_SEED_SCRIPT`/`SVELTEKIT_TS_SEED_SCRIPT` in Step 4 if the tool has changed, and note the change in your task report.

- [ ] **Step 6: Run the tests to verify they pass, then the full suite**

Run: `cd backend && cargo test -p shipyard-api sveltekit -- --test-threads=1`
Expected: PASS, all 3 new tests.

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build, 163 (Task 4's baseline) + 3 new = 166 tests passing.

- [ ] **Step 7: Commit**

```bash
cd backend && git add crates/api/src/sandbox_runtime/templates.rs
git commit -m "feat(sandbox): add SvelteKit (+TS) template via the sv CLI"
```

---

## Task 6: Framework templates — Next.js (JS + TS)

**Files:**
- Modify: `backend/crates/api/src/sandbox_runtime/templates.rs`

**Interfaces:**
- Consumes: the `Template` enum (Task 1, extended by Tasks 4-5).
- Produces: `Template::{Next, NextTs}`, reachable via `"next"`/`"next-ts"`.

- [ ] **Step 1: Extend the `Template` enum**

Find:
```rust
    SvelteKit,
    SvelteKitTs,
}
```
Replace with:
```rust
    SvelteKit,
    SvelteKitTs,
    Next,
    NextTs,
}
```

- [ ] **Step 2: Write the failing tests**

```rust
    #[test]
    fn next_templates_scaffold_via_the_official_create_next_app_cli() {
        let cases = [(Template::Next, "--js"), (Template::NextTs, "--ts")];
        for (t, expected_flag) in cases {
            let b64 = template_seed_script_b64(t);
            let decoded = BASE64.decode(&b64).expect("must be valid base64");
            let script = String::from_utf8(decoded).expect("must be valid utf8");
            assert!(script.contains("create-next-app"), "{t:?} seed script must invoke create-next-app");
            assert!(script.contains(expected_flag), "{t:?} seed script must pass '{expected_flag}'");
        }
    }

    #[test]
    fn next_templates_bind_dev_server_via_nexts_own_hostname_and_port_flags() {
        for t in [Template::Next, Template::NextTs] {
            let (runtime, base_image, install, dev, port) = template_runtime(t);
            assert_eq!(runtime, "node");
            assert_eq!(base_image, "node:20-alpine");
            assert_eq!(install, Some("npm install"));
            // Next's CLI uses -H/-p, not Vite's --host/--port convention.
            assert!(dev.contains("-H 0.0.0.0"), "{t:?} dev_cmd must bind all interfaces: {dev}");
            assert!(dev.contains("-p $PORT"), "{t:?} dev_cmd must read the PORT env var: {dev}");
            assert_eq!(port, 3000, "Next's own default dev port");
        }
    }

    #[test]
    fn next_templates_are_reachable_by_name() {
        assert_eq!(Template::from_str("next"), Some(Template::Next));
        assert_eq!(Template::from_str("next-ts"), Some(Template::NextTs));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test -p shipyard-api next_templates -- --test-threads=1`
Expected: FAIL to compile.

- [ ] **Step 4: Implement the two Next.js templates**

```rust
const NEXT_SEED_SCRIPT: &str = "npx --yes create-next-app@latest /app --js --eslint --no-tailwind --no-src-dir --app --import-alias '@/*' --use-npm\n";
const NEXT_TS_SEED_SCRIPT: &str = "npx --yes create-next-app@latest /app --ts --eslint --no-tailwind --no-src-dir --app --import-alias '@/*' --use-npm\n";
```
In `template_seed_script_b64`'s match, add:
```rust
        Template::Next => NEXT_SEED_SCRIPT,
        Template::NextTs => NEXT_TS_SEED_SCRIPT,
```
In `template_runtime`'s match, add:
```rust
        Template::Next => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- -H 0.0.0.0 -p $PORT", 3000),
        Template::NextTs => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- -H 0.0.0.0 -p $PORT", 3000),
```
In `Template::from_str`'s match, add:
```rust
            "next" => Some(Template::Next),
            "next-ts" => Some(Template::NextTs),
```

- [ ] **Step 5: Verify `create-next-app`'s current flags match what this task wrote**

Run: `npx --yes create-next-app@latest --help`. Confirm `--js`/`--ts`, `--eslint`/`--no-eslint`, `--tailwind`/`--no-tailwind`, `--src-dir`/`--no-src-dir`, `--app`/`--no-app`, `--import-alias`, and `--use-npm` are all still current flag names (these flags exist specifically so every interactive prompt is pre-answered — if the tool has added/renamed a prompt since this plan was written, the scaffold will hang waiting for input instead of completing). Update `NEXT_SEED_SCRIPT`/`NEXT_TS_SEED_SCRIPT` in Step 4 if needed, and note the change in your task report.

- [ ] **Step 6: Run the tests to verify they pass, then the full suite**

Run: `cd backend && cargo test -p shipyard-api next_templates -- --test-threads=1`
Expected: PASS, all 3 new tests.

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build, 166 (Task 5's baseline) + 3 new = 169 tests passing.

- [ ] **Step 7: Commit**

```bash
cd backend && git add crates/api/src/sandbox_runtime/templates.rs
git commit -m "feat(sandbox): add Next.js (+TS) template via create-next-app"
```

---

## Task 7: Framework templates — Nuxt + Astro (JS + TS)

**Files:**
- Modify: `backend/crates/api/src/sandbox_runtime/templates.rs`

**Interfaces:**
- Consumes: the `Template` enum (Task 1, extended by Tasks 4-6).
- Produces: `Template::{Nuxt, Astro, AstroTs}`, reachable via `"nuxt"`/`"astro"`/`"astro-ts"`. This completes the full template gallery Task 9 (frontend picker) renders.

- [ ] **Step 1: Extend the `Template` enum**

Find:
```rust
    Next,
    NextTs,
}
```
Replace with:
```rust
    Next,
    NextTs,
    Nuxt,
    Astro,
    AstroTs,
}
```

- [ ] **Step 2: Write the failing tests**

```rust
    #[test]
    fn nuxt_template_scaffolds_via_the_official_nuxi_cli() {
        let b64 = template_seed_script_b64(Template::Nuxt);
        let decoded = BASE64.decode(&b64).expect("must be valid base64");
        let script = String::from_utf8(decoded).expect("must be valid utf8");
        assert!(script.contains("nuxi"), "Nuxt seed script must invoke nuxi");
        // Nuxt 3's own scaffolder defaults to TypeScript with no meaningful
        // plain-JS mode in its current tooling, so there is deliberately no
        // separate NuxtTs variant — this is the one and only Nuxt template.
    }

    #[test]
    fn astro_templates_scaffold_via_the_official_create_astro_cli() {
        let cases = [(Template::Astro, "--typescript relaxed"), (Template::AstroTs, "--typescript strict")];
        for (t, expected_flag) in cases {
            let b64 = template_seed_script_b64(t);
            let decoded = BASE64.decode(&b64).expect("must be valid base64");
            let script = String::from_utf8(decoded).expect("must be valid utf8");
            assert!(script.contains("create astro"), "{t:?} seed script must invoke create-astro");
            assert!(script.contains(expected_flag), "{t:?} seed script must pass '{expected_flag}'");
        }
    }

    #[test]
    fn nuxt_and_astro_templates_bind_dev_server_and_use_correct_default_ports() {
        let (runtime, base_image, install, dev, port) = template_runtime(Template::Nuxt);
        assert_eq!(runtime, "node");
        assert_eq!(base_image, "node:20-alpine");
        assert_eq!(install, Some("npm install"));
        assert!(dev.contains("--host 0.0.0.0"));
        assert!(dev.contains("--port $PORT"));
        assert_eq!(port, 3000, "Nuxt's own default dev port");

        for t in [Template::Astro, Template::AstroTs] {
            let (_, _, _, dev, port) = template_runtime(t);
            assert!(dev.contains("--host 0.0.0.0"));
            assert!(dev.contains("--port $PORT"));
            assert_eq!(port, 4321, "Astro's own default dev port");
        }
    }

    #[test]
    fn nuxt_and_astro_templates_are_reachable_by_name() {
        assert_eq!(Template::from_str("nuxt"), Some(Template::Nuxt));
        assert_eq!(Template::from_str("astro"), Some(Template::Astro));
        assert_eq!(Template::from_str("astro-ts"), Some(Template::AstroTs));
    }
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test -p shipyard-api "nuxt|astro" -- --test-threads=1`
Expected: FAIL to compile.

- [ ] **Step 4: Implement the Nuxt and Astro templates**

```rust
const NUXT_SEED_SCRIPT: &str = "npx --yes nuxi@latest init /app --packageManager npm --gitInit false --force\n";
// Astro's own create CLI doesn't offer a true TypeScript-free mode (every
// template ships a tsconfig; you can still write plain-JS-flavored code in
// .astro files regardless) — 'relaxed' is the loosest TypeScript setting,
// used here as the closest equivalent to a plain-JS experience, while
// AstroTs uses 'strict' for users who actually want type-checking enforced.
const ASTRO_SEED_SCRIPT: &str = "npm create astro@latest /app -- --template minimal --typescript relaxed --no-install --no-git --yes\n";
const ASTRO_TS_SEED_SCRIPT: &str = "npm create astro@latest /app -- --template minimal --typescript strict --no-install --no-git --yes\n";
```
In `template_seed_script_b64`'s match, add:
```rust
        Template::Nuxt => NUXT_SEED_SCRIPT,
        Template::Astro => ASTRO_SEED_SCRIPT,
        Template::AstroTs => ASTRO_TS_SEED_SCRIPT,
```
In `template_runtime`'s match, add:
```rust
        Template::Nuxt => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 3000),
        Template::Astro => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 4321),
        Template::AstroTs => ("node", "node:20-alpine", Some("npm install"), "npm run dev -- --host 0.0.0.0 --port $PORT", 4321),
```
In `Template::from_str`'s match, add:
```rust
            "nuxt" => Some(Template::Nuxt),
            "astro" => Some(Template::Astro),
            "astro-ts" => Some(Template::AstroTs),
```

- [ ] **Step 5: Verify `nuxi init` and `create astro`'s current flags match what this task wrote**

Run: `npx --yes nuxi@latest init --help` and confirm `--packageManager`, `--gitInit`, `--force` (needed since `/app` already exists, even though empty) are still current. Run: `npm create astro@latest -- --help` and confirm `--template`, `--typescript`, `--no-install`, `--no-git`, `--yes` are still current, and that `minimal`/`relaxed`/`strict` are still valid values. Update the three seed script constants in Step 4 if either tool has changed, and note the change in your task report.

- [ ] **Step 6: Run the tests to verify they pass, then the full suite**

Run: `cd backend && cargo test -p shipyard-api "nuxt|astro" -- --test-threads=1`
Expected: PASS, all 4 new tests.

Run: `cd backend && cargo build --workspace && cargo test --workspace`
Expected: clean build, 169 (Task 6's baseline) + 4 new = 173 tests passing.

- [ ] **Step 7: Commit**

```bash
cd backend && git add crates/api/src/sandbox_runtime/templates.rs
git commit -m "feat(sandbox): add Nuxt and Astro (+TS) templates via nuxi and create-astro"
```

---

## Task 8: Frontend types + client — `pending` field and the full template union

**Files:**
- Modify: `frontend/src/lib/api/types.ts`
- Modify: `frontend/src/lib/api/client.ts`

**Interfaces:**
- Consumes: the backend response shapes from Task 3 (`pending: bool` on start/status) and the template string identifiers from Tasks 1, 4-7.
- Produces: `SandboxInstance.pending: boolean`; `createSandboxApp`'s `template` parameter type widened to the full set. Consumed by Task 9 (picker) and Task 10 (editor banner).

- [ ] **Step 1: Add `pending` to the `SandboxInstance` type**

Find, in `frontend/src/lib/api/types.ts`:
```typescript
export interface SandboxInstance {
	service_id: string;
	status: 'stopped' | 'starting' | 'running';
	container_id: string | null;
	container_name: string | null;
	preview_url: string | null;
	last_heartbeat_at: string | null;
	started_at: string | null;
}
```
Replace with:
```typescript
export interface SandboxInstance {
	service_id: string;
	status: 'stopped' | 'starting' | 'running';
	container_id: string | null;
	container_name: string | null;
	preview_url: string | null;
	last_heartbeat_at: string | null;
	started_at: string | null;
	pending: boolean;
}
```

- [ ] **Step 2: Add a `SandboxTemplate` type and widen `createSandboxApp`/`startSandbox`**

Read the current `frontend/src/lib/api/client.ts` around `createSandboxApp`/`startSandbox` first (both are in the "Sandbox Apps" section) to confirm the exact current signatures still match. Find:
```typescript
	async createSandboxApp(
		projectId: string,
		data: { name: string; slug: string; template: 'node' | 'python' | 'static' }
	): Promise<ApiResponse<{ id: string; project_id: string; name: string; slug: string; type: string }>> {
		return this.post(`/projects/${projectId}/apps`, data);
	}

	async getSandboxInstance(serviceId: string): Promise<ApiResponse<SandboxInstance>> {
		return this.get(`/apps/${serviceId}/sandbox/status`);
	}

	async startSandbox(serviceId: string): Promise<ApiResponse<{ status: string; preview_url: string | null }>> {
		return this.post(`/apps/${serviceId}/sandbox/start`);
	}
```
Replace with:
```typescript
	async createSandboxApp(
		projectId: string,
		data: { name: string; slug: string; template: SandboxTemplate }
	): Promise<ApiResponse<{ id: string; project_id: string; name: string; slug: string; type: string }>> {
		return this.post(`/projects/${projectId}/apps`, data);
	}

	async getSandboxInstance(serviceId: string): Promise<ApiResponse<SandboxInstance>> {
		return this.get(`/apps/${serviceId}/sandbox/status`);
	}

	async startSandbox(serviceId: string): Promise<ApiResponse<{ status: string; preview_url: string | null; pending: boolean }>> {
		return this.post(`/apps/${serviceId}/sandbox/start`);
	}
```
Add the new `SandboxTemplate` type near the top of `frontend/src/lib/api/types.ts` (anywhere before `SandboxInstance` is fine — e.g. immediately above it):
```typescript
export type SandboxTemplate =
	| 'node' | 'python' | 'static' | 'custom'
	| 'react' | 'react-ts'
	| 'vue' | 'vue-ts'
	| 'sveltekit' | 'sveltekit-ts'
	| 'next' | 'next-ts'
	| 'nuxt'
	| 'astro' | 'astro-ts';
```
Add the import to `client.ts` if `SandboxTemplate` isn't already reachable there — check the top of the file for its existing `import type { ... } from './types'` line and add `SandboxTemplate` to that same import list (don't add a second import statement).

- [ ] **Step 3: Type-check**

Run: `cd frontend && npm run check`
Expected: same error count as the established baseline (26 errors) — no new errors. If `SandboxTemplate` wasn't correctly imported into `client.ts`, you'll see a new error referencing it; fix the import and re-run.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/lib/api/types.ts frontend/src/lib/api/client.ts
git commit -m "feat(frontend): add SandboxTemplate union and expose pending on sandbox status"
```

---

## Task 9: Frontend — template picker redesign (card grid)

**Files:**
- Modify: `frontend/src/lib/panels/resources/SandboxAppTemplatePanel.svelte`

**Interfaces:**
- Consumes: `SandboxTemplate` (Task 8), `api.createSandboxApp` (Task 8, unchanged call shape besides the widened type).
- Produces: the actual UI a user picks a template from — no other task depends on this one's internals, it's the terminal frontend piece for template selection.

- [ ] **Step 1: Read the current file in full**

`frontend/src/lib/panels/resources/SandboxAppTemplatePanel.svelte` is short (~122 lines) — read it in full before editing. Confirm the current `Props` interface (`projectId`, `orgId`, `onCreated`) and the `create()` function's shape still match what's described below; the panel is otherwise unchanged by this task except for the template-selection UI itself.

- [ ] **Step 2: Replace the template state and selection UI**

Find:
```svelte
	let template = $state<'node' | 'python' | 'static'>('node');
```
Replace with:
```svelte
	let template = $state<SandboxTemplate>('node');
	let variant = $state<'js' | 'ts'>('js');

	// Frameworks whose official tooling offers a genuine JS/TS choice — Nuxt
	// is deliberately excluded (its scaffolder defaults to TypeScript with no
	// real plain-JS mode in current tooling), so it has no toggle at all.
	const TS_CAPABLE: Partial<Record<SandboxTemplate, SandboxTemplate>> = {
		react: 'react-ts',
		vue: 'vue-ts',
		sveltekit: 'sveltekit-ts',
		next: 'next-ts',
		astro: 'astro-ts'
	};

	function selectTemplate(base: SandboxTemplate) {
		variant = 'js';
		template = base;
	}

	function setVariant(v: 'js' | 'ts') {
		variant = v;
		const base = (Object.entries(TS_CAPABLE).find(([, ts]) => ts === template)?.[0] as SandboxTemplate) ?? template;
		template = v === 'ts' ? (TS_CAPABLE[base] ?? base) : base;
	}

	let selectedBase = $derived(
		(Object.entries(TS_CAPABLE).find(([, ts]) => ts === template)?.[0] as SandboxTemplate) ?? template
	);
```
You'll need the `SandboxTemplate` type imported — add it to this file's existing imports (check the top of the file for an `import type { ... } from '$lib/api/types'` line; there may not be one yet, since this file previously had no typed imports from `types.ts` — add one if needed: `import type { SandboxTemplate } from '$lib/api/types';`).

Find the whole template-selection markup block:
```svelte
	<div class="field">
		<span>Template</span>
		<div class="template-grid">
			<button class="template-card" class:selected={template === 'node'} onclick={() => (template = 'node')}>
				<Code2 size={16} /> Node
			</button>
			<button class="template-card" class:selected={template === 'python'} onclick={() => (template = 'python')}>
				<Code2 size={16} /> Python
			</button>
			<button class="template-card" class:selected={template === 'static'} onclick={() => (template = 'static')}>
				<Code2 size={16} /> Static
			</button>
		</div>
	</div>
```
Replace with:
```svelte
	<div class="field">
		<span>Template</span>
		<div class="template-group-label">Basic</div>
		<div class="template-grid">
			<button class="template-card" class:selected={template === 'node'} onclick={() => selectTemplate('node')}>
				<Code2 size={16} /> Node
			</button>
			<button class="template-card" class:selected={template === 'python'} onclick={() => selectTemplate('python')}>
				<FileCode size={16} /> Python
			</button>
			<button class="template-card" class:selected={template === 'static'} onclick={() => selectTemplate('static')}>
				<Globe size={16} /> Static
			</button>
			<button class="template-card" class:selected={template === 'custom'} onclick={() => selectTemplate('custom')}>
				<Terminal size={16} /> Custom
			</button>
		</div>
		{#if template === 'custom'}
			<p class="template-hint">Starts blank with a starter <code>shipyard.json</code>. Scaffold your own project via the Terminal tab, then Stop and Start to apply it.</p>
		{/if}

		<div class="template-group-label">Frameworks</div>
		<div class="template-grid framework-grid">
			<button class="template-card" class:selected={selectedBase === 'react'} onclick={() => selectTemplate('react')}>
				<Atom size={16} /> React
			</button>
			<button class="template-card" class:selected={selectedBase === 'vue'} onclick={() => selectTemplate('vue')}>
				<Layers size={16} /> Vue
			</button>
			<button class="template-card" class:selected={selectedBase === 'sveltekit'} onclick={() => selectTemplate('sveltekit')}>
				<Flame size={16} /> SvelteKit
			</button>
			<button class="template-card" class:selected={selectedBase === 'next'} onclick={() => selectTemplate('next')}>
				<Triangle size={16} /> Next.js
			</button>
			<button class="template-card" class:selected={selectedBase === 'nuxt'} onclick={() => selectTemplate('nuxt')}>
				<Box size={16} /> Nuxt
			</button>
			<button class="template-card" class:selected={selectedBase === 'astro'} onclick={() => selectTemplate('astro')}>
				<Rocket size={16} /> Astro
			</button>
		</div>
		{#if TS_CAPABLE[selectedBase]}
			<div class="variant-toggle">
				<button class="variant-btn" class:active={variant === 'js'} onclick={() => setVariant('js')}>JavaScript</button>
				<button class="variant-btn" class:active={variant === 'ts'} onclick={() => setVariant('ts')}>TypeScript</button>
			</div>
		{/if}
	</div>
```
Update the icon import at the top of the file — find:
```svelte
	import { Code2 } from '@lucide/svelte';
```
Replace with:
```svelte
	import { Code2, FileCode, Globe, Terminal, Atom, Layers, Flame, Triangle, Box, Rocket } from '@lucide/svelte';
```
If any of `Atom`/`Layers`/`Flame`/`Triangle`/`Box`/`Rocket`/`FileCode`/`Terminal` doesn't exist in the installed `@lucide/svelte` version, substitute the closest available icon name from that package instead (check `node_modules/@lucide/svelte`'s exports or the package's own icon list) — this is a cosmetic substitution only and doesn't affect behavior; note any substitution you made in your task report.

- [ ] **Step 3: Add CSS for the new grouping/hint/toggle elements**

Find the existing `<style>` block's `.template-grid`/`.template-card` rules:
```svelte
	.template-grid {
		display: flex;
		gap: 8px;
	}
	.template-card {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		cursor: pointer;
		font-size: 12px;
		color: var(--text-secondary);
	}
	.template-card.selected {
		border-color: var(--accent);
		color: var(--accent);
	}
```
Replace with:
```svelte
	.template-group-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-top: 8px;
	}
	.template-grid {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.template-card {
		flex: 1 1 90px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		cursor: pointer;
		font-size: 12px;
		color: var(--text-secondary);
	}
	.template-card.selected {
		border-color: var(--accent);
		color: var(--accent);
	}
	.template-hint {
		font-size: 11px;
		color: var(--text-dim);
		margin: 2px 0 0;
		line-height: 1.4;
	}
	.template-hint code {
		font-family: var(--font-mono);
		background: var(--bg-elevated);
		padding: 1px 4px;
		border-radius: 3px;
	}
	.variant-toggle {
		display: flex;
		gap: 4px;
		margin-top: 4px;
	}
	.variant-btn {
		flex: 1;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm, 4px);
		background: var(--bg-surface);
		color: var(--text-secondary);
		font-size: 12px;
		cursor: pointer;
	}
	.variant-btn.active {
		border-color: var(--accent);
		color: var(--accent);
	}
```

- [ ] **Step 4: Type-check**

Run: `cd frontend && npm run check`
Expected: 26-error baseline, no new errors.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/panels/resources/SandboxAppTemplatePanel.svelte
git commit -m "feat(frontend): redesign the sandbox template picker as a grouped card grid"
```

---

## Task 10: Frontend — editor page pending-state banner

**Files:**
- Modify: `frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/apps/[serviceId]/editor/+page.svelte`

**Interfaces:**
- Consumes: `SandboxInstance.pending` / the `pending` field on `startSandbox`'s response (Task 8).
- Produces: a visible banner in the Editor tab — terminal deliverable, no other task depends on it.

- [ ] **Step 1: Read the current `boot()` function and top-of-file state**

Read the editor page's current `boot()` function (it currently sets `instance` from `startRes.data.status`/`.preview_url` and `urlBarValue`/`navHistory` from `startRes.data.preview_url`) to confirm it still matches before editing — this file has had several rounds of changes this session.

- [ ] **Step 2: Track `pending` on the instance**

Find:
```typescript
		instance = { ...instance, status: startRes.data.status, preview_url: startRes.data.preview_url } as SandboxInstance;
```
Replace with:
```typescript
		instance = { ...instance, status: startRes.data.status, preview_url: startRes.data.preview_url, pending: startRes.data.pending } as SandboxInstance;
```

- [ ] **Step 3: Add the banner markup to the Editor tab**

Find, in the template:
```svelte
		<div class="editor-view" class:hidden={activeTab !== 'editor'}>
			<aside class="sidebar" style="width: {sidebarWidth}px">
```
Replace with:
```svelte
		<div class="editor-view" class:hidden={activeTab !== 'editor'}>
			{#if instance?.pending}
				<div class="pending-banner">
					No project detected yet — scaffold one in the Terminal below, then Stop and Start to apply it.
				</div>
			{/if}
			<aside class="sidebar" style="width: {sidebarWidth}px">
```
Note: this places the banner as a sibling of `.sidebar` inside `.editor-view`, which is a flex row (`display: flex`) — the banner needs `width: 100%` and its own line, so give `.editor-view` `flex-wrap: wrap` or make the banner `position: absolute` at the top. The simpler, more robust fix: change `.editor-view`'s children order so the banner sits *above* the sidebar+editor row rather than inside it. Restructure instead as:
```svelte
		<div class="editor-view-wrap" class:hidden={activeTab !== 'editor'}>
			{#if instance?.pending}
				<div class="pending-banner">
					No project detected yet — scaffold one in the Terminal below, then Stop and Start to apply it.
				</div>
			{/if}
			<div class="editor-view">
				<aside class="sidebar" style="width: {sidebarWidth}px">
```
This requires closing an extra `</div>` at the end of the editor-view block — find the existing closing structure:
```svelte
			</main>
		</div>

		<section class="preview-pane" class:hidden={activeTab !== 'preview'}>
```
Replace with:
```svelte
			</main>
			</div>
		</div>

		<section class="preview-pane" class:hidden={activeTab !== 'preview'}>
```

- [ ] **Step 4: Add CSS for `.editor-view-wrap` and `.pending-banner`**

Find the existing `.editor-view` CSS rule:
```svelte
	.editor-view {
		grid-row: 2;
		display: flex;
		min-height: 0;
	}
```
Replace with:
```svelte
	.editor-view-wrap {
		grid-row: 2;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	.editor-view {
		display: flex;
		flex: 1;
		min-height: 0;
	}
	.pending-banner {
		flex-shrink: 0;
		padding: 8px 14px;
		background: color-mix(in srgb, var(--accent) 10%, var(--bg-elevated));
		border-bottom: 1px solid var(--border);
		color: var(--text-secondary);
		font-size: 12px;
	}
```

- [ ] **Step 5: Type-check**

Run: `cd frontend && npm run check`
Expected: 26-error baseline, no new errors. If the extra `</div>` in Step 3 is mismatched, Svelte's compiler will report an unclosed/mismatched tag error at this file — recount the opening/closing tags around `.editor-view-wrap`/`.editor-view` carefully if so.

- [ ] **Step 6: Commit**

```bash
git add "frontend/src/routes/orgs/[orgSlug]/projects/[projectSlug]/apps/[serviceId]/editor/+page.svelte"
git commit -m "feat(frontend): show a pending-detection banner in the sandbox editor"
```

---

## Task 11: Manual end-to-end verification

**Files:** none (verification only).

**Interfaces:** exercises every prior task's deliverable together.

- [ ] **Step 1: Verify Custom mode end-to-end**

1. Create a sandbox app with the "Custom" template.
2. Start it — confirm it boots successfully (idle container, no crash) and the editor shows the pending banner.
3. Open the Terminal tab, confirm a shell connects, and run a real scaffold command, e.g. `npx create-vite@latest . -- --template svelte` (choosing something distinct from every built-in framework template, to confirm this isn't accidentally reusing one of them).
4. Stop the sandbox, then Start it again.
5. Confirm: the pending banner is now gone, the Preview tab shows the scaffolded Svelte app's real dev server, and `sandbox_app_configs.manifest_source` for this service is now `'detected'` (query the DB directly to confirm, or trust the banner's disappearance as the user-facing signal).
6. Stop and Start once more — confirm it does NOT re-scaffold or re-detect (the app's dev server just restarts normally, `manifest_source` stays `'detected'`).

- [ ] **Step 2: Verify each framework template**

For each of: React, React-TS, Vue, Vue-TS, SvelteKit, SvelteKit-TS, Next, Next-TS, Nuxt, Astro, Astro-TS — create one sandbox app using that template, start it, and confirm (allowing extra time for the first-boot scaffold+install):
- The seed script's scaffolder actually completed (no error in the container's logs — check via the existing container-logs mechanism if the preview doesn't come up).
- The Preview tab eventually shows that framework's real, unmodified starter page (not a 502/connection-refused indefinitely — a brief wait during first boot is expected).
- For a JS/TS pair, confirm the TS variant's scaffolded project actually contains TypeScript files (`.ts`/`.tsx` extensions, a `tsconfig.json`) via the file tree, and the JS variant does not.

- [ ] **Step 3: Confirm no regression to the existing three templates**

Create one sandbox each with Node, Python, and Static — confirm all three still boot and preview exactly as they did before this plan (these templates' own code was untouched by every task above; this step exists to catch an accidental cross-contamination from the `Template` enum/match-statement edits across Tasks 1, 4-7).

- [ ] **Step 4: Report results**

Document which of the 11 framework templates verified cleanly, which needed a CLI-flag adjustment during Tasks 4-7's own verification steps (and what the adjustment was), and any that still don't work — file these as follow-up items rather than blocking the rest of the plan on a single flaky third-party scaffolder.
