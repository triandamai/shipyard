# Editor UI — Design Spec

**Date:** 2026-09-24
**Status:** Approved for implementation planning
**Scope:** Sub-project 2 of the "in-app code editor with live preview and one-button publish" feature. Builds on the sandbox runtime backend (sub-project 1, merged). Covers: a new backend file-management API for a running sandbox's volume, and the frontend editor UI (file tree, CodeMirror editor, live preview, terminal) that consumes it. The publish pipeline (turning a sandbox's state into a real deployed service) remains a separate, future sub-project.

## Context

Sub-project 1 built the sandbox runtime: isolated per-app containers with start/stop/heartbeat lifecycle, persistent volumes, stack auto-detection, and Traefik-routed live preview. It exposes no way to read or write the files inside a sandbox's volume, and the frontend has no UI for any of this yet — `SandboxApp` doesn't exist as a type, and no route or panel references sandbox apps at all.

This codebase already has CodeMirror 6 installed and wired up (`frontend/src/lib/components/CodeEditor.svelte`, currently YAML-locked, one consumer: `DockerComposePanel.svelte`) — no Monaco anywhere. It also has a proven pattern for talking to a running container interactively: `ExecPanel.svelte` mints a short-lived exec token via `POST .../exec/token`, then opens a raw `WebSocket` streaming binary frames into an `@xterm/xterm` terminal. This sub-project reuses both rather than introducing new primitives.

## Decisions

### File access: exec into the running sandbox container

The backend cannot access a sandbox's Docker volume filesystem directly (same remote-node constraint as sub-project 1 — the volume may live on a node reached only through the Docker API). File operations are implemented as shell commands (`ls`, `cat`, redirection) run via Docker exec against the sandbox's *own running container* — the same mechanism `ExecPanel` already uses for its terminal, not a separate throwaway helper container. Consequence: **editing requires the sandbox to be running.** Opening the editor auto-starts it first (same idempotent `POST /sandbox/start` sub-project 1 already built), exactly like a cold preview-link visit already does.

### Terminal included in v1

The editor ships with an embedded terminal from the start, reusing `ExecPanel`'s existing token-mint + WebSocket + xterm.js pattern unchanged against the sandbox's container. This is additive reuse, not new infrastructure, so it doesn't meaningfully expand scope.

### Placement: dedicated full-page route

A new route (`.../apps/[serviceId]/editor`) rather than the existing 480px slide-over drawer, which is too narrow for a file tree + editor + preview + terminal split layout. A lighter `SandboxAppDetailPanel.svelte` (following the existing `ServiceDetailPanel` convention) still shows in the normal drawer for the app's summary/status/start-stop controls, with an "Open Editor" button linking to the full route. The app still appears on the project's topology canvas like every other resource.

### App creation requires a template, to resolve a bootstrap ordering problem

Since editing requires the sandbox running, and sub-project 1's first-start fails outright on an empty/undetectable volume, a brand-new app would be stuck (can't start with nothing to detect, can't edit without starting). Resolution: creating an app requires picking a starter template (Node, Python, or Static — the same three stacks sub-project 1 already detects). Creation populates `sandbox_app_configs` directly from the template (skipping probe detection for that first start, `manifest_source = 'manifest'` — reusing the existing enum value, no schema change) and seeds a minimal starter file set into the empty volume as part of the container's own startup command (see Architecture below), so `npm install`/`pip install` has something to act on immediately.

## Architecture

### Backend: file management API

New routes under `/api/apps/:id/files/*` in `sandbox_runtime`, all requiring `status = 'running'` (return an actionable "start the sandbox first" error otherwise, no new lifecycle state):

