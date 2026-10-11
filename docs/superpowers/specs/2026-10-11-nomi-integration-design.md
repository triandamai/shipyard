# Nomi on Shipyard: first-class integration

**Date:** 2026-10-11
**Status:** Design agreed in review (see "Decisions"); "What to prepare" lists the inputs needed before Phase 1. Supersedes Phase 1 of `2026-10-10-llm-support-design.md`.
**Companion:** `2026-10-11-nomi-integration-guide.md` (the developer guide, published at `/docs/nomi` once the endpoints ship).

## What we're building

Nomi is a coding agent that builds web apps (SvelteKit, Next.js, Nuxt, React/Vue/Svelte SPAs) on Postgres with Drizzle. Nomi already runs each user's **development** in its own web containers. Shipyard takes over from there: it is where those apps are **deployed and published**, and where development databases live.

Nomi is the interface. Shipyard is driven through the Open API, and a Nomi admin decides whether their users may also open the Shipyard dashboard.

## Decisions

| # | Decision |
| --- | --- |
| 1 | **Shipyard owns the web addresses.** Hostnames are generated under a Shipyard-managed base domain, like sandbox preview URLs. |
| 2 | **Flat hostnames, auto-generated.** `<app>-<env>.<base>` when that name is free; otherwise an auto-generated `app-<8 hex>.<base>`, exactly as sandboxes fall back to `preview-<id>.<base>`. |
| 3 | **Shipyard is for deployment only.** Development happens in Nomi's containers, so Shipyard environments are `staging` and `production` (plus optional previews later). No `dev` environment on Shipyard. |
| 4 | **Nomi is the interface; dashboard access is a Nomi admin's choice.** Off by default, granted per user through the API. |
| 5 | **Shipyard provides development databases** for Nomi's dev containers, from a shared Postgres (one database and role per app), so development doesn't need its own server per app. |
| 6 | **Base domain is Shipyard-branded** and dedicated to user apps, separate from the dashboard's domain (exact name chosen by the operator; see "What to prepare"). |
| 7 | **Preview environments** per branch or change, in Phase 2. |
| 8 | **Free plan sleeps idle environments**: staging and previews stop after inactivity and wake on the next request. Production never sleeps. |

## How it works

### Mapping

| Nomi | Shipyard |
| --- | --- |
| Organization | Organization (`external_id` = Nomi org id) |
| App | A group of projects sharing `app_key` |
| `staging`, `production` | One **project** per environment (`app_key` + `environment` columns, unique per org): own network, own Postgres, own variables |
| App's server | Service in that project, built from source (or `kind: static` for SPAs) |
| App's production/staging database | `postgres` service from the existing template, with its persistent volume |
| App's development database | A database + role on the shared **dev Postgres** (not a service per app) |

### Sequence

```text
Nomi user creates org "Acme"
  Nomi → POST /orgs {name: "Acme", external_id: "nomi_org_123"}        (integration key)
  ← org + org API key (Nomi stores it encrypted)

User creates app "shop"
  Nomi → POST /apps {app_key: "shop", environments: ["staging","production"],
                     database: "postgres", dev_database: true}          (org key)
  Shipyard: 2 projects, each with web + postgres, DATABASE_URL wired,
            hostnames shop-staging.<base> and shop-production.<base>
            (or app-3f9c1a2e.<base> if taken);
            a database "shop_acme_dev" + role on the shared dev Postgres
  ← environments with URLs, and the dev DATABASE_URL

User develops in Nomi
  Nomi's dev container uses the dev DATABASE_URL; drizzle-kit push/migrate
  runs against it from Nomi. Shipyard is not involved otherwise.

User clicks "Deploy to staging"
  Nomi → POST /apps/shop/builds (source.tar.gz, commit)  → image in registry
  Nomi → POST /apps/shop/environments/staging/deploy {build_id}
  Shipyard: release command (drizzle-kit migrate) in the new image
            → fails: stop, staging keeps the old version
            → passes: update service
  → webhook deployment.succeeded {url}

User clicks "Publish"
  Nomi → POST /apps/shop/environments/production/deploy {build_id}    (same digest)

Something breaks
  → webhook service.crashed {exit_code, log_tail}
  Nomi's agent reads logs, fixes the code in the dev container, redeploys
```

### Build: one image, promoted

