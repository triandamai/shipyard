# Shipyard Artifactory — Plan

## Approach

Implement a native multi-artifact registry in Rust (`crates/registry`). All artifact kinds — Docker images, static bundles, edge functions, and others — go through the same OCI push/pull flow for consistency. The registry is purely a storage service: it stores artifacts at push time, and the deployment engine copies them out at deploy time. No lazy serving from the registry at runtime.

### Why native Rust over wrapping Docker Distribution

- No extra sidecar to orchestrate
- Auth shares the same JWT system — no separate token service
- Storage backend is a Rust trait — swap S3 ↔ local disk at setup time
- Deep RBAC with the existing `shipyard:<org_id>:<resource>:<action>` pattern
- Blob GC, quota tracking, access logs — all native
- One push protocol for all artifact kinds → consistent client tooling

---

## Artifact Types

| Kind | ID | What it stores | Size profile | Must-have |
|---|---|---|---|---|
| Container image | `docker_image` | OCI manifest + layer blobs | 50 MB – 2 GB | ✅ |
| Static bundle | `static_bundle` | Zipped build output (`dist/`, `out/`, `.next/out`) | 1 KB – 500 MB | ✅ |
| Edge function | `edge_function` | Single esbuild/rollup JS bundle or WASM binary | 1 KB – 50 MB | ✅ |
| Build cache | `build_cache` | `node_modules`, Cargo registry, pip wheels (per-project) | 50 MB – 2 GB | ✅ |
| WASM module | `wasm_module` | Standalone `.wasm` binary | 10 KB – 20 MB | later |
| DB snapshot | `db_snapshot` | pg_dump / mysqldump compressed archive | 1 MB – 100 GB | later |
| Helm chart | `helm_chart` | `.tgz` Helm package (future k8s) | small | later |
| Generic blob | `generic_blob` | Catch-all for user uploads | any | later |

Must-have for initial implementation: `docker_image`, `static_bundle`, `edge_function`, `build_cache`.

---

## Decisions

| # | Question | Decision |
|---|---|---|
| 1 | Push protocol for non-Docker types | OCI push flow for **all** kinds — consistent client tooling, same auth token flow |
| 2 | Bundle serving at runtime | Artifactory is storage only — deploy engine **copies** the artifact to the target node at deploy time, not streamed on request |
| 3 | Build cache scope | Per-project only |
| 4 | Small artifact storage | Always use the storage backend (S3 or local disk) — no inline DB bytea, no exceptions |

---

## OCI Push Flow for All Artifact Kinds

Docker images already use the OCI Distribution Spec chunked upload. All other kinds are wrapped to look like single-layer OCI images so they go through the exact same flow:

```
PUT /v2/<namespace>/<repo>/blobs/uploads/   → initiate upload
PATCH /v2/.../blobs/uploads/<uuid>          → stream bytes
PUT /v2/.../blobs/uploads/<uuid>?digest=... → finalise blob
PUT /v2/<namespace>/<repo>/manifests/<tag>  → push manifest
```

For `static_bundle` and `edge_function`, the build engine:
1. Zips / bundles the output
2. Treats the zip/bundle as a single OCI layer blob
3. Generates a minimal OCI manifest wrapping it (with `kind` label in annotations)
4. Pushes the manifest tagged with the git SHA and `latest`

Clients pull back via:
```
GET /v2/<namespace>/<repo>/manifests/<tag>   → manifest (includes blob digest)
GET /v2/<namespace>/<repo>/blobs/<digest>    → raw artifact bytes
```

The `kind` label in the OCI manifest annotations (`org.shipyard.artifact.kind`) is how the deployment engine knows what to do with the bytes it pulls.

---

## Storage Backend

Single `StorageBackend` trait, two implementations — chosen at setup time, not swappable at runtime.

```
StorageBackend
  put(key: &str, data: AsyncRead) -> Result<()>
  get(key: &str) -> Result<AsyncRead>
  delete(key: &str) -> Result<()>
  exists(key: &str) -> Result<bool>
```

Storage key format: `blobs/<digest_algo>/<digest_hex>` (content-addressed, shared across all artifact kinds and namespaces). A blob with the same digest is stored once regardless of how many manifests reference it.

Build cache blobs are large and should only be used with S3 backend in production. The setup wizard will warn if build cache is enabled on local disk storage.

---

## Crate Layout

