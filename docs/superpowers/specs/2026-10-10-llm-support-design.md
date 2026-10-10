# LLM Support for Shipyard

**Date:** 2026-10-10
**Status:** Draft design, open questions below. Phase 1 is ready to turn into an implementation plan once the questions are answered.

## Goal

Make Shipyard something language models can **read**, **operate** and **run**:

1. **Read**: an LLM can learn how Shipyard works from its docs without scraping HTML.
2. **Operate**: an agent (Claude Code, Claude Desktop, Cursor, a CI bot) can inspect and deploy a user's Shipyard through a safe, scoped interface.
3. **Run**: users can deploy LLM workloads (model servers, gateways, chat UIs) on Shipyard with persistent model storage and, where available, GPUs.
4. **Assist** (optional, later): Shipyard itself uses a model to explain failures, bring-your-own-key, off by default.

Each phase stands alone and ships independently. Phases 0–1 are the core; 2–4 depend on what users ask for.

## Where Shipyard is today

- **Docs**: Markdown is now the source for every docs page (`landing/src/content/docs/*.md`). Served as HTML, as raw `/docs/<page>.md`, and indexed by `/llms.txt` and `/llms-full.txt`. (Phase 0, shipped with this design.)
- **Open API** (`backend/crates/openapi`, `/openapi/v1`): API-key auth with scopes `read`, `deploy`, `write`, `admin`. Endpoints: org, projects, services (list/get/patch), trigger deploy, list/get deployments, API keys. **Missing for agents:** logs, env vars (names only), domains, volumes, rollback, service status/health, project topology, creating services.
- **Persistent volumes**: deploys now attach a named volume to every image-declared `VOLUME` path (PR #1), which model servers need (Ollama `/root/.ollama`, Hugging Face cache).
- **Swarm resources**: `ServiceSpec` has CPU/memory limits and placement constraints; no generic resources (GPUs).
- **Audit log**: exists for dashboard actions; API-key actions need an actor marker so agent actions are distinguishable.

## Phase 0: LLM-readable docs (shipped)

- Docs content lives in Markdown with frontmatter; one renderer serves HTML, `.md`, `llms.txt` and `llms-full.txt` (https://llmstxt.org format).
- Every page links its Markdown via `<link rel="alternate" type="text/markdown">` and offers "View as .md" / "Copy page as Markdown".
- nginx serves `.md` as `text/markdown; charset=utf-8`.

**Follow-ups:** add an "Agents" docs page once Phase 1 exists; keep `llms.txt` descriptions current when pages are added (it's generated from frontmatter, so this is just writing good `description`s).

## Phase 1: Shipyard MCP server

Let agents operate Shipyard through the Model Context Protocol, reusing Open API keys and scopes so there is one permission model.

### Shape

- **Remote MCP endpoint** served by the backend at `/mcp` (Streamable HTTP transport), authenticated with an existing `ship_…` API key as a bearer token. No separate stdio binary to install: any MCP client that supports remote servers connects with a URL and a key.
- Implemented in a new crate `backend/crates/mcp` using the official Rust SDK (`rmcp`), mounted next to `openapi` in `crates/api/src/main.rs`. Tools call the same query/engine functions the Open API handlers use; no HTTP round-trip to itself.
- Tool calls are written to the audit log with `actor = api_key:<key prefix>` and `via = mcp`.

### Tools (first cut)

| Tool | Scope | Notes |
| --- | --- | --- |
| `list_projects` | read | |
| `get_project_topology` | read | services, volumes, domains, edges, as one compact JSON |
| `get_service` | read | config, status, replicas, image, ports, volumes, domains |
| `list_deployments` / `get_deployment` | read | includes step logs |
| `get_service_logs` | read | tail N lines, optional `since`; env var values masked |
| `list_env_var_names` | read | names only, never values |
| `deploy_service` | deploy | returns deployment id; agent then polls `get_deployment` |
| `rollback_service` | deploy | to a previous deployment id |
| `set_env_vars` | write | values write-only; requires `confirm: true` |
| `scale_service` | write | |

Resources: each docs page as `shipyard://docs/<page>` (the same Markdown), so an agent connected to a Shipyard instance also has its manual.

### Safety

- Scopes are the gate. Recommend `read` + `deploy` keys for agents; `write` and `admin` are opt-in.
- Destructive tools (`delete_*`) are **not** in Phase 1.
- Env var **values** never leave the server. Logs returned to agents mask any occurrence of the service's own env var values (there is no log redaction today; this adds it, and the dashboard log viewer can reuse it).
- Per-key rate limit on tool calls (e.g. 60/min) and on `deploy_service` (e.g. 6/min/service).
- Every tool result carries `request_id` for tracing back to the audit log.

### Open API gaps to close first (also useful without MCP)

`GET /services/:id/logs`, `GET /services/:id/env` (names), `GET /projects/:id/topology`, `POST /services/:id/rollback`, `GET /services/:id/domains`, `GET /services/:id/volumes`. Each gets an Open API doc section in `api.md` so the docs and the tools stay in sync.

### Done when

- An MCP client connected with a `read`+`deploy` key can find a failing service, read its logs, redeploy it and watch the deployment finish, with every action in the audit log.
- `docs/agents.md` explains connecting Claude Code / Claude Desktop / Cursor, with copyable config.

## Phase 2: Agent-safe operations

Things that make letting an agent deploy less nerve-wracking, built on Phase 1:

- **Dry run**: `deploy_service { dry_run: true }` returns the resolved spec diff (image, env names, mounts, ports) without applying it.
- **Approval mode**: an org setting where `write`-scope tool calls create a pending change that a human approves in the dashboard (toast + audit entry).
- **Agent identity**: API keys get an optional `agent` label shown in the audit log and deployment history ("Deployed by Claude Code via key ship_ab12").

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

1. **Who is the first agent user?** Claude Code users deploying their own apps (favours Phase 1 tools + docs), or teams wanting a chat ops bot (favours Phase 2 approval mode sooner)?
2. **MCP transport**: remote `/mcp` only, or also publish a small stdio package for clients without remote support?
3. **Is Phase 3 in scope this year?** GPU support needs real hardware to test; if no one has GPU nodes, ship CPU templates (Ollama with small models, LiteLLM) first.
4. **Assistant data policy**: is sending redacted logs to a third-party model acceptable for self-hosters, or should Phase 4 be local-model only?

## Out of scope

- Hosting or reselling model inference.
- Training or fine-tuning workflows.
- Any feature that sends user data to a model provider without an explicit admin opt-in.