- `GET /files/tree` — recursive listing (`path`, `name`, `is_dir`) via `find /app`, excluding common noise directories (`node_modules`, `.git`, `__pycache__`), capped at depth 8 and 2000 entries (truncated with a flag in the response, not an error, past that).
- `GET /files/content?path=...` — read one file via `cat`, capped at 1 MB; rejects binary content (detected via a null-byte check on the first chunk) with a clear error rather than corrupting the editor.
- `PUT /files/content?path=...` — write one file's full contents (request body), via an exec'd heredoc write.
- `POST /files/mkdir`, `DELETE /files/entry?path=...`, `POST /files/rename` — the remaining standard file-tree operations.

All routes funnel through one new `exec_in_sandbox(state, service_id, cmd) -> AppResult<String>` helper, built on the same container-exec capability `ExecPanel`'s existing backend route already uses.

**Path safety is the central risk in this section.** Every `path` parameter is validated by a single shared function — reject `..`, absolute paths outside `/app`, and symlink escape attempts — before it is ever interpolated into a shell command. No handler builds its own validation.

### Backend: template-seeded first boot

Each template (Node/Python/Static) has a small, hardcoded starter file set (analogous to sub-project 1's `SANDBOX_PROBE_SCRIPT` constant — plain Rust constants, not stored in the DB). At creation time, the chosen template's files are packed (tar + base64) and passed to the sandbox's *first* container as an env var (`SEED_FILES_B64`). The container's startup command becomes:

```sh
sh -c 'if [ -z "$(ls -A /app)" ] && [ -n "$SEED_FILES_B64" ]; then
         echo "$SEED_FILES_B64" | base64 -d | tar -xz -C /app;
       fi; {install_cmd} && {dev_cmd}'
```

This check is unconditionally included in every sandbox's startup command (cheap, idempotent — a no-op once `/app` is non-empty), rather than adding a second code path only for template-created apps. Probe-detected apps simply never have `SEED_FILES_B64` set, so the conditional never fires for them.

### Frontend: routing and layout

- **`orgs/[orgSlug]/projects/[projectSlug]/apps/[serviceId]/editor/+page.svelte`** — full-viewport IDE layout: file tree (new component, no existing precedent) on the left, a tabbed CodeMirror editor in the center (generalizing `CodeEditor.svelte` to accept a language extension instead of its current YAML lock — `@codemirror/lang-javascript` is already installed and unused), a live-preview `<iframe>` pointing at the sandbox's `preview_url` (no polling needed — the dev server's own HMR, already working per sub-project 1, updates it live), and a collapsible terminal panel reusing `ExecPanel` as-is.
- **`SandboxAppDetailPanel.svelte`** — the existing-convention drawer panel: status, start/stop controls, preview URL link, "Open Editor" button.
- **API additions**: a `SandboxApp` interface in `types.ts` (mirroring `Domain`'s flat-field style) and a `// ─── Sandbox Apps ───` section of methods on the existing `api` client singleton.

### Lifecycle wiring

- Opening the editor calls `POST /sandbox/start` (idempotent, already built) and shows a "waking up" state while it provisions.
- The editor page sends `POST /sandbox/heartbeat` on an interval while open — the piece of sub-project 1's design that had no caller until now.
- `navigator.sendBeacon` to `POST /sandbox/stop` on tab close/navigate-away, best-effort; the existing idle reaper remains the reliable backstop.
- Status changes are plain request/response, not routed through the MQTT bus — the editor page is the only thing driving these transitions and already observes the result of each call it makes.

## Testing

- **Backend**: unit tests for the path-safety validator (the one place a bug is genuinely dangerous) and for exec-command construction (correct quoting/escaping of paths and file content), independent of a live Docker daemon.
- **Frontend**: no existing test precedent for panels in this codebase; this follows suit.
- **Manual verification**: full loop — create an app from a template, confirm the editor opens with starter files visible, edit a file, confirm the live preview updates via HMR, open the terminal and run a command, close the tab, confirm the idle reaper eventually stops it. Requires the same real Docker/gVisor environment this macOS machine can't provide — carries the same Task 12-style gap as sub-project 1.
