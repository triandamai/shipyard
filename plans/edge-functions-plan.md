# Edge Functions Support — Full Design Plan

Shipyard currently supports Docker Swarm services, static sites, and SSR apps. This plan adds a fourth deployment type: **edge functions** — short-lived, stateless HTTP handlers written locally, deployed via `git push` or direct upload, auto-detected by convention, and optionally configured via `shipyard.json`.

---

## 1. User Experience (the whole flow)

```
# Day 1 — zero config needed
mkdir functions/
echo "export default (req) => Response.json({ ok: true })" > functions/hello.ts
git push origin main
→ Shipyard detects functions/ directory
→ deploys hello.ts as an edge function
→ live at: https://fn.shipyard.example.com/acme/hello
```

```
# Day 2 — opt into config for customization
cat shipyard.json
{
  "functions": {
    "dir": "src/handlers",
    "entries": {
      "send-email": {
        "file": "src/handlers/email.ts",
        "timeout": 10,
        "env": ["RESEND_API_KEY", "FROM_ADDRESS"]
      }
    }
  }
}
git push origin main
→ Shipyard reads shipyard.json, uses its entries as the canonical function list
→ deploys send-email from src/handlers/email.ts
```

```
# Alternative — no git, just upload a zip
zip -r fns.zip functions/
curl -X POST https://shipyard.example.com/api/orgs/acme/edge-functions/upload \
  -H "Authorization: Bearer $TOKEN" \
  -F file=@fns.zip
→ same detection engine runs
→ all functions inside are deployed
```

---

## 2. Project Structure Convention

### Default (convention-only, no config file)

```
my-project/
├── functions/                  ← Shipyard scans this by default
│   ├── send-email.ts           ← fn name: send-email
│   ├── resize-image.ts         ← fn name: resize-image
│   └── stripe-webhook.ts       ← fn name: stripe-webhook
└── package.json
```

Rules:
- Directory: `functions/` at repo root.
- Files: `*.ts` and `*.js` only. Subdirectories are ignored in v1.
- Function name: filename without extension, already kebab-case assumed. If camelCase or PascalCase filename, convert to kebab-case automatically (`sendEmail.ts` → `send-email`).
- Export: file must export a default function with signature `(req: Request) => Response | Promise<Response>`. Files that do not export a valid default handler are skipped with a warning.

### With `shipyard.json` (config overrides convention)

```jsonc
// shipyard.json (at repo root)
{
  "functions": {
    "dir": "src/handlers",          // optional: override the scanned directory
    "runtime": "deno",              // optional: global runtime (only "deno" in v1)
    "entries": {                    // optional: explicit function list (overrides dir scan)
      "send-email": {
        "file": "src/handlers/email.ts",   // path relative to repo root
        "timeout": 10,                     // seconds, overrides tier default
        "env": ["RESEND_API_KEY"]          // whitelist of env var keys to inject
      },
      "resize-image": {
        "file": "src/handlers/image.ts",
        "timeout": 30
      }
    }
  }
}
```

**Resolution order:**

1. If `shipyard.json` exists AND has `functions.entries` → use `entries` as the canonical list. Ignore directory scan entirely.
2. Else if `shipyard.json` exists AND has `functions.dir` → scan that directory instead of `functions/`.
3. Else → scan `functions/` at repo root.

If `shipyard.json` is present but has no `functions` key, edge function detection is skipped (the file may belong to other Shipyard config in the future).

---

## 3. Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│                       Deploy Path                         │
│                                                           │
│  git push ──► webhook ──► clone repo ──► detect ──┐      │
│                                                    │      │
│  zip upload ──────────────────── extract ──► detect┤      │
│                                                    │      │
│                                          DetectedFunctions│
│                                                    │      │
│                                          EdgeFunctionManager
│                                                    │      │
│                                          DB write  │      │
│                                          + reload  │      │
└──────────────────────────────────────────────────-─┘      │
                                                    ▼
