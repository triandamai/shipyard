---
title: Container registry
description: Push, pull and deploy Docker images and static site bundles from Shipyard's built-in OCI registry.
---

# Container registry

Shipyard includes a built-in OCI-compatible container registry. Push images from your local machine or CI pipeline, then deploy them to your Swarm services with zero external dependencies. The registry speaks the standard Docker Distribution API — any tool that works with Docker Hub or GHCR works here too.

It is not limited to Docker images: the same registry stores **static site bundles** (and edge function bundles) as OCI artifacts, so a CI pipeline can build a site once, push it, and have Shipyard deploy it — see [Static site bundles](#static-sites).

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

## Static site bundles {#static-sites}

Any OCI artifact pushed with the `org.shipyard.artifact.kind` manifest annotation is stored as that kind instead of a Docker image. A static site is pushed as a single **zip** layer of your build output with `kind=static_bundle`; a static site service bound to that `repo:tag` deploys it straight from the registry — no build step on the server.

| Kind | Annotation value | Content |
|---|---|---|
| Docker image | *(none — the default)* | Standard image layers |
| Static site | `static_bundle` | One `.zip` layer of the built site |
| Edge function | `edge_function` | One JS bundle layer |

Optional annotations are recorded as metadata and shown in the registry browser:

| Annotation | Example |
|---|---|
| `org.shipyard.framework` | `vite` |
| `org.shipyard.build_command` | `npm run build` |
| `org.shipyard.output_dir` | `dist` |

### Push with ORAS

`docker push` only pushes images, so use [ORAS](https://oras.land) — the standard CLI for pushing arbitrary OCI artifacts. It logs in with the same `ship_` API key.

```bash
# 1. Log in (username is ignored, password is a key with registry:manage)
oras login registry.your-domain.com -u ci -p ship_xxxxxxxxxxxxxxxxxxxx

# 2. Zip the *contents* of your build output (index.html at the zip root)
npm run build
(cd dist && zip -qr ../site.zip .)

# 3. Push it as a static_bundle
oras push registry.your-domain.com/acme/frontend/web:v1.2.0 \
  --artifact-type application/vnd.shipyard.static-bundle \
  --annotation "org.shipyard.artifact.kind=static_bundle" \
  --annotation "org.shipyard.framework=vite" \
  site.zip:application/zip
```

> [!NOTE]
> The bundle must be a **.zip**. Zipping the output folder itself (`dist/index.html` inside the zip) also works — a single wrapping directory is unwrapped automatically, and a `shipyard.json` with `build.output` at the zip root is honoured.

> [!TIP]
> Every push of a version tag also moves the `latest` tag to the same bundle. Bind a site to `latest` for continuous deploys, or to a fixed tag to pin a release.

### Deploy a bundle

1. In a project, add a **Static site** and choose **Source → Shipyard Artifactory**
2. Pick the bundle (`namespace/repo:tag`) from the registry
3. Leave **Auto-deploy on push** checked to redeploy whenever that tag is pushed again
4. Click **Deploy** for the first release

Each deploy fetches the newest bundle pushed to the bound tag, extracts it, and serves it — the previous version stays on disk for instant rollback. Auto-deploy can be toggled later from the service's settings.

### GitHub Actions {#static-sites-ci}

Add `SHIPYARD_REGISTRY_KEY` (a `registry:manage` API key) to your repository secrets. This workflow builds the site, pushes it tagged with the commit SHA (which also updates `latest`), and — with auto-deploy on — Shipyard rolls it out:

```yaml title=".github/workflows/deploy-site.yml"
name: Deploy site to Shipyard

on:
  push:
    branches: [main]

env:
  REGISTRY: registry.your-domain.com
  REPO: acme/frontend/web

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: npm

      - run: npm ci && npm run build

      - name: Package bundle
        run: cd dist && zip -qr ../site.zip .

      - uses: oras-project/setup-oras@v1

      - name: Log in to Shipyard registry
        run: echo "${{ secrets.SHIPYARD_REGISTRY_KEY }}" | oras login $REGISTRY -u ci --password-stdin

      - name: Push static bundle
        run: |
          oras push $REGISTRY/$REPO:sha-${GITHUB_SHA::7} \
            --artifact-type application/vnd.shipyard.static-bundle \
            --annotation "org.shipyard.artifact.kind=static_bundle" \
            --annotation "org.shipyard.build_command=npm run build" \
            --annotation "org.shipyard.output_dir=dist" \
            site.zip:application/zip
```

To ship only on releases instead, trigger on `push: tags: ['v*']`, push `$REPO:${GITHUB_REF_NAME}`, and bind the site to `latest`.

## Deleting tags {#delete-tags}

Remove a single tag without touching the rest of the repository. In the dashboard, open **Registry → namespace → repository** and click the trash icon on the tag's row. **Delete repository** is still available to remove every tag at once.

The same action is available over the API with a session token or an API key that has `registry:manage`:

```bash
curl -X DELETE \
  -H "Authorization: Bearer ship_xxxxxxxxxxxxxxxxxxxx" \
  "https://ship.example.com/api/orgs/<org-id>/registry/namespaces/<namespace-id>/repos/web/tags/v1.2.0?kind=static_bundle"
```

| Behaviour | Detail |
|---|---|
| `kind` query param | Optional. A tag is unique per kind, so omit it to delete every kind under that tag name |
| Other tags | Kept — deleting `v1.2.0` does not remove `latest`, even if both point at the same digest |
| Bound services | Returns `409 Conflict` while a service's artifact source uses the tag; re-point the service first |
| Storage | Manifests and blobs are left for garbage collection, since other tags may reference them |

Platform admins can delete tags in any organization via `DELETE /api/admin/registry/namespaces/<namespace-id>/repos/<repo>/tags/<tag>`.

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
