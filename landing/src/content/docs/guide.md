---
title: Guide
description: Install Shipyard, then manage organizations, projects, services, deployments and infrastructure.
---

# Guide

## Installation {#installation}

Shipyard runs on any Linux VPS with Docker installed. One script sets everything up.

### Prerequisites

- Ubuntu 22+ / Debian 12+ / any modern Linux distro
- Docker ≥ 24 and Docker Compose v2 (the script installs them if missing)
- A domain name pointed at your server (optional but recommended for HTTPS)
- Ports `80`, `443`, and `8080` open

### Run the install script

```bash
curl -fsSL https://shipyard.trian.space/install.sh | sudo bash
```

The script will prompt you for:

- **Domain** — e.g. `ship.example.com` (or leave blank to use the server IP)
- **Enable HTTPS** — Yes sets up Let's Encrypt via Traefik automatically
- **Admin email** — used for the Let's Encrypt certificate

> [!NOTE]
> All configuration is written to `/opt/shipyard/`. The stack runs as Docker Compose services. To restart: `cd /opt/shipyard && docker compose restart`

### What gets installed

- **shipyard-backend** — Rust API server (Axum)
- **shipyard-frontend** — SvelteKit dashboard
- **PostgreSQL** — all platform state
- **Traefik** — reverse proxy + automatic TLS
- **RMQTT** — MQTT broker for real-time events