┌─────────────────────────────────────────────────────────┐
│                  Runtime (per org)                       │
│                                                          │
│  Traefik ──► Edge Runtime Container (Deno)               │
│              │                                           │
│              ├── V8 isolate: send-email                  │
│              ├── V8 isolate: resize-image                │
│              └── V8 isolate: stripe-webhook              │
└──────────────────────────────────────────────────────────┘
```

### Components

| Component | Location | Role |
|---|---|---|
| `FunctionDetector` | `crates/engine/src/edge_fn_detector.rs` | Scans repo/zip for function files, reads `shipyard.json` |
| `EdgeFunctionManager` | `crates/api/src/edge_functions/manager.rs` | CRUD, deploy orchestration, quota checks |
| `EdgeRuntimeWorker` | `crates/api/src/edge_functions/runtime_worker.rs` | Background worker — ensures runtime containers exist per org |
| Edge Runtime container | Docker image `ghcr.io/shipyard/edge-runtime:v1` | Deno HTTP server; V8 isolate per function |
| `edge_functions` table | PostgreSQL | Function metadata |
| `edge_function_deployments` table | PostgreSQL | Code snapshots, deploy history |
| `edge_function_invocation_logs` table | PostgreSQL | Rolling 7-day invocation log |

---

## 4. Deploy Path 1 — Git Push

### How it connects to the existing webhook system

Shipyard already handles `git push` webhooks from GitHub, GitLab, and Bitbucket in `crates/api/src/webhooks/`. The webhook fires when any push happens to the tracked branch.

For edge functions, a new entity type is introduced: an **edge function group** — a link between a git repository and an org's function set. This is separate from a `service`. One repo can deploy both a service (SSR/container) and a set of edge functions at the same time.

When a webhook fires:
1. The existing handler identifies the pushed branch and matched services (existing logic).
2. **New**: also checks `edge_function_groups` table for any group linked to that repo + branch.
3. If a match is found, triggers `EdgeFunctionManager::deploy_from_git(group_id, commit_sha)`.

### `deploy_from_git` flow

```
1. Clone repo at commit_sha into temp dir  (same git clone logic as services)
2. Read shipyard.json if present
3. Run FunctionDetector::detect(root_path, config)
   → returns Vec<DetectedFunction>
4. Diff against previously deployed functions:
   - new functions     → create + deploy
   - changed functions → redeploy (git diff on file path between old_sha and new_sha)
   - removed functions → delete
5. For each changed/new function:
   a. Read file contents
   b. Call EdgeFunctionManager::deploy_function(fn_id, code, deployed_by=system)
6. Publish MQTT events
7. Cleanup temp dir
```

**Diff-based deploys**: only functions whose source files changed between the previous commit SHA and the new one are redeployed. This prevents redeploying 10 functions when only 1 changed. Implemented using `git diff --name-only old_sha new_sha`.

### Edge function group linking (UI flow)

In the frontend, under the edge functions section, the user can connect a git repo:
- Select provider (GitHub / GitLab / Bitbucket) — reuses existing OAuth token.
- Select repo and branch.
- Shipyard registers the webhook (same as services).
- Creates an `edge_function_groups` row.
- Runs an initial deploy immediately.

---

## 5. Deploy Path 2 — Direct Zip Upload

### Endpoint

```
POST /api/orgs/:org_id/edge-functions/upload
Content-Type: multipart/form-data
  file: <zip file>
```

### Flow

```
1. Receive zip (max size: 10 MB)
2. Extract to scratchpad temp dir
3. Run FunctionDetector::detect(temp_root, config)
4. For each DetectedFunction:
   a. Check quota (max function count for tier)
   b. Upsert edge_functions row (create if name is new, update if exists)
   c. Call EdgeFunctionManager::deploy_function(fn_id, code, deployed_by=user_id)
