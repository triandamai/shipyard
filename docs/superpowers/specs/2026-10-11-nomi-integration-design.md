# Nomi on Shipyard: first-class integration

**Date:** 2026-10-11
**Status:** Draft design for review. Supersedes Phase 1 of `2026-10-10-llm-support-design.md`.
**Companion:** `2026-10-11-nomi-integration-guide.md` (the developer guide, published at `/docs/nomi` once the endpoints ship).

## What we're building

Nomi is a coding agent that builds web apps (SvelteKit, Next.js, Nuxt, React/Vue/Svelte SPAs) on Postgres with Drizzle. Shipyard becomes the place those apps run. A Nomi user never logs into Shipyard: Nomi drives it through the Open API.

Requirements from Nomi:

1. Every app gets **environments**: `dev`, `staging`, `prod`.
2. A reliable way to **build and deploy** any of those frameworks.
3. **Postgres per environment**, with Drizzle migrations applied on deploy.
4. Users can **read build logs, runtime logs and errors** inside Nomi (and Nomi's agent can read them to fix its own code).
5. **Creating an org in Nomi creates a Shipyard org.**
6. A **dedicated Nomi guide** in the docs.

## Answers to the questions asked

### Docker image or something else?

**Build an OCI image once per commit, then promote the same image digest from dev to staging to prod.** Never rebuild per environment: what passed staging is byte-for-byte what reaches prod, and promotion takes seconds.

Who builds it:

| Option | How | Verdict |
| --- | --- | --- |
| **A. Shipyard builds from uploaded source** | Nomi uploads a source tarball; Shipyard builds it with the app's Dockerfile, or its existing auto-detection for SvelteKit / Next.js / Nuxt / Angular, and stores the image in its built-in registry | **Recommended.** Nomi needs no build infrastructure. Reuses `crates/engine` build + `crates/registry`. |
| B. Nomi builds and pushes | Nomi runs BuildKit, `docker push`es to the Shipyard registry with an org API key (works today) | Supported as an alternative if Nomi already has builders. |
| C. Git push | Nomi pushes to a git host; Shipyard clones and builds | Not needed: adds a git host and webhook for no gain. |
| D. Buildpacks / Nixpacks | Detect and build without a Dockerfile | Slower and less predictable than a Dockerfile Nomi controls. |

**Nomi's templates should ship a `Dockerfile`** (SvelteKit with `adapter-node`, Next.js `output: 'standalone'`, Nuxt `node-server`). Shipyard's auto-detection stays as a fallback, but a Dockerfile Nomi owns gives deterministic builds and lets Nomi's agent fix build errors in a file it understands.

**Pure SPAs** (Vite React/Vue/Svelte with no server) don't need a container: Nomi uploads the built `dist/` and Shipyard serves it from its static server, which is cheaper and faster. The API takes `kind: "static"` for these.

### Can the backend run on edge functions?

**Not for these apps.** SvelteKit, Next.js and Nuxt already contain their backend (server routes, form actions, server components, auth sessions). Moving that code into Shipyard edge functions would mean rewriting the framework's server layer as standalone handlers, and edge functions here have hard limits: one shared Deno runtime per org, a 10 s (Free) / 30 s (Pro) request timeout, and a 1–10 MB request body cap. Long Drizzle queries, file uploads and streaming responses would hit them.

The simpler thing to maintain is **one container per app per environment**, running the framework's own Node server, talking to that environment's Postgres. Edge functions remain useful for small side jobs (incoming webhooks, a public form endpoint) and Nomi can generate them when a user asks for exactly that.

### Reading logs and errors

Three kinds, all through the Open API and pushed to Nomi by webhook:

| What | Endpoint | Notes |
| --- | --- | --- |
| Build and deploy logs | `GET /deployments/:id/logs`, `?follow=1` streams (SSE) | Step-by-step, including Drizzle migration output |
| Runtime logs | `GET /services/:id/logs?since=&tail=&level=error`, `?follow=1` streams | Merged across replicas; stderr is `level=error`; env var values masked |
| Errors and events | `GET /services/:id/events` | Crash with exit code, OOM kill, restart loop, health check failing, deploy failed |
| Push | Webhook to Nomi: `deployment.succeeded`, `deployment.failed`, `service.crashed` | Signed with HMAC so Nomi can trust it |

The important loop: **a failed build or a crash goes back to Nomi's agent** with the log tail, the agent fixes the code, and redeploys to `dev`. That's what makes Shipyard feel built for Nomi.

### Org creation

Shipyard gets a **platform integration** credential (issued by a Shipyard superadmin, one per integrator):

- `POST /openapi/v1/orgs` with `{ name, external_id: "<nomi org id>", plan }` creates the org. It is **idempotent on `external_id`**: retrying never creates a second org.
- The response includes an **org-scoped API key** (shown once). Nomi stores it encrypted and uses it for everything inside that org.
- `PATCH` (rename, change plan/quota), `POST .../suspend` and `DELETE` mirror Nomi's org lifecycle.
- The integration key can only touch orgs it created.

### Documentation

A **Nomi guide** at `/docs/nomi` (draft in the companion file), written for Nomi's engineers and for Nomi's agent: it is plain Markdown like the other pages, so it's in `/llms.txt` and the agent can read it directly.

## How it works

### Mapping

| Nomi | Shipyard |
| --- | --- |
| Organization | Organization (`external_id` = Nomi org id) |
| App | A group of projects sharing `app_key` |
| Environment (`dev` / `staging` / `prod`) | One **project** per environment: `app_key` + `environment` columns, unique per org |
| App's web server | Service in that project (`kind: image` built from source, or `kind: static`) |
| App's database | `postgres` service from the existing template, with its persistent volume, in the same project |

**Why a project per environment** instead of environments inside one project: each environment gets its own network, its own Postgres and its own env vars with no chance of staging talking to prod's database, and the dashboard's canvas, logs and permissions already work per project. Two nullable columns on `projects` (`app_key`, `environment`) let the API and dashboard group them as one app.

### Sequence

```text
Nomi user creates org "Acme"
  Nomi → POST /orgs {name: "Acme", external_id: "nomi_org_123"}       (integration key)
  ← org + org API key (Nomi stores it)

User creates app "shop" in Nomi
  Nomi → POST /apps {app_key: "shop", environments: ["dev","staging","prod"],
                     framework: "sveltekit", database: "postgres"}     (org key)
  Shipyard creates 3 projects, each with: web service + postgres service,
  DATABASE_URL wired to that project's postgres, a hostname per environment:
    shop-dev.acme.nomi.app   shop-staging.acme.nomi.app   shop.acme.nomi.app
  ← app with environment ids and URLs

Agent finishes a change
  Nomi → POST /apps/shop/builds  (multipart: source.tar.gz, commit: "a91f2c0")
  ← build id; Shipyard builds image → registry, digest sha256:…
  Nomi → POST /apps/shop/environments/dev/deploy {build_id}
  Shipyard: run release command (drizzle-kit migrate) with the new image
            → if it fails, stop; dev keeps running the old version
            → update service (stop-first if it holds data, else start-first)
  → webhook deployment.succeeded {url: https://shop-dev.acme.nomi.app}

User clicks "Promote to staging", then "Publish"
  Nomi → POST /apps/shop/environments/staging/deploy {build_id}       (same digest, no rebuild)
  Nomi → POST /apps/shop/environments/prod/deploy {build_id}

Something breaks
  Shipyard → webhook service.crashed {exit_code: 1, log_tail: [...]}
  Nomi's agent reads GET /services/:id/logs?level=error, fixes, redeploys dev
  Or the user hits "Roll back": POST /apps/shop/environments/prod/rollback {to: previous}
```

### Database and Drizzle

- Each environment's Postgres is a normal Shipyard service with a persistent volume (PR #1 guarantees it survives redeploys and Shipyard updates) and is only reachable inside that project's network.
- `DATABASE_URL` is set on the web service using the existing service reference (`platform-<slug>` host), so it follows the database if it moves.
- **Release command** (new): each service can declare a command run once per deploy in a one-off container from the *new* image with the environment's variables, before traffic moves. Nomi sets it to `npx drizzle-kit migrate` (or `node build/migrate.js`). Non-zero exit fails the deploy and leaves the old version serving.
- Nightly `pg_dump` backups for `prod` to the configured storage (Phase 3), plus "copy prod data to staging" as an explicit, logged action, never automatic.

### Environment variables

Per environment, write-only (`PUT /apps/:app/environments/:env/variables`). Nomi can mark a variable as shared across environments; Shipyard still stores it per project so a prod-only change never leaks into dev. Values are never returned by the API and are masked in logs.

### Domains

A wildcard DNS record (`*.nomi.app`) points at Shipyard's Traefik; hostnames are assigned automatically per environment and get Let's Encrypt certificates. Custom domains for prod go through `POST /apps/:app/environments/prod/domains`, and the existing Cloudflare integration can create the DNS record when the user's zone is on Cloudflare.

### Where it runs

Nomi operates one Shipyard control plane. All Nomi orgs start on shared worker nodes with per-org CPU/memory quotas; paid orgs can move to dedicated nodes using Shipyard's existing node provisioning. Nothing in the API changes between the two.

## API surface

All under `/openapi/v1`, documented in `api.md` and the Nomi guide.

| Endpoint | Credential | New / exists |
| --- | --- | --- |
| `POST /orgs`, `PATCH /orgs/:id`, `POST /orgs/:id/suspend`, `DELETE /orgs/:id` | integration | new |
| `POST /apps`, `GET /apps/:app`, `DELETE /apps/:app` | org | new (creates/groups projects) |
| `POST /apps/:app/builds` (source tarball or static bundle), `GET /builds/:id`, `GET /builds/:id/logs` | org | new (reuses engine build) |
| `POST /apps/:app/environments/:env/deploy {build_id}` | org | new (wraps existing deploy) |
| `POST /apps/:app/environments/:env/rollback` | org | new |
| `PUT /apps/:app/environments/:env/variables` | org | new |
| `POST/DELETE /apps/:app/environments/:env/domains` | org | new |
| `GET /deployments/:id`, `GET /deployments/:id/logs` | org | `get` exists, logs new |
| `GET /services/:id/logs`, `GET /services/:id/events` | org | new (dashboard has logs internally) |
| `POST /webhooks` (register Nomi's endpoint + secret) | org or integration | new |

## Delivery

| Phase | Ships | Unblocks for Nomi |
| --- | --- | --- |
| **1. Provision and deploy** | Integration keys, `POST /orgs`, `POST /apps` with environments, source-tarball builds into the registry, deploy by build id, build/deploy logs, `DATABASE_URL` wiring | An app goes from Nomi to a live `dev` URL |
| **2. Promote and migrate** | Release command (Drizzle), promote across environments, rollback, per-environment variables, runtime logs (SSE), events | dev → staging → prod with migrations and a rollback button |
| **3. Operate** | Webhooks, custom domains, prod backups, quotas per Nomi plan, `/docs/nomi` published | Production-ready, self-serve for Nomi users |

Each phase ends with an end-to-end test: a fixture SvelteKit + Drizzle app driven only through the Open API.

## Open questions

1. **Hostname scheme**: `shop-dev.acme.nomi.app` (flat, one wildcard cert per level) or `dev.shop.acme.nomi.app` (needs deeper wildcards)? Recommend flat.
2. **Who owns the `nomi.app` DNS zone and certificates**: Nomi or the Shipyard instance? Recommend Shipyard's Traefik with a DNS-01 wildcard cert.
3. **Preview environments per branch/PR** (`shop-pr-42`)? Cheap to add on top of this model; decide whether Nomi wants them in Phase 2.
4. **Free-tier limits**: sleep idle `dev` environments (scale to zero, like sandbox idle timeout) to save capacity?
5. **Shipyard dashboard access for Nomi users**: none (Nomi is the only UI), or read-only SSO for power users later?

## Out of scope

- Shipyard hosting Nomi's own agent or model calls.
- Managed Postgres features beyond one instance per environment (read replicas, PITR) until a customer needs them.
