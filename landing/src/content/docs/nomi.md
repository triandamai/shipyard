---
title: Nomi integration
description: Instructions for the coding agent implementing Nomi's Shipyard integration: org provisioning, dev databases, secrets, publish, environments, webhooks and termination.
---

# Nomi integration

This page is written for the coding agent that implements Nomi's side of the Shipyard integration. Read all of it before writing code. Every request and response shape here matches the server; when this page and your assumptions disagree, this page wins.

Nomi is an agent orchestrator that builds web apps (SvelteKit, Next.js, React, Vue; Postgres with Drizzle). Shipyard is where those apps are deployed and published. Nomi does development in its own containers; Shipyard provides the development database, runs published environments, owns their web addresses, and reports everything back through webhooks.

| Item | Value |
|---|---|
| Base URL | `https://<shipyard>/openapi/v1/integration` (also reachable as `/integration`, `/integrations` and the site root; use the first) |
| Auth | `Authorization: Bearer ship_int_<64 hex>` (see [Credentials](#credentials)) |
| Content type | `application/json`, except publish (`multipart/form-data`) |
| Response envelope | `{ "data": ..., "error": null }` or `{ "data": null, "error": { "code", "message" } }` |

## Quick start for the coding agent {#quickstart}

If you read nothing else, this is the whole integration in order:

1. `PUT /orgs/{org}` when Nomi creates an organization.
2. `PUT /orgs/{org}/apps/{app}` with `"dev_database": true` when a project is created. Keep the two `*_url_ref` values.
3. Server-side only: reveal `app_url_ref`, set it as `DATABASE_URL` in the dev container.
4. On publish: zip the source, build the [manifest](#manifest), `POST .../envs/{env}/publish`. You get `202` and a `publish_id`. **The app is not live yet.**
5. Wait for the `publish.succeeded` webhook (or poll `GET .../publishes/{publish_id}` every 3 s). Only then show the user `urls[0]`.
6. If it failed, `GET .../deployments/{deployment_id}` returns `failure` with the step, phase, message and log tail. Fix the code, publish again. See [the fix loop](#fix-loop).
7. User wants another address: `GET /orgs/{org}/domains/check?subdomain=...` as they type, then `PUT .../envs/{env}/domain`. See [Choosing a domain](#domains).
8. `POST .../duplicate` for another environment, `DELETE .../apps/{app}` to terminate.

Before you ship, run the readiness checker against a real Shipyard (it creates and deletes a throwaway org): `python3 scripts/check-nomi-integration.py` in the Shipyard repo. See [Verifying a Shipyard](#verify).

## Credentials {#credentials}

Three kinds of credential work on this API. Each only reaches the organizations it owns.

| Credential | Reaches | Can do |
|---|---|---|
| Integration key `ship_int_...` | Orgs created through it (`PUT /orgs/{org}`), plus orgs a superadmin linked to the integration | Everything in this page, within its scopes (`manage`, `secrets:reveal`) |
| Org API key `ship_...` (created in the dashboard) | Only its own org | Same routes by `manage`-equivalent scopes: needs `write` or `admin`. Cannot create orgs, change plans, manage webhooks or delete the org |
| User access token (JWT) | Orgs where the user is a member | Same as an org key by role. Cannot create orgs, change plans, manage webhooks or delete the org |

Use an integration key for everything Nomi's server does. Integration keys never see an org they did not create, so an org that already existed in Shipyard (made in the dashboard) is invisible until a superadmin links it:

```bash
curl -X PUT https://<shipyard>/api/admin/integrations/<integration_id>/orgs/<org_uuid> \
  -H "Authorization: Bearer <admin session token>" -H "Content-Type: application/json" \
  -d '{"external_id":"org_2Nf8sK"}'   # your id for it; DELETE the same path to unlink
```

## Rules {#rules}

Follow these without exception.

1. **Secret values never enter a model's context.** Not in prompts, tool results, logs shown to the agent, or files the agent can read. The agent works with secret **refs** (`sec_...`) and env var **names** only.
2. **Use two keys.** The key available to agent tools has only the `manage` scope. Only Nomi's server-side code that injects env vars into dev containers, or shows a user their own database login, holds a key with `secrets:reveal`. Reveal calls are audit-logged.
3. **Creating orgs, apps, dev databases and secrets is an idempotent `PUT`** keyed by Nomi's own ids: retry freely on network errors and 5xx. Publish and duplicate are `POST`s; before retrying one, check the environment's last publish (`GET .../envs/{env}`).
4. **Verify every webhook signature** and treat delivery as at-least-once and unordered: dedupe on the event `id`, order by `created_at`.
5. **Prefer webhooks to polling.** If you must poll, poll the publish (`GET .../publishes/{id}`) no faster than every 3 seconds.
6. **Don't send `node_modules`, `.git`, build output or `.env` files** in the source zip. Shipyard builds from source.
7. **Accepted is not live.** `202` from publish or duplicate means the build started. Never show the user a link, or tell them the app is up, before `publish.succeeded` (or `status: "succeeded"` and `live: true` on the environment). Before then the URL returns a Shipyard 502/404 page.
8. **Never invent a URL.** Use `urls` from a succeeded publish/deployment. Failed and running deployments have none.
9. **A failed publish does not take the previous version down.** The old release keeps serving until a new one succeeds, so a failure is safe to retry.

## How Nomi maps to Shipyard {#mapping}

| Nomi | Shipyard | Identifier you use |
|---|---|---|
| Organization (or personal workspace) | Organization | Your org id, as `{org}` (`external_id`) |
| Project | App | A slug you choose, as `{app}` (`a-z0-9-`, max 63) |
| Development | Dev database on a shared Postgres server | (per app) |
| Published app | Environment: one Shipyard project per environment | `{env}`: `production`, `staging`, `preview-123`, ... |
| Secrets the user enters | Secret store, app-wide or per environment | Env var name, e.g. `STRIPE_KEY` |

Web addresses belong to Shipyard: production gets `https://<app>.<apps-domain>`, other environments `https://<app>-<env>.<apps-domain>`. If a name is taken, Shipyard appends a random suffix. Always use the URL Shipyard returns; never construct it. To let a user pick their own name, see [Choosing a domain](#domains).

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
{ "name": "Ana's Workspace", "plan": "free", "labels": { "nomi.app/workspace": "personal" } }
// response 201
{ "data": {
  "id": "01a1...", "external_id": "org_2Nf8sK", "name": "Ana's Workspace",
  "slug": "ana-s-workspace-k3x9qa", "plan": "free", "created_at": "...",
  "integration": "nomi",
  "labels": { "shipyard.io/integration": "nomi", "nomi.app/workspace": "personal" },
  "created": true
} }
```

`plan` is optional (`free` on create). Send it again to change the plan. Renames: send the new `name`. `labels` is optional; see [Labels and versions](#labels). `integration` is the integration that created the org, and the Shipyard dashboard shows it as a badge on the org.

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

The environment is created on first publish. `url` is the address reserved for it, and `live` is `false`: nothing answers there until `publish.succeeded` arrives. Treat `url` as a name, not a link, until then.

A publish is refused with `403` if the org's billing state doesn't allow deploys (canceled or past due), and with `400` for an invalid manifest, a missing secret, or a zip that doesn't contain the service `path`.

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
  "domain": { "subdomain": "todo" },
  "labels": { "nomi.app/template": "nextjs" }
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
| `domain.subdomain` | First label of the hostname. Default `<app>` (production) or `<app>-<env>`. You can send the same manifest to every environment: if the name is already used by another environment, a non-production environment gets `<subdomain>-<env>` instead (`todo` → `todo-staging`). Only used the first time: once an environment has an address, change it with [`PUT .../domain`](#domains) |
| `domain.service` | Which web service gets the URL. Default: the first web service |
| `labels` | Labels for the environment's project. See [Labels and versions](#labels) |
| `services[].labels` | Labels for that service |

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

**Status vocabulary** (everywhere in this API): publish and deployment `status` is `running`, `succeeded` or `failed`. Shipyard's internal value is kept in `raw_status` for debugging; branch on `status`. Step `status` is `succeeded`, `failed`, `running`, `pending` or `skipped` (steps after a failed one are `skipped`). Each step has a `phase`: `validate`, `build`, `configure`, `deploy` or `finalize`.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/publishes/{publish_id}

```json
{ "data": {
  "publish_id": "01a1...", "status": "succeeded",
  "urls": ["https://todo.apps.example.dev"], "error": null,
  "deployments": [
    { "service": "db", "deployment_id": "01a1...", "status": "succeeded", "raw_status": "success" },
    { "service": "web", "deployment_id": "01a1...", "status": "succeeded", "raw_status": "success" }
  ],
  "created_at": "...", "finished_at": "..."
} }
```

On failure, `error` names the service and the cause; fetch that deployment ([below](#deployment-check)) for the step, phase and log tail. Other lookups:

- `GET /orgs/{org}/publishes/{publish_id}`: same body, when you only have the id.
- `GET /orgs/{org}/apps/{app}/publishes?limit=20`: recent publishes, newest first.

## 4. Environments {#flow-envs}

Each environment is independent: its own Shipyard project with its own services, database, generated secrets, volumes and URL. Publish to any `{env}` name (`production`, `staging`, `dev`, `preview-123`, ...) to create it. You can publish the same manifest and source to each one, and publishing one environment never touches another. To start one from an existing environment:

### POST /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/duplicate

```json
// request: copy production into staging
{ "to": "staging", "copy_secrets": true }
```

The copy uses the source environment's last manifest and source, gets an **empty** database, newly generated secrets and its own URL, and deploys. `copy_secrets` copies environment-specific user secrets; app-wide secrets are shared anyway. Response: same as publish (`202`).

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}

Services with status, running replica count, URLs, last deployment, `version` and `labels`, plus the environment's `labels` and the last publish.

```json
{ "data": {
  "environment": "production", "project_id": "01b2...",
  "labels": { "shipyard.io/integration": "nomi", "shipyard.io/app": "todo", "shipyard.io/environment": "production" },
  "services": [{
    "name": "web", "status": "running", "live": true, "urls": ["https://todo.apps.example.dev"],
    "version": "3f2a91c",
    "labels": { "shipyard.io/integration": "nomi", "shipyard.io/app": "todo",
                "shipyard.io/environment": "production", "shipyard.io/version": "3f2a91c" },
    "...": "..."
  }],
  "last_publish": { "...": "..." }
} }
```

### DELETE /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}

Removes the environment: services, database and its data, volumes, domains and DNS records.

## Labels and versions {#labels}

Orgs, environments (projects) and services carry key/value labels. The Shipyard dashboard shows them as tags. The integration badge shows which platform manages an object, so support staff can tell a Nomi org from one created in the dashboard.

**Labels Shipyard sets.** Keys under `shipyard.io/` are written by Shipyard and are read-only for you. If you send one, the request is refused with `400`.

| Key | On | Value |
|---|---|---|
| `shipyard.io/integration` | org, project, service | The integration that manages it, e.g. `nomi` |
| `shipyard.io/app` | project, service | The `{app}` key |
| `shipyard.io/environment` | project, service | The `{env}` name |
| `shipyard.io/version` | service | The tag of the image the service runs, set after each successful deploy |

**Version.** Every git or publish build is tagged with the short commit sha of the source it built (the same `shipyard/<service>:<sha>` tag shown in the deploy log). When the deploy succeeds, that tag becomes the service's `shipyard.io/version`, and `GET .../envs/{env}` returns it as `version`. A failed deploy leaves the previous version in place, matching what is actually serving. Services that run a registry image get the image's tag (`1.4.2`, `latest`). Static sites get the commit sha.

**Your own labels.** Send `labels` on `PUT /orgs/{org}`, at the top level of the manifest (for the environment's project) and on each manifest service. Use them for anything Nomi wants to see next to the app in Shipyard, such as a template name, a Nomi build number or a release name.

```json
{ "labels": { "nomi.app/template": "nextjs", "nomi.app/release": "spring-launch", "old-label": null } }
```

- Labels are merged: keys you send are set, keys you leave out stay, and a `null` value removes that key.
- Keys: 1–63 characters of `a-z 0-9 . / _ -`. Prefix them with `nomi.app/` so they never clash with another integration's labels.
- Values: up to 256 characters. At most 32 labels per request.

## Choosing a domain {#domains}

Every environment gets `<name>.<apps-domain>` on its first publish. When a user wants a different name, use these endpoints. Names are shared across the whole platform, so a name another app already uses is refused, and Shipyard suggests free ones nearby.

Typing flow in Nomi: call **check** as the user types (debounced), show the answer, and call **change** when they confirm.

### GET /openapi/v1/integration/orgs/{org}/domains/check?subdomain=my-shop

Optional `app` and `env`: with them, the environment's own current name counts as available (so re-confirming the current name isn't an error).

```json
// free
{ "data": { "subdomain": "my-shop", "hostname": "my-shop.apps.example.dev", "available": true, "suggestions": [] } }

// already used by someone else
{ "data": {
  "subdomain": "todo", "hostname": "todo.apps.example.dev", "available": false,
  "reason": "taken", "message": "'todo' is already in use",
  "suggestions": [
    { "subdomain": "todo-app", "hostname": "todo-app.apps.example.dev" },
    { "subdomain": "my-todo",  "hostname": "my-todo.apps.example.dev" },
    { "subdomain": "todo-hq",  "hostname": "todo-hq.apps.example.dev" }
  ] } }

// not a valid name (letters, digits, '-', 3 to 63 characters, not reserved)
{ "data": { "subdomain": "ab_c", "hostname": null, "available": false,
            "reason": "invalid", "message": "Use only letters a-z, digits 0-9 and '-'", "suggestions": [ ... ] } }
```

The answer is `200` in every case; read `available`. Input is lowercased and trimmed. `reason` is `taken` or `invalid`. Reserved names (`www`, `api`, `admin`, `mail`, `status`, anything starting `preview-`, and similar) count as invalid. Suggestions are checked against the platform at the time of the call, but a name can still be taken a moment later, so handle `409` on change.

### PUT /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/domain

```json
// request
{ "subdomain": "my-shop" }
// response 200
{ "data": { "hostname": "my-shop.apps.example.dev", "url": "https://my-shop.apps.example.dev",
            "previous_hostname": "todo.apps.example.dev", "changed": true } }
```

- The new name is attached before the old one is removed, so the app stays reachable. The old name becomes free for others.
- TLS and the DNS record for the new name are set up the same way as on publish.
- Asking for the name the environment already has returns `200` with `"changed": false`.
- If the name is taken or invalid, **nothing changes** and you get `409`, with the same `suggestions` under `data`:

```json
{ "data": { "available": false, "reason": "taken", "suggestions": [ { "subdomain": "todo-app", "hostname": "todo-app.apps.example.dev" } ] },
  "error": { "code": "CONFLICT", "message": "'todo' is already in use" } }
```

- An environment that hasn't been published has no address yet: you get `409` until the first publish.
- Later publishes keep the chosen name. `domain.subdomain` in a manifest does not move an environment that already has an address.
- Emits `domain.attached` for the new name and `domain.removed` for the old one.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/domain

Returns `{ "hostname", "subdomain", "url" }` for the current platform address, or nulls when there is none yet.

Only the platform address (`<name>.<apps-domain>`) is managed here. Custom domains the user owns are not part of this API yet.

## Moving an app to another organization {#transfer}

When a project moves to another Nomi organization, move its Shipyard app too, so the two stay in step. Both organizations must belong to your integration (both created through it, or linked by a superadmin).

### POST /openapi/v1/integration/orgs/{org}/apps/{app}/transfer

```json
// request
{ "to_org": "org_9Zk2", "app_key": "todo-2" }   // app_key is optional
// response 200
{ "data": {
  "app": "todo", "from_org": "org_2Nf8sK", "to_org": "org_9Zk2",
  "environments": [ { "environment": "production", "project_id": "01a1...", "slug": "todo-production" } ],
  "redeploying": [ { "service": "web", "deployment_id": "01a1..." } ],
  "skipped": []
} }
```

What moves, in one step: the app, all its environments (production, staging, ...), their services and data, secrets (app-wide and per environment), the dev database, publish history and the image registry namespace. Nothing is copied, so **web addresses, volumes and databases stay as they are** and URLs keep working.

What you should know:

- **Containers restart once.** Each container service is redeployed from the image it already runs so it joins the new org's private network. Watch `redeploying[].deployment_id` like any deployment. Static sites don't restart. A service with no successful deployment is listed in `skipped`.
- **Same app key.** If the destination already has an app with this key you get `409`; send `app_key` to rename it on the way.
- **Slugs.** If a project slug is in use in the destination, the moved project gets a short suffix. The environment name does not change.
- **Refused with `409`** while a publish or deployment of the app is running (try again when it finishes). **`403`** when the destination's plan can't hold the app's environments, or its billing doesn't allow deploys. `400` if `to_org` is the app's current org, `404` if either org is unknown to your key.
- **Access.** Integration key only. Project-level access that was given to members of the old org is removed; the new org's members and roles apply.
- **Events.** `app.transferred` is emitted in both orgs (data: `app`, `from_org`, `to_org`, `environments`). After this call `GET /orgs/{old}/apps/{app}` returns `404` and the app answers under the new org.
- **Cloudflare.** DNS records created for the app's hostnames stay in place. If the new org uses a different Cloudflare connection, manage those hostnames there.

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
| `service.error_rate_alert` | 5xx error rate over threshold | `error_rate`, `threshold`, `requests`, `errors_5xx` |
| `domain.attached`, `domain.removed` | Hostname added / removed | `hostname`, `tls` |
| `volume.created`, `volume.attached`, `volume.deleted` | Volume changes | `volume`, `mount_path` |
| `network.created`, `network.attached`, `network.detached`, `network.deleted` | Network changes | `network_id` |
| `dev_database.created`, `dev_database.deleted` | Dev database | `database`, `app_url_ref`, `owner_url_ref` |
| `secret.created`, `secret.updated`, `secret.deleted` | Secret store (never the value) | `ref`, `name`, `environment` |
| `database.provisioned` | An environment's Postgres was created | `service`, `host` |

`GET /event-types` lists them. The same envelopes are available as a feed: `GET /orgs/{org}/events?after=<event id>&type=deployment.*&app=todo&limit=100` returns `{ "events": [...], "next_after": "<id>" }`, oldest first. Use it to catch up after downtime. Events are kept 30 days.

## Logs, deployments, usage and metrics {#observability}

Show users failures in their own words; give the agent the same logs so it can fix the app. Nomi's Monitor sheet (`MonitorPanel.svelte`) displays live usage, structured runtime logs, and 7-day traffic and container resource history. Secret values are masked as `****` in everything below.

### Checking a deployment {#deployment-check}

The same body comes from any of:

- `GET /orgs/{org}/apps/{app}/envs/{env}/deployments/{deployment_id}`
- `GET /orgs/{org}/deployments/{deployment_id}` (no app/env needed)
- `GET /deployments/{deployment_id}` (the id alone; the key's orgs are searched)

Query: `logs=false` returns status and steps only; `after=<timestamp>` returns log lines after the previous call's `next_after`; `logs_limit` (default 2000, max 5000).

```json
{ "data": {
  "deployment_id": "01a1...", "service": "web", "status": "failed", "raw_status": "failed",
  "image": null, "urls": [],
  "failure": {
    "step": "build_image", "phase": "build",
    "message": "npm run build exited with code 1",
    "tail": ["src/routes/+page.svelte:12:5 'x' is not defined", "error during build"]
  },
  "steps": [
    { "name": "clone_repo", "phase": "build", "status": "succeeded", "started_at": "...", "finished_at": "..." },
    { "name": "build_image", "phase": "build", "status": "failed", "error": "..." },
    { "name": "deploy", "phase": "deploy", "status": "skipped" }
  ],
  "logs": [ { "level": "error", "message": "...", "at": "..." } ],
  "next_after": "2026-10-16T09:00:00Z"
} }
```

`urls` is present only when `status` is `succeeded`. `failure` is `null` unless it failed. Poll with `after=next_after` to stream a running build without re-reading the log.

- `GET .../envs/{env}/deployments?service=web&limit=20`: recent deployments, newest first.
- `GET .../envs/{env}`: services with `status`, `running`/`replicas`, `urls`, `live` and the last deployment. `live` is `true` only when at least one replica is running and the service has a URL.

### Rollback and restart {#rollback}

- `POST .../envs/{env}/services/{service}/rollback`, body optional `{ "deployment_id": "..." }`. Redeploys the image of an earlier successful deployment (default: the one before the current), without a rebuild, so it takes seconds. Database migrations are **not** undone. Calling it twice flips between the two newest releases. It returns `400` if there is nothing to return to.
- `POST .../envs/{env}/restart`: restarts every service with its current env (use after changing a secret). Returns the services restarted.
- `GET .../envs/{env}/services/{service}/env`: the service's env vars as `[{ name, secret, secret_ref }]`. Values are never returned.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/services/{service}/logs

Runtime output of all replicas. Query: `tail` (default 200, max 5000), `since` (`10m`, `1h` or RFC 3339). Returns `{ "service", "lines": [{ "at": "<RFC 3339>", "level": "info|warn|error", "message": "...", "service": "web" }] }`. Levels are extracted from structured JSON output where available and infer stream/severity otherwise. Secret values are masked.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/usage

Per running container: `cpu_percent`, `memory_usage_bytes`, `memory_limit_bytes`, network and block I/O, and configured limits. Containers on other nodes show `stats: null`.

### GET /openapi/v1/integration/orgs/{org}/apps/{app}/envs/{env}/metrics

Bucketed traffic and container resource history. Query: `range` (`1h`, `24h`, `7d`; default `24h`), `step` (`1m`, `5m`, `1h`, `1d`; default matched to range).

```json
{
  "data": {
    "range": "24h",
    "step": "1h",
    "buckets": [
      {
        "at": "2026-10-26T09:00:00Z",
        "visitors": 34,
        "requests": 812,
        "errors_5xx": 2,
        "errors_4xx": 11,
        "p50_ms": 96,
        "p95_ms": 288,
        "containers": [
          { "service": "web", "container_id": "…", "cpu_percent": 21.4, "memory_percent": 58.1, "memory_bytes": 312475648, "memory_limit_bytes": 536870912 }
        ]
      }
    ],
    "totals": { "visitors": 412, "requests": 9811, "errors_5xx": 38, "errors_4xx": 120, "p50_ms": 120, "p95_ms": 310 },
    "previous_totals": { "visitors": 349, "requests": 8702, "errors_5xx": 41, "errors_4xx": 98, "p50_ms": 131, "p95_ms": 342 }
  },
  "error": null
}
```

Unmeasured metrics return `null`. Historical metrics are retained for at least 7 days.

## The fix loop for coding agents {#fix-loop}

When a publish fails the agent should read the cause, fix the source and publish again. Do not retry blindly.

```ts
async function publishUntilLive(ctx, maxAttempts = 3) {
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    const pub = await shipyard.publish(ctx);                 // 202
    const result = await waitForPublish(pub.publish_id);     // webhook, or poll every 3 s
    if (result.status === "succeeded") return result.urls[0];

    const failed = result.deployments.find((d) => d.status === "failed");
    const dep = await shipyard.getDeployment(failed.deployment_id);
    const { phase, step, message, tail } = dep.failure;

    if (phase === "validate") await agent.fixManifest(message);          // bad manifest, missing secret
    else if (phase === "build") await agent.fixCode(message, tail);       // compile/type/install errors
    else if (phase === "configure") await agent.askUser(message);         // secrets, limits, plan
    else /* deploy, finalize */ {
      // The image built but the app didn't come up: read runtime logs.
      const logs = await shipyard.serviceLogs(ctx, failed.service, { tail: 200 });
      await agent.fixRuntime(message, logs.lines);
    }
  }
  throw new Error("giving up after 3 failed publishes");
}
```

| `failure.phase` | Usual cause | Agent action |
|---|---|---|
| `validate` | Manifest rejected, secret not set, zip missing `path` | Fix the manifest or set the secret (ask the user for values) |
| `build` | `npm ci`/`build` failed, no Dockerfile detected, wrong Node version | Fix source; read `tail` |
| `configure` | Plan limit, invalid resources, domain taken | Ask the user or lower the request |
| `deploy` | Container exits at start, port mismatch, migration error, health check fails | Read service logs; listen on `0.0.0.0:$PORT`, check migrations |

## Framework recipes {#recipes}

Shipyard detects SvelteKit, Next.js, Nuxt, Angular and TanStack Start when there is no Dockerfile, and runs Node 22. If detection fails or you need control, ship a Dockerfile.

| Stack | Manifest service | Source requirements |
|---|---|---|
| SvelteKit | `{ "name":"web", "kind":"web", "port":3000 }` | `@sveltejs/adapter-node`; set `ORIGIN` to the public URL |
| Next.js | `{ "name":"web", "kind":"web", "port":3000 }` | `output: "standalone"` |
| Vite React/Vue (static) | `{ "name":"site", "kind":"static" }` | `npm run build` outputs `dist/`; no port needed. Served by nginx with SPA fallback |
| Node API + worker | two services, one `web`, one `worker` | Worker has no URL or port |

`kind: "static"` builds the site and serves the output with nginx; the port is detected, so omit `port`. A static site has no server env at runtime: bake public config at build time.

## Verifying a Shipyard {#verify}

Before relying on a Shipyard instance, run the checker from the Shipyard repo. It uses only the Python standard library, creates a throwaway org, and removes it afterwards:

```bash
SHIPYARD_URL=https://<shipyard>/openapi/v1/integration \
SHIPYARD_KEY=ship_int_... \
python3 scripts/check-nomi-integration.py --reveal-key ship_int_...   # reveal key optional
```

It exercises auth, org and app creation, dev database (refs only), secrets, webhooks (with a test ping), publishing a static site and a container, rollback, restart, a deliberately failing build (checks the `failure` object and that the old version keeps serving), duplicate, terminate and org delete. It prints `ok`/`FAIL` per check and exits non-zero on failures. Publish checks need the instance to be able to build and pull images, so run it against a real deployment, not a bare dev stack.

## Troubleshooting {#troubleshooting}

| Symptom | Cause | Fix |
|---|---|---|
| `404 NOT_FOUND` on an org that exists | Org was created in the dashboard, not via this integration | Link it with the claim endpoint ([Credentials](#credentials)) |
| `403` on `PUT /orgs/{org}` with an org key | Only integration keys create orgs | Use the integration key |
| `403` on reveal | Key lacks `secrets:reveal` | Use the reveal key, server-side only |
| Publish `403` | Org billing is canceled or past due | Fix the plan; `PUT /orgs/{org}` with `plan` if you own billing |
| URL shows a Shipyard 502 after publish | You opened `url` before `publish.succeeded` | Wait for the webhook |
| `publish.failed` at `validate` with "missing secret" | Manifest references `{ "secret": ... }` that was never set | `PUT .../secrets/{name}` first |
| Container starts then exits | Not listening on `0.0.0.0:$PORT`, or migration error | Read service logs |
| Webhook signature mismatch | Body was parsed before verifying | Verify against the raw bytes |

## Not available yet {#not-available}

These came up in design and are **not** built. Don't call routes for them; use the workaround.

| Feature | Status | Workaround |
|---|---|---|
| Promote a built image from staging to production without rebuilding | Not built | `duplicate` re-deploys from the same source (rebuilds); rollback only moves within one service |
| Preview environments that expire automatically | Not built | Delete the environment yourself (`DELETE .../envs/{env}`) when the preview closes |
| Sleep/wake of idle (free-plan) environments | Not built | Environments stay up; scale replicas to 0 via the dashboard if needed |
| Server-sent-event log follow | Not built | Poll `logs?since=` or the deployment `after` cursor |
| Members and login links (Nomi users opening the Shipyard dashboard) | Not built | Nomi is the interface; the superadmin can use the dashboard |
| Org suspend | Not built | Delete the org, or cancel its plan |
| Release command as a separate step | Not built | Run migrations on start |

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
- [ ] "Change address" → `GET /orgs/{org}/domains/check` while typing, `PUT .../envs/{env}/domain` on confirm; on `409` show `data.suggestions`.
- [ ] Project moved to another org → `POST /orgs/{org}/apps/{app}/transfer` with `to_org`; store the returned app key if you passed `app_key`.
- [ ] Project deleted → confirm → `DELETE /orgs/{org}/apps/{app}`.
- [ ] Logs and Monitor panel → runtime logs (structured), live usage (`/usage`), and historical metrics (`/metrics`) for traffic and containers.
- [ ] Monitor sheet alerts → handle `service.resource_alert` and `service.error_rate_alert` webhooks.
- [ ] After publish: never show `url` until `publish.succeeded`; on failure read `failure` from the deployment and run the [fix loop](#fix-loop).
- [ ] Run `scripts/check-nomi-integration.py` against the target Shipyard before go-live.
- [ ] Catch-up job: on start, page `GET /orgs/{org}/events?after=<last seen>` for orgs with activity.