Unchanged from review: Nomi uploads a source tarball, Shipyard builds it with the app's `Dockerfile` (or its SvelteKit / Next.js / Nuxt / Angular auto-detection) into its built-in registry, and the **same image digest** is deployed to staging and then production. SPAs upload a built `dist/` and are served by the static server. Nomi pushing its own images to the registry stays available as an alternative.

### Backend: the framework's own server, not edge functions

Unchanged: SvelteKit, Next.js and Nuxt contain their backend; it runs in the app's container. Edge functions (one shared Deno runtime per org, 10–30 s timeout, 1–10 MB bodies) stay for small side endpoints only.

### Web addresses (decisions 1 and 2)

Mirrors the sandbox preview setup, which already does this for `preview-<id>.<preview_base_domain>`:

- New setting `apps.base_domain`: a Shipyard-branded domain used only for user apps (for example `shipyardapp.dev`), never the dashboard's own domain, so a user's app can't read or set the dashboard's cookies. It is later submitted to the Public Suffix List so apps can't set cookies for each other either.
- One **wildcard DNS record** `*.<base>` pointing at Shipyard's Traefik, created through the existing Cloudflare integration's wildcard sync (the same code path as the sandbox preview wildcard), and a **wildcard certificate** via Traefik's DNS-01 resolver, so new apps are live instantly with no per-host certificate wait.
- Hostname per environment, single label so the one wildcard covers it:
  - `<app>-<env>.<base>` if that label is valid and unused (`shop-staging`, `shop-production`);
  - otherwise `app-<8 hex>.<base>`, generated and stored, never reused.
- Production may also get a **custom domain** later (`POST .../production/domains`); the generated hostname keeps working.

### Preview environments (decision 7)

- `POST /apps/:app/previews {name, build_id}` creates a short-lived environment (`name` is a branch or change label, e.g. `pr-42`) as its own project with web service, hostname `<app>-<name>.<base>` (or generated), and variables copied from staging.
- Database: a fresh database on the shared dev Postgres by default (cheap, isolated, empty plus migrations), or `"database": "staging-copy"` to restore a snapshot of staging's data.
- Redeploy with `POST /apps/:app/previews/:name/deploy {build_id}`; `DELETE` removes the project, its database and hostname. Previews untouched for 7 days are deleted automatically, with a webhook first.
- Counted against a per-plan preview limit.

### Sleeping when idle (decision 8)

Free plan only, for staging and previews (production never sleeps):

- **Idle** = no HTTP request for 30 minutes (per plan, configurable), read from Traefik access logs per router; deployed apps have no editor heartbeat like sandboxes do.
- On idle, the service is scaled to zero and its route is pointed at Shipyard's **cold-start responder** (the one sandbox previews already use): the next request shows a short "Starting…" page, scales the service back to one replica and reloads when it's healthy, typically a few seconds.
- Databases keep their data (persistent volumes); the dedicated staging Postgres sleeps with its app and starts first on wake.
- Nomi sees `environment.slept` / `environment.woke` webhooks and a `state: sleeping` field, and can wake an environment ahead of time with `POST .../wake`.

### Development databases (decision 5)

Nomi's dev containers need Postgres while the user builds. Running a Postgres container per app for development wastes memory, so Shipyard runs **one shared dev Postgres** (a regular Shipyard service in a system project, with a persistent volume) and hands out isolated databases on it:

- `POST /apps/:app/dev-database` creates database `<app>_<org>_dev` and a role that owns only that database (`CONNECT` revoked from `PUBLIC`, no superuser, connection limit per role), and returns its `DATABASE_URL`.
- `POST /apps/:app/dev-database/reset` drops and recreates it; `DELETE` removes it when the app is deleted.
- Reachable from Nomi's dev containers over **TLS on a dedicated port**, restricted to Nomi's egress IPs (Traefik TCP router with an IP allowlist), never public.
- Size and connection quotas per org plan; databases idle for 30 days are reported to Nomi before cleanup.
- **Opt-in alternative**: `"dev_database": "dedicated"` provisions a separate small Postgres service for that app instead of a shared database, for users who need extensions or a specific Postgres version.

Staging and production always get their own dedicated Postgres services (with persistent volumes, and stop-first restarts); they never share.

### Migrations

Per service **release command** run once per deploy in a one-off container from the new image with that environment's variables, before traffic moves. Nomi sets it to `npx drizzle-kit migrate` (or `node migrate.js`). A non-zero exit fails the deploy and leaves the previous version serving. During development, migrations run from Nomi's container against the dev database.

### Logs and errors