5. Return list of deployed functions with their public URLs
6. Cleanup temp dir
```

Upload does a **full replace** for all detected functions. Functions not present in the zip but previously deployed from a prior upload are left untouched (not deleted). Deletion is explicit via the delete endpoint.

---

## 6. `FunctionDetector` — Detection Engine

Lives in `crates/engine/src/edge_fn_detector.rs`. Used by both the git deploy path and the zip upload path.

### Output type

```rust
pub struct DetectedFunction {
    pub name: String,          // kebab-case slug, unique within the deploy
    pub file_path: String,     // relative path from root (e.g. "functions/send-email.ts")
    pub code: String,          // full file contents
    pub runtime: String,       // "deno" (only option in v1)
    pub timeout_secs: u32,     // from config entry or tier default
    pub env_whitelist: Vec<String>, // from config entry, empty = all org env vars
}
```

### Detection logic (pseudocode)

```rust
pub fn detect(root: &Path, shipyard_config: Option<ShipyardConfig>) -> Vec<DetectedFunction> {
    let entries = match shipyard_config {
        // Case 1: explicit entries in shipyard.json
        Some(cfg) if cfg.functions.entries.is_some() => {
            cfg.functions.entries.unwrap()
                .into_iter()
                .map(|(name, entry)| resolve_entry(root, name, entry))
                .collect()
        }
        // Case 2: custom dir in shipyard.json
        Some(cfg) if cfg.functions.dir.is_some() => {
            scan_directory(root.join(cfg.functions.dir.unwrap()))
        }
        // Case 3: convention — scan functions/
        _ => scan_directory(root.join("functions")),
    };

    entries.into_iter().filter(|e| validate_export(&e.code)).collect()
}

fn scan_directory(dir: &Path) -> Vec<DetectedFunction> {
    // glob dir/*.ts and dir/*.js
    // filename without extension → kebab_case → function name
    // read file contents
}

fn validate_export(code: &str) -> bool {
    // simple text check: contains "export default"
    // does NOT parse AST in v1; warn if check fails, skip the file
    code.contains("export default")
}
```

### `shipyard.json` parsing

```rust
// crates/engine/src/edge_fn_config.rs
#[derive(Deserialize)]
pub struct ShipyardConfig {
    pub functions: Option<FunctionsConfig>,
}

#[derive(Deserialize)]
pub struct FunctionsConfig {
    pub dir: Option<String>,
    pub runtime: Option<String>,
    pub entries: Option<HashMap<String, FunctionEntry>>,
}

