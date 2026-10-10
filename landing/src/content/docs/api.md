---
title: API reference
description: Shipyard's Open API: authentication, scopes, endpoints and response formats for CI/CD and automation.
---

# API reference

The Shipyard Open API lets you integrate with Shipyard from CI/CD pipelines, automation scripts, and external tools. All resources are scoped to your organization and authenticated with API keys.

| Item | Value |
|---|---|
| Base URL | `https://your-shipyard/openapi/v1` |
| Auth header | `Authorization: Bearer ship_…` |
| Format | `Content-Type: application/json` |
| Scopes | `read · deploy · write · admin` |

## Authentication {#authentication}

All requests must include a valid API key. Keys are created in **Settings → API Keys** in the Shipyard dashboard. Every key starts with the prefix `ship_`.

### Sending the key

Two headers are accepted — use whichever fits your client:

```http title="Option 1 — Authorization header (recommended)"
Authorization: Bearer ship_your_key_here
```

```http title="Option 2 — X-API-Key header"
X-API-Key: ship_your_key_here
```

### Full example

```bash
curl https://ship.example.com/openapi/v1/projects \
  -H "Authorization: Bearer ship_your_key_here"
```

> [!WARNING]
> Keys are shown only once at creation time. Store them securely — if lost, revoke and create a new one.

## Scopes {#scopes}

Each API key is issued with one or more scopes that control what it can do. Scopes are **not** additive — a key with `deploy` scope cannot read data unless it also has `read`.

| Scope | What it allows |
|---|---|
| `read` | Read organizations, projects, services, and deployment history |
| `deploy` | Trigger deployments on services |
| `write` | Update service configuration (name, replicas) |
| `admin` | Create and revoke API keys — elevated access |

> [!NOTE]
> For CI/CD pipelines that only trigger deployments, issue a key with the `deploy` scope only — no read or write access needed.

## Base URL & versioning {#base-url}

All endpoints are prefixed with `/openapi/v1`. Replace `ship.example.com` with your Shipyard domain in every request.

```text title="base URL"
https://ship.example.com/openapi/v1
```

A `GET /openapi/v1/` request returns the API name and version — useful as a health check.

```bash
curl https://ship.example.com/openapi/v1/ \
  -H "Authorization: Bearer ship_…"
```

```json title="200 OK"
{
  "name": "Shipyard Open API",
  "version": "v1",
  "docs": "https://shipyard.trian.space/docs/api"
}
```

## Errors {#errors}

All error responses share a common JSON body:

```json title="error envelope"
{
  "error": {
    "code":    "NOT_FOUND",
    "message": "Service 'abc...' not found"
  }
}
```

| HTTP status | code | When it occurs |
|---|---|---|
| `400` | `BAD_REQUEST` | Invalid or missing request body / query parameter |
| `401` | `UNAUTHORIZED` | Missing, malformed, or expired API key |
| `403` | `FORBIDDEN` | Key lacks the required scope, or resource belongs to another org |
| `404` | `NOT_FOUND` | The requested resource does not exist |
| `500` | `INTERNAL_ERROR` | Unexpected server error |

## Pagination {#pagination}

List endpoints accept `page` and `per_page` query parameters. The response wraps the array in a `data` + `meta` envelope.

| Parameter | Default | Max | Description |
|---|---|---|---|
| `page` | `1` | — | 1-based page index |
| `per_page` | `20` | `100` | Items per page |

```json title="paginated response envelope"
{
  "data": [ … ],
  "meta": {
    "total":    142,
    "page":     1,
    "per_page": 20
  },
  "request_id": "019…"
}
```

> [!NOTE]
> `request_id` is a UUIDv7 generated per request. Include it when reporting issues.

## Organizations {#ep-orgs}

An API key is tied to exactly one organization. These endpoints let you inspect it.

### GET /openapi/v1/orgs

Requires scope: `read`

Returns the organization this API key belongs to.