| What | Endpoint |
| --- | --- |
| Build logs | `GET /builds/:id/logs` (`?follow=1` streams) |
| Deploy logs, including migration output | `GET /deployments/:id/logs` |
| Runtime logs, merged across replicas | `GET /apps/:app/environments/:env/logs?since=&level=error&follow=1` |
| Crashes, OOM kills, restart loops, failing health checks | `GET /apps/:app/environments/:env/events` |
| Push to Nomi | Signed webhooks: `build.*`, `deployment.*`, `service.crashed` |

Env var values are masked in every log line returned.

### Dashboard access (decision 4)

Nomi is the only interface by default. A Nomi admin can grant a user access to the Shipyard dashboard for their org:

- `POST /orgs/:id/members {email, role}` with `role` ∈ `viewer` (read-only: logs, canvas, metrics) or `admin`; idempotent per email. Removing the user in Nomi calls `DELETE /orgs/:id/members/:email`.
- `POST /orgs/:id/login-links {email}` returns a single-use URL valid for 5 minutes, so an allowed user can jump from Nomi into the Shipyard dashboard without a separate password. Only issued for users already granted access.
- Default role for granted users is `viewer`, so a dashboard click can't change what Nomi manages unless the admin chose `admin`.
- Org setting `dashboard_access: off | invited` (managed with the integration key) is the master switch Nomi's admin toggles.

### Org provisioning

Integration key (`ship_int_…`, one per integrator, issued by a Shipyard superadmin) can only manage orgs it created:

- `POST /orgs {name, external_id, plan}`, idempotent on `external_id`, returns the org and an org-scoped key (shown once).
- `PATCH /orgs/:id`, `POST /orgs/:id/suspend`, `DELETE /orgs/:id` mirror Nomi's org lifecycle.

### Where it runs

One Shipyard control plane operated for Nomi. Orgs share worker nodes with per-org CPU/memory quotas; paid orgs can move to dedicated nodes with Shipyard's existing node provisioning. The shared dev Postgres runs on a node of its own so development load never touches production apps.

## API surface

All under `/openapi/v1`.

| Endpoint | Key | Phase |
| --- | --- | --- |
| `POST /orgs`, `PATCH /orgs/:id`, `POST /orgs/:id/suspend`, `DELETE /orgs/:id` | integration | 1 |
| `POST /apps`, `GET /apps/:app`, `DELETE /apps/:app` | org | 1 |
| `POST /apps/:app/dev-database`, `.../reset`, `DELETE` | org | 1 |
| `POST /apps/:app/builds`, `GET /builds/:id`, `GET /builds/:id/logs` | org | 1 |
| `POST /apps/:app/environments/:env/deploy {build_id}` | org | 1 |
| `GET /deployments/:id/logs` | org | 1 |
| `POST /apps/:app/environments/:env/rollback` | org | 2 |
| `POST /apps/:app/previews`, `.../previews/:name/deploy`, `DELETE .../previews/:name` | org | 2 |
| `POST /apps/:app/environments/:env/wake`; sleep on idle (free plan) | org | 2 |
| `PUT /apps/:app/environments/:env/variables` | org | 2 |
| `GET /apps/:app/environments/:env/logs`, `.../events` | org | 2 |
| `POST /webhooks` | org | 3 |
| `POST/DELETE /apps/:app/environments/production/domains` | org | 3 |
| `POST /orgs/:id/members`, `DELETE /orgs/:id/members/:email`, `POST /orgs/:id/login-links` | integration | 3 |

## Delivery

| Phase | Ships | Unblocks for Nomi |
| --- | --- | --- |
| **1. Provision, develop, deploy** | Integration keys and `POST /orgs`; `apps.base_domain` with wildcard DNS + certificate and generated hostnames; `POST /apps` (staging + production); shared dev Postgres and `dev-database` endpoints; source builds into the registry; deploy by build id; build and deploy logs | Nomi users get a dev database, and an app reaches a live staging URL |
| **2. Publish safely** | Release command for Drizzle, promote the same build to production, rollback, per-environment variables, runtime logs (SSE), events, preview environments, sleep-on-idle for free staging/previews | Staging → production with migrations, previews per change, a rollback button |
| **3. Operate** | Webhooks, custom domains for production, production backups, plan quotas, dashboard access (members + login links), `/docs/nomi` published | Production-ready and self-serve |

Each phase ends with an end-to-end test driven only through the Open API: a fixture SvelteKit + Drizzle app that gets a dev database, builds, deploys to staging, runs a migration and is promoted to production.