#[derive(Deserialize)]
pub struct FunctionEntry {
    pub file: String,
    pub timeout: Option<u32>,
    pub env: Option<Vec<String>>,
}
```

Parsed with `serde_json`. If `shipyard.json` fails to parse, the deploy aborts with an error message telling the user exactly what is wrong (not a silent fallback to convention).

---

## 7. Database Schema

### New migration file

`crates/db/migrations/YYYYMMDDXXXXXX_edge_functions.sql`

#### `edge_function_groups`

Links a git repo to an org's function set. One group per repo+branch per org.

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `org_id` | UUID | FK → `organizations(id)` CASCADE |
| `provider` | TEXT | `github`, `gitlab`, `bitbucket` |
| `repo_url` | TEXT | Clone URL |
| `branch` | TEXT | Tracked branch |
| `webhook_secret` | TEXT | For verifying push events |
| `last_deployed_sha` | TEXT nullable | Commit SHA of last successful deploy |
| `created_at` | TIMESTAMPTZ | |

#### `edge_functions`

One row per function definition.

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `org_id` | UUID | FK → `organizations(id)` CASCADE |
| `group_id` | UUID nullable | FK → `edge_function_groups(id)` SET NULL. NULL = uploaded directly |
| `name` | TEXT | Kebab-case slug. Unique per org. |
| `runtime` | TEXT | `deno` |
| `file_path` | TEXT | Original file path within repo/zip (informational) |
| `env_vars` | JSONB | Encrypted key-value pairs (AES, same scheme as services) |
| `env_whitelist` | TEXT[] | If non-empty, only these keys are injected into the isolate |
| `timeout_secs` | INT | Per-function timeout. Capped by tier max. |
| `status` | `edge_fn_status` enum | `inactive`, `active`, `deploying`, `error` |
| `last_deployed_at` | TIMESTAMPTZ | |
| `created_at` | TIMESTAMPTZ | |
| `updated_at` | TIMESTAMPTZ | |

Unique constraint: `(org_id, name)`.

#### `edge_function_deployments`

Append-only deploy history. Code snapshots stored here.

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `function_id` | UUID | FK → `edge_functions(id)` CASCADE |
| `commit_sha` | TEXT nullable | Git commit SHA if deployed from git. NULL if uploaded. |
| `code_bundle` | TEXT | Full source file contents (≤ tier bundle size limit) |
| `deployed_by` | UUID nullable | FK → `users(id)` SET NULL. NULL = system (webhook trigger) |
| `status` | TEXT | `pending`, `live`, `rolled_back`, `error` |
| `error` | TEXT nullable | Error message if deploy failed |
| `created_at` | TIMESTAMPTZ | |

Only one `live` deployment per function at a time. Enforced in `EdgeFunctionManager::deploy_function`.

#### `edge_function_invocation_logs`

| Column | Type | Notes |
|---|---|---|
| `id` | UUID PK | |
| `function_id` | UUID | FK → `edge_functions(id)` CASCADE |
| `request_id` | TEXT | UUID generated by runtime per invocation |
| `method` | TEXT | HTTP method |
| `path` | TEXT | Request path |
| `status_code` | INT | Response HTTP status |
| `duration_ms` | INT | Wall-clock execution time |
| `error` | TEXT nullable | Runtime error message if function threw |
| `logged_at` | TIMESTAMPTZ | |

Index: `(function_id, logged_at DESC)` for efficient log queries.
Retention: deleted after 7 days by background worker.

#### New Postgres enum

```sql
CREATE TYPE edge_fn_status AS ENUM (
    'inactive', 'active', 'deploying', 'error'
);
```

---

## 8. Runtime Container Design

### One container per org

A single Deno HTTP server container runs per org that has at least one active function. All the org's functions run as V8 isolates inside it. Orgs are isolated from each other at the container boundary.

### Container startup

On start the container calls `GET /internal/edge-runtime/manifest?org_id=...` on the Manager to get all live functions. It compiles each function's code into a V8 isolate and holds them in an in-memory map keyed by function name.

### Hot reload

When a deploy completes, the Manager calls `POST /reload` on the container (no restart). The container re-fetches the manifest and hot-swaps only the changed isolate.

### Internal dispatch

Traefik forwards all requests for an org's functions to the container on port 8000. The container dispatches based on the path segment:

```
https://fn.shipyard.example.com/acme/send-email/anything
                                       ^^^^^^^^^
                                       isolate key
```

### Security constraints

| Permission | Allowed |
|---|---|
| Outbound network | HTTPS only. No localhost, no RFC-1918 ranges. |
| Env vars | Only keys in `env_whitelist` (or all org env vars if whitelist is empty) |
| Filesystem read | None |
| Filesystem write | None |
| Subprocess execution | None |
| V8 heap per isolate | 64 MB max |
| Execution timeout | Per function (5s free / 15s pro / 30s max) |

### Runtime Docker service spec

```
image:    ghcr.io/shipyard/edge-runtime:v1
name:     shipyard-edge-{org_id_short}   (first 8 chars of UUID)
replicas: 1
network:  platform_proxy
env:
  SHIPYARD_RUNTIME_API_URL  = http://shipyard-api:3001
  SHIPYARD_RUNTIME_ORG_ID   = {org_id}
  SHIPYARD_RUNTIME_SECRET   = {AppConfig::edge_fn_runtime_secret}
resources:
  memory limit: 256 MB (free) / 512 MB (pro) / 1 GB (max)
  cpu limit:    0.25 (free) / 0.5 (pro) / 1.0 (max)
labels:
  traefik.enable: true
  traefik.http.routers.edge-{org_id_short}.rule:
    Host(`fn.{app_domain}`) && PathPrefix(`/{org_slug}/`)
  traefik.http.middlewares.edge-{org_id_short}-strip.stripprefix.prefixes: /{org_slug}
  traefik.http.routers.edge-{org_id_short}.middlewares: edge-{org_id_short}-strip
  traefik.http.services.edge-{org_id_short}.loadbalancer.server.port: 8000