```
backend/crates/
├── registry/                         NEW
│   ├── src/
│   │   ├── lib.rs
│   │   ├── router.rs                 OCI HTTP endpoints (/v2/...)
│   │   ├── auth.rs                   Docker token auth flow
│   │   ├── blobs.rs                  chunked upload, content-addressed fetch
│   │   ├── manifests.rs              push/pull manifests, tag resolution
│   │   ├── kinds.rs                  per-kind manifest builder + annotation parser
│   │   └── storage/
│   │       ├── mod.rs                StorageBackend trait
│   │       ├── local.rs              filesystem (default at setup)
│   │       └── s3.rs                 S3 / MinIO
│   └── Cargo.toml
└── api/src/
    └── artifactory/                  Artifact management REST API (repos, tags, quota)
        └── mod.rs
```

---

## Database Schema

### Platform settings additions

```sql
ALTER TABLE platform_settings
  ADD COLUMN registry_storage      TEXT NOT NULL DEFAULT 'local',
  ADD COLUMN registry_local_path   TEXT,
  ADD COLUMN registry_s3_endpoint  TEXT,
  ADD COLUMN registry_s3_bucket    TEXT,
  ADD COLUMN registry_s3_access_key TEXT,
  ADD COLUMN registry_s3_secret_key TEXT,
  ADD COLUMN registry_s3_region    TEXT,
  ADD COLUMN registry_hostname     TEXT;  -- e.g. registry.shipyard.local
```

### Namespaces

One per org (shared) or one per project (isolated). Build cache namespaces are always per-project.

```sql
CREATE TABLE registry_namespaces (
  id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id      UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  project_id  UUID REFERENCES projects(id) ON DELETE CASCADE,  -- NULL = org-level
  slug        TEXT NOT NULL,   -- e.g. "myorg" or "myorg/myproject"
  is_public   BOOLEAN NOT NULL DEFAULT FALSE,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(org_id, project_id)
);
```

### Unified artifact table

Replaces the OCI-only `registry_tags` table. One row per `(namespace, repo, tag, kind)`.

```sql
CREATE TABLE artifacts (
  id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  kind         TEXT NOT NULL,             -- 'docker_image' | 'static_bundle' | 'edge_function' | 'build_cache' | ...
  namespace_id UUID NOT NULL REFERENCES registry_namespaces(id) ON DELETE CASCADE,
  repo         TEXT NOT NULL,             -- e.g. "api-service", "landing-site", "auth-fn"
  tag          TEXT NOT NULL,             -- "latest", "v1.2.3", git SHA
  manifest_digest TEXT NOT NULL REFERENCES registry_manifests(digest),
  size_bytes   BIGINT NOT NULL,
  metadata     JSONB NOT NULL DEFAULT '{}',  -- kind-specific fields (see below)
  pushed_by    UUID REFERENCES users(id),
  pushed_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(namespace_id, repo, tag, kind)
);
```

**`metadata` shape per kind:**

```jsonc
// docker_image
{ "architecture": "amd64", "os": "linux", "layers": 12 }

// static_bundle
{ "framework": "sveltekit", "build_command": "npm run build", "output_dir": "build", "file_count": 342 }

// edge_function
{ "runtime": "js", "entry_point": "index.js", "bundler": "esbuild" }

// build_cache
{ "tool": "npm", "lock_file_hash": "sha256:abc...", "cache_key": "npm-lock-abc123" }
```

### Shared blob store (unchanged from original plan)

```sql
CREATE TABLE registry_blobs (
  digest      TEXT PRIMARY KEY,           -- sha256:abc...
  size_bytes  BIGINT NOT NULL,
  storage_key TEXT NOT NULL,              -- S3 key or local path
  ref_count   INT NOT NULL DEFAULT 0,     -- for GC
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE registry_manifests (
  digest      TEXT PRIMARY KEY,
  media_type  TEXT NOT NULL,
  content     TEXT NOT NULL,              -- raw OCI manifest JSON
  size_bytes  BIGINT NOT NULL,
  created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
```

### External registries

```sql
CREATE TABLE external_registries (
  id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  org_id       UUID NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
  name         TEXT NOT NULL,
  registry_url TEXT NOT NULL,             -- e.g. registry-1.docker.io
  username     TEXT,
  password_enc TEXT,                      -- encrypted at rest
  created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
  UNIQUE(org_id, registry_url)
);
```

---

## Deploy Engine Integration

At deploy time the engine:

1. Calls `GET /v2/<ns>/<repo>/manifests/<tag>` → gets the OCI manifest
2. Reads the `org.shipyard.artifact.kind` annotation to know how to handle it
3. Pulls the blob(s) via `GET /v2/<ns>/<repo>/blobs/<digest>`
4. Dispatches to the kind-specific deploy handler:

