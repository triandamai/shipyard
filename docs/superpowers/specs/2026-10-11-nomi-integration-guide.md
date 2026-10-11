---
title: Nomi
description: Deploy apps built by Nomi to Shipyard with dev, staging and production environments, Postgres, Drizzle migrations and logs.
---

<!-- DRAFT. Describes the endpoints in 2026-10-11-nomi-integration-design.md.
     Publish as landing/src/content/docs/nomi.md (and add it to PAGES in
     landing/src/lib/docs/index.ts) when Phase 1 ships; mark endpoints that
     are not live yet before then. -->

# Nomi

Nomi builds web apps; Shipyard runs them. This guide is for engineers wiring Nomi to a Shipyard instance, and for Nomi's agent, which can read it as Markdown at `/docs/nomi.md`.

Everything here goes through the Open API at `https://<shipyard>/openapi/v1`. Nomi users never need a Shipyard login.

## How the pieces map {#mapping}

| In Nomi | In Shipyard |
|---|---|
| Organization | Organization, linked by `external_id` |
| App | An app: one project per environment |
| `dev`, `staging`, `prod` | Projects grouped under the app, each with its own network, Postgres and variables |
| The app's server | A service built from your source |
| The app's database | A Postgres service with a persistent volume |

Builds are made once and promoted: the image that passed `staging` is exactly the one that goes to `prod`.

## Credentials {#credentials}

There are two kinds of key.

| Key | Prefix | Used for |
|---|---|---|
| Integration key | `ship_int_` | Creating and managing the organizations Nomi owns. Issued once by a Shipyard admin. |
| Organization key | `ship_` | Everything inside one organization. Returned when you create the org; store it encrypted. |

Send either as `Authorization: Bearer <key>`.

> [!WARNING]
> The organization key is shown once, in the response that creates the org. If you lose it, create a new one with the integration key and revoke the old one.

## Create an organization {#create-org}

Call this when a user creates an organization in Nomi. It is safe to retry: the same `external_id` always returns the same organization.

```bash
curl -X POST https://ship.example.com/openapi/v1/orgs \
  -H "Authorization: Bearer ship_int_…" \
  -H "Content-Type: application/json" \
  -d '{ "name": "Acme", "external_id": "nomi_org_123", "plan": "free" }'
```

```json title="201 Created"
{
  "data": {
    "org": { "id": "018f…", "name": "Acme", "slug": "acme", "external_id": "nomi_org_123" },
    "api_key": "ship_…"
  }
}
```

Rename with `PATCH /orgs/:id`, pause with `POST /orgs/:id/suspend`, and remove with `DELETE /orgs/:id` when the Nomi organization is deleted.

## Create an app {#create-app}

One call creates every environment, each with a web service, a Postgres database wired through `DATABASE_URL`, and a hostname.

```bash
curl -X POST https://ship.example.com/openapi/v1/apps \
  -H "Authorization: Bearer ship_…" \
  -H "Content-Type: application/json" \
  -d '{
    "app_key": "shop",
    "kind": "server",
    "environments": ["dev", "staging", "prod"],
    "database": "postgres",
    "port": 3000,
    "release_command": "npx drizzle-kit migrate"
  }'
```

```json title="201 Created"
{
  "data": {
    "app_key": "shop",
    "environments": {
      "dev":     { "url": "https://shop-dev.acme.nomi.app" },
      "staging": { "url": "https://shop-staging.acme.nomi.app" },
      "prod":    { "url": "https://shop.acme.nomi.app" }
    }
  }
}
```

Use `"kind": "static"` for single-page apps with no server (Vite React, Vue or Svelte). Static apps have no database or release command and are served directly, with no container.

## Prepare the app {#prepare}

Ship a `Dockerfile` in every server template. Shipyard can detect SvelteKit, Next.js and Nuxt without one, but a Dockerfile you control builds the same way every time, and your agent can fix it when a build fails.

### SvelteKit

Use `@sveltejs/adapter-node`.

```dockerfile title="Dockerfile"
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
COPY --from=build /app/drizzle.config.ts ./
ENV NODE_ENV=production PORT=3000
EXPOSE 3000
CMD ["node", "build"]
```

### Next.js

Set `output: 'standalone'` in `next.config.js`.

```dockerfile title="Dockerfile"
FROM node:22-alpine AS build
WORKDIR /app
COPY package*.json ./
RUN npm ci
COPY . .
RUN npm run build

FROM node:22-alpine
WORKDIR /app
COPY --from=build /app/.next/standalone ./
COPY --from=build /app/.next/static ./.next/static
COPY --from=build /app/public ./public
COPY --from=build /app/drizzle ./drizzle
ENV NODE_ENV=production PORT=3000 HOSTNAME=0.0.0.0
EXPOSE 3000
CMD ["node", "server.js"]
```