```

---

## 9. Deno Runtime Container Implementation

The runtime image is a standalone Deno TypeScript file built into a Docker image separately from the Rust API.

### `runtime/main.ts` (the full runtime server)

```typescript
const isolates = new Map<string, (req: Request) => Promise<Response>>();
const ORG_ID   = Deno.env.get("SHIPYARD_RUNTIME_ORG_ID")!;
const API_URL  = Deno.env.get("SHIPYARD_RUNTIME_API_URL")!;
const SECRET   = Deno.env.get("SHIPYARD_RUNTIME_SECRET")!;

async function loadManifest() {
  const res = await fetch(`${API_URL}/internal/edge-runtime/manifest`, {
    headers: {
      "Authorization": `Bearer ${SECRET}`,
      "X-Org-Id": ORG_ID,
    },
  });
  if (!res.ok) throw new Error(`Manifest fetch failed: ${res.status}`);

  const fns: { name: string; code: string; env: Record<string, string>; timeout_secs: number }[]
    = await res.json();

  isolates.clear();
  for (const fn of fns) {
    const blob = new Blob([fn.code], { type: "application/typescript" });
    const url  = URL.createObjectURL(blob);
    const mod  = await import(url);
    // wrap with env injection and timeout
    isolates.set(fn.name, wrapIsolate(mod.default, fn.env, fn.timeout_secs));
    URL.revokeObjectURL(url);
  }
}

function wrapIsolate(
  handler: (req: Request) => Promise<Response>,
  env: Record<string, string>,
  timeoutSecs: number
) {
  return async (req: Request): Promise<Response> => {
    // inject env vars into Deno.env scope for this call
    // (achieved by patching globalThis.Deno.env.get for the duration)
    const originalGet = Deno.env.get.bind(Deno.env);
    (Deno.env as any).get = (key: string) => env[key] ?? originalGet(key);

    try {
      return await Promise.race([
        handler(req),
        new Promise<never>((_, reject) =>
          setTimeout(() => reject(new Error("Function timeout")), timeoutSecs * 1000)
        ),
      ]);
    } finally {
      (Deno.env as any).get = originalGet;
    }
  };
}

await loadManifest();

Deno.serve({ port: 8000 }, async (req) => {
  const url      = new URL(req.url);
  const segments = url.pathname.split("/").filter(Boolean);
  const fnName   = segments[0];

  // /reload endpoint — called by Manager after deploy
  if (req.method === "POST" && fnName === "reload") {
    const auth = req.headers.get("Authorization");
    if (auth !== `Bearer ${SECRET}`) return new Response("Unauthorized", { status: 401 });
    await loadManifest();
    return Response.json({ ok: true, loaded: isolates.size });
  }

  const handler = isolates.get(fnName);
  if (!handler) return Response.json({ error: "function not found" }, { status: 404 });

  const start = Date.now();
  let status = 200;
  let error: string | null = null;

  try {
    // forward request with fn-name prefix stripped
    const forwardedPath = "/" + segments.slice(1).join("/") + url.search;
    const forwarded = new Request(`https://fn${forwardedPath}`, {
      method:  req.method,
      headers: req.headers,
      body:    req.body,
    });
    const response = await handler(forwarded);
    status = response.status;

    // async log — fire and forget
    logInvocation(fnName, req.method, forwardedPath, status, Date.now() - start, null);
    return response;

  } catch (e: any) {
    error  = e.message;
    status = 500;
    logInvocation(fnName, req.method, url.pathname, status, Date.now() - start, error);
    return Response.json({ error: e.message }, { status: 500 });
  }
});

