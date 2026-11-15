# Generic Static Artifact Deployment & Nomi Integration Guide

## 1. Overview

Shipyard now provides native, generic **Static Artifact Deployment** capabilities for external applications, CI/CD pipelines, and agentic platforms (such as Nomi).

### Key Architectural Characteristics
- **Decoupled from Docker**: Static deployments run with zero Docker container overhead, instant serving, and zero cold-start latency.
- **Immutable Previews**: Each upload produces an immutable deployment record accessible via `/dpl/:deployment_id/*path`.
- **Atomic Aliases**: Zero-downtime alias cutovers (e.g., `production`, `staging`, `main`) via `/alias/:project_id/:alias/*path`.
- **RFC 9111 Cache Headers**:
  - `HTML` & mutable aliases: `no-cache, no-store, must-revalidate`
  - Hashed immutable assets (e.g. `main-a1b2c3d4.js`): `public, max-age=31536000, immutable`
  - Non-hashed static assets (e.g. `logo.png`): `public, max-age=3600, must-revalidate`
- **Safe Extraction**: Strict zip-slip defense, uncompressed and compressed size quota protection, file count limits, single-root unwrap (`dist/` or `build/` directories auto-unwrapped), and symlink rejection.
- **Verification Gate**: Ensures `index.html` exists and is non-empty before transitioning from `VERIFYING` to `READY`.

---

## 2. Nomi Integration (One-Shot Publish)

For the Nomi team, Shipyard provides a dedicated one-shot publish endpoint that handles project creation/upsert, zip extraction, verification, alias assignment, and audit logging in a single HTTP call.

### Endpoint
```http
POST /openapi/v1/integration/orgs/:org_id/static-publish
Authorization: Bearer <ship_int_token>
Content-Type: multipart/form-data
```

### Request Fields (Multipart)
| Field | Type | Description | Required | Default |
| :--- | :--- | :--- | :--- | :--- |
| `artifact` | File | The `.zip` archive containing built HTML/CSS/JS files | **Yes** | - |
| `project_slug` | String | URL-safe project identifier (e.g. `my-nomi-app`) | **Yes** | - |
| `name` | String | Human-readable project name | No | Same as `project_slug` |
| `alias` | String | Alias to atomically point to this deployment | No | `"production"` |
| `domain` | String | Custom domain to bind to this project/alias (e.g. `app.example.com`) | No | - |
| `spa_fallback` | Boolean | Enable SPA routing (`index.html` on 404s) | No | `true` |
| `entry_file` | String | Primary entry file to verify and serve | No | `"index.html"` |
| `metadata` | JSON | Arbitrary JSON metadata (agent ID, prompt, commit) | No | `{"source": "nomi"}` |

### Response
```json
{
  "success": true,
  "data": {
    "project_id": "0193309a-4f5b-7b00-8cb5-123456789abc",
    "project_slug": "my-nomi-app",
    "deployment_id": "0193309a-5001-7c00-9abc-987654321def",
    "status": "READY",
    "preview_url": "/dpl/0193309a-5001-7c00-9abc-987654321def",
    "alias": "production",
    "alias_url": "/alias/0193309a-4f5b-7b00-8cb5-123456789abc/production",
    "domain": "app.example.com",
    "domain_url": "https://app.example.com",
    "archive_size_bytes": 1048576,
    "extracted_size_bytes": 3145728,
    "file_count": 28,
    "checksum_sha256": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
  }
}
```

---

## 3. Generic Static Deployments API (For CI/CD & External Apps)

### Authentication
Send standard headers with an org API key (`ship_...`) or dashboard user JWT:
```http
Authorization: Bearer <token>
```

### Endpoints Reference

#### Projects
- `POST /api/static/orgs/:org_id/projects`: Create project
  ```json
  {
    "name": "Documentation Site",
    "slug": "docs",
    "framework": "vite",
    "spa_fallback": true
  }
  ```
- `GET /api/static/orgs/:org_id/projects`: List projects
- `GET /api/static/projects/:project_id`: Get project
- `PUT /api/static/projects/:project_id`: Update project settings
- `DELETE /api/static/projects/:project_id`: Delete project and all associated deployments

#### Deployments
- `POST /api/static/projects/:project_id/deployments`: Upload and deploy zip archive
  - Multipart fields: `artifact` (file), `entry_file` (optional), `is_private` (optional), `alias` (optional)
- `GET /api/static/projects/:project_id/deployments?limit=20&offset=0`: List deployment history
- `GET /api/static/deployments/:deployment_id`: Get deployment status and metadata
- `DELETE /api/static/deployments/:deployment_id`: Delete deployment
- `GET /api/static/deployments/:deployment_id/events`: List chronological deployment lifecycle events

#### Aliases
- `GET /api/static/projects/:project_id/aliases`: List project aliases
- `POST /api/static/projects/:project_id/aliases`: Atomically assign or reassign an alias
  ```json
  {
    "name": "staging",
    "deployment_id": "<uuid>"
  }
  ```
- `DELETE /api/static/projects/:project_id/aliases/:alias_name`: Delete an alias

#### Custom Domains
- `GET /api/static/projects/:project_id/domains`: List attached custom domains
- `POST /api/static/projects/:project_id/domains`: Attach a custom domain
  ```json
  {
    "hostname": "docs.example.com",
    "alias": "production",
    "tls_enabled": true,
    "cert_provider": "letsencrypt"
  }
  ```
- `DELETE /api/static/projects/:project_id/domains/:domain_id`: Detach a custom domain and clean up DNS/TLS routing

---

## 4. Serving & Security

- **Immutable Preview URL**: `/dpl/:deployment_id/` and `/dpl/:deployment_id/*path`
- **Alias URL**: `/alias/:project_id/:alias/` and `/alias/:project_id/:alias/*path`
- **Private Previews**: When `is_private` is true, a cryptographic preview token is generated. Requests must supply it via `?token=<preview_token>` query parameter or `x-preview-token` header.
- **Deployment Isolation**: Requests attempting `../` or unauthorized path traversal are rejected immediately with 400 Bad Request.
