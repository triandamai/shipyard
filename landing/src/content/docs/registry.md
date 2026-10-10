---
title: Container registry
description: Push, pull and deploy images from Shipyard's built-in OCI registry.
---

# Container registry

Shipyard includes a built-in OCI-compatible container registry. Push images from your local machine or CI pipeline, then deploy them to your Swarm services with zero external dependencies. The registry speaks the standard Docker Distribution API — any tool that works with Docker Hub or GHCR works here too.

| Item | Value |
|---|---|
| Registry URL | `registry.your-domain.com` |
| Auth | `API key (ship_...)` |
| Protocol | `OCI Distribution Spec v1.1` |
| Image path | `org/project/repo:tag` |

## Quick start {#quickstart}

1. Create an API key with `registry:manage` scope in **Settings → API Keys**
2. Run `docker login registry.your-domain.com -u anyuser -p ship_xxx`
3. Tag your image: `docker tag myapp registry.your-domain.com/acme/myproject/myapp:latest`
4. Push: `docker push registry.your-domain.com/acme/myproject/myapp:latest`
5. In a Shipyard service, set the image to the full registry URL and deploy

## API keys {#api-keys}

The registry uses Shipyard API keys for authentication — not separate registry credentials. Keys start with `ship_` and are scoped to control exactly what the holder can do.

| Scope | Allows |
|---|---|
| `registry:view` | Pull images only — read-only access to all repositories in the organization |
| `registry:manage` | Pull *and* push — full read/write access; also includes `registry:view` |

### Creating a key

1. Go to **Settings → API Keys**
2. Click **New API key**
3. Give the key a name (e.g. *CI Push* or *Read-only pull*)
4. Select `registry:manage` (push + pull) or `registry:view` (pull only)
5. Optionally set an expiry date
6. Copy the key immediately — it is shown only once

> [!WARNING]
> Keys are stored as SHA-256 hashes. If you lose a key, revoke it and create a new one. Never commit keys to source control — use CI/CD secrets instead.

## docker login {#docker-login}

The registry implements the Docker token authentication spec. Docker automatically negotiates a short-lived JWT when you run `docker login` — you never interact with the token directly.

### Login command

```bash
docker login registry.your-domain.com \
  -u anyuser \
  -p ship_xxxxxxxxxxxxxxxxxxxx
```

> [!NOTE]
> The **username is ignored** — pass any non-empty string. Only the password (your `ship_...` API key) is used for authentication.

### How it works

1. Docker sends `GET /v2/` to check the registry
2. The registry returns `401 Unauthorized` with a `WWW-Authenticate` header pointing to `/auth/registry/token`
3. Docker sends Basic auth (username + API key) to the token endpoint
4. Shipyard validates the API key and returns a signed 15-minute JWT
5. Docker uses the JWT for all subsequent push and pull requests

> [!TIP]
> Credentials are cached in `~/.docker/config.json`. You only need to run `docker login` once per machine unless you rotate the key.

## Image naming {#image-names}

Images are namespaced under your organization slug, then a project slug, then a repository name. This mirrors the Shipyard project hierarchy and prevents name collisions between organizations.

```text title="format"
# Format
registry.your-domain.com/<org-slug>/<project-slug>/<repo>:<tag>

# Examples
registry.your-domain.com/acme/backend/api:latest
registry.your-domain.com/acme/backend/api:v1.2.3
registry.your-domain.com/acme/frontend/web:sha-abc1234
```

| Segment | What it maps to | Example |
|---|---|---|
| `org-slug` | Your organization's URL slug | `acme` |
| `project-slug` | The project containing the service | `backend` |
| `repo` | Arbitrary repository name — usually the service name | `api` |
| `tag` | Image tag — `latest`, a version, or a git SHA | `v1.2.3` |

> [!NOTE]
> The org and project slugs are visible in the Shipyard URL when you navigate to a project: `https://ship.example.com/orgs/acme/projects/backend`

## Push & pull {#push-pull}

