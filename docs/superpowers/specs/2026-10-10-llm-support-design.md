# LLM Support for Shipyard

**Date:** 2026-10-10
**Status:** Draft design. Phase 1 changed on 2026-10-10 from an MCP server to Open API endpoints for Nomi.

## Goal

Make Shipyard something language models can **read**, **operate** and **run**:

1. **Read**: an LLM can learn how Shipyard works from its docs without scraping HTML.
2. **Operate**: an integrating platform (first: Nomi, an agent orchestrator) can create, deploy and publish apps on a user's Shipyard through the scoped Open API.
3. **Run**: users can deploy LLM workloads (model servers, gateways, chat UIs) on Shipyard with persistent model storage and, where available, GPUs.
4. **Assist** (optional, later): Shipyard itself uses a model to explain failures, bring-your-own-key, off by default.

Each phase stands alone and ships independently. Phases 0–1 are the core; 2–4 depend on what users ask for.

## Where Shipyard is today

- **Docs**: Markdown is now the source for every docs page (`landing/src/content/docs/*.md`). Served as HTML, as raw `/docs/<page>.md`, and indexed by `/llms.txt` and `/llms-full.txt`. (Phase 0, shipped with this design.)
- **Open API** (`backend/crates/openapi`, `/openapi/v1`): API-key auth with scopes `read`, `deploy`, `write`, `admin`. Endpoints: org, projects, services (list/get/patch), trigger deploy, list/get deployments, API keys. **Missing for integrators:** logs, env vars (names only), domains, volumes, rollback, service status/health, project topology, creating services.
- **Persistent volumes**: deploys now attach a named volume to every image-declared `VOLUME` path (PR #1), which model servers need (Ollama `/root/.ollama`, Hugging Face cache).
- **Swarm resources**: `ServiceSpec` has CPU/memory limits and placement constraints; no generic resources (GPUs).
- **Audit log**: exists for dashboard actions; API-key actions need an actor marker so agent actions are distinguishable.

## Phase 0: LLM-readable docs (shipped)

- Docs content lives in Markdown with frontmatter; one renderer serves HTML, `.md`, `llms.txt` and `llms-full.txt` (https://llmstxt.org format).
- Every page links its Markdown via `<link rel="alternate" type="text/markdown">` and offers "View as .md" / "Copy page as Markdown".
- nginx serves `.md` as `text/markdown; charset=utf-8`.

**Follow-ups:** document every new Open API endpoint in `api.md` as Phase 1 lands; keep `llms.txt` descriptions current when pages are added (it's generated from frontmatter, so this is just writing good `description`s).

## Phase 1: Open API for platform integrators (Nomi)

**Decision (2026-10-10): no MCP server.** The first integrator is Nomi, an agent orchestrator that builds web apps and uses Shipyard to deploy and publish them. Deploying is a fixed pipeline from Nomi's side (create, upload, deploy, wait, return a URL), so it belongs in Nomi's code calling a typed HTTP API, not in an LLM choosing tools turn by turn. Nomi exposes its own high-level tools (e.g. `publish_app`) to its models and implements them on this API. An MCP layer can be added later on top of the same endpoints if a generic agent client needs it; nothing here depends on it.

### What Nomi needs that the Open API lacks

Creating projects, services and static uploads exist today only in the dashboard API (`crates/api`, session auth). The Open API (`/openapi/v1`, `ship_…` keys) can list and trigger deploys of services that already exist. The built-in registry already accepts `ship_…` keys for `docker push`.

| Endpoint | Scope | Purpose |
| --- | --- | --- |
| `POST /projects`, `DELETE /projects/:id` | write | one project per Nomi app |
| `POST /projects/:id/services` | write | `type`: `static`, `image` or `git`, with ports and resources |
| `POST /services/:id/static/upload` | deploy | zip of a built site; no Docker needed on Nomi's side |
| `PUT /services/:id/env` | write | set env vars; values are write-only |
| `POST /services/:id/domains`, `GET /services/:id/url` | write / read | attach a domain or get the generated public URL |
| `POST /services/:id/deploy` (exists), `GET /deployments/:id` (exists) | deploy / read | deploy and poll |
| `GET /services/:id/logs` | read | show build and runtime failures to the user; env var values masked |
| `POST /services/:id/rollback` | deploy | back to a previous deployment id |
| Deployment webhook (`deployment.succeeded` / `deployment.failed`) | — | so Nomi can stop polling |

Each endpoint reuses the dashboard handler's logic rather than duplicating it, is written to the audit log with `actor = api_key:<prefix>`, and gets a section in `landing/src/content/docs/api.md`.

### Tenancy

To decide with Nomi: one Shipyard org per Nomi user (clean isolation, per-user quotas and keys) or one shared org with a project per app (simpler, shared limits). The API above works for both; per-user orgs additionally need `POST /orgs` on an admin-scoped key.

### Client

A small TypeScript client (`@shipyard/client`) generated from, or hand-written against, the Open API so Nomi's integration is a few calls: `createProject`, `createService`, `uploadStatic`, `deploy`, `waitForDeployment`, `getUrl`.

### Done when

Nomi, holding one API key, can take a built web app from zero to a live HTTPS URL, show the user build logs when it fails, redeploy and roll back, with every action in the audit log, and the API docs page covers every endpoint it calls.

## Phase 2: Agent-safe operations

Things that make letting an automated integrator deploy less nerve-wracking, built on Phase 1:

- **Dry run**: `POST /services/:id/deploy?dry_run=true` returns the resolved spec diff (image, env names, mounts, ports) without applying it.
- **Approval mode**: an org setting where `write`-scope API calls create a pending change that a human approves in the dashboard (toast + audit entry).
- **Agent identity**: API keys get an optional `agent` label shown in the audit log and deployment history ("Deployed by Nomi via key ship_ab12").

## Phase 3: Run LLM workloads on Shipyard

- **Templates**: Ollama, Open WebUI, LiteLLM (gateway), vLLM, Text Generation Inference. Each pre-declares its persistent model volume, health check, port and sensible memory limit, using the existing template mechanism.
- **GPU nodes**: Swarm generic resources. Node setup (`worker-setup.sh`) detects NVIDIA GPUs and registers `NVIDIA-GPU` generic resources; services get a `gpus` count in `ServiceSpec` that maps to `Resources.Reservations.GenericResources`. The scheduler then places them only on GPU nodes.
- **Model storage**: volumes for model caches pinned to the node that holds them (the placement logic from PR #1 already does this); show volume size in the service panel since models are tens of GB.
- **Private by default**: model servers get no public domain unless the user adds one; Traefik basic-auth or API-key middleware option for exposed gateways.

## Phase 4: In-app assistant (optional)

- Off by default. An admin configures a provider and key (Anthropic, OpenAI-compatible, or a local Ollama on the same Shipyard).
- First feature: **"Explain this failure"** on a failed deployment: sends the step logs (redacted) and the service config summary, returns cause and fix. No chat surface until this proves useful.
- Never sends env var values, registry credentials or API keys; the redacted payload is shown to the user before sending.

## Open questions

1. **Nomi tenancy**: one Shipyard org per Nomi user, or one shared org with a project per app?
2. **What Nomi ships first**: static front-ends (zip upload) or full-stack apps (image or git)? This orders the Phase 1 endpoints.
3. **Is Phase 3 in scope this year?** GPU support needs real hardware to test; if no one has GPU nodes, ship CPU templates (Ollama with small models, LiteLLM) first.
4. **Assistant data policy**: is sending redacted logs to a third-party model acceptable for self-hosters, or should Phase 4 be local-model only?
5. **MCP later?** Revisit only if a generic agent client (not Nomi) needs Shipyard; it would be a thin layer over the Phase 1 endpoints.

## Out of scope

- Hosting or reselling model inference.
- Training or fine-tuning workflows.
- Any feature that sends user data to a model provider without an explicit admin opt-in.