> [!NOTE]
> The release command runs inside this image, so anything it needs (`drizzle-kit`, your `drizzle/` migrations folder, `drizzle.config.ts`) must be in the final stage. If you prune dev dependencies, either keep `drizzle-kit` as a dependency or ship a small `migrate.js` that uses `drizzle-orm/node-postgres/migrator` and set `release_command` to `node migrate.js`.

Read the database URL from the environment, never from a file:

```ts title="src/lib/server/db.ts"
import { drizzle } from 'drizzle-orm/node-postgres';
import { Pool } from 'pg';

export const db = drizzle(new Pool({ connectionString: process.env.DATABASE_URL }));
```

## Build {#build}

Upload the source as a gzipped tarball. Leave out `node_modules`, `.git` and build output.

```bash
tar --exclude=node_modules --exclude=.git --exclude=build --exclude=.next -czf source.tar.gz .

curl -X POST https://ship.example.com/openapi/v1/apps/shop/builds \
  -H "Authorization: Bearer ship_…" \
  -F "source=@source.tar.gz" \
  -F "commit=a91f2c0"
```

```json title="202 Accepted"
{ "data": { "build_id": "019a…", "status": "building" } }
```

For a static app, upload the built `dist/` folder as a zip in the `bundle` field instead of `source`.

Follow the build with `GET /builds/:id/logs?follow=1` (Server-Sent Events) or wait for the `build.succeeded` / `build.failed` webhook.

## Deploy and promote {#deploy}

Deploy a finished build to an environment:

```bash
curl -X POST https://ship.example.com/openapi/v1/apps/shop/environments/dev/deploy \
  -H "Authorization: Bearer ship_…" \
  -H "Content-Type: application/json" \
  -d '{ "build_id": "019a…" }'
```

Shipyard then:

1. Runs the release command (`npx drizzle-kit migrate`) once, in a container from the new image, with that environment's variables.
2. If it fails, stops: the environment keeps serving the previous version, and the deployment is marked failed with the migration output in its logs.
3. Otherwise replaces the running service and marks the deployment succeeded.

To promote, deploy the same `build_id` to the next environment. There is no rebuild, so it takes seconds.

To roll back, `POST /apps/shop/environments/prod/rollback` returns to the previous successful deployment. Rolling back does not undo migrations: write migrations that the previous version can still run against.

## Variables {#variables}

Set variables per environment. Values are write-only: the API never returns them and they are masked in logs.

```bash
curl -X PUT https://ship.example.com/openapi/v1/apps/shop/environments/prod/variables \
  -H "Authorization: Bearer ship_…" \
  -H "Content-Type: application/json" \
  -d '{ "set": { "STRIPE_SECRET_KEY": "sk_live_…" }, "unset": ["DEBUG"] }'
```

`DATABASE_URL` is set for you and points at that environment's own database. Changing variables takes effect on the next deploy.

## Logs and errors {#logs}

| You want | Call |
|---|---|
| Why a build failed | `GET /builds/:id/logs` |
| What a deploy did, including migrations | `GET /deployments/:id/logs` |
| What the app is printing now | `GET /apps/shop/environments/prod/logs?follow=1` |
| Only errors | `GET /apps/shop/environments/prod/logs?level=error&since=1h` |
| Crashes, out-of-memory kills, restart loops | `GET /apps/shop/environments/prod/events` |

`?follow=1` streams Server-Sent Events. Lines from every replica are merged in time order and each carries its replica number.

When a build or deploy fails, pass the log tail to your agent with the files it touched. That is usually enough for it to fix the code and deploy to `dev` again.

## Webhooks {#webhooks}

Register one endpoint for the organization:

```bash
curl -X POST https://ship.example.com/openapi/v1/webhooks \
  -H "Authorization: Bearer ship_…" \
  -H "Content-Type: application/json" \
  -d '{ "url": "https://api.nomi.app/hooks/shipyard", "events": ["build.*", "deployment.*", "service.crashed"] }'
```

Each delivery has a `Shipyard-Signature` header: an HMAC-SHA256 of the raw body with the secret returned when you registered. Reject deliveries whose signature doesn't match.

| Event | Sent when |
|---|---|
| `build.succeeded`, `build.failed` | A build finishes |
| `deployment.succeeded`, `deployment.failed` | A deploy finishes, with the environment URL or the failing step |
| `service.crashed` | A running app exits unexpectedly, with exit code and the last log lines |

## Limits {#limits}

Limits follow the organization's plan and are set when you create or update the org.

| Limit | What happens at the limit |
|---|---|
| CPU and memory per environment | The container is capped; it is not stopped |
| Apps per organization | `POST /apps` returns `409` with `code: "quota_exceeded"` |
| Build time | Builds stop after 15 minutes and fail with a timeout in the log |
| Source upload size | 200 MB compressed |
