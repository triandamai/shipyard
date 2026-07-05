<script lang="ts">
	import { onMount } from 'svelte';
	import { Anchor, ChevronRight, Menu, X, Copy, Check } from '@lucide/svelte';

	let activeSection = $state('overview');
	let sidebarOpen   = $state(false);
	let copied        = $state<string | null>(null);

	const nav = [
		{
			group: 'Getting Started',
			items: [
				{ id: 'overview',        label: 'Overview' },
				{ id: 'authentication',  label: 'Authentication' },
				{ id: 'scopes',          label: 'Scopes' },
				{ id: 'base-url',        label: 'Base URL & Versioning' },
				{ id: 'errors',          label: 'Errors' },
				{ id: 'pagination',      label: 'Pagination' },
			],
		},
		{
			group: 'Endpoints',
			items: [
				{ id: 'ep-orgs',        label: 'Organizations' },
				{ id: 'ep-projects',    label: 'Projects' },
				{ id: 'ep-services',    label: 'Services' },
				{ id: 'ep-deployments', label: 'Deployments' },
				{ id: 'ep-keys',        label: 'API Keys' },
			],
		},
		{
			group: 'Reference',
			items: [
				{ id: 'ref-objects',    label: 'Object Schemas' },
				{ id: 'ref-scopes',     label: 'Scope Matrix' },
			],
		},
	];

	onMount(() => {
		const observer = new IntersectionObserver(
			(entries) => {
				for (const e of entries) {
					if (e.isIntersecting) activeSection = e.target.id;
				}
			},
			{ rootMargin: '-20% 0px -70% 0px' },
		);
		document.querySelectorAll('section[id]').forEach((el) => observer.observe(el));
		return () => observer.disconnect();
	});

	function scrollTo(id: string) {
		document.getElementById(id)?.scrollIntoView({ behavior: 'smooth', block: 'start' });
		activeSection = id;
		sidebarOpen = false;
	}

	function copyCode(id: string, text: string) {
		navigator.clipboard.writeText(text).then(() => {
			copied = id;
			setTimeout(() => { if (copied === id) copied = null; }, 2000);
		});
	}

	// ── Code snippets (defined here to avoid Svelte parser issues with ${...}) ──

	const snip = {
		auth1: 'Authorization: Bearer ship_your_key_here',
		auth2: 'X-API-Key: ship_your_key_here',
		curlAuth: `curl https://ship.example.com/openapi/v1/projects \\\n  -H "Authorization: Bearer ship_your_key_here"`,
		curlInfo: `curl https://ship.example.com/openapi/v1/ \\\n  -H "Authorization: Bearer ship_…"`,
		curlOrgs: `curl https://ship.example.com/openapi/v1/orgs \\\n  -H "Authorization: Bearer ship_…"`,
		curlProjects: `curl "https://ship.example.com/openapi/v1/projects?page=1&per_page=20" \\\n  -H "Authorization: Bearer ship_…"`,
		curlSvcs: `curl "https://ship.example.com/openapi/v1/projects/PROJECT_ID/services" \\\n  -H "Authorization: Bearer ship_…"`,
		curlPatch: `curl -X PATCH https://ship.example.com/openapi/v1/services/SERVICE_ID \\\n  -H "Authorization: Bearer ship_…" \\\n  -H "Content-Type: application/json" \\\n  -d '{"replicas": 3}'`,
		curlDeploy: `curl -X POST https://ship.example.com/openapi/v1/services/SERVICE_ID/deploy \\\n  -H "Authorization: Bearer ship_…" \\\n  -H "Content-Type: application/json" \\\n  -d '{"source_ref": "v2.4.0"}'`,
		ghActions: [
			'- name: Deploy to Shipyard',
			'  run: |',
			'    curl -fsS -X POST \\',
			'      -H "Authorization: Bearer ${{ secrets.SHIPYARD_API_KEY }}" \\',
			'      -H "Content-Type: application/json" \\',
			'      -d \'{"source_ref":"${{ github.sha }}"}\' \\',
			'      "https://ship.example.com/openapi/v1/services/${{ vars.SERVICE_ID }}/deploy"',
		].join('\n'),
		pollScript: [
			'# Poll until done (bash)',
			'DEPLOYMENT_ID="019…"',
			'while true; do',
			'  STATUS=$(curl -fsS \\',
			'    -H "Authorization: Bearer ship_…" \\',
			'    "https://ship.example.com/openapi/v1/deployments/$DEPLOYMENT_ID" \\',
			'    | jq -r \'.data.status\')',
			'  echo "Status: $STATUS"',
			'  [[ "$STATUS" == "success" || "$STATUS" == "failed" ]] && break',
			'  sleep 5',
			'done',
		].join('\n'),
		curlCreateKey: `curl -X POST https://ship.example.com/openapi/v1/keys \\\n  -H "Authorization: Bearer ship_…" \\\n  -H "Content-Type: application/json" \\\n  -d '{\n    "name":   "ci-deploy",\n    "scopes": ["deploy"],\n    "expires_at": "2025-01-01T00:00:00Z"\n  }'`,
		curlRevoke: `curl -X DELETE https://ship.example.com/openapi/v1/keys/KEY_ID \\\n  -H "Authorization: Bearer ship_…"`,
	};
</script>

<svelte:head>
	<title>API Reference — Shipyard</title>
	<meta name="description" content="Shipyard Open API v1 reference — authentication, endpoints, request/response schemas, and code examples." />
</svelte:head>