| Kind | Deploy action |
|---|---|
| `docker_image` | `docker pull` from registry, `docker service update` |
| `static_bundle` | unzip to serving directory on the static node |
| `edge_function` | copy bundle to edge nodes, reload edge runtime |
| `build_cache` | restore to build workspace before build starts |

---

## Permission Model

Same `shipyard:<org_id>:<resource>:<action>` pattern:

| Permission | Grants |
|---|---|
| `shipyard:<org_id>:registry:read` | Pull any artifact from org namespace |
| `shipyard:<org_id>:registry:write` | Push any artifact to org namespace |
| `shipyard:<org_id>:registry:admin` | Manage repos, delete tags, manage external registries |
| `shipyard:<org_id>:<proj_id>:registry:read` | Pull from project namespace only |
| `shipyard:<org_id>:<proj_id>:registry:write` | Push to project namespace only |

Owners/admins bypass all checks. The token endpoint issues short-lived JWTs (15 min) scoped to `artifact:<namespace>/<repo>:pull|push`.

---

## OCI Auth Flow

```
docker push registry.shipyard.local/myorg/myproject/api:latest
│
├─ 1. Registry returns 401 + Www-Authenticate → Shipyard token endpoint
│
├─ 2. Client calls GET /auth/registry/token?scope=artifact:myorg/myproject/api:push
│       → validates Bearer token or Basic auth
│       → checks registry:write permission for namespace
│       → returns short-lived JWT with granted scope
│
└─ 3. Client retries push with JWT → registry accepts
```

Same flow for all artifact kinds. The client does not need to know the kind — it's embedded in the manifest annotations.

---

## Frontend Additions

**Settings → Artifactory:**
- First-setup wizard: choose Local disk or S3/MinIO, fill credentials
- Show registry hostname + copy-able `docker login` command
- Storage stats (total size, per-kind breakdown, top repos)

**Settings → External Registries:**
- Add / test / delete external registry credentials
- Shows which projects reference each registry

**Per-project → Artifactory tab:**
- List artifacts grouped by kind (`docker_image`, `static_bundle`, `edge_function`, `build_cache`)
- Per-repo tag list: size, digest (truncated), pushed-by, pushed-at, linked deployment
- Copy pull command for each tag
- Delete tag / GC untagged manifests

---

## Phases

| Phase | Status | Scope | Notes |
|---|---|---|---|
| 1 | ✅ Done | DB migrations + `StorageBackend` trait (local + S3) | 5 migrations (040–044); local impl ships; S3 impl stubbed (needs Rust ≥1.94.1 for aws-sdk-s3 v1) |
| 2 | ✅ Done | Core OCI registry crate — blobs, manifests, chunked upload/download | `crates/registry` created; `blobs.rs` + `manifests.rs` wired; mounted in API via `FromRef<AppState>` |
| 3 | ✅ Done | `kinds.rs` — per-kind manifest builder + annotation parser | `org.shipyard.artifact.kind` annotation; `wrap_single_blob()` for engine use |
| 4 | ✅ Done | Docker token auth endpoint + permission wiring | `GET /auth/registry/token`; 15 min JWT; `GET /v2/` returns 401 + `Www-Authenticate` challenge |
| 5 | ✅ Done | Build engine integration — push `docker_image`, `static_bundle`, restore `build_cache` | `ArtifactPusher` in registry crate; engine calls it after each build (best-effort, non-fatal); 7 call sites updated |
| 6 | ✅ Done | External registry CRUD + credential encryption | AES-256-GCM encryption; `/orgs/:org_id/registry/*` endpoints + admin endpoints |
| 7 | ✅ Done | Frontend: sidebar + tenant Registry page + admin Registry page | `Package` icon in tenant sidebar above Settings; Registry in admin Infrastructure group; two route pages |

## Implementation Notes

- **Rust version constraint**: `aws-sdk-s3` v1 requires Rust ≥1.94.1. The S3 backend is compiled in behind the `s3` feature flag and the stub gracefully warns + falls back to local disk until the toolchain is upgraded.
- **State wiring**: Registry handlers use `State<RegistryState>`; the outer `AppState` satisfies the extractor via `impl FromRef<AppState> for RegistryState`. The router is generic: `registry_router::<S>()` where `RegistryState: FromRef<S>`.
- **No DATABASE_URL needed at compile time**: Registry crate uses dynamic `sqlx::query()` calls (not `query!` macros) so it builds without a live DB or `.sqlx` cache.
