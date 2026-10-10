---
title: Sandbox apps
description: Write and preview small apps in the browser with Shipyard's in-browser editor.
---

# Sandbox apps

A Sandbox App is a disposable, containerized coding environment with a real terminal, a file editor, and a live preview URL — no local setup required. Pick a framework template and Shipyard scaffolds it with the official CLI, installs dependencies, and boots the dev server automatically. Or start from a blank **Custom** container and scaffold whatever you want by hand.

| Item | Value |
|---|---|
| Preview URL | `preview-<id>.your-domain.dev` |
| Idle timeout | `20 minutes (default)` |
| Templates | `4 basic + 11 framework variants` |
| Terminal | `Real shell, via WebSocket` |

## Quick start {#quickstart}

1. Inside a project, click **New App** and choose a Sandbox App
2. Pick a template — a framework (React, Vue, SvelteKit, Next.js, Nuxt, Astro), a basic runtime (Node, Python, Static), or **Custom** for a blank container
3. Give it a name — the slug is generated automatically — and click **Create App**
4. Open the app to reach the editor: on first boot, Shipyard scaffolds the template, installs dependencies, and starts the dev server
5. Edit files, use the terminal, and watch the **Preview** tab update live

> [!NOTE]
> A framework template's first boot runs a real `npm create …` scaffold and install over the network — it takes noticeably longer than a plain Node sandbox's first start. Subsequent stop/start cycles are fast since the scaffolded project already lives on the sandbox's persistent volume.

## Basic templates {#basic-templates}

Four built-in starting points beyond the framework gallery below. Each resolves to a fully-declared runtime at creation time — nothing to detect.

| Template | Base image | Install | Dev command | Port |
|---|---|---|---|---|
| **Node** | `node:20-alpine` | npm install | `npm run dev` | 3000 |
| **Python** | `python:3.12-slim` | pip install -r requirements.txt | `python app.py` | 8000 |
| **Static** | `nginx:alpine` | — | `nginx (static)` | 8080 |
| **Custom** | `node:22-alpine` | — | `sleep infinity (idle)` | — |

