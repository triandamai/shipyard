---
title: Nomi integration
description: Instructions for the coding agent implementing Nomi's Shipyard integration: org provisioning, dev databases, secrets, publish, environments, webhooks and termination.
---

# Nomi integration

This page is written for the coding agent that implements Nomi's side of the Shipyard integration. Read all of it before writing code. Every request and response shape here matches the server; when this page and your assumptions disagree, this page wins.

Nomi is an agent orchestrator that builds web apps (SvelteKit, Next.js, React, Vue; Postgres with Drizzle). Shipyard is where those apps are deployed and published. Nomi does development in its own containers; Shipyard provides the development database, runs published environments, owns their web addresses, and reports everything back through webhooks.

| Item | Value |
|---|---|
| Base URL | `https://<shipyard>/openapi/v1/integration` |
| Auth | `Authorization: Bearer ship_int_<64 hex>` |
| Content type | `application/json`, except publish (`multipart/form-data`) |
| Response envelope | `{ "data": ..., "error": null }` or `{ "data": null, "error": { "code", "message" } }` |

## Rules {#rules}

Follow these without exception.

1. **Secret values never enter a model's context.** Not in prompts, tool results, logs shown to the agent, or files the agent can read. The agent works with secret **refs** (`sec_...`) and env var **names** only.
2. **Use two keys.** The key available to agent tools has only the `manage` scope. Only Nomi's server-side code that injects env vars into dev containers, or shows a user their own database login, holds a key with `secrets:reveal`. Reveal calls are audit-logged.
3. **Creating orgs, apps, dev databases and secrets is an idempotent `PUT`** keyed by Nomi's own ids: retry freely on network errors and 5xx. Publish and duplicate are `POST`s; before retrying one, check the environment's last publish (`GET .../envs/{env}`).
4. **Verify every webhook signature** and treat delivery as at-least-once and unordered: dedupe on the event `id`, order by `created_at`.
5. **Prefer webhooks to polling.** If you must poll, poll the publish (`GET .../publishes/{id}`) no faster than every 3 seconds.
6. **Don't send `node_modules`, `.git`, build output or `.env` files** in the source zip. Shipyard builds from source.

## How Nomi maps to Shipyard {#mapping}

| Nomi | Shipyard | Identifier you use |
|---|---|---|
| Organization (or personal workspace) | Organization | Your org id, as `{org}` (`external_id`) |
| Project | App | A slug you choose, as `{app}` (`a-z0-9-`, max 63) |
| Development | Dev database on a shared Postgres server | (per app) |
| Published app | Environment: one Shipyard project per environment | `{env}`: `production`, `staging`, `preview-123`, ... |
| Secrets the user enters | Secret store, app-wide or per environment | Env var name, e.g. `STRIPE_KEY` |

Web addresses belong to Shipyard: production gets `https://<app>.<apps-domain>`, other environments `https://<app>-<env>.<apps-domain>`. If a name is taken, Shipyard appends a random suffix. Always use the URL Shipyard returns; never construct it.

## Setup {#setup}

The Shipyard operator configures, in `/opt/shipyard/.env`:

```bash
# Published apps get <app>.<this> and <app>-<env>.<this>; needs a wildcard DNS record to Traefik.
SHIPYARD__INTEGRATION__APPS_BASE_DOMAIN=apps.example.dev
# Postgres server for dev databases; the role needs only CREATEDB and CREATEROLE.
SHIPYARD__INTEGRATION__DEV_DB_ADMIN_URL=postgres://devdb_admin:...@devdb.internal:5432/postgres
# How dev containers reach that server, if different from the admin URL.
SHIPYARD__INTEGRATION__DEV_DB_PUBLIC_HOST=devdb.example.dev
SHIPYARD__INTEGRATION__DEV_DB_PUBLIC_PORT=5432
SHIPYARD__INTEGRATION__DEV_DB_SSLMODE=require
# Tenants must not reach each other (see Guide → Tenant isolation).
SHIPYARD__DOCKER__TENANT_ISOLATION=true
```