<!-- ─── Top nav ──────────────────────────────────────────────────────────────── -->
<header class="topbar">
	<nav class="topbar-inner">
		<a href="/" class="brand">
			<Anchor size={18} strokeWidth={2.5} />
			<span>Shipyard</span>
		</a>
		<div class="topbar-links">
			<a href="/" class="topbar-link">Home</a>
			<a href="/docs" class="topbar-link">Docs</a>
			<a href="/docs/api" class="topbar-link active">API Reference</a>
			<a href="https://github.com/triandamai/shipyard" target="_blank" rel="noopener noreferrer" class="topbar-link">GitHub</a>
		</div>
		<button class="mobile-menu-btn" onclick={() => sidebarOpen = !sidebarOpen} aria-label="Toggle menu">
			{#if sidebarOpen}<X size={18} />{:else}<Menu size={18} />{/if}
		</button>
	</nav>
</header>

<!-- ─── Layout ───────────────────────────────────────────────────────────────── -->
<div class="docs-layout">

	<!-- Sidebar -->
	<aside class="sidebar" class:open={sidebarOpen}>
		<nav class="sidebar-nav">
			{#each nav as group}
				<div class="nav-group">
					<div class="nav-group-label">{group.group}</div>
					{#each group.items as item}
						<button
							class="nav-item"
							class:active={activeSection === item.id}
							onclick={() => scrollTo(item.id)}
						>
							<ChevronRight size={12} />
							{item.label}
						</button>
					{/each}
				</div>
			{/each}
		</nav>
	</aside>

	<!-- Content -->
	<main class="content">

		<!-- ── Overview ───────────────────────────────────────────── -->
		<section id="overview">
			<div class="page-eyebrow">Open API · v1</div>
			<h1>API Reference</h1>
			<p>
				The Shipyard Open API lets you integrate with Shipyard from CI/CD pipelines,
				automation scripts, and external tools. All resources are scoped to your organization
				and authenticated with API keys.
			</p>

			<div class="quick-cards">
				<div class="quick-card">
					<div class="qc-label">Base URL</div>
					<code>https://your-shipyard/openapi/v1</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Auth header</div>
					<code>Authorization: Bearer ship_…</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Format</div>
					<code>Content-Type: application/json</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Scopes</div>
					<code>read · deploy · write · admin</code>
				</div>
			</div>
		</section>

		<!-- ── Authentication ─────────────────────────────────────── -->
		<section id="authentication">
			<h2>Authentication</h2>
			<p>
				All requests must include a valid API key. Keys are created in
				<strong>Settings → API Keys</strong> in the Shipyard dashboard.
				Every key starts with the prefix <code>ship_</code>.
			</p>

			<h3>Sending the key</h3>
			<p>Two headers are accepted — use whichever fits your client:</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">Option 1 — Authorization header (recommended)</span>
					<button class="copy-btn" onclick={() => copyCode('auth1', 'Authorization: Bearer ship_your_key_here')}>
						{#if copied === 'auth1'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>Authorization: Bearer ship_your_key_here</pre>
			</div>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">Option 2 — X-API-Key header</span>
					<button class="copy-btn" onclick={() => copyCode('auth2', 'X-API-Key: ship_your_key_here')}>
						{#if copied === 'auth2'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>X-API-Key: ship_your_key_here</pre>
			</div>

			<h3>Full example</h3>
			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash</span>
					<button class="copy-btn" onclick={() => copyCode('curl-auth', snip.curlAuth)}>
						{#if copied === 'curl-auth'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.curlAuth}</pre>
			</div>

			<div class="callout callout-warn">
				Keys are shown only once at creation time. Store them securely — if lost, revoke and create a new one.
			</div>
		</section>

		<!-- ── Scopes ──────────────────────────────────────────────── -->
		<section id="scopes">
			<h2>Scopes</h2>
			<p>
				Each API key is issued with one or more scopes that control what it can do.
				Scopes are <strong>not</strong> additive — a key with <code>deploy</code> scope
				cannot read data unless it also has <code>read</code>.
			</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Scope</th><th>What it allows</th></tr>
					</thead>
					<tbody>
						<tr>
							<td><span class="scope-badge scope-read">read</span></td>
							<td>Read organizations, projects, services, and deployment history</td>
						</tr>
						<tr>
							<td><span class="scope-badge scope-deploy">deploy</span></td>
							<td>Trigger deployments on services</td>
						</tr>
						<tr>
							<td><span class="scope-badge scope-write">write</span></td>
							<td>Update service configuration (name, replicas)</td>
						</tr>
						<tr>
							<td><span class="scope-badge scope-admin">admin</span></td>
							<td>Create and revoke API keys — elevated access</td>
						</tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-info">
				For CI/CD pipelines that only trigger deployments, issue a key with the
				<code>deploy</code> scope only — no read or write access needed.
			</div>
		</section>

		<!-- ── Base URL ────────────────────────────────────────────── -->
		<section id="base-url">
			<h2>Base URL & Versioning</h2>
			<p>
				All endpoints are prefixed with <code>/openapi/v1</code>. Replace
				<code>ship.example.com</code> with your Shipyard domain in every request.
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">base URL</span>
				</div>
				<pre>https://ship.example.com/openapi/v1</pre>
			</div>

			<p>
				A <code>GET /openapi/v1/</code> request returns the API name and version — useful
				as a health check.
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash</span>
					<button class="copy-btn" onclick={() => copyCode('curl-info', snip.curlInfo)}>
						{#if copied === 'curl-info'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.curlInfo}</pre>
			</div>

			<div class="code-block">
				<div class="code-header"><span class="code-label">200 OK</span></div>
				<pre>{`{
  "name": "Shipyard Open API",
  "version": "v1",
  "docs": "https://shipyard.trian.space/docs/api"
}`}</pre>
			</div>
		</section>

		<!-- ── Errors ──────────────────────────────────────────────── -->
		<section id="errors">
			<h2>Errors</h2>
			<p>All error responses share a common JSON body:</p>

			<div class="code-block">
				<div class="code-header"><span class="code-label">error envelope</span></div>
				<pre>{`{
  "error": {
    "code":    "NOT_FOUND",
    "message": "Service 'abc...' not found"
  }
}`}</pre>
			</div>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>HTTP status</th><th>code</th><th>When it occurs</th></tr>
					</thead>
					<tbody>
						<tr><td><span class="status-badge s400">400</span></td><td><code>BAD_REQUEST</code></td><td>Invalid or missing request body / query parameter</td></tr>
						<tr><td><span class="status-badge s401">401</span></td><td><code>UNAUTHORIZED</code></td><td>Missing, malformed, or expired API key</td></tr>
						<tr><td><span class="status-badge s403">403</span></td><td><code>FORBIDDEN</code></td><td>Key lacks the required scope, or resource belongs to another org</td></tr>
						<tr><td><span class="status-badge s404">404</span></td><td><code>NOT_FOUND</code></td><td>The requested resource does not exist</td></tr>
						<tr><td><span class="status-badge s500">500</span></td><td><code>INTERNAL_ERROR</code></td><td>Unexpected server error</td></tr>
					</tbody>
				</table>
			</div>
		</section>

		<!-- ── Pagination ──────────────────────────────────────────── -->
		<section id="pagination">
			<h2>Pagination</h2>
			<p>
				List endpoints accept <code>page</code> and <code>per_page</code> query parameters.
				The response wraps the array in a <code>data</code> + <code>meta</code> envelope.
			</p>

			<div class="table-wrap">
				<table>
					<thead><tr><th>Parameter</th><th>Default</th><th>Max</th><th>Description</th></tr></thead>
					<tbody>
						<tr><td><code>page</code></td><td><code>1</code></td><td>—</td><td>1-based page index</td></tr>
						<tr><td><code>per_page</code></td><td><code>20</code></td><td><code>100</code></td><td>Items per page</td></tr>
					</tbody>
				</table>
			</div>

			<div class="code-block">
				<div class="code-header"><span class="code-label">paginated response envelope</span></div>
				<pre>{`{
  "data": [ … ],
  "meta": {
    "total":    142,
    "page":     1,
    "per_page": 20
  },
  "request_id": "019…"
}`}</pre>
			</div>

			<div class="callout callout-info">
				<code>request_id</code> is a UUIDv7 generated per request. Include it when reporting issues.
			</div>
		</section>

		<!-- ════════════════════════════════════════════════════════
		     ENDPOINTS
		     ════════════════════════════════════════════════════════ -->

		<!-- ── Organizations ──────────────────────────────────────── -->
		<section id="ep-orgs">
			<h2>Organizations</h2>
			<p>
				An API key is tied to exactly one organization. These endpoints let you inspect it.
			</p>

			<!-- GET /orgs -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/orgs</code>
					<span class="scope-req"><span class="scope-badge scope-read">read</span></span>
				</div>
				<p class="ep-desc">Returns the organization this API key belongs to.</p>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">bash</span>
						<button class="copy-btn" onclick={() => copyCode('curl-orgs', snip.curlOrgs)}>
							{#if copied === 'curl-orgs'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.curlOrgs}</pre>
				</div>

				<div class="code-block">
					<div class="code-header"><span class="code-label">200 OK</span></div>
					<pre>{`{
  "data": {
    "id":         "018f…",
    "name":       "Acme Corp",
    "slug":       "acme",
    "created_at": "2024-01-15T09:00:00Z"
  },
  "request_id": "019…"
}`}</pre>
				</div>
			</div>

			<!-- GET /orgs/:org_id -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/orgs/<span class="param">{'{org_id}'}</span></code>
					<span class="scope-req"><span class="scope-badge scope-read">read</span></span>
				</div>
				<p class="ep-desc">
					Convenience alias — resolves only if <code>org_id</code> matches the key's organization.
					Returns <code>403</code> otherwise.
				</p>
			</div>
		</section>

		<!-- ── Projects ───────────────────────────────────────────── -->
		<section id="ep-projects">
			<h2>Projects</h2>

			<!-- GET /projects -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/projects</code>
					<span class="scope-req"><span class="scope-badge scope-read">read</span></span>
				</div>
				<p class="ep-desc">List all projects in the organization, newest first.</p>

				<div class="params-table">
					<div class="params-label">Query parameters</div>
					<div class="table-wrap">
						<table>
							<thead><tr><th>Name</th><th>Type</th><th>Description</th></tr></thead>
							<tbody>
								<tr><td><code>page</code></td><td>integer</td><td>Page index (default 1)</td></tr>
								<tr><td><code>per_page</code></td><td>integer</td><td>Items per page, max 100 (default 20)</td></tr>
							</tbody>
						</table>
					</div>
				</div>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">bash</span>
						<button class="copy-btn" onclick={() => copyCode('curl-projects', snip.curlProjects)}>
							{#if copied === 'curl-projects'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.curlProjects}</pre>
				</div>

				<div class="code-block">
					<div class="code-header"><span class="code-label">200 OK</span></div>
					<pre>{`{
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
}`}</pre>
				</div>
			</div>

			<!-- GET /projects/:id -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/projects/<span class="param">{'{project_id}'}</span></code>
					<span class="scope-req"><span class="scope-badge scope-read">read</span></span>
				</div>
				<p class="ep-desc">Get a single project by ID. Returns <code>404</code> if it doesn't exist or belongs to another org.</p>
			</div>
		</section>

		<!-- ── Services ───────────────────────────────────────────── -->
		<section id="ep-services">
			<h2>Services</h2>

			<!-- GET /projects/:id/services -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/projects/<span class="param">{'{project_id}'}</span>/services</code>
					<span class="scope-req"><span class="scope-badge scope-read">read</span></span>
				</div>
				<p class="ep-desc">List all services in a project, newest first.</p>

				<div class="params-table">
					<div class="params-label">Query parameters</div>
					<div class="table-wrap">
						<table>
							<thead><tr><th>Name</th><th>Type</th><th>Description</th></tr></thead>
							<tbody>
								<tr><td><code>page</code></td><td>integer</td><td>Page index (default 1)</td></tr>
								<tr><td><code>per_page</code></td><td>integer</td><td>Items per page, max 100 (default 20)</td></tr>
							</tbody>
						</table>
					</div>
				</div>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">bash</span>
						<button class="copy-btn" onclick={() => copyCode('curl-svcs', snip.curlSvcs)}>
							{#if copied === 'curl-svcs'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.curlSvcs}</pre>
				</div>

				<div class="code-block">
					<div class="code-header"><span class="code-label">200 OK</span></div>
					<pre>{`{
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
}`}</pre>
				</div>
			</div>

			<!-- GET /services/:id -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/services/<span class="param">{'{service_id}'}</span></code>
					<span class="scope-req"><span class="scope-badge scope-read">read</span></span>
				</div>
				<p class="ep-desc">Get a single service by ID. Returns <code>404</code> if it doesn't exist or belongs to another org.</p>
			</div>

			<!-- PATCH /services/:id -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method patch">PATCH</span>
					<code class="path">/openapi/v1/services/<span class="param">{'{service_id}'}</span></code>
					<span class="scope-req"><span class="scope-badge scope-write">write</span></span>
				</div>
				<p class="ep-desc">Update a service's name and/or replica count. At least one field must be provided.</p>

				<div class="params-table">
					<div class="params-label">Request body</div>
					<div class="table-wrap">
						<table>
							<thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Description</th></tr></thead>
							<tbody>
								<tr><td><code>name</code></td><td>string</td><td>no</td><td>New display name for the service</td></tr>
								<tr><td><code>replicas</code></td><td>integer ≥ 0</td><td>no</td><td>Desired replica count; 0 stops the service</td></tr>
							</tbody>
						</table>
					</div>
				</div>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">bash</span>
						<button class="copy-btn" onclick={() => copyCode('curl-patch', snip.curlPatch)}>
							{#if copied === 'curl-patch'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.curlPatch}</pre>
				</div>

				<div class="code-block">
					<div class="code-header"><span class="code-label">200 OK — updated service object</span></div>
					<pre>{`{
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
}`}</pre>
				</div>
			</div>
		</section>

		<!-- ── Deployments ─────────────────────────────────────────── -->
		<section id="ep-deployments">
			<h2>Deployments</h2>

			<!-- POST /services/:id/deploy -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method post">POST</span>
					<code class="path">/openapi/v1/services/<span class="param">{'{service_id}'}</span>/deploy</code>
					<span class="scope-req"><span class="scope-badge scope-deploy">deploy</span></span>
				</div>
				<p class="ep-desc">
					Trigger a new deployment for a service. The deployment runs asynchronously —
					poll <code>GET /deployments/:id</code> to track status.
					If the platform deployment parallelism limit is reached, the deployment is queued and returns <code>202 Accepted</code> with status <code>queued</code>.
				</p>

				<div class="params-table">
					<div class="params-label">Request body (optional)</div>
					<div class="table-wrap">
						<table>
							<thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Description</th></tr></thead>
							<tbody>
								<tr><td><code>source_ref</code></td><td>string</td><td>no</td><td>Git ref, image tag, or label to record against the deployment (e.g. <code>main</code>, <code>v1.2.3</code>)</td></tr>
							</tbody>
						</table>
					</div>
				</div>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">bash</span>
						<button class="copy-btn" onclick={() => copyCode('curl-deploy', snip.curlDeploy)}>
							{#if copied === 'curl-deploy'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.curlDeploy}</pre>
				</div>

				<div class="code-block">
					<div class="code-header"><span class="code-label">202 Accepted</span></div>
					<pre>{`{
  "data": {
    "deployment_id": "019…",
    "status":        "running"
  },
  "request_id": "019…"
}`}</pre>
				</div>

				<div class="callout callout-tip">
					GitHub Actions example — deploy on every push to <code>main</code>:
				</div>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">yaml — .github/workflows/deploy.yml</span>
						<button class="copy-btn" onclick={() => copyCode('gh-actions', snip.ghActions)}>
							{#if copied === 'gh-actions'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.ghActions}</pre>
				</div>
			</div>

			<!-- GET /services/:id/deployments -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/services/<span class="param">{'{service_id}'}</span>/deployments</code>
					<span class="scope-req"><span class="scope-badge scope-read">read</span></span>
				</div>
				<p class="ep-desc">List all deployments for a service, newest first. Paginated.</p>

				<div class="code-block">
					<div class="code-header"><span class="code-label">200 OK</span></div>
					<pre>{`{
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
}`}</pre>
				</div>
			</div>

			<!-- GET /deployments/:id -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/deployments/<span class="param">{'{deployment_id}'}</span></code>
					<span class="scope-req"><span class="scope-badge scope-read">read</span></span>
				</div>
				<p class="ep-desc">
					Get a single deployment by ID. Use this to poll for completion after triggering a deploy.
				</p>

				<div class="table-wrap">
					<table>
						<thead><tr><th>Status value</th><th>Meaning</th></tr></thead>
						<tbody>
							<tr><td><code>pending</code></td><td>Created but not yet picked up</td></tr>
							<tr><td><code>queued</code></td><td>Waiting for parallelism slot</td></tr>
							<tr><td><code>running</code></td><td>In progress</td></tr>
							<tr><td><code>success</code></td><td>Completed successfully</td></tr>
							<tr><td><code>failed</code></td><td>Completed with an error</td></tr>
							<tr><td><code>cancelled</code></td><td>Cancelled before completion</td></tr>
						</tbody>
					</table>
				</div>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">bash — poll for completion</span>
						<button class="copy-btn" onclick={() => copyCode('poll', snip.pollScript)}>
							{#if copied === 'poll'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.pollScript}</pre>
				</div>
			</div>
		</section>

		<!-- ── API Keys ────────────────────────────────────────────── -->
		<section id="ep-keys">
			<h2>API Keys</h2>
			<p>Manage API keys programmatically. All key endpoints require the <span class="scope-badge scope-admin">admin</span> scope.</p>

			<!-- GET /keys -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method get">GET</span>
					<code class="path">/openapi/v1/keys</code>
					<span class="scope-req"><span class="scope-badge scope-admin">admin</span></span>
				</div>
				<p class="ep-desc">List all API keys for the organization. Paginated. The secret key value is never returned.</p>

				<div class="code-block">
					<div class="code-header"><span class="code-label">200 OK</span></div>
					<pre>{`{
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
}`}</pre>
				</div>
			</div>

			<!-- POST /keys -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method post">POST</span>
					<code class="path">/openapi/v1/keys</code>
					<span class="scope-req"><span class="scope-badge scope-admin">admin</span></span>
				</div>
				<p class="ep-desc">Create a new API key. The full key is returned only in this response — store it immediately.</p>

				<div class="params-table">
					<div class="params-label">Request body</div>
					<div class="table-wrap">
						<table>
							<thead><tr><th>Field</th><th>Type</th><th>Required</th><th>Description</th></tr></thead>
							<tbody>
								<tr><td><code>name</code></td><td>string</td><td>yes</td><td>Human-readable label for the key</td></tr>
								<tr><td><code>scopes</code></td><td>string[]</td><td>yes</td><td>One or more of: <code>read</code>, <code>deploy</code>, <code>write</code>, <code>admin</code></td></tr>
								<tr><td><code>expires_at</code></td><td>ISO 8601 timestamp</td><td>no</td><td>Optional expiry; <code>null</code> means never expires</td></tr>
							</tbody>
						</table>
					</div>
				</div>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">bash</span>
						<button class="copy-btn" onclick={() => copyCode('curl-create-key', snip.curlCreateKey)}>
							{#if copied === 'curl-create-key'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.curlCreateKey}</pre>
				</div>

				<div class="code-block">
					<div class="code-header"><span class="code-label">201 Created — key shown once</span></div>
					<pre>{`{
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
}`}</pre>
				</div>
			</div>

			<!-- DELETE /keys/:id -->
			<div class="endpoint-block">
				<div class="endpoint-line">
					<span class="method delete">DELETE</span>
					<code class="path">/openapi/v1/keys/<span class="param">{'{key_id}'}</span></code>
					<span class="scope-req"><span class="scope-badge scope-admin">admin</span></span>
				</div>
				<p class="ep-desc">
					Revoke an API key immediately. Returns <code>204 No Content</code> on success.
					You cannot revoke the key that is currently authenticating the request.
				</p>
				<div class="code-block">
					<div class="code-header">
						<span class="code-label">bash</span>
						<button class="copy-btn" onclick={() => copyCode('curl-revoke', snip.curlRevoke)}>
							{#if copied === 'curl-revoke'}<Check size={12} />{:else}<Copy size={12} />{/if}
						</button>
					</div>
					<pre>{snip.curlRevoke}</pre>
				</div>
			</div>
		</section>

		<!-- ════════════════════════════════════════════════════════
		     REFERENCE
		     ════════════════════════════════════════════════════════ -->

		<!-- ── Object Schemas ─────────────────────────────────────── -->
		<section id="ref-objects">
			<h2>Object Schemas</h2>

			<h3>Organization</h3>
			<div class="schema-block">
				<div class="schema-row"><span class="field-name">id</span><span class="field-type">UUID</span><span class="field-desc">Unique identifier</span></div>
				<div class="schema-row"><span class="field-name">name</span><span class="field-type">string</span><span class="field-desc">Display name</span></div>
				<div class="schema-row"><span class="field-name">slug</span><span class="field-type">string</span><span class="field-desc">URL-safe identifier</span></div>
				<div class="schema-row"><span class="field-name">created_at</span><span class="field-type">ISO 8601</span><span class="field-desc">Creation timestamp</span></div>
			</div>

			<h3>Project</h3>
			<div class="schema-block">
				<div class="schema-row"><span class="field-name">id</span><span class="field-type">UUID</span><span class="field-desc">Unique identifier</span></div>
				<div class="schema-row"><span class="field-name">org_id</span><span class="field-type">UUID</span><span class="field-desc">Owning organization</span></div>
				<div class="schema-row"><span class="field-name">name</span><span class="field-type">string</span><span class="field-desc">Display name</span></div>
				<div class="schema-row"><span class="field-name">slug</span><span class="field-type">string</span><span class="field-desc">URL-safe identifier</span></div>
				<div class="schema-row"><span class="field-name">created_at</span><span class="field-type">ISO 8601</span><span class="field-desc">Creation timestamp</span></div>
				<div class="schema-row"><span class="field-name">updated_at</span><span class="field-type">ISO 8601</span><span class="field-desc">Last modification timestamp</span></div>
			</div>

			<h3>Service</h3>
			<div class="schema-block">
				<div class="schema-row"><span class="field-name">id</span><span class="field-type">UUID</span><span class="field-desc">Unique identifier</span></div>
				<div class="schema-row"><span class="field-name">project_id</span><span class="field-type">UUID</span><span class="field-desc">Owning project</span></div>
				<div class="schema-row"><span class="field-name">name</span><span class="field-type">string</span><span class="field-desc">Display name</span></div>
				<div class="schema-row"><span class="field-name">slug</span><span class="field-type">string</span><span class="field-desc">URL-safe identifier</span></div>
				<div class="schema-row"><span class="field-name">type</span><span class="field-type">enum</span><span class="field-desc"><code>docker</code> · <code>git</code> · <code>static</code> · <code>database</code> · <code>docker_compose</code></span></div>
				<div class="schema-row"><span class="field-name">status</span><span class="field-type">string</span><span class="field-desc">Current container status (e.g. <code>running</code>, <code>stopped</code>)</span></div>
				<div class="schema-row"><span class="field-name">replicas</span><span class="field-type">integer</span><span class="field-desc">Desired replica count</span></div>
				<div class="schema-row"><span class="field-name">created_at</span><span class="field-type">ISO 8601</span><span class="field-desc">Creation timestamp</span></div>
				<div class="schema-row"><span class="field-name">updated_at</span><span class="field-type">ISO 8601</span><span class="field-desc">Last modification timestamp</span></div>
			</div>

			<h3>Deployment</h3>
			<div class="schema-block">
				<div class="schema-row"><span class="field-name">id</span><span class="field-type">UUID</span><span class="field-desc">Unique identifier (UUIDv7 — sortable by time)</span></div>
				<div class="schema-row"><span class="field-name">service_id</span><span class="field-type">UUID</span><span class="field-desc">Service that was deployed</span></div>
				<div class="schema-row"><span class="field-name">triggered_by</span><span class="field-type">string</span><span class="field-desc">Initiator: <code>api-key:name</code>, <code>webhook</code>, or <code>dashboard</code></span></div>
				<div class="schema-row"><span class="field-name">source_ref</span><span class="field-type">string</span><span class="field-desc">Git ref, image tag, or label supplied at trigger time</span></div>
				<div class="schema-row"><span class="field-name">status</span><span class="field-type">enum</span><span class="field-desc"><code>pending</code> · <code>queued</code> · <code>running</code> · <code>success</code> · <code>failed</code> · <code>cancelled</code></span></div>
				<div class="schema-row"><span class="field-name">created_at</span><span class="field-type">ISO 8601</span><span class="field-desc">When the deployment was created</span></div>
				<div class="schema-row"><span class="field-name">finished_at</span><span class="field-type">ISO 8601 · nullable</span><span class="field-desc"><code>null</code> while in progress</span></div>
			</div>

			<h3>API Key (list view)</h3>
			<div class="schema-block">
				<div class="schema-row"><span class="field-name">id</span><span class="field-type">UUID</span><span class="field-desc">Unique identifier</span></div>
				<div class="schema-row"><span class="field-name">org_id</span><span class="field-type">UUID</span><span class="field-desc">Owning organization</span></div>
				<div class="schema-row"><span class="field-name">name</span><span class="field-type">string</span><span class="field-desc">Human-readable label</span></div>
				<div class="schema-row"><span class="field-name">key_prefix</span><span class="field-type">string</span><span class="field-desc">First 8 hex chars — identifies the key without exposing it</span></div>
				<div class="schema-row"><span class="field-name">scopes</span><span class="field-type">string[]</span><span class="field-desc">Granted scopes</span></div>
				<div class="schema-row"><span class="field-name">last_used_at</span><span class="field-type">ISO 8601 · nullable</span><span class="field-desc"><code>null</code> if never used</span></div>
				<div class="schema-row"><span class="field-name">expires_at</span><span class="field-type">ISO 8601 · nullable</span><span class="field-desc"><code>null</code> means never expires</span></div>
				<div class="schema-row"><span class="field-name">created_at</span><span class="field-type">ISO 8601</span><span class="field-desc">Creation timestamp</span></div>
			</div>
		</section>

		<!-- ── Scope Matrix ────────────────────────────────────────── -->
		<section id="ref-scopes">
			<h2>Scope Matrix</h2>
			<p>Quick reference — which scope each endpoint requires.</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr>
							<th>Endpoint</th>
							<th>Method</th>
							<th>Required scope</th>
						</tr>
					</thead>
					<tbody>
						<tr><td><code>/orgs</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-read">read</span></td></tr>
						<tr><td><code>/orgs/:org_id</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-read">read</span></td></tr>
						<tr><td><code>/projects</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-read">read</span></td></tr>
						<tr><td><code>/projects/:id</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-read">read</span></td></tr>
						<tr><td><code>/projects/:id/services</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-read">read</span></td></tr>
						<tr><td><code>/services/:id</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-read">read</span></td></tr>
						<tr><td><code>/services/:id</code></td><td><span class="method-sm patch">PATCH</span></td><td><span class="scope-badge scope-write">write</span></td></tr>
						<tr><td><code>/services/:id/deploy</code></td><td><span class="method-sm post">POST</span></td><td><span class="scope-badge scope-deploy">deploy</span></td></tr>
						<tr><td><code>/services/:id/deployments</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-read">read</span></td></tr>
						<tr><td><code>/deployments/:id</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-read">read</span></td></tr>
						<tr><td><code>/keys</code></td><td><span class="method-sm get">GET</span></td><td><span class="scope-badge scope-admin">admin</span></td></tr>
						<tr><td><code>/keys</code></td><td><span class="method-sm post">POST</span></td><td><span class="scope-badge scope-admin">admin</span></td></tr>
						<tr><td><code>/keys/:id</code></td><td><span class="method-sm delete">DELETE</span></td><td><span class="scope-badge scope-admin">admin</span></td></tr>
					</tbody>
				</table>
			</div>
		</section>

	</main>
</div>

<style>
	:global(*, *::before, *::after) { box-sizing: border-box; margin: 0; padding: 0; }
	:global(html) { scroll-behavior: smooth; }
	:global(body) {
		font-family: 'Inter', system-ui, -apple-system, sans-serif;
		background: #0a0a0f;
		color: #cbd5e1;
		line-height: 1.7;
		-webkit-font-smoothing: antialiased;
	}

	/* ── Topbar ──────────────────────────────────────────────────── */
	.topbar {
		position: sticky; top: 0; z-index: 100;
		background: rgba(10,10,15,0.85);
		backdrop-filter: blur(16px);
		border-bottom: 1px solid rgba(255,255,255,0.07);
	}
	.topbar-inner {
		max-width: 1280px; margin: 0 auto; padding: 0 24px;
		height: 56px; display: flex; align-items: center; gap: 24px;
	}
	.brand {
		display: flex; align-items: center; gap: 8px;
		font-size: 15px; font-weight: 700; color: #fff;
		text-decoration: none; flex-shrink: 0;
	}
	.brand :global(svg) { color: #3b82f6; }
	.topbar-links { display: flex; align-items: center; gap: 4px; margin-left: auto; }
	.topbar-link {
		padding: 5px 12px; font-size: 13px; font-weight: 500;
		color: rgba(255,255,255,0.5); text-decoration: none;
		border-radius: 6px; transition: color 0.15s, background 0.15s;
	}
	.topbar-link:hover { color: #fff; background: rgba(255,255,255,0.06); }
	.topbar-link.active { color: #60a5fa; }
	.mobile-menu-btn {
		display: none; align-items: center; justify-content: center;
		width: 36px; height: 36px; background: transparent;
		border: 1px solid rgba(255,255,255,0.1); border-radius: 6px;
		color: rgba(255,255,255,0.6); cursor: pointer; margin-left: auto;
	}

	/* ── Layout ──────────────────────────────────────────────────── */
	.docs-layout {
		max-width: 1280px; margin: 0 auto;
		display: grid; grid-template-columns: 240px 1fr;
		min-height: calc(100vh - 56px);
	}

	/* ── Sidebar ─────────────────────────────────────────────────── */
	.sidebar {
		position: sticky; top: 56px; height: calc(100vh - 56px);
		overflow-y: auto; border-right: 1px solid rgba(255,255,255,0.07);
		padding: 24px 0; scrollbar-width: thin;
		scrollbar-color: rgba(255,255,255,0.1) transparent;
	}
	.sidebar-nav { display: flex; flex-direction: column; gap: 24px; padding: 0 16px; }
	.nav-group { display: flex; flex-direction: column; gap: 2px; }
	.nav-group-label {
		font-size: 10px; font-weight: 700; letter-spacing: 0.1em;
		text-transform: uppercase; color: rgba(255,255,255,0.3);
		padding: 0 8px; margin-bottom: 4px;
	}
	.nav-item {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 8px; font-size: 13px; font-weight: 500;
		color: rgba(255,255,255,0.45); background: transparent;
		border: none; border-radius: 6px; cursor: pointer;
		text-align: left; width: 100%;
		transition: color 0.15s, background 0.15s;
	}
	.nav-item :global(svg) { flex-shrink: 0; opacity: 0; transition: opacity 0.15s; }
	.nav-item:hover { color: rgba(255,255,255,0.8); background: rgba(255,255,255,0.05); }
	.nav-item.active { color: #60a5fa; background: rgba(59,130,246,0.1); }
	.nav-item.active :global(svg) { opacity: 1; }

	/* ── Content ─────────────────────────────────────────────────── */
	.content {
		padding: 48px 64px 96px 64px;
		max-width: 820px;
	}

	section {
		padding-top: 16px;
		margin-bottom: 64px;
		scroll-margin-top: 72px;
	}
	section:first-child { padding-top: 0; }

	.page-eyebrow {
		font-size: 11px; font-weight: 700; letter-spacing: 0.1em;
		text-transform: uppercase; color: #60a5fa; margin-bottom: 10px;
	}

	h1 {
		font-size: 2rem; font-weight: 800; color: #f1f5f9;
		letter-spacing: -0.03em; margin-bottom: 16px;
	}
	h2 {
		font-size: 1.5rem; font-weight: 700; color: #f1f5f9;
		letter-spacing: -0.02em; margin-bottom: 14px;
		padding-bottom: 10px; border-bottom: 1px solid rgba(255,255,255,0.07);
	}
	h3 {
		font-size: 1rem; font-weight: 650; color: #e2e8f0;
		margin-top: 28px; margin-bottom: 10px;
	}

	p { color: rgba(255,255,255,0.6); margin-bottom: 14px; font-size: 14.5px; }

	code {
		font-family: 'Fira Code', 'JetBrains Mono', ui-monospace, monospace;
		font-size: 12.5px; color: #93c5fd;
		background: rgba(59,130,246,0.1); padding: 1px 5px; border-radius: 4px;
	}

	/* ── Quick cards ─────────────────────────────────────────────── */
	.quick-cards {
		display: grid; grid-template-columns: 1fr 1fr;
		gap: 12px; margin: 20px 0;
	}
	.quick-card {
		background: rgba(255,255,255,0.03); border: 1px solid rgba(255,255,255,0.08);
		border-radius: 8px; padding: 14px 16px;
	}
	.qc-label {
		font-size: 10px; font-weight: 700; text-transform: uppercase;
		letter-spacing: 0.08em; color: rgba(255,255,255,0.3); margin-bottom: 6px;
	}
	.quick-card code { background: none; padding: 0; font-size: 13px; color: #e2e8f0; }

	/* ── Code blocks ─────────────────────────────────────────────── */
	.code-block {
		background: #0d1017; border: 1px solid rgba(255,255,255,0.08);
		border-radius: 8px; overflow: hidden; margin: 12px 0;
	}
	.code-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 6px 14px;
		background: rgba(255,255,255,0.03);
		border-bottom: 1px solid rgba(255,255,255,0.06);
	}
	.code-label {
		font-size: 11px; font-weight: 600; letter-spacing: 0.06em;
		text-transform: uppercase; color: rgba(255,255,255,0.3);
	}
	.copy-btn {
		background: transparent; border: none; cursor: pointer;
		color: rgba(255,255,255,0.3); display: flex; align-items: center;
		padding: 2px 4px; border-radius: 3px; transition: color 0.15s;
	}
	.copy-btn:hover { color: rgba(255,255,255,0.7); }
	.code-block pre {
		padding: 16px; font-family: 'Fira Code', 'JetBrains Mono', ui-monospace, monospace;
		font-size: 13px; line-height: 1.7; color: #93c5fd;
		overflow-x: auto; white-space: pre;
	}

	/* ── Callouts ─────────────────────────────────────────────────── */
	.callout {
		display: flex; align-items: flex-start; gap: 10px;
		padding: 12px 16px; border-radius: 8px;
		font-size: 13.5px; line-height: 1.6; margin: 16px 0;
	}
	.callout::before { flex-shrink: 0; font-weight: 700; margin-top: 1px; }
	.callout-info  { background: rgba(59,130,246,0.08);  border: 1px solid rgba(59,130,246,0.2);  color: #93c5fd; }
	.callout-info::before  { content: 'ℹ'; color: #60a5fa; }
	.callout-warn  { background: rgba(234,179,8,0.07);   border: 1px solid rgba(234,179,8,0.2);   color: #fde68a; }
	.callout-warn::before  { content: '⚠'; color: #facc15; }
	.callout-tip   { background: rgba(34,197,94,0.07);   border: 1px solid rgba(34,197,94,0.2);   color: #86efac; }
	.callout-tip::before   { content: '✦'; color: #4ade80; }

	/* ── Tables ──────────────────────────────────────────────────── */
	.table-wrap { overflow-x: auto; margin: 12px 0; }
	table { width: 100%; border-collapse: collapse; font-size: 13.5px;
		border: 1px solid rgba(255,255,255,0.08); border-radius: 8px; overflow: hidden; }
	thead th {
		padding: 9px 14px; text-align: left;
		font-size: 11px; font-weight: 600; letter-spacing: 0.05em; text-transform: uppercase;
		color: rgba(255,255,255,0.35); background: rgba(255,255,255,0.03);
		border-bottom: 1px solid rgba(255,255,255,0.08);
	}
	tbody td {
		padding: 10px 14px; color: rgba(255,255,255,0.6);
		border-bottom: 1px solid rgba(255,255,255,0.05);
		vertical-align: top;
	}
	tbody tr:last-child td { border-bottom: none; }
	tbody tr:hover td { background: rgba(255,255,255,0.02); }

	/* ── Scope badges ────────────────────────────────────────────── */
	.scope-badge {
		display: inline-block; padding: 2px 8px;
		font-size: 11px; font-weight: 700; border-radius: 999px;
		font-family: 'Fira Code', monospace; letter-spacing: 0.02em;
	}
	.scope-read   { background: rgba(59,130,246,0.15); color: #93c5fd; }
	.scope-deploy { background: rgba(234,179,8,0.15);  color: #fde68a; }
	.scope-write  { background: rgba(34,197,94,0.15);  color: #86efac; }
	.scope-admin  { background: rgba(239,68,68,0.15);  color: #fca5a5; }

	/* ── HTTP method badges ──────────────────────────────────────── */
	.method {
		display: inline-block; padding: 3px 10px;
		font-size: 11px; font-weight: 800; border-radius: 5px;
		font-family: 'Fira Code', monospace; letter-spacing: 0.04em;
		flex-shrink: 0;
	}
	.method-sm {
		display: inline-block; padding: 1px 7px;
		font-size: 10px; font-weight: 800; border-radius: 4px;
		font-family: 'Fira Code', monospace;
	}
	.get    { background: rgba(59,130,246,0.2);  color: #60a5fa; }
	.post   { background: rgba(34,197,94,0.2);   color: #4ade80; }
	.patch  { background: rgba(234,179,8,0.2);   color: #facc15; }
	.delete { background: rgba(239,68,68,0.2);   color: #f87171; }

	/* ── Status code badges ──────────────────────────────────────── */
	.status-badge {
		display: inline-block; padding: 2px 7px;
		font-size: 11px; font-weight: 700; border-radius: 4px;
		font-family: 'Fira Code', monospace;
	}
	.s400 { background: rgba(234,179,8,0.15);  color: #fde68a; }
	.s401 { background: rgba(239,68,68,0.15);  color: #fca5a5; }
	.s403 { background: rgba(239,68,68,0.15);  color: #fca5a5; }
	.s404 { background: rgba(148,163,184,0.15); color: #94a3b8; }
	.s500 { background: rgba(239,68,68,0.2);   color: #f87171; }

	/* ── Endpoint blocks ─────────────────────────────────────────── */
	.endpoint-block {
		border: 1px solid rgba(255,255,255,0.07);
		border-radius: 10px; padding: 20px;
		margin: 20px 0; background: rgba(255,255,255,0.015);
	}
	.endpoint-line {
		display: flex; align-items: center; gap: 10px;
		margin-bottom: 10px; flex-wrap: wrap;
	}
	.path {
		font-family: 'Fira Code', monospace; font-size: 13.5px;
		background: none; color: #e2e8f0; padding: 0; flex: 1;
	}
	.param { color: #f0abfc; }
	.scope-req { margin-left: auto; flex-shrink: 0; }
	.ep-desc { font-size: 13.5px; color: rgba(255,255,255,0.55); margin-bottom: 12px; }

	.params-table { margin: 12px 0; }
	.params-label {
		font-size: 10px; font-weight: 700; text-transform: uppercase;
		letter-spacing: 0.08em; color: rgba(255,255,255,0.3); margin-bottom: 6px;
	}

	/* ── Schema blocks ───────────────────────────────────────────── */
	.schema-block {
		border: 1px solid rgba(255,255,255,0.07); border-radius: 8px;
		overflow: hidden; margin: 10px 0 20px;
	}
	.schema-row {
		display: grid; grid-template-columns: 160px 160px 1fr;
		gap: 12px; padding: 9px 14px;
		border-bottom: 1px solid rgba(255,255,255,0.05);
		font-size: 13px; align-items: baseline;
	}
	.schema-row:last-child { border-bottom: none; }
	.field-name { font-family: 'Fira Code', monospace; color: #93c5fd; font-size: 12.5px; }
	.field-type { color: rgba(255,255,255,0.35); font-size: 12px; font-style: italic; }
	.field-desc { color: rgba(255,255,255,0.55); font-size: 12.5px; }

	/* ── Responsive ──────────────────────────────────────────────── */
	@media (max-width: 900px) {
		.docs-layout { grid-template-columns: 1fr; }
		.sidebar {
			position: fixed; top: 56px; left: 0; bottom: 0; z-index: 50;
			width: 260px; background: #0d0d14;
			border-right: 1px solid rgba(255,255,255,0.1);
			transform: translateX(-100%); transition: transform 0.25s ease;
		}
		.sidebar.open { transform: translateX(0); }
		.mobile-menu-btn { display: flex; }
		.topbar-links { display: none; }
		.content { padding: 32px 24px 80px; }
		.quick-cards { grid-template-columns: 1fr; }
		.schema-row { grid-template-columns: 1fr 1fr; }
		.schema-row .field-desc { grid-column: span 2; }
	}
	@media (max-width: 480px) {
		h1 { font-size: 1.6rem; }
		h2 { font-size: 1.25rem; }
		.content { padding: 24px 16px 80px; }
		.schema-row { grid-template-columns: 1fr; }
		.schema-row .field-desc { grid-column: span 1; }
	}
</style>