> [!TIP]
> **Custom** boots an idle container (`sleep infinity`) with nothing installed yet — see [Scaffold your own](#custom-mode) for the full workflow.

## Framework gallery {#framework-templates}

Each framework template invokes that framework's own official, non-interactive scaffolder on first boot — the same command you'd run on your own machine. Dependencies install as a separate, explicit step, then the dev server starts bound to `0.0.0.0` and the sandbox's assigned port so the preview can actually reach it.

| Framework | Variants | Scaffolder | Dev server default port |
|---|---|---|---|
| **React** | JS / TS | `npm create vite@latest` | 5173 |
| **Vue** | JS / TS | `npm create vite@latest` | 5173 |
| **SvelteKit** | JS / TS | `npx sv create` | 5173 |
| **Next.js** | JS / TS | `npx create-next-app@latest` | 3000 |
| **Nuxt** | TS only | `npx nuxi@latest init` | 3000 |
| **Astro** | JS / TS | `npm create astro@latest` | 4321 |

> [!NOTE]
> Nuxt ships as a single template — its own scaffolder defaults to TypeScript with no meaningful plain-JS mode in current tooling, so there's no separate JS variant to offer.

> [!WARNING]
> Astro's own tooling requires a newer Node runtime than the other frameworks, so Astro sandboxes run on a different base image under the hood. This is handled automatically — nothing to configure.

## Scaffold your own (Custom mode) {#custom-mode}

Choose the **Custom** template to skip the gallery entirely. It boots an empty, idle container — nothing installed, no dev server running — so you can scaffold exactly what you want by hand from the Terminal tab.

### The workflow

```bash title="inside the sandbox terminal"
# Inside the sandbox's Terminal tab
npx create-react-app .
# or: npm create vite@latest . -- --template svelte
# or literally anything else — it's just a shell

# Then, from the app's canvas panel:
#   Stop  ->  Start
# Shipyard re-detects the real stack and boots its dev server.
```

Shipyard only re-checks what's in the container when the sandbox restarts — it never watches the filesystem while it's running. That's why the last step is always **Stop**, then **Start**, from the app's canvas panel: that recreates the container and re-runs stack detection against whatever you scaffolded.

> [!NOTE]
> Until a real project is detected, the editor shows a banner: *"No project detected yet — scaffold one in the Terminal below, then stop and start this sandbox from its app panel on the project canvas to apply it."* The container stays alive and the terminal stays usable the whole time.

### Prefer to declare it yourself?

A Custom sandbox starts with a starter file at `/app/shipyard.json.example`. Rename it to `shipyard.json`, fill in your own values, and restart — Shipyard trusts an explicit manifest over guessing from files on disk. See the [manifest format](#manifest) below.

## shipyard.json manifest {#manifest}

Every sandbox — template-created or Custom — can be pointed at an explicit manifest instead of relying on auto-detection. Drop a `shipyard.json` file in the project root and restart the sandbox.

```json title="shipyard.json"
{
  "app": {
    "runtime": "node",
    "install": "npm install",
    "dev": "npm run dev",
    "port": 3000
  }
}
```

| Field | Meaning |
|---|---|
| `runtime` | Label only — informational, shown in the UI |
| `install` | Shell command run once before the dev server starts |
| `dev` | Shell command that starts the dev server — must bind `0.0.0.0`, not just `localhost` |
| `port` | The port the dev server listens on — the preview route is wired to this exact port |

> [!WARNING]
> A manifest takes priority over every file-presence heuristic (`package.json`, `requirements.txt`, `index.html`, etc.). If `shipyard.json` exists, its values are used as-is — Shipyard will not second-guess it.

## Editor & files {#editor}

Opening a sandbox app lands you on its editor page: a file tree on the left, a code editor in the center, and Preview / Terminal alongside it.

### File tree

- Click any file to open it in the editor
- Create new files directly from the tree — folders are created automatically from the path you type
- Changes autosave as you type; a save indicator confirms when a write completes

### Hot reload

Saving a file that the running dev server watches (which is every framework template — that's exactly what `npm run dev` is for) reflects in the **Preview** tab automatically, the same as running the dev server on your own machine.

### Preview sizing

The Preview tab includes desktop and mobile width presets, plus a free-drag handle, so you can sanity-check responsive layouts without leaving the editor.

## Terminal {#terminal}

A real shell into the running sandbox container, always available below the Editor/Preview panel — not a separate tab you have to switch to. Use it to run scaffolding CLIs, install extra packages, inspect logs, or debug directly.

> [!NOTE]
> The terminal connects over a WebSocket authenticated with a short-lived (5-minute) token minted specifically for that connection — it is re-issued automatically, you never have to think about it.

## Live preview {#preview}

Every sandbox gets its own preview URL, routed through Traefik straight to the running dev server:

```text title="preview URL format"
https://preview-a1b2c3d4.shipyard-apps.dev
```

The preview link works for anyone with the URL — no Shipyard login required — so you can share a running sandbox with a teammate or a client directly.

> [!TIP]
> Visiting a stopped sandbox's preview URL automatically wakes it back up. The first request after a cold start waits for the container and dev server to come up before the page loads.

## Idle timeout & quota {#idle-and-quota}

Sandboxes are meant to be disposable, not long-running services — Shipyard stops them automatically when nobody's using them, and caps how many can run at once per organization.

### Idle timeout

While the editor tab is open it sends a periodic heartbeat. A sandbox with no heartbeat for **20 minutes** (the default; configurable per deployment) is automatically stopped — its files and data are untouched, only the running container and dev server are torn down. Opening the app again starts it right back up.

### Concurrent sandbox limit

Each plan sets a maximum number of sandboxes that may be *running or starting* at the same time across the whole organization. Stop an unused sandbox — or wait for one to idle out — to free up a slot.

> [!NOTE]
> Stopping a sandbox doesn't delete it. Files on its volume, its assigned template, and its preview URL are all preserved — starting it again picks up exactly where you left off.