Then a Shipyard superadmin creates the integration and its keys once, in the dashboard API (session auth):

```bash
# create the integration
curl -X POST https://<shipyard>/api/admin/integrations \
  -H "Authorization: Bearer <admin session token>" -H "Content-Type: application/json" \
  -d '{"name":"nomi"}'

# key for agent tools and normal operations
curl -X POST https://<shipyard>/api/admin/integrations/<integration_id>/keys \
  -H "Authorization: Bearer <admin session token>" -H "Content-Type: application/json" \
  -d '{"name":"nomi-agent","scopes":["manage"]}'

# key for the server code that injects secrets
curl -X POST https://<shipyard>/api/admin/integrations/<integration_id>/keys \
  -H "Authorization: Bearer <admin session token>" -H "Content-Type: application/json" \
  -d '{"name":"nomi-injector","scopes":["manage","secrets:reveal"]}'
```

The full key is returned once. Store both in Nomi's server environment:

```bash
SHIPYARD_URL=https://<shipyard>/openapi/v1/integration
SHIPYARD_KEY=ship_int_...          # manage
SHIPYARD_REVEAL_KEY=ship_int_...   # manage + secrets:reveal, server-only
SHIPYARD_WEBHOOK_SECRET=whsec_...  # from POST /webhooks, see below
```

Then register Nomi's webhook endpoint once (see [Webhooks](#webhooks)).

## 1. A user creates an organization {#flow-org}