```bash
curl https://ship.example.com/openapi/v1/orgs \
  -H "Authorization: Bearer ship_…"
```

```json title="200 OK"
{
  "data": {
    "id":         "018f…",
    "name":       "Acme Corp",
    "slug":       "acme",
    "created_at": "2024-01-15T09:00:00Z"
  },
  "request_id": "019…"
}
```

### GET /openapi/v1/orgs/{org_id}

Requires scope: `read`

Convenience alias — resolves only if `org_id` matches the key's organization. Returns `403` otherwise.

## Projects {#ep-projects}

### GET /openapi/v1/projects

Requires scope: `read`

List all projects in the organization, newest first.

**Query parameters**

| Name | Type | Description |
|---|---|---|
| `page` | integer | Page index (default 1) |
| `per_page` | integer | Items per page, max 100 (default 20) |

```bash
curl "https://ship.example.com/openapi/v1/projects?page=1&per_page=20" \
  -H "Authorization: Bearer ship_…"
```

```json title="200 OK"
{
  "data": [
    {
      "id":         "018f…",
      "org_id":     "018e…",
      "name":       "Production",
      "slug":       "production",
      "created_at": "2024-03-01T12:00:00Z",
      "updated_at": "2024-03-10T08:30:00Z"
    }
  ],
  "meta": { "total": 3, "page": 1, "per_page": 20 },
  "request_id": "019…"
}
```

### GET /openapi/v1/projects/{project_id}

Requires scope: `read`

Get a single project by ID. Returns `404` if it doesn't exist or belongs to another org.

## Services {#ep-services}

### GET /openapi/v1/projects/{project_id}/services

Requires scope: `read`

List all services in a project, newest first.

**Query parameters**

| Name | Type | Description |
|---|---|---|
| `page` | integer | Page index (default 1) |
| `per_page` | integer | Items per page, max 100 (default 20) |

```bash
curl "https://ship.example.com/openapi/v1/projects/PROJECT_ID/services" \
  -H "Authorization: Bearer ship_…"
```

```json title="200 OK"
{
  "data": [
    {
      "id":           "018f…",
      "project_id":   "018e…",
      "name":         "api",
      "slug":         "api",
      "type":         "git",
      "status":       "running",
      "replicas":     2,
      "created_at":   "2024-03-01T12:00:00Z",
      "updated_at":   "2024-03-10T08:30:00Z"
    }
  ],
  "meta": { "total": 5, "page": 1, "per_page": 20 },
  "request_id": "019…"
}
```

### GET /openapi/v1/services/{service_id}

Requires scope: `read`

Get a single service by ID. Returns `404` if it doesn't exist or belongs to another org.

### PATCH /openapi/v1/services/{service_id}

Requires scope: `write`

Update a service's name and/or replica count. At least one field must be provided.

**Request body**

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | no | New display name for the service |
| `replicas` | integer ≥ 0 | no | Desired replica count; 0 stops the service |

```bash
curl -X PATCH https://ship.example.com/openapi/v1/services/SERVICE_ID \
  -H "Authorization: Bearer ship_…" \
  -H "Content-Type: application/json" \
  -d '{"replicas": 3}'
```

```json title="200 OK — updated service object"
{
  "data": {
    "id":         "018f…",
    "project_id": "018e…",
    "name":       "api",
    "slug":       "api",
    "type":       "git",
    "status":     "running",
    "replicas":   3,
    "created_at": "2024-03-01T12:00:00Z",
    "updated_at": "2024-03-10T09:00:00Z"
  },
  "request_id": "019…"
}
```

## Deployments {#ep-deployments}

### POST /openapi/v1/services/{service_id}/deploy

Requires scope: `deploy`

Trigger a new deployment for a service. The deployment runs asynchronously — poll `GET /deployments/:id` to track status. If the platform deployment parallelism limit is reached, the deployment is queued and returns `202 Accepted` with status `queued`.

**Request body (optional)**