async function logInvocation(
  fnName: string, method: string, path: string,
  statusCode: number, durationMs: number, error: string | null
) {
  await fetch(`${API_URL}/internal/edge-runtime/log`, {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${SECRET}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      org_id: ORG_ID, fn_name: fnName,
      method, path, status_code: statusCode,
      duration_ms: durationMs, error,
    }),
  }).catch(() => {}); // never let logging crash the function response
}
```

### `Dockerfile` for the runtime image

```dockerfile
FROM denoland/deno:2.0.0
WORKDIR /runtime
COPY main.ts .
# Pre-cache Deno std lib
RUN deno cache main.ts
CMD ["deno", "run", \
  "--allow-net", \
  "--allow-env", \
  "--allow-read=/dev/null", \
  "--v8-flags=--max-heap-size=512", \
  "main.ts"]
```

Note: `--max-heap-size=512` is the container-level V8 heap. The per-isolate `64 MB` limit is enforced by the Manager when rejecting oversized bundles — not directly in Deno v1 (per-context heap limits require V8 API bindings not yet exposed in Deno).

---

## 10. API Endpoints

All under `/api/orgs/:org_id/edge-functions`. Standard JWT auth + org-member check applies.

### Function management

| Method | Path | Description |
|---|---|---|
| `GET` | `/` | List all functions. Returns name, status, last_deployed_at, public URL, invocation count (last 24h). |
| `GET` | `/:fn_id` | Function detail + current live deployment info. |
| `PUT` | `/:fn_id` | Update env vars, timeout override. Does not redeploy. |
| `DELETE` | `/:fn_id` | Delete function and all deployments. Removes isolate from runtime. |
| `POST` | `/:fn_id/rollback/:dep_id` | Make a previous deployment live again. Triggers reload. |
| `GET` | `/:fn_id/deployments` | Deployment history (paginated, newest first). |
| `GET` | `/:fn_id/logs` | Invocation logs. Query: `limit` (max 100), `before` (cursor timestamp). |

### Deploy endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/upload` | Multipart zip upload. Detect + deploy all functions inside. Returns list of deployed names and URLs. |
| `POST` | `/groups` | Link a git repo. Body: `{ provider, repo_url, branch }`. Registers webhook, runs initial deploy. |
| `GET` | `/groups` | List linked git repos. |
| `DELETE` | `/groups/:group_id` | Unlink repo. Does not delete deployed functions. |
| `POST` | `/groups/:group_id/deploy` | Manually trigger a re-deploy from the linked repo (re-clones latest). |

### Internal endpoints (not public-facing, auth via shared secret header)

| Method | Path | Description |
|---|---|---|
| `GET` | `/internal/edge-runtime/manifest` | Called by runtime container on startup/reload. Returns all live functions for the org with decrypted env vars. |
| `POST` | `/internal/edge-runtime/log` | Runtime container posts invocation log entries here. |

---

## 11. `EdgeFunctionManager`

File: `crates/api/src/edge_functions/manager.rs`

### Key methods

```rust
impl EdgeFunctionManager {
    // called by webhook handler and manual deploy trigger
    pub async fn deploy_from_git(&self, group_id: Uuid, commit_sha: &str) -> Result<DeployReport>;

    // called by zip upload endpoint
    pub async fn deploy_from_upload(&self, org_id: Uuid, zip_bytes: Bytes, user_id: Uuid) -> Result<DeployReport>;

    // called by both of the above; also by rollback
    pub async fn deploy_function(&self, fn_id: Uuid, code: &str, commit_sha: Option<&str>, deployed_by: Option<Uuid>) -> Result<()>;

    // reload the org's runtime container (called after any deploy or delete)
    async fn reload_runtime(&self, org_id: Uuid) -> Result<()>;

    // quota gate — call before creating a new function
    pub async fn check_function_quota(&self, org_id: Uuid) -> Result<(), QuotaError>;

    // build the JSON manifest for the runtime container
    pub async fn build_manifest(&self, org_id: Uuid) -> Result<Vec<FunctionManifestEntry>>;
}
```

### `DeployReport` (returned to API caller)

```rust
pub struct DeployReport {
    pub deployed: Vec<String>,   // function names successfully deployed
    pub skipped:  Vec<String>,   // detected but unchanged (git diff showed no change)
    pub failed:   Vec<(String, String)>, // (name, error message)
    pub deleted:  Vec<String>,   // functions removed because file was deleted from repo
}
```