When an organization (or a user's personal workspace) is created in Nomi, create it in Shipyard in the same request handler. Shipyard returns `201` the first time and `200` after, with the same org.

### PUT /openapi/v1/integration/orgs/{org}

```json
// request
{ "name": "Ana's Workspace", "plan": "free" }
// response 201
{ "data": {
  "id": "01a1...", "external_id": "org_2Nf8sK", "name": "Ana's Workspace",
  "slug": "ana-s-workspace-k3x9qa", "plan": "free", "created_at": "...", "created": true
} }
```

`plan` is optional (`free` on create). Send it again to change the plan. Renames: send the new `name`.

### GET /openapi/v1/integration/orgs/{org}

Returns the org and its apps with their environments.

## 2. A user creates a project and starts building {#flow-dev}

Create the app, with its dev database if the project needs Postgres:

### PUT /openapi/v1/integration/orgs/{org}/apps/{app}

```json
// request
{ "name": "Todo", "dev_database": true }
// response 201
{ "data": {
  "key": "todo", "name": "Todo", "created": true,
  "dev_database": {
    "database": "dev_01a1...", "host": "devdb.example.dev", "port": 5432,
    "app_role": "dev_01a1..._app", "owner_role": "dev_01a1..._owner",
    "app_url_ref": "sec_61490a48...", "owner_url_ref": "sec_b76e4154...",
    "created": true
  }
} }
```

You can also create or fetch the dev database on its own with `PUT` / `GET .../apps/{app}/dev-database`, and drop it with `DELETE`.

**Wiring the dev container.** In the code that starts the dev container (not in agent tools):

1. Reveal `app_url_ref` with the reveal key: `POST /secrets/{app_url_ref}/reveal` returns `{ "value": "postgres://..." }`.
2. Set it as `DATABASE_URL` in the container's environment.
3. Tell the agent only that `DATABASE_URL` is set. Drizzle reads `process.env.DATABASE_URL`; `drizzle-kit push` or `migrate` work against it.

**The user's own login.** `owner_url_ref` is the user's credential for their database client. Reveal it only in response to the user's own request in Nomi's UI, and show it only to them. The owner role can read and change everything the app creates.

Each app's database is private: other apps' roles can't connect to it. The app role has a 20-connection limit; use a pool of 5 or fewer in dev.

## 3. A user publishes {#flow-publish}

When the user clicks publish, send the source and a manifest. Shipyard builds it, provisions what the manifest asks for (database, volumes, domain, generated secrets) and deploys, database first.

### POST /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/publish

`multipart/form-data` with two parts:

| Part | Content |
|---|---|
| `manifest` | JSON, below |
| `source` | Zip of the project (optional: omit it to re-apply a changed manifest to the last upload) |

```bash
curl -X POST "$SHIPYARD_URL/orgs/org_2Nf8sK/apps/todo/envs/production/publish" \
  -H "Authorization: Bearer $SHIPYARD_KEY" \
  -F "manifest=<manifest.json" -F "source=@todo.zip"
```

Response `202`:

```json
{ "data": {
  "publish_id": "01a1...", "app": "todo", "environment": "production",
  "project_id": "01a1...", "status": "running",
  "services": { "web": "01a1..." },
  "url": "https://todo.apps.example.dev",
  "warnings": []
} }
```

The environment is created on first publish. The `url` is reserved immediately; it serves the app once `publish.succeeded` arrives.

### The manifest {#manifest}

```json
{
  "services": [
    {
      "name": "web",
      "kind": "web",
      "path": ".",
      "port": 3000,
      "replicas": 1,
      "env": {
        "NODE_ENV": "production",
        "ORIGIN": "https://todo.apps.example.dev",
        "SESSION_SECRET": { "generate": "hex32" },
        "STRIPE_KEY": { "secret": "STRIPE_KEY" },
        "DATABASE_URL": { "database": "url" }
      },
      "volumes": [{ "path": "/app/uploads" }]
    }
  ],
  "database": { "engine": "postgres" },
  "domain": { "subdomain": "todo" }
}
```

| Field | Meaning |
|---|---|
| `services[].name` | `a-z0-9-`, unique, not `db`. Keep it stable: same name = same service, updated in place |
| `services[].kind` | `web` (default; serves HTTP on `port`, gets the URL) or `worker` |
| `services[].path` | Directory in the zip for this service (monorepos). Default `.` |
| `services[].dockerfile` | Dockerfile relative to `path`. Omit to use `Dockerfile`, or auto-detection (SvelteKit, Next.js, Nuxt, Angular, TanStack Start) when there is none |
| `services[].port` | Port the app listens on. Required for `web` |
| `services[].replicas` | 1–20, capped by the plan |
| `services[].cpu`, `memory_mb` | Optional limits (free plan caps at 0.5 CPU / 512 MB) |
| `services[].env` | The service's complete env. Keys not listed are removed |
| `services[].volumes` | Paths whose data survives deploys |
| `database` | `{ "engine": "postgres" }` gives the environment its own Postgres with a persistent volume |
| `domain.subdomain` | First label of the hostname. Default `<app>` (production) or `<app>-<env>` |
| `domain.service` | Which web service gets the URL. Default: the first web service |

Env values:

| Value | Result |
|---|---|
| `"text"` | Plain value |
| `{ "generate": "hex32" }` | Generated once per environment, then kept. Kinds: `password`, `hex32`, `base64_32`, `uuid` |
| `{ "secret": "STRIPE_KEY" }` | A secret set with `PUT .../secrets/STRIPE_KEY`. Publish fails listing missing secrets |
| `{ "database": "url" }` | From the environment's database: `url`, `host`, `port`, `user`, `password`, `name` |

Unknown fields are rejected (a typo fails loudly instead of being ignored).

### Preparing the source {#prepare}

- **Listen on `0.0.0.0` and the manifest `port`.** Read `PORT` if your framework does.
- **SvelteKit:** use `@sveltejs/adapter-node`; set `ORIGIN` to the URL. Build output runs with `node build`.
- **Next.js:** `output: "standalone"` in `next.config`; run `node server.js`.
- **Static SPA (React, Vue):** a Dockerfile that builds and serves with a static server on the port.
- **Migrations:** run them on start, before listening. With Drizzle, call `migrate(db, { migrationsFolder: "./drizzle" })` from `drizzle-orm/node-postgres/migrator` in the server entry, and commit the generated `drizzle/` folder. Don't use `drizzle-kit push` in production.
- **Health:** respond `200` on `/` or a cheap route within a few seconds of starting.

A Dockerfile that works for SvelteKit (adapter-node):

```dockerfile
FROM node:22-alpine AS build
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build && npm prune --omit=dev

FROM node:22-alpine
WORKDIR /app
COPY --from=build /app/build ./build
COPY --from=build /app/node_modules ./node_modules
COPY --from=build /app/package.json ./
COPY --from=build /app/drizzle ./drizzle
ENV PORT=3000
EXPOSE 3000
CMD ["node", "build"]
```

### Following a publish {#publish-status}

Webhooks tell you everything: `publish.started`, then for each service `deployment.started`, `deployment.step_started` / `deployment.step_succeeded` / `deployment.step_failed`, `deployment.succeeded` or `deployment.failed`, then `publish.succeeded` (with `urls`) or `publish.failed` (with `error`).

`publish.succeeded` means every service is running, not just deployed. Show the user the URL from it.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/publishes/{publish_id}

```json
{ "data": {
  "publish_id": "01a1...", "status": "succeeded",
  "urls": ["https://todo.apps.example.dev"], "error": null,
  "deployments": [
    { "service": "db", "deployment_id": "01a1...", "status": "success" },
    { "service": "web", "deployment_id": "01a1...", "status": "success" }
  ],
  "created_at": "...", "finished_at": "..."
} }
```

`status`: `running`, `succeeded` or `failed`. On failure, `error` names the service and the cause; fetch the deployment for the build log.

## 4. Environments {#flow-envs}

Each environment is independent: its own services, database, generated secrets and URL. Publish to any `{env}` name to create it. To start one from an existing environment:

### POST /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/duplicate

```json
// request: copy production into staging
{ "to": "staging", "copy_secrets": true }
```

The copy uses the source environment's last manifest and source, gets an **empty** database, newly generated secrets and its own URL, and deploys. `copy_secrets` copies environment-specific user secrets; app-wide secrets are shared anyway. Response: same as publish (`202`).

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}