| Field | Type | Required | Description |
|---|---|---|---|
| `source_ref` | string | no | Git ref, image tag, or label to record against the deployment (e.g. `main`, `v1.2.3`) |

```bash
curl -X POST https://ship.example.com/openapi/v1/services/SERVICE_ID/deploy \
  -H "Authorization: Bearer ship_…" \
  -H "Content-Type: application/json" \
  -d '{"source_ref": "v2.4.0"}'
```

```json title="202 Accepted"
{
  "data": {
    "deployment_id": "019…",
    "status":        "running"
  },
  "request_id": "019…"
}
```

> [!TIP]
> GitHub Actions example — deploy on every push to `main`:

```yaml title=".github/workflows/deploy.yml"
- name: Deploy to Shipyard
  run: |
    curl -fsS -X POST \
      -H "Authorization: Bearer ${{ secrets.SHIPYARD_API_KEY }}" \
      -H "Content-Type: application/json" \
      -d '{"source_ref":"${{ github.sha }}"}' \
      "https://ship.example.com/openapi/v1/services/${{ vars.SERVICE_ID }}/deploy"
```

### GET /openapi/v1/services/{service_id}/deployments

Requires scope: `read`

List all deployments for a service, newest first. Paginated.

```json title="200 OK"
{
  "data": [
    {
      "id":           "019…",
      "service_id":   "018f…",
      "triggered_by": "api-key:ci-key",
      "source_ref":   "v2.4.0",
      "status":       "success",
      "created_at":   "2024-03-10T09:00:00Z",
      "finished_at":  "2024-03-10T09:02:35Z"
    }
  ],
  "meta": { "total": 12, "page": 1, "per_page": 20 },
  "request_id": "019…"
}
```

### GET /openapi/v1/deployments/{deployment_id}

Requires scope: `read`

Get a single deployment by ID. Use this to poll for completion after triggering a deploy.

| Status value | Meaning |
|---|---|
| `pending` | Created but not yet picked up |
| `queued` | Waiting for parallelism slot |
| `running` | In progress |
| `success` | Completed successfully |
| `failed` | Completed with an error |
| `cancelled` | Cancelled before completion |

```bash title="poll for completion"
# Poll until done (bash)
DEPLOYMENT_ID="019…"
while true; do
  STATUS=$(curl -fsS \
    -H "Authorization: Bearer ship_…" \
    "https://ship.example.com/openapi/v1/deployments/$DEPLOYMENT_ID" \
    | jq -r '.data.status')
  echo "Status: $STATUS"
  [[ "$STATUS" == "success" || "$STATUS" == "failed" ]] && break
  sleep 5
done
```

## API keys {#ep-keys}

Manage API keys programmatically. All key endpoints require the `admin` scope.

### GET /openapi/v1/keys

Requires scope: `admin`

List all API keys for the organization. Paginated. The secret key value is never returned.

```json title="200 OK"
{
  "data": [
    {
      "id":           "019…",
      "org_id":       "018e…",
      "name":         "ci-deploy",
      "key_prefix":   "a1b2c3d4",
      "scopes":       ["deploy"],
      "last_used_at": "2024-03-10T09:00:00Z",
      "expires_at":   null,
      "created_at":   "2024-02-01T00:00:00Z"
    }
  ],
  "meta": { "total": 2, "page": 1, "per_page": 20 },
  "request_id": "019…"
}
```

### POST /openapi/v1/keys

Requires scope: `admin`

Create a new API key. The full key is returned only in this response — store it immediately.

**Request body**

| Field | Type | Required | Description |
|---|---|---|---|
| `name` | string | yes | Human-readable label for the key |
| `scopes` | string[] | yes | One or more of: `read`, `deploy`, `write`, `admin` |
| `expires_at` | ISO 8601 timestamp | no | Optional expiry; `null` means never expires |

```bash
curl -X POST https://ship.example.com/openapi/v1/keys \
  -H "Authorization: Bearer ship_…" \
  -H "Content-Type: application/json" \
  -d '{
    "name":   "ci-deploy",
    "scopes": ["deploy"],
    "expires_at": "2025-01-01T00:00:00Z"
  }'
```