> [!NOTE]
> **Multi-node cluster support included**
>
> This installs Shipyard on a single VPS as the **manager node**. You can scale to a full Docker Swarm cluster at any time — no reinstall needed. Add more VPS machines as worker nodes with one script and Swarm distributes your workloads automatically across all machines.
>
> [Learn about Swarm & Multi-node](#swarm)

## Initial setup {#initial-setup}

On first visit, Shipyard shows a setup wizard.

1. Open `http://<your-server>` (or your domain if HTTPS is configured)
2. Create your **admin account** — enter an email and password
3. Create your first **organization** — give it a name and a URL slug
4. You are taken to the dashboard and are ready to deploy

> [!TIP]
> The setup wizard only runs once. After the admin account is created, the `/setup` route redirects to login.

## Manage organization {#manage-org}

Organizations are the top-level container for all projects and members. You can belong to multiple organizations.

### General settings

Navigate to **Settings → General** to rename the organization or change its URL slug.

> [!WARNING]
> Changing the slug changes all URLs. Share the new link with your team after updating.

### Switching organizations

Click the organization name in the top-left of the sidebar to open the org switcher. You can create a new organization from there as well.

## Members & roles {#members}

Navigate to **Settings → Members** to manage who has access to the organization.

### Roles

| Role | What they can do |
|---|---|
| Owner | Full access — billing, delete org, manage all settings |
| Admin | Manage members, projects, services, and all settings |
| Member | Deploy and manage services in assigned projects |
| Viewer | Read-only access — can view logs and deployment status |

### Inviting members

1. Click **Invite member**
2. Enter the email address and choose a role
3. Optionally assign them to specific projects
4. Click **Send invitation** — the invitee receives an email with a join link

> [!NOTE]
> SMTP must be configured under **Settings → SMTP** for invitation emails to be delivered.

### Fine-grained permissions

Owners and Admins bypass all permission checks automatically. For Members and Viewers, you can grant individual org-level permissions (e.g. `settings:write`, `keys:read`) and project-level access tiers (*View / Deploy / Manage*). Click a member → **Org Permissions** or **Project Access** to edit.

See the full reference in [Permissions reference](#permissions)

## Permissions reference {#permissions}

Shipyard uses a two-layer permission model. **Org-level** permissions control access to platform-wide features. **Project-level** tiers control what a member can do within a specific project.

Permission strings follow the pattern `shipyard:<org_id>:<resource>:<action>` for org-level and `shipyard:<org_id>:<project_id>:<resource>:<action>` for project-level. Owners and Admins bypass all checks; these only apply to Members and Viewers.

### Org-level permissions

These are granted directly to a member within the organization and are independent of any project.

| Permission string | Label | What it allows |
|---|---|---|
| `settings:read` | View settings | Read org settings, main domain, and Traefik config |
| `settings:write` | Edit settings | Modify org settings, SMTP, and domain config |
| `members:read` | View members | See the member list and their roles |
| `members:invite` | Invite members | Send invitations to new members |
| `members:manage` | Manage members | Change roles, set permissions, and remove members |
| `projects:read` | View all projects | Access any project in the organization |
| `projects:write` | Manage projects | Create and delete projects |
| `providers:read` | View providers | View connected Git provider accounts and webhook config (Settings → Providers) |
| `providers:write` | Manage providers | Connect / disconnect GitHub, GitLab, Bitbucket and set webhook secrets |
| `infra:read` | View infrastructure | View system metrics, swarm nodes, join tokens, and core service health |
| `infra:write` | Manage infrastructure | Add/remove swarm nodes and modify cluster config |
| `static:read` | View static server | View nginx static server configuration and site conf files (Settings → Static) |
| `docker:read` | View Docker | Browse containers, services, volumes, and networks |
| `docker:write` | Manage Docker | Prune containers and perform destructive Docker operations |
| `deployments:read` | View deployments | View deployment history and status across all projects |
| `deployments:write` | Manage deployments | Configure deployment parallelism and settings |
| `smtp:read` | View SMTP config | View outgoing email configuration |
| `smtp:write` | Manage SMTP config | Edit and test SMTP / email settings |
| `audit:read` | View audit logs | Read organization activity history |
| `keys:read` | View API keys | List API keys in the organization |
| `keys:write` | Manage API keys | Create and revoke API keys |
| `system:update` | Update Shipyard | Trigger platform updates and view update logs |

> [!NOTE]
> Full string stored in the database: `shipyard:<org_id>:settings:read`. The UI works with the suffix only (`settings:read`) and prepends the org ID at save time.

### Project-level permission tiers

When assigning a member to a project you choose a tier — *View*, *Deploy*, or *Manage*. Each tier is additive: *Deploy* includes everything in *View*, and *Manage* includes everything in *Deploy*.

| Tier | What it allows | Permission strings granted |
|---|---|---|
| View | Read-only access to services and deployments | `project:view`  
 `service:view` |
| Deploy | Trigger deployments, restarts, and rebuilds | *(View permissions, plus:)*  
 `service:deploy` |
| Manage | Create, edit, delete services; manage envs, domains, volumes, networks; access DB client | *(Deploy permissions, plus:)*  
 `project:manage` · `service:write` · `service:delete`  
 `env:read` · `env:write` · `domain:write`  
 `volume:write` · `network:write` |

> [!NOTE]
> Full string format: `shipyard:<org_id>:<project_id>:service:write`. These are stored as an array in `project_members.permissions` and checked by each backend endpoint.

### Which tier is needed for each feature

| Feature | Minimum tier / permission |
|---|---|
| View services, logs, topology | View |
| View deployment history | View |
| Trigger deploy / redeploy | Deploy |
| Restart / stop / start service | Deploy |
| Create or edit a service | Manage |
| Delete a service | Manage |
| Add / edit environment variables | Manage |
| Add / edit domains | Manage |
| Manage volumes & networks | Manage |
| Open DB Client | Manage (`service:write`) |
| View org settings | Org: `settings:read` |
| Edit org settings / SMTP / OAuth | Org: `settings:write` |
| View infrastructure metrics | Org: `infra:read` |
| Invite new members | Org: `members:invite` |
| Change member roles / permissions | Org: `members:manage` |
| View / create API keys | Org: `keys:read` / `keys:write` |
| Trigger Shipyard platform update | Org: `system:update` |

## Manage projects {#projects}

Projects group related services together. Every service belongs to exactly one project.

### Creating a project

1. Click **New Project** in the sidebar
2. Enter a project name — the slug is generated automatically
3. Click **Create**

### Topology canvas

Each project has a visual canvas showing all its services as nodes. Drag nodes to reposition them. Edges between nodes represent network connections between services.

- Click a node to open the **Service Detail Panel** on the right
- Status dots update in real time via MQTT — no page refresh needed
- Right-click a node to access quick actions (deploy, stop, delete)

## Services & resources {#services}

A service is a containerized workload. Shipyard supports several service types:

| Type | Description |
|---|---|
| `docker` | Pull and run any Docker image |
| `git` | Build from a Git repository (Dockerfile or Nixpacks) |
| `static` | Serve static files via nginx — defaults to `nginx:alpine` |
| `database` | Run a database image with preset options (Postgres, MySQL, Redis, etc.) |
| `docker_compose` | Import a Compose file as managed services |
| `sandbox_app` | Disposable coding sandbox — browser terminal, live preview, and one-click framework scaffolding (React, Vue, SvelteKit, Next.js, Nuxt, Astro) |

> [!NOTE]
> Sandbox apps are a different workflow from the other service types — see the dedicated [Sandbox Apps](/docs/sandbox) guide for templates, the Custom scaffold-your-own mode, and the editor/terminal/preview UI.

### Creating a service

1. Inside a project, click **Add Service**
2. Choose the service type
3. Fill in the image, port, and replica count
4. Click **Create** — the service is created in *idle* state
5. Click **Deploy** to start it

### Resource limits

In the **Service Detail Panel → Settings**, scroll to *Resource Limits*. Set a CPU limit (in cores, e.g. `0.5`) and a memory limit (in MB, e.g. `512`). These are enforced by Docker Swarm on each task.

### Replicas & scaling

Adjust the **Replicas** field in Settings and hit Save. Swarm spreads replicas across available nodes. With a single node all replicas run on that node.

## Environment variables {#env-vars}

Click the **Env** button in the service header to open the environment variable manager.

- Variables marked **Secret** are stored encrypted and masked in the UI
- Changes take effect on the next deployment — existing containers are not restarted automatically
- Use `__IMAGE__` as a special key to override the service image at deploy time

> [!TIP]
> Bulk-import variables by pasting a `.env` file format (KEY=value) into the import field.

## Domains & HTTPS {#domains}

Open a service → **Domains** tab to add a custom hostname.

1. Click **Add domain**
2. Enter the hostname (e.g. `api.example.com`)
3. Set the internal port the service listens on
4. Toggle **TLS** to enable Let's Encrypt — Traefik requests and renews the certificate automatically

> [!NOTE]
> The domain must resolve to your server's IP before TLS provisioning will succeed. Traefik uses HTTP-01 challenge by default.

You can assign multiple domains to a single service. Each domain becomes its own Traefik router rule.

## Deploy from Git {#deploy-git}

Create a service with type `git`, connect a repository, and Shipyard builds and deploys it on demand.

### Build process

1. Shipyard clones the repository at the configured branch
2. If a `Dockerfile` is found in the directory path, it builds with Docker Build
3. Otherwise it falls back to **Nixpacks** for automatic language detection
4. The built image is pushed to the local registry and deployed to Swarm

### Deployment steps

Every deployment goes through numbered steps visible in the **Deploy** tab:

0. Validate config
1. Acquire image (clone + build, or pull)
2. Prepare networks
3. Prepare volumes
4. Configure domains
5. Create or update Swarm service
6. Write audit log

Click any step to expand its log output.

## Deploy Docker image {#deploy-image}

Create a service with type `docker` (or `database` / `static`), set the image tag, and click **Deploy**.

### Image digest pinning

After pulling the image Shipyard resolves it to its `sha256` digest (`nginx@sha256:abc...`). Docker Swarm compares image refs by string — using the digest means Swarm detects a new image even when the tag (`:latest`) hasn't changed. This is how redeployments pick up the freshest image without a manual restart.

### Private registry

In **Settings → Docker Image → Registry Credentials**, enter the registry URL, username, and password. These are stored as service-scoped secrets and injected into the Swarm service spec at deploy time.

### Database presets

For `database` type services, the Settings tab shows one-click preset buttons: `postgres:16`, `mysql:8`, `redis:7-alpine`, `mongo:7`, `mariadb:11`. Clicking a preset fills the image field.

## Docker Compose import {#deploy-compose}

Navigate to a project → **Import Compose** to turn a `docker-compose.yml` into managed Shipyard services.

1. Paste or upload your Compose YAML
2. Set the root service name — used as the parent in the topology canvas
3. Click **Import** — Shipyard creates a service, network, and volume entry for each definition in the file
4. Deploy each imported service individually or trigger them in order

> [!WARNING]
> `build:` directives are not supported during import — use a pre-built image reference instead.

## Webhook triggers {#webhooks}

Trigger a deployment from any external system (GitHub Actions, GitLab CI, a cron job) without using the dashboard.

### Getting the webhook URL

1. Open a service → **Settings** tab → scroll to *Webhook*
2. Click **Reveal token** to see your webhook URL
3. Copy the URL — it looks like `POST /api/projects/:id/services/:id/deploy/webhook?token=...`

### Triggering a deployment

```bash
curl -X POST "https://ship.example.com/api/projects/PROJECT_ID/services/SERVICE_ID/deploy/webhook?token=TOKEN"
```

A `201 Created` response means a deployment was queued. The response body contains the deployment ID.

### GitHub Actions example

```yaml
- name: Deploy to Shipyard
  run: |
    curl -fsS -X POST "${{ secrets.SHIPYARD_WEBHOOK_URL }}"
```

## Rollback {#rollback}

Every successful deployment records the exact image digest that was running. You can roll back to any prior successful deployment.

### How to roll back

1. Open a service → **Deploy** tab
2. Find a past deployment with status success
3. Click the **↩** rollback button next to it
4. A new deployment is created — steps 0 and 1 are skipped (image is already known), the service is updated to the pinned digest

> [!NOTE]
> Only deployments created after the rollback feature was enabled have a recorded image digest. Earlier deployments show no rollback button.

## Audit log {#audit-log}

Navigate to **Settings → Audit** to view a full history of actions taken across the organization.

Each entry records:

- The **action** (e.g. `service.deploy`, `member.invite`, `service.delete`)
- The **user** who performed it
- The **resource** affected
- The **IP address** of the request
- The **timestamp**

The log is paginated (50 entries per page). Use the Prev / Next buttons to navigate.

## Git providers {#git-providers}

Connect your GitHub, GitLab, or Bitbucket account to enable OAuth login and private repository access. Navigate to **Settings → Providers** to manage all connections. Requires the `providers:read` permission (or Admin/Owner role).

### Personal access token (PAT)

1. Open **Settings → Providers** and click **Connect** next to the provider
2. Paste a Personal Access Token with repository read scope
3. Click **Save Token** — Shipyard stores it encrypted and uses it for all deploys

### GitHub OAuth setup

1. Go to GitHub → **Settings → Developer settings → OAuth Apps → New OAuth App**
2. Set *Homepage URL* to your Shipyard domain
3. Set *Authorization callback URL* to `https://ship.example.com/auth/oauth/github/callback`
4. Copy the **Client ID** and **Client Secret**
5. In Shipyard, open **Settings → Providers**, click **Connect via OAuth** and complete the flow

GitLab and Bitbucket follow the same pattern — create an OAuth application in each provider's developer settings and complete the OAuth flow in Shipyard.

### Webhook secret

To verify push event signatures from your provider, set a **Webhook Secret** on the Providers page and configure the same value in each webhook you create on GitHub/GitLab. The incoming webhook URL is shown on the same page and in each service's detail panel.

> [!NOTE]
> Admins can grant `providers:read` / `providers:write` to Members so they can view or manage provider connections without full settings access.

## SMTP {#smtp}

Configure SMTP so Shipyard can send invitation emails and notifications.

### Configuration

1. Go to **Settings → SMTP**
2. Toggle **Enable SMTP**
3. Enter host, port, username, password, and from address
4. Click **Save**, then **Send test email** to verify delivery

| Field | Example |
|---|---|
| Host | `smtp.gmail.com` |
| Port | `587` (STARTTLS) or `465` (SSL) |
| Username | `you@gmail.com` |
| Password | App password (not your account password) |
| From address | `shipyard@example.com` |

> [!NOTE]
> SMTP settings are read from the database at send-time. Changing them takes effect immediately — no backend restart required.

## API keys {#api-keys}

API keys give programmatic access to Shipyard's API — useful for CI/CD pipelines or automation scripts.

### Creating a key

1. Go to **Settings → API Keys**
2. Click **New API key**
3. Give it a name, choose scopes, and optionally set an expiry date
4. Copy the key immediately — it is shown only once

Keys look like `ship_...` and are authenticated via the `Authorization: Bearer ship_...` header.

```bash
curl https://ship.example.com/api/orgs \
  -H "Authorization: Bearer ship_your_key_here"
```

> [!WARNING]
> Keys are stored as SHA-256 hashes. If you lose the key, revoke it and create a new one.

## Infra monitoring {#infra}

Navigate to **Settings → Infrastructure** to view real-time metrics about the host machine.

### Metrics streamed live

- **CPU** — usage percentage with color-coded gauge (green / orange / red)
- **Memory** — used vs total, with swap if present
- **Disk** — per-mount usage and percentage
- **Network** — cumulative RX/TX bytes per interface since boot (Docker internal interfaces are hidden)

Metrics are delivered via Server-Sent Events (SSE) — the *Live* badge in the toolbar turns green when the stream is connected. Click **Reconnect** if the stream drops.

## Swarm & multi-node {#swarm}

Shipyard runs on Docker Swarm. You can add additional VPS nodes to distribute workloads across machines.

### How Swarm works

- Your Shipyard server is the **manager node** — it schedules services across the cluster
- Additional VPS machines join as **worker nodes** — they run containers but have no scheduling control
- Swarm automatically re-schedules replicas if a worker goes offline
- Stateless services (APIs, web apps) work across nodes with zero extra config
- Stateful services (databases) should be pinned to a specific node using placement constraints

### Adding a worker node

On the new VPS, run the guided setup script:

```bash
curl -fsSL https://shipyard.trian.space/worker-setup.sh | sudo bash
```

The script will:

1. Check and install Docker if needed
2. Prompt for the manager address and join token (find these in **Settings → Infrastructure → Join Tokens**)
3. Optionally configure Docker Hub, GitHub Container Registry (`ghcr.io`), or a custom registry
4. Join the swarm and verify the connection

### Join tokens

Find the ready-to-copy `docker swarm join` commands in **Settings → Infrastructure → Join Tokens**. There are two token types:

| Token type | Use for |
|---|---|
| worker | Standard nodes that run workloads — use this for most VPS additions |
| manager | Nodes that also participate in scheduling — use for HA setups (3+ managers) |

> [!WARNING]
> Worker nodes only need Docker installed — no Shipyard stack, no Postgres, no Traefik.

### Viewing nodes

The **Swarm Nodes** table on the infra page shows each node's hostname, role, status (*ready / down*), availability (*active / drain / pause*), address, and Docker engine version.

## Static server {#static-server}

Shipyard includes an nginx-based static file server (`shipyard-nginx-static`) that hosts static sites deployed from the platform. Navigate to **Settings → Static** to inspect its current nginx configuration. Requires the `static:read` permission (or `infra:read` / Admin).

### Conf file viewer

The left panel lists all `.conf` files inside the container's `/etc/nginx/conf.d/` directory. Select a file to view its full content in the right panel. Files are named after the service slug (e.g. `my-landing-page.conf`).

> [!NOTE]
> A conf file is only written when at least one domain is assigned to a static service. Services without a domain return 404 until a domain is attached.

## Docker resources {#docker-resources}

Navigate to **Settings → Docker** to inspect and manage raw Docker resources on the host.

### Containers

Lists all containers (running and stopped) with their image, status, and port bindings. Use **Prune stopped containers** to reclaim disk space.

### Services

Lists all Swarm services currently deployed — name, image, running vs desired replicas, and exposed ports.

### Volumes

Lists all named volumes with their driver, mountpoint, and scope. You can remove unused volumes from here.

### Networks

Lists all Docker networks — driver, scope, subnet, and attached container count. Internal networks created by Shipyard for service isolation appear here.

## MQTT settings {#mqtt}

Shipyard includes an embedded RMQTT broker that powers real-time updates in the dashboard (service status, topology changes, deployment logs).

Navigate to **Settings → MQTT** to view:

- **Connected clients** — browser tabs and backend workers subscribed to the broker
- **Active subscriptions** — per-client topic list
- **Active topics** — topic names with subscriber counts

### Topic structure

```text title="topics"
shipyard/services/{service_id}/status
shipyard/services/{service_id}/containers
shipyard/deployments/{deployment_id}/logs
shipyard/topology/{project_id}
```

> [!NOTE]
> You can subscribe to these topics from any MQTT client (e.g. MQTTX) using the broker credentials shown in Settings → MQTT.

## Traefik settings {#traefik}

Traefik is the reverse proxy that routes incoming requests to your services and handles TLS certificates.

Navigate to **Settings → Traefik** to inspect the generated configuration files.

### Static config

The static configuration (`traefik.yml`) sets up the entry points and certificate resolver. It is generated once at install time. Key settings:

```yaml
entryPoints:
  web:      # port 80  — HTTP, redirects to HTTPS
  websecure: # port 443 — HTTPS
certificatesResolvers:
  letsencrypt:
    acme:
      email: your@email.com
      storage: /letsencrypt/acme.json
```

### Dynamic config

Shipyard writes dynamic configuration (routers and services) automatically when you add or update a domain on a service. View the current dynamic config in **Settings → Traefik → Dynamic**.

> [!TIP]
> If a certificate isn't being issued, check that port 80 is open and the domain resolves to your server's IP. Traefik logs are visible via `docker logs shipyard-traefik-1`.

## Update profile {#profile}

Click your avatar or email in the top-right of the sidebar → **Profile**.

### Change email

Enter a new email address and confirm your current password. The change takes effect immediately.

### Change password

Enter your current password, then the new password twice. You will be logged out of all other sessions.

## Update Shipyard {#update}

Navigate to **Settings → General** to check for and apply updates.

### One-click update

1. The current version is shown alongside the latest available release
2. Click **Update now** — Shipyard pulls the new images and restarts the stack
3. The update log streams in real time so you can follow the progress

Alternatively, update manually from the server:

```bash
cd /opt/shipyard
docker compose pull
docker compose up -d
```

> [!NOTE]
> Database migrations run automatically on startup. No manual migration step is needed.

## Command palette {#command-palette}

The command palette gives you keyboard-driven access to any page, service, or action in Shipyard without touching the mouse.

### Opening the palette

<kbd>⌘</kbd> + <kbd>K</kbd> on macOS

<kbd>Ctrl</kbd> + <kbd>K</kbd> on Windows / Linux

### What you can do

- Search services by name and jump to them directly
- Navigate to any settings page
- Trigger a deployment on a specific service
- Switch organizations
- Open the audit log, API keys, or member management

Type to filter. Use <kbd>↑</kbd> <kbd>↓</kbd> to navigate results and <kbd>Enter</kbd> to select. Press <kbd>Esc</kbd> to close.
