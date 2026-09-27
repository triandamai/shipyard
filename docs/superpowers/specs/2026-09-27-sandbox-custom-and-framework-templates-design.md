# Sandbox Custom Mode + Framework Template Gallery — Design Spec

**Date:** 2026-09-27
**Status:** Approved for implementation planning
**Scope:** (1) A "Custom" sandbox creation option that boots an idle, blank container with a `shipyard.json` starter manifest, so the user can scaffold their own project via the sandbox's terminal and have Shipyard pick it up automatically. (2) A curated framework template gallery (React, Vue, SvelteKit, Next.js, Nuxt, Astro — JS/TS variants where each framework's own tooling genuinely supports both) as built-in, one-click sandbox creation options alongside the existing Node/Python/Static templates.

## Context

Shipyard's sandbox apps already support three creation templates (Node, Python, Static — `backend/crates/api/src/sandbox_runtime/templates.rs`) and a separate probe-based auto-detection path (`backend/crates/engine/src/sandbox_probe.rs`) used when a sandbox is created with no template at all. Both paths converge on the same `sandbox_app_configs` row shape (`runtime`, `base_image`, `install_cmd`, `dev_cmd`, `port`, `manifest_source`), and both already support a `shipyard.json` manifest (`{"app": {"runtime", "install", "dev", "port"}}`) that — when present in `/app` — takes priority over file-presence heuristics in `detect_stack`.

The critical existing-behavior gap this design closes: `create_app` (`backend/crates/api/src/sandbox_runtime/routes.rs`) creates a template-based sandbox by inserting a **fully-resolved** `sandbox_app_configs` row directly, explicitly **skipping** probe detection ("the template already declares the runtime... there is nothing to detect"). Once inserted, `provision_sandbox`'s `fetch_config` call finds this row on every future start and never re-probes. This means today, there is no way for a sandbox to start in an "empty, not yet decided" state and later have Shipyard notice a real project the user creates by hand — exactly what a "Custom" mode needs.