## What to prepare

### Shipyard operator

| Item | Why | Notes |
| --- | --- | --- |
| **Apps domain** (Shipyard-branded, e.g. `shipyardapp.dev`) | Generated hostnames for every user app | Must not be the dashboard domain. Registered and moved to Cloudflare. |
| **Cloudflare API token** for that zone | Wildcard `*.<base>` DNS record and DNS-01 wildcard certificate | Permissions: Zone:Read, DNS:Edit, limited to that zone. Connected to the DNS-owner org like the sandbox preview domain. |
| **Let's Encrypt contact email** | Certificate expiry and account notices | |
| **App worker nodes** | Where staging, previews and production run | Start with 2 × 4 vCPU / 8 GB; add nodes as orgs grow. Docker installed, joined with `worker-setup.sh`. |
| **Build capacity** | Source tarballs are built into images | A dedicated builder node (4 vCPU / 8 GB / 100 GB disk) keeps builds off app nodes. |
| **Dev Postgres node** | Shared development databases | 2 vCPU / 4 GB / 100 GB SSD to start, isolated from app nodes. Port (e.g. `5433`) open only to Nomi's egress IPs. |
| **Registry storage** | Built images for promote and rollback | Local disk on the control plane (≥ 200 GB) or S3-compatible storage; retention policy (e.g. keep last 20 builds per app). |
| **Backup bucket** (Phase 3) | Nightly `pg_dump` of production databases | S3-compatible bucket and keys; retention (e.g. 14 daily, 8 weekly). |
| **Plan limits** | Quotas Shipyard enforces per org | Free / paid values for apps, previews, CPU and memory per environment, build minutes, dev database size and connections, idle timeout. |
| **Abuse and terms** | Public hosting of user-generated apps | Terms of use for hosted apps, an abuse contact address on the apps domain, and the Public Suffix List submission. |
| **Integration key for Nomi** | Nomi's credential for orgs | Created once in Shipyard admin when Phase 1 ships, handed to Nomi through a secret manager. |

### Nomi

| Item | Why | Notes |
| --- | --- | --- |
| **Static egress IPs** of the dev containers | Dev database allowlist | List every IP the containers connect from; tell Shipyard before Phase 1 testing. |
| **Secret storage** | Integration key, per-org Shipyard keys, webhook secret | Encrypted at rest; org keys looked up by Nomi org id. |
| **Data model additions** | Link Nomi records to Shipyard | `org.shipyard_org_id`, `org.shipyard_key`, `app.app_key`, per build `build_id` and commit, per environment URL and state. |
| **Templates per framework** | Deterministic builds | `Dockerfile` (SvelteKit `adapter-node`, Next.js `output: 'standalone'`, Nuxt `node-server`), `.dockerignore`, listening on `PORT`, a `GET /healthz` route, `drizzle.config.ts` reading `DATABASE_URL`, migrations in `drizzle/` shipped in the image, and a `migrate.js` fallback. |
| **Packaging** | Upload source | `tar.gz` without `node_modules`, `.git` and build output; under 200 MB. SPAs: zip of `dist/`. |
| **Webhook endpoint** | Build, deploy, crash, sleep events | Public HTTPS URL, verifies `Shipyard-Signature` (HMAC-SHA256), idempotent on event id. |
| **Agent tools** | Let the agent publish and self-repair | `deploy_to_staging`, `publish`, `create_preview`, `read_build_logs`, `read_runtime_errors`; feed failed log tails back into the agent. |
| **UI** | Nomi is the interface | Deploy / publish buttons, environment URLs and states (including sleeping), logs view (SSE), write-only variables editor, previews list, rollback, and an admin-only toggle for Shipyard dashboard access. |
| **Preview trigger rule** | When a preview is created | Per branch, per change request, or manual; and the naming (`pr-42`, branch slug). |
| **Plan mapping** | Nomi plan → Shipyard plan | Which Nomi tiers are "free" (sleeping) on Shipyard. |

### Send to the Shipyard side before Phase 1 starts

1. The apps domain name, once it is on Cloudflare, and the scoped API token.
2. Nomi's dev-container egress IPs.
3. Nomi's webhook URL.
4. The plan limits table (free and paid).

## Out of scope

- Running Nomi's development containers on Shipyard (Nomi already does this).
- Managed Postgres features beyond one instance per staging/production environment (replicas, point-in-time recovery).