### File layout

```
crates/api/src/edge_functions/
├── mod.rs
├── manager.rs
├── models.rs          ← EdgeFunction, EdgeFunctionDeployment, FunctionManifestEntry
├── routes.rs          ← Axum handlers for all endpoints
├── quota.rs           ← tier quota constants + check_function_quota
└── runtime_worker.rs  ← EdgeRuntimeWorker background worker

crates/engine/src/
├── edge_fn_detector.rs  ← FunctionDetector, DetectedFunction
└── edge_fn_config.rs    ← ShipyardConfig, FunctionsConfig, FunctionEntry (serde)
```

---

## 12. `EdgeRuntimeWorker` (Background Worker)

Runs every 60 seconds. Added to the worker list in `crates/api/src/main.rs`.

### Tick

```
1. Query: SELECT DISTINCT org_id FROM edge_functions WHERE status = 'active'
2. For each active org:
   a. Check if Docker Swarm service shipyard-edge-{org_id_short} exists
   b. If not: create it (see §8 runtime spec)
   c. If yes: GET http://{container_ip}/health — if unhealthy >2 min, log warning and recreate
3. Query: SELECT org_id FROM compute... (find orgs whose runtime container exists but have 0 active functions)
4. For each stale org: remove the Docker Swarm service
```

Also handles tier changes: if an org upgrades from free to pro, the worker detects that the container's memory limit no longer matches the tier and recreates it with the correct resource limits.

---

## 13. Tier Quotas

| Quota | Free | Pro | Max |
|---|---|---|---|
| Max functions per org | 1 | 10 | unlimited |
| Max bundle size | 128 KB | 512 KB | 2 MB |
| Max execution timeout | 5 s | 15 s | 30 s |
| Max invocations / day | 10,000 | 500,000 | unlimited |
| Runtime container memory | 256 MB | 512 MB | 1 GB |
| Runtime container CPU | 0.25 vCPU | 0.5 vCPU | 1.0 vCPU |

Daily invocation count is a `COUNT(*)` query on `edge_function_invocation_logs WHERE org_id = ... AND logged_at > NOW() - INTERVAL '1 day'`. When the limit is hit, the runtime container returns `429` via the `/reload`-pushed env var `MAX_DAILY_INVOCATIONS_REACHED=true` without touching user isolates.

---

## 14. MQTT Events

Same broker as service deployment events. Published by `EdgeFunctionManager`.

| Topic | Event type | Payload |
|---|---|---|
| `platform/orgs/{org_id}/edge-functions` | `fn.deployed` | `{ fn_id, name, commit_sha, status }` |
| `platform/orgs/{org_id}/edge-functions` | `fn.error` | `{ fn_id, name, error }` |
| `platform/orgs/{org_id}/edge-functions` | `fn.deleted` | `{ fn_id, name }` |
| `platform/orgs/{org_id}/edge-functions` | `fn.group.linked` | `{ group_id, repo_url, branch }` |

---

## 15. Frontend UI

### New routes

| Route | Content |
|---|---|
| `/orgs/[orgSlug]/edge-functions` | Function list: name, status badge, last deployed, 24h invocation count. Upload button. Link repo button. |
| `/orgs/[orgSlug]/edge-functions/upload` | Drag-and-drop zip upload or folder picker. Shows detected functions before confirming deploy. |
| `/orgs/[orgSlug]/edge-functions/connect-repo` | Git provider selector → repo → branch → confirm. Same OAuth token as services. |
| `/orgs/[orgSlug]/edge-functions/[fnSlug]` | Function detail: public URL, status, env vars editor, deployment history list, rollback button. |
| `/orgs/[orgSlug]/edge-functions/[fnSlug]/logs` | Invocation log table: timestamp, method, path, status code, duration. Auto-refresh via MQTT. |

### Upload preview (pre-deploy confirmation)