Services with status, running replica count, URLs and last deployment, plus the last publish.

### DELETE /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}

Removes the environment: services, database and its data, volumes, domains and DNS records.

## 5. A user deletes a project or organization {#flow-terminate}

### DELETE /openapi/v1/integration/orgs/{org}/apps/{app}

Terminates the app everywhere: every environment (workloads, data, volumes, domains, DNS records, source), the dev database and its roles, and all secrets. Emits `app.terminated`. Irreversible: ask the user to confirm in Nomi first.

### DELETE /openapi/v1/integration/orgs/{org}

Terminates every app of the org, then deletes the org. Emits `org.deleted`.

## Secrets {#secrets}

Users enter API keys and other secrets in Nomi's UI; Nomi's server stores them in Shipyard. The agent may name the secrets an app needs (`STRIPE_KEY`) and reference them in the manifest; it never sees or sets values.

### PUT /openapi/v1/integration/orgs/{org}/apps/{app}/secrets/{name}

```json
// set a value (from the user, server-side only)
{ "value": "sk_live_...", "environment": "production" }
// or let Shipyard generate one
{ "generate": "hex32" }
// response: a ref, never the value
{ "data": { "ref": "sec_07db...", "name": "STRIPE_KEY", "environment": "production", "bound_env_vars_updated": 1 } }
```

Omit `environment` for an app-wide secret; an environment-specific one wins over it. Changing a secret updates every env var bound to it; services pick it up on their next publish or restart.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/secrets

Names, refs, kinds, environments and where each is used (`production/web:STRIPE_KEY`). No values.

### DELETE /openapi/v1/integration/orgs/{org}/apps/{app}/secrets/{name}

`?environment=` for an environment-specific one. Refused while an env var uses it.

### POST /openapi/v1/integration/secrets/{ref}/reveal