```bash
# Tag a local image
docker tag myapp:latest registry.your-domain.com/acme/backend/api:latest

# Push
docker push registry.your-domain.com/acme/backend/api:latest

# Pull
docker pull registry.your-domain.com/acme/backend/api:latest
```

### Deploying a pushed image

After pushing, point a Shipyard service at the full registry URL. The simplest way is to set the **Image** field in the service settings to the full registry path and click **Deploy**.

You can also override the image per-deployment via the special environment variable `__IMAGE__`:

```text title="service env vars"
# In a service's environment variables, override the image at deploy time
__IMAGE__=registry.your-domain.com/acme/backend/api:v1.2.3
```

### Private registry credentials in Swarm

For services pulling from the registry at deploy time, Shipyard automatically injects the registry credentials as a Docker Swarm secret so worker nodes can pull without separate `docker login` calls. No extra configuration is needed on worker nodes.

## CI/CD integration {#ci-cd}

Store your API key as a CI secret and use it to authenticate during your build pipeline. Combine with a webhook trigger to redeploy after every successful push.

### GitHub Actions

Add `SHIPYARD_REGISTRY_KEY` to your repository secrets, then:

```yaml title=".github/workflows/build.yml"
name: Build and push to Shipyard registry

on:
  push:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Log in to Shipyard registry
        uses: docker/login-action@v3
        with:
          registry: registry.your-domain.com
          username: ci
          password: ${{ secrets.SHIPYARD_REGISTRY_KEY }}

      - name: Build and push
        uses: docker/build-push-action@v5
        with:
          push: true
          tags: registry.your-domain.com/acme/backend/api:latest
```

### GitLab CI

```yaml title=".gitlab-ci.yml"
variables:
  IMAGE: registry.your-domain.com/acme/backend/api

before_script:
  - docker login $CI_REGISTRY -u ci -p $SHIPYARD_REGISTRY_KEY

build:
  script:
    - docker build -t $IMAGE:$CI_COMMIT_SHORT_SHA .
    - docker push $IMAGE:$CI_COMMIT_SHORT_SHA
```

> [!TIP]
> After pushing a new image tag, trigger a Shipyard service deployment automatically with a [webhook trigger](/docs#webhooks) — no manual clicking required.

## Registry setup {#setup}

The registry is bundled with Shipyard and enabled by default. To make it reachable via its own hostname (recommended), set one environment variable in your `/opt/shipyard/.env` file:

```env title="/opt/shipyard/.env"
SHIPYARD__REGISTRY__HOSTNAME=registry.your-domain.com
```

### DNS setup

Point your registry subdomain at the same server IP as your main Shipyard domain. An **A record** (or CNAME to your main domain) is sufficient:

| Record type | Name | Value |
|---|---|---|
| `A` | `registry` | Your server's public IP |

TLS is provisioned automatically by Traefik via Let's Encrypt — no manual certificate management needed.

> [!NOTE]
> Restart the Shipyard stack after changing `.env`: `cd /opt/shipyard && docker compose restart shipyard-backend`

## Traefik routing {#traefik}

Shipyard auto-generates a Traefik dynamic configuration on every startup. The registry gets its own router rule that adds the `/registry` path prefix before forwarding to the backend — so the standard OCI paths (`/v2/…`) work transparently at the registry hostname.

```yaml title="generated traefik config (excerpt)"
http:
  routers:
    shipyard-registry:
      rule: "Host(`registry.your-domain.com`)"
      entryPoints: [websecure]
      service: shipyard-backend
      middlewares:
        - shipyard-registry-prefix
      tls:
        certResolver: letsencrypt

  middlewares:
    shipyard-registry-prefix:
      addPrefix:
        prefix: "/registry"
```

This means `registry.your-domain.com/v2/…` is routed to the backend as `/registry/v2/…`. The Traefik config file is re-written on every backend startup, so it is self-healing — if the file is lost the routes are restored automatically.

> [!NOTE]
> View the current generated config in the dashboard under **Settings → Traefik → Dynamic**.