```json title="201 Created — key shown once"
{
  "data": {
    "id":         "019…",
    "name":       "ci-deploy",
    "key":        "ship_a1b2c3d4…",
    "key_prefix": "a1b2c3d4",
    "scopes":     ["deploy"],
    "expires_at": "2025-01-01T00:00:00Z",
    "created_at": "2024-03-10T09:00:00Z"
  },
  "request_id": "019…"
}
```

### DELETE /openapi/v1/keys/{key_id}

Requires scope: `admin`

Revoke an API key immediately. Returns `204 No Content` on success. You cannot revoke the key that is currently authenticating the request.

```bash
curl -X DELETE https://ship.example.com/openapi/v1/keys/KEY_ID \
  -H "Authorization: Bearer ship_…"
```

## Object schemas {#ref-objects}

### Organization

| Field | Type | Description |
|---|---|---|
| `id` | UUID | Unique identifier |
| `name` | string | Display name |
| `slug` | string | URL-safe identifier |
| `created_at` | ISO 8601 | Creation timestamp |

### Project

| Field | Type | Description |
|---|---|---|
| `id` | UUID | Unique identifier |
| `org_id` | UUID | Owning organization |
| `name` | string | Display name |
| `slug` | string | URL-safe identifier |
| `created_at` | ISO 8601 | Creation timestamp |
| `updated_at` | ISO 8601 | Last modification timestamp |

### Service

| Field | Type | Description |
|---|---|---|
| `id` | UUID | Unique identifier |
| `project_id` | UUID | Owning project |
| `name` | string | Display name |
| `slug` | string | URL-safe identifier |
| `type` | enum | docker · git · static · database · docker_compose |
| `status` | string | Current container status (e.g. running, stopped) |
| `replicas` | integer | Desired replica count |
| `created_at` | ISO 8601 | Creation timestamp |
| `updated_at` | ISO 8601 | Last modification timestamp |

### Deployment

| Field | Type | Description |
|---|---|---|
| `id` | UUID | Unique identifier (UUIDv7 — sortable by time) |
| `service_id` | UUID | Service that was deployed |
| `triggered_by` | string | Initiator: api-key:name, webhook, or dashboard |
| `source_ref` | string | Git ref, image tag, or label supplied at trigger time |
| `status` | enum | pending · queued · running · success · failed · cancelled |
| `created_at` | ISO 8601 | When the deployment was created |
| `finished_at` | ISO 8601 · nullable | null while in progress |

### API key (list view)

| Field | Type | Description |
|---|---|---|
| `id` | UUID | Unique identifier |
| `org_id` | UUID | Owning organization |
| `name` | string | Human-readable label |
| `key_prefix` | string | First 8 hex chars — identifies the key without exposing it |
| `scopes` | string[] | Granted scopes |
| `last_used_at` | ISO 8601 · nullable | null if never used |
| `expires_at` | ISO 8601 · nullable | null means never expires |
| `created_at` | ISO 8601 | Creation timestamp |

## Scope matrix {#ref-scopes}

Quick reference — which scope each endpoint requires.

| Endpoint | Method | Required scope |
|---|---|---|
| `/orgs` | `GET` | `read` |
| `/orgs/:org_id` | `GET` | `read` |
| `/projects` | `GET` | `read` |
| `/projects/:id` | `GET` | `read` |
| `/projects/:id/services` | `GET` | `read` |
| `/services/:id` | `GET` | `read` |
| `/services/:id` | `PATCH` | `write` |
| `/services/:id/deploy` | `POST` | `deploy` |
| `/services/:id/deployments` | `GET` | `read` |
| `/deployments/:id` | `GET` | `read` |
| `/keys` | `GET` | `admin` |
| `/keys` | `POST` | `admin` |
| `/keys/:id` | `DELETE` | `admin` |