The terminal (`SandboxTerminal.svelte` + `backend/crates/api/src/sandbox_runtime/exec.rs`) already works against any running sandbox container, and seed scripts (`seed_script_b64`, run once on first boot only if `/app` is empty — `manager.rs`'s `build_startup_command`) are already just arbitrary shell commands, with no technical restriction to writing static file content specifically.

## Decisions

### A new `manifest_source` state: `'pending'`

Extends the existing `sandbox_app_configs.manifest_source` CHECK constraint (`'undetected' | 'detected' | 'manifest'`) with a fourth value, `'pending'`. This is the state a Custom-created sandbox starts in, and the *only* state where `provision_sandbox` re-runs stack detection on every start rather than trusting the cached row. Once detection succeeds, the row is updated in place and `manifest_source` transitions to `'detected'` or `'manifest'` (matching `DetectionSource`'s own vocabulary) — permanently exiting `'pending'`, after which the sandbox behaves exactly like any other resolved sandbox (cached config, no more re-probing). This is a one-way transition: once real, always real.

### Custom sandbox creation

`create_app` gains a new `Template::Custom` branch that inserts a `sandbox_app_configs` row with:
- `runtime = "custom"`, `base_image = "node:20-alpine"` (a safe default capable of running any of the scaffolding CLIs a user might reach for by hand)
- `install_cmd = NULL`, `dev_cmd = "sleep infinity"` (keeps the container alive with nothing running yet — critical, since `provision_sandbox` errors out and never creates the container at all if stack detection fails on a truly empty `/app`, so this placeholder is what makes an idle, terminal-only sandbox possible in the first place)
- `port = 3000` (unused until a real stack is detected)
- `manifest_source = 'pending'`
- `seed_script_b64` writes a starter `shipyard.json` into `/app` (not a real project) with example values (`{"app": {"runtime": "node", "install": "npm install", "dev": "npm run dev", "port": 3000}}`) so a user who wants to hand-declare their stack, rather than rely on file-presence auto-detection, has a template to edit.

### Re-detection on restart

`provision_sandbox` (`manager.rs`), after `fetch_config` returns an existing row, checks `manifest_source == "pending"` as a new branch *before* the existing "config is None → probe_and_detect" branch. If pending, it re-runs the same `probe_and_detect` used for brand-new sandboxes against the sandbox's current `/app` (reusing the existing probe-container mechanism unchanged). Two outcomes:
- **Detection succeeds** (finds a real `package.json`, or a `shipyard.json` the user edited by hand): `sandbox_app_configs` is updated with the detected `runtime`/`base_image`/`install_cmd`/`dev_cmd`/`port`, and `manifest_source` is set to the detected source (`'detected'` or `'manifest'`). The container is then created and started using this newly-resolved config, exactly as the probe-detection path already does for a from-scratch sandbox.
- **Detection still fails** (still-empty `/app`): the existing placeholder config (`dev_cmd = "sleep infinity"`) is used unchanged, and the row stays `'pending'`. No error is surfaced — an idle container is a valid, expected state for a Custom sandbox the user hasn't scaffolded yet.

This means the user's actual workflow is: create a Custom sandbox → open it (idle) → open the Terminal tab → run their scaffolding tool (`npx create-react-app .`, or anything else) → **Stop, then Start** the sandbox again (the existing stop/start toggle already recreates the container from scratch on every start; no new endpoint is introduced) → Shipyard detects the new project and boots its real dev server. If they'd rather hand-declare, editing the seeded `shipyard.json` and restarting works identically, going through the exact same manifest-priority path `detect_stack` already implements for every other sandbox.

### Framework template gallery: six frameworks, real scaffolders

New `Template` variants: `React`, `ReactTs`, `Vue`, `VueTs`, `SvelteKit`, `SvelteKitTs`, `Next`, `NextTs`, `Nuxt`, `Astro`, `AstroTs` (Nuxt ships as a single variant — Nuxt 3's own scaffolder defaults to TypeScript with no meaningful plain-JS mode in its current tooling, so forcing an artificial split there would be hollow; every other framework's official CLI genuinely supports both). Each follows the same three-stage separation of concerns the existing Node template already uses, rather than letting a scaffolder's own bundled installer run unsupervised:

1. **Seed script** (runs once, first boot only, if `/app` is empty): invokes that framework's own official non-interactive scaffolder (`npm create vite@latest . -- --template react`/`react-ts`/`vue`/`vue-ts`, `npx create-next-app@latest` with its `--ts`/`--js` flag, `npx nuxi@latest init`, `npx sv create` with its TypeScript flag, `npm create astro@latest` with its TypeScript option) with that tool's own auto-install disabled where supported, so it only writes source files.
2. **`install_cmd`**: uniformly `"npm install"` for all variants — kept as a separate, explicit step rather than trusting each scaffolder's bundled install, matching the existing Node template's own convention exactly.
3. **`dev_cmd`**: each framework's real dev-server invocation, explicitly bound to `0.0.0.0` and `$PORT` (e.g. `npm run dev -- --host 0.0.0.0 --port $PORT`; Next.js uses its own `-H`/`-p` flags instead of Vite-style `--host`/`--port`) — required because a bare `npm run dev` binds to localhost-only on most of these tools and would never be reachable from outside the container.
4. **`port`**: each framework's own real default (Vite family — React/Vue/SvelteKit: 5173; Next/Nuxt: 3000; Astro: 4321), fed through the same `PORT` env var mechanism the Node template already relies on, so the declared port and the actual bound port can never drift apart.

**Not locked in by this spec:** the exact current non-interactive CLI flag syntax for `create-next-app`/`nuxi`/`sv create`/`create astro` — these tools version their flags independently of Shipyard's release cycle, and hard-coding a remembered-but-possibly-stale flag here would make the plan wrong on day one regardless of how carefully it's written. The implementation plan verifies each tool's actual current flags against its own `--help` output as a concrete, executed step, not from memory.

**First-boot latency is an accepted, non-regressive tradeoff.** These seed scripts run a real network install + scaffold, taking substantially longer than the current instant static-content seed scripts. This requires no new async handling: `start_sandbox`'s HTTP response already returns as soon as the container *starts* (not once the app inside is listening) — a slow scaffold behaves exactly like today's already-slow `npm install` on an existing Node-template sandbox's first boot. The preview simply won't resolve until the dev server inside finishes booting, same as today.

## Architecture

### Backend: `templates.rs` changes

```rust
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Template {
    Node, Python, Static, Custom,
    React, ReactTs, Vue, VueTs,
    SvelteKit, SvelteKitTs,
    Next, NextTs,
    Nuxt,
    Astro, AstroTs,
}
```
`template_runtime(t) -> (runtime, base_image, install_cmd, dev_cmd, port)` gains one arm per new variant (see Decisions above for exact values/pattern). `template_seed_script_b64(t)` gains one new seed script per variant — the scaffolder invocation for the eleven framework variants, and the `shipyard.json`-writing starter script for `Custom`. `Template::from_str`/`to_str` (or equivalent) gain matching arms — the frontend's `template` field is still a plain string over the wire, unchanged shape.

### Backend: `manager.rs` — `provision_sandbox`'s new branch

```rust
let mut config = fetch_config(&state.db, service_id).await?;
let needs_redetect = config.as_ref().map(|c| c.manifest_source == "pending").unwrap_or(false);
if config.is_none() || needs_redetect {
    match probe_and_detect(state, volume_name).await {
        Ok(stack) => {
            // UPDATE sandbox_app_configs SET runtime=..., base_image=..., install_cmd=...,
            //   dev_cmd=..., port=..., manifest_source=<stack.source>, updated_at=NOW()
            //   WHERE service_id = $1
            config = fetch_config(&state.db, service_id).await?;
        }
        Err(e) if config.is_some() => {
            // Already have a pending row (idle placeholder) to fall back on —
            // detection failing just means /app is still empty; keep serving
            // the placeholder, do not error out.
        }
        Err(e) => return Err(AppError::BadRequest(format!("Could not start sandbox: {e}"))),
    }
}
let config = config.ok_or_else(|| AppError::Internal("sandbox config missing after insert".to_string()))?;
```
This is a genuinely new code path (a from-scratch sandbox with `config.is_none()` already calls `probe_and_detect` and *inserts* a fresh row on success today; a `'pending'` sandbox already *has* a row and needs an *update*, and must tolerate detection failure without erroring, unlike the from-scratch path) — the exact SQL/control-flow split is an implementation-plan-level decision, not re-litigated here.

### Backend: `routes.rs` — `create_app`'s new branch + status exposure

`create_app` gains a `Template::Custom` arm inserting the placeholder row (`manifest_source = 'pending'`) instead of `'manifest'`. The existing `GET /apps/:service_id/sandbox/status` response gains one new field, `pending: bool` (`manifest_source == "pending"`), for the frontend's editor-page nudge to key off — no new endpoint.

### Frontend: template picker

`SandboxAppTemplatePanel.svelte`'s current 3-button row is replaced with a scrollable, grouped card grid ("Basic": Node/Python/Static/Custom; "Frameworks": the six, each one card with an inline JS/TS toggle where that framework's tooling supports both — not a doubled flat list). `api.createSandboxApp`'s `template` field grows to accept the new string values; no other client-side shape changes.

### Frontend: editor page pending-state nudge

The editor page's existing sandbox-status poll (already used to populate `SandboxInstance`) gains the new `pending` field. When `bootState === 'ready' && instance?.pending`, the Editor tab shows a dismissible-per-session banner: *"No project detected yet — scaffold one in the Terminal below, then Stop and Start to apply it."* No new polling loop — this rides on data already being fetched.

## Testing

- **Backend unit tests**: `template_runtime`/`template_seed_script_b64` for every new variant, mirroring the existing Node/Python/Static test shape (seed script decodes to valid shell containing the expected scaffolder invocation; runtime/base_image/install/dev/port match the documented values). A `manager.rs` test for the `'pending'`-triggers-redetect branch, and for the "detection fails while pending → placeholder config retained, no error" branch — both pure/mockable against `probe_and_detect`'s already-testable seams.
- **Integration**: no automated test can safely exercise a real `npx create-next-app` network scaffold in CI (same category of gap this codebase already accepts for the Cloudflare integration's real-account verification). Manual verification (the plan's final task): create a Custom sandbox, scaffold a project by hand via the terminal, Stop/Start, confirm the real dev server and preview come up; separately, create one sandbox per new framework template and confirm each one's preview renders that framework's real starter page.
