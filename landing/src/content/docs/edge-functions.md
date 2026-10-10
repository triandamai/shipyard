---
title: Edge functions
description: Deploy TypeScript functions from a git repository to Shipyard's Deno runtime.
---

# Edge functions

Edge Functions let you run serverless TypeScript or JavaScript handlers alongside your other Shipyard services. Drop `.ts` files into a `functions/` directory in any Git repository — Shipyard detects them automatically on every push and deploys them to a Deno runtime scoped to your organization.

| Item | Value |
|---|---|
| Invoke URL | `/fn/<org-slug>/<fn-name>` |
| Runtime | `Deno 1.x · TypeScript native` |
| Deploy trigger | `Git push → auto-detect` |
| Custom domain | `CNAME → edge runtime` |

## Quick start {#quickstart}

Three steps from zero to a running function.

### Step 1: Write a function

Create a file inside `functions/` and export a default async function that accepts a `Request` and returns a `Response`. No framework, no boilerplate.

```ts title="functions/hello.ts"
// functions/hello.ts
export default async function handler(req: Request): Promise<Response> {
  const name = new URL(req.url).searchParams.get("name") ?? "world";
  return new Response(`Hello, ${name}!`, {
    headers: { "Content-Type": "text/plain" },
  });
}
```

### Step 2: Connect a repo with an edge function group

In the Shipyard dashboard, open your project → **Edge Functions** and create a new function group. Connect it to your Git repository. Shipyard will clone the repo and scan the `functions/` directory on every deploy.

> [!NOTE]
> You can attach one function group per repository branch. Multiple branches in the same repo can each have their own group.

### Step 3: Invoke your function

Once deployed, your function is reachable at the path-based URL immediately — no DNS changes needed.

```bash
curl https://your-shipyard.example.com/fn/my-org/hello?name=Claude
```

```text title="response"
Hello, Claude!
```

## Repo structure {#repo-structure}

Shipyard's function detector (`edge_fn_detector`) scans your repository after every deploy. It resolves functions in this order:

1. If `shipyard.json` defines an `entries` map, use those explicit paths.
2. Otherwise scan the configured `dir` (default `functions/`) for `.ts` and `.js` files.
3. A file is registered as a function only if it contains `export default`.
4. The function name is the filename without extension (e.g. `send-email.ts` → `send-email`).

```text title="example repo layout"
my-repo/
├── functions/
│   ├── hello.ts          ← auto-detected (export default)
│   ├── send-email.ts     ← auto-detected
│   └── utils.ts          ← skipped (no export default)
├── shipyard.json         ← optional overrides
└── ...
```

> [!WARNING]
> Files without `export default` are silently skipped — they will not appear in the deploy report and will not be invocable. Check your deploy report in the dashboard if a function is missing.

### Handler signature

Every function must export a default async function with this exact signature. Shipyard passes the raw `Request` object from the Deno HTTP server.

```ts
export default async function handler(req: Request): Promise<Response> { … }
```

## shipyard.json {#shipyard-json}

Place a `shipyard.json` file at the root of your repository to override defaults. All fields are optional — an empty file or no file at all is valid.

```json title="shipyard.json"
{
  "functions": {
    "dir": "functions",
    "runtime": "deno",
    "timeout_ms": 10000,
    "env": [
      "DATABASE_URL",
      "STRIPE_KEY"
    ],
    "entries": {
      "send-email": "functions/mailer.ts",
      "resize-image": "functions/images/resize.ts"
    }
  }
}
```

| Field | Type | Default | Description |
|---|---|---|---|
| `functions.dir` | string | `"functions"` | Directory to scan for function files |
| `functions.runtime` | string | `"deno"` | Only `deno` is supported currently |
| `functions.timeout_ms` | integer | `10000` | Per-request timeout in milliseconds (max 30 000) |
| `functions.env` | string[] | `[]` | Allowlist of environment variable names to inject into the runtime. Variables not on this list are not accessible inside functions. |
| `functions.entries` | object | `{}` | Explicit name → path mapping. When set, overrides directory scanning. Useful for functions nested in subdirectories. |

> [!TIP]
> Use `entries` when your functions live deeper in the repo (e.g. `src/edge/mailer.ts`) or when you want to control the public function name independently from the filename.

## Environment variables {#env-vars}

Set environment variables for your edge function group in the Shipyard dashboard under **Edge Functions → Settings → Environment**. Variables are encrypted at rest and injected into the Deno runtime on deploy.

Inside your function, access them via `Deno.env.get()`. Only variables listed in `functions.env` in `shipyard.json` (or all variables if `env` is omitted) are passed through.

```bash title="accessing env vars"
export default async function handler(req: Request): Promise<Response> {
  const dbUrl = Deno.env.get("DATABASE_URL");
  // dbUrl is available because "DATABASE_URL" is in the env allowlist
  return new Response("ok");
}
```