Before committing a zip upload, the frontend shows a diff-like view:
```
Detected 3 functions in upload:
  ✓ send-email       (new)
  ✓ resize-image     (updated — previously deployed 2 days ago)
  ✗ old-function     (will not be affected — not in this upload)

[Cancel]  [Deploy 2 Functions]
```

### Sidebar

Add "Edge Functions" link to org sidebar between "Services" and "Settings". Show a count badge when functions exist.

---

## 16. AppConfig Additions

| Config key | Env var | Default | Notes |
|---|---|---|---|
| `edge_fn_enabled` | `SHIPYARD__EDGE_FN_ENABLED` | `false` | Feature flag |
| `edge_fn_runtime_image` | `SHIPYARD__EDGE_FN_RUNTIME_IMAGE` | `ghcr.io/shipyard/edge-runtime:v1` | |
| `edge_fn_runtime_secret` | `SHIPYARD__EDGE_FN_RUNTIME_SECRET` | none | Required when edge functions enabled |
| `edge_fn_max_bundle_kb_free` | `SHIPYARD__EDGE_FN_MAX_BUNDLE_KB_FREE` | `128` | |
| `edge_fn_max_bundle_kb_pro` | `SHIPYARD__EDGE_FN_MAX_BUNDLE_KB_PRO` | `512` | |
| `edge_fn_max_bundle_kb_max` | `SHIPYARD__EDGE_FN_MAX_BUNDLE_KB_MAX` | `2048` | |
| `edge_fn_max_invocations_free` | `SHIPYARD__EDGE_FN_MAX_INVOCATIONS_FREE` | `10000` | Per day |
| `edge_fn_max_invocations_pro` | `SHIPYARD__EDGE_FN_MAX_INVOCATIONS_PRO` | `500000` | Per day |

---

## 17. What Is Out of Scope for v1

- **WASM runtime**: Deno only. WASM via `wasmtime` is a separate runtime path.
- **KV / shared state**: Functions are fully stateless. No persistent store between invocations.
- **Custom domains per function**: Only `fn.{app_domain}/{org}/{fn}` in v1.
- **Multi-region**: Functions run on the org's assigned node (or Manager's local Docker for free tier). True edge distribution is a multi-region concern.
- **Invocation-based metered billing**: Tier daily caps only. Per-GB-second Stripe metered billing is deferred.
- **Function-to-function calls**: No internal mesh in v1.
- **Monorepo support**: In v1, one git group = one `functions/` directory. Multiple function sets from one repo is deferred.
- **Scheduled functions (cron)**: HTTP-triggered only in v1.

---

## 18. Implementation Order

Each step is independently shippable and testable.

| Step | What | Depends on |
|---|---|---|
| 1 | DB migration: all 4 new tables + enum | — |
| 2 | `FunctionDetector` + `ShipyardConfig` parser (Rust, engine crate) | — |
| 3 | `EdgeFunctionManager` CRUD + models (no runtime yet) | 1 |
| 4 | API routes: list, get, delete, env var update | 3 |
| 5 | Zip upload endpoint + detection integration | 2, 3 |
| 6 | Git group linking + webhook extension | 2, 3 |
| 7 | Diff-based git deploy (only redeploy changed files) | 6 |
| 8 | Deno runtime container image (built + pushed to registry) | — |
| 9 | `EdgeRuntimeWorker`: create/delete/health-check containers | 3, 8 |
| 10 | Manifest endpoint + hot reload wired up | 3, 9 |
| 11 | Traefik labels on runtime container + public URL routing | 9 |
| 12 | Invocation log endpoint + runtime posting logs | 10 |
| 13 | MQTT events on deploy/error/delete | 3 |
| 14 | Quota enforcement (count gate + daily invocation cap) | 3, 12 |
| 15 | Invocation log retention worker (7-day cleanup) | 12 |
| 16 | Frontend: list + upload page | 5 |
| 17 | Frontend: git connect + repo link flow | 6 |
| 18 | Frontend: function detail + env vars + history + rollback | 4 |
| 19 | Frontend: invocation logs page with MQTT live updates | 12, 13 |