Needs `secrets:reveal`. Returns `{ "ref", "name", "value" }`. Server-side only; see [Rules](#rules).

## Webhooks {#webhooks}

### POST /openapi/v1/integration/webhooks

```json
// request
{ "url": "https://nomi.example.com/hooks/shipyard", "events": ["*"] }
// response: the secret is shown once
{ "data": { "id": "01a1...", "url": "...", "events": ["*"], "org": null, "secret": "whsec_..." } }
```

`events` takes exact types (`deployment.failed`), prefixes (`deployment.*`) or `*`. Add `"org": "<org>"` to limit an endpoint to one org. Manage with `GET /webhooks`, `PATCH /webhooks/{id}` (`url`, `events`, `enabled`), `DELETE /webhooks/{id}`, `POST /webhooks/{id}/rotate-secret`, and `POST /webhooks/{id}/test` (sends a signed `ping` and returns the result).

Each request is a `POST` with this JSON body:

```json
{
  "id": "01a12df4-...",
  "type": "publish.succeeded",
  "created_at": "2026-10-12T02:32:49.966835Z",
  "org": { "id": "01a1...", "external_id": "org_2Nf8sK" },
  "app": "todo",
  "environment": "production",
  "project_id": "01a1...",
  "service_id": null,
  "service": null,
  "data": { "publish_id": "01a1...", "urls": ["https://todo.apps.example.dev"], "error": null }
}
```

Headers: `Shipyard-Event-Id`, `Shipyard-Event-Type`, `Shipyard-Delivery-Id`, and `Shipyard-Signature: t=<unix seconds>,v1=<hex>`, where `v1 = HMAC-SHA256(secret, "<t>.<raw body>")`.

Verify before parsing (Node):

```ts
import crypto from "node:crypto";

export function verifyShipyard(rawBody: Buffer, header: string, secret: string, toleranceSec = 300): boolean {
  const parts = Object.fromEntries(header.split(",").map((p) => p.split("=", 2)));
  const t = Number(parts.t);
  if (!t || Math.abs(Date.now() / 1000 - t) > toleranceSec) return false;
  const expected = crypto.createHmac("sha256", secret).update(`${t}.`).update(rawBody).digest("hex");
  const given = Buffer.from(parts.v1 ?? "", "hex");
  const want = Buffer.from(expected, "hex");
  return given.length === want.length && crypto.timingSafeEqual(given, want);
}
```

Reply `2xx` within 10 seconds; do slow work after replying. Anything else is retried after 10 s, 30 s, 2 min, 10 min, 30 min, 1 h, 2 h, 4 h, 8 h and 12 h, then marked failed. `GET /webhooks/{id}/deliveries?status=failed` lists failures; `POST /deliveries/{id}/redeliver` retries one.

### Event types {#event-types}

| Type | When | `data` |
|---|---|---|
| `org.created`, `org.deleted` | Org provisioned / deleted | `name`, `plan` / `apps_terminated` |
| `app.created`, `app.terminated` | App created / terminated | `app`, `environments` |
| `project.created`, `project.duplicated`, `project.deleted` | Environment created / copied / removed | `app`, `environment`, `from_environment` |
| `publish.started`, `publish.succeeded`, `publish.failed` | Publish lifecycle | `publish_id`, `urls`, `error`, `deployments` |
| `deployment.started`, `deployment.succeeded`, `deployment.failed` | Per-service deploy | `deployment_id`, `urls`, `image`, `error` |
| `deployment.step_started`, `deployment.step_succeeded`, `deployment.step_failed` | Build and deploy progress | `deployment_id`, `step`, `error` |
| `service.created`, `service.deleted`, `service.env_changed` | Service changes | `slug`, `type` |
| `service.crashed` | A container exited unexpectedly | `container_id`, `replica`, `exit_code` |
| `service.resource_alert` | CPU or memory over threshold | `metric`, `value`, `threshold` |
| `domain.attached`, `domain.removed` | Hostname added / removed | `hostname`, `tls` |
| `volume.created`, `volume.attached`, `volume.deleted` | Volume changes | `volume`, `mount_path` |
| `network.created`, `network.attached`, `network.detached`, `network.deleted` | Network changes | `network_id` |
| `dev_database.created`, `dev_database.deleted` | Dev database | `database`, `app_url_ref`, `owner_url_ref` |
| `secret.created`, `secret.updated`, `secret.deleted` | Secret store (never the value) | `ref`, `name`, `environment` |
| `database.provisioned` | An environment's Postgres was created | `service`, `host` |

`GET /event-types` lists them. The same envelopes are available as a feed: `GET /orgs/{org}/events?after=<event id>&type=deployment.*&app=todo&limit=100` returns `{ "events": [...], "next_after": "<id>" }`, oldest first. Use it to catch up after downtime. Events are kept 30 days.

## Logs, deployments and usage {#observability}

Show users failures in their own words; give the agent the same logs so it can fix the app. Secret values are masked as `****` in everything below.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/deployments/{deployment_id}

Status, image, steps with timings, and the build/deploy log (`logs: [{ level, message, at }]`).

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/services/{service}/logs

Runtime output of all replicas. Query: `tail` (default 200, max 5000), `since` (`10m`, `1h` or RFC 3339). Returns `{ "service", "lines": ["<RFC 3339 time> <line>", ...] }`.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/usage

Per running container: `cpu_percent`, `memory_usage_bytes`, `memory_limit_bytes`, network and block I/O, and configured limits. Containers on other nodes show `stats: null`.

## Errors and limits {#errors}

| Status | `error.code` | Typical cause | What to do |
|---|---|---|---|
| 400 | `BAD_REQUEST` | Invalid manifest, missing secret, bad name | Fix the request; the message says what |
| 401 | `UNAUTHORIZED` | Missing, wrong or revoked key | Check `SHIPYARD_KEY` |
| 403 | `FORBIDDEN` | Missing scope, plan limit | Use the reveal key / upgrade the plan |
| 404 | `NOT_FOUND` | Unknown org, app, env, or one another integration owns | Create it first (`PUT`) |
| 409 | `CONFLICT` | Environment exists (duplicate), secret in use | Pick another name / unbind first |
| 5xx | `INTERNAL_ERROR`, `DATABASE_ERROR`, `DOCKER_ERROR` | Shipyard or Docker problem | Retry with backoff |

Limits: source zip 200 MB (configurable), 20 services per environment, 1–20 replicas within the plan, projects per org per the plan (each environment is a project).

## Implementation checklist {#checklist}

- [ ] Server env: `SHIPYARD_URL`, `SHIPYARD_KEY` (manage), `SHIPYARD_REVEAL_KEY` (manage + reveal, never exposed to agent tools), `SHIPYARD_WEBHOOK_SECRET`.
- [ ] Webhook route: raw body, signature check, dedupe on event `id`, reply `2xx` fast, process async.
- [ ] Org created in Nomi → `PUT /orgs/{org}`. Org renamed → same call. Org deleted → `DELETE /orgs/{org}` after user confirmation.
- [ ] Project created → `PUT /orgs/{org}/apps/{app}` with `dev_database` when the stack uses Postgres.
- [ ] Dev container start → reveal `app_url_ref` server-side → `DATABASE_URL` in the container env. Agent sees the name only.
- [ ] "Database credentials" button → reveal `owner_url_ref` for that user only.
- [ ] Secrets form → `PUT .../secrets/{name}` from the server; agent tools can list names and refs, never values.
- [ ] Publish button → zip source (no `node_modules`, `.git`, `.env`, build output), build manifest, `POST .../publish`; show progress from `deployment.step_*`; on `publish.succeeded` show `urls[0]`; on `publish.failed` show `error` and fetch the deployment log for the agent.
- [ ] "New environment" → `POST .../envs/{env}/duplicate`.
- [ ] Project deleted → confirm → `DELETE /orgs/{org}/apps/{app}`.
- [ ] Logs tab → runtime logs and deployment logs; usage → `/usage`.
- [ ] Catch-up job: on start, page `GET /orgs/{org}/events?after=<last seen>` for orgs with activity.

## Not available yet {#roadmap}

- Dashboard access for Nomi users (members and login links). Nomi is the interface for now.
- Release commands as a separate step (run migrations on start instead).
- Preview environments that sleep when idle, and per-plan egress rules.