> [!WARNING]
> Never log or return the values of secrets in your response body. Edge function logs are visible to all members of your organization.

## Invoking functions {#invoking}

Functions can be invoked via two URL patterns: the built-in path-based URL (always available) and a custom domain (optional, see next section).

### Path-based URL

Every function is reachable under `/fn/<org-slug>/<fn-name>` on your Shipyard domain — no configuration required.

```text title="URL format"
https://<your-shipyard>/fn/<org-slug>/<fn-name>[/extra/path][?query]
```

All HTTP methods are forwarded verbatim — GET, POST, PUT, PATCH, DELETE, and OPTIONS. Query parameters, request headers (excluding hop-by-hop headers), and the request body are passed through unchanged.

```bash title="GET with query params"
curl https://your-shipyard.example.com/fn/my-org/hello?name=Claude
```

```bash title="POST with JSON body"
curl -X POST https://your-shipyard.example.com/fn/my-org/send-email \
  -H "Content-Type: application/json" \
  -d '{"to":"user@example.com","subject":"Welcome"}'
```

### Error responses

| Status | Meaning |
|---|---|
| `404` | Organization slug not found or function name not deployed |
| `502` | Edge runtime is not running for this organization |
| `504` | Function exceeded its `timeout_ms` limit |

## Custom domains {#custom-domains}

Attach a custom domain to your edge function group so functions are reachable directly at your domain — without the `/fn/<org>/` prefix.

### Adding a domain

1. Go to **Edge Functions → Domains → Add Domain** in the dashboard.
2. Enter your custom hostname (e.g. `api.acme.com`).
3. Add a CNAME record pointing to the Shipyard edge runtime for your organization.
4. TLS is provisioned automatically via Let's Encrypt once DNS propagates.

```text title="CNAME record"
# CNAME your custom domain to the Shipyard edge runtime
api.acme.com.  IN  CNAME  shipyard-edge-<org-short-id>.example.com.
```

Once the domain is active, functions are invoked at the root path — the function name becomes the first path segment.

```bash title="invoke via custom domain"
curl https://api.acme.com/send-email
```

> [!NOTE]
> Custom domain routing is handled by Traefik. Each domain gets its own dynamic Traefik configuration rule (`Host(...)`) that routes all traffic directly to the edge runtime — bypassing the `/fn/` path entirely.

### Removing a domain

Removing a domain from the dashboard immediately removes the Traefik routing rule. Existing DNS records are not affected — you must remove the CNAME yourself if you no longer need it.

## Deployment history {#deploy-history}

Every time you click **Deploy** in the dashboard, Shipyard creates an immutable deployment record for each function it detects. You can browse the full history, view the exact code from any past deployment, and restore any previous version in one click.

### Deployment statuses

| Status | Meaning |
|---|---|
| live | Currently serving traffic — the most recent successful deployment |
| pending | Deploy in progress (git clone + function scan running) |
| error | Deploy failed (check the error message in the history row) |
| rolled_back | Superseded by a newer deployment or a manual rollback |

### Viewing code from a past deployment

In the dashboard, expand any function in the Edge Functions panel and click **History**. Each row shows the commit SHA, deploy time, and status. Click **View** on any row to open a read-only code viewer showing the exact code bundle from that deployment.

### Restoring a previous deployment (rollback)

Click **Restore** on any past deployment row to make it live again. Shipyard copies the code bundle from that snapshot into a new deployment record with status `live` and marks the previous live deployment as `rolled_back`. The rollback takes effect immediately — no rebuild or git clone required.

> [!TIP]
> Rollbacks are instant because the code bundle is stored alongside each deployment record — no git history access is needed at restore time.

### Deploy report

After every deploy, the dashboard shows a summary of what happened:

| Field | Meaning |
|---|---|
| `deployed` | Functions that were detected and successfully deployed |
| `skipped` | Files that were scanned but had no `export default` |
| `failed` | Files that were detected but failed to compile or load |
| `deleted` | Functions present in the previous deploy but no longer in the repo |

## Limits & quotas {#limits}

Limits are enforced per organization. Contact support to request higher limits on paid plans.

| Limit | Free | Pro |
|---|---|---|
| Edge function groups | 1 | Unlimited |
| Functions per group | 5 | 100 |
| Custom domains per group | 0 | 10 |
| Deployment history retained | 10 per function | Unlimited |
| Request timeout (`timeout_ms` max) | 10 000 ms | 30 000 ms |
| Max request body size | 1 MB | 10 MB |
| Concurrent requests per runtime | 50 | 500 |

> [!WARNING]
> The free plan does not support custom domains. Upgrade to Pro to attach your own hostname to an edge function group.
