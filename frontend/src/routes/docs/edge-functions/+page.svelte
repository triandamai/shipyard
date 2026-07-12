<script lang="ts">
	import { page } from '$app/state';

	const origin = page.url.origin;
	const fnUrl  = `${origin}/fn/{org-slug}/{function-name}`;
</script>

<svelte:head>
	<title>Edge Functions · Shipyard Docs</title>
	<meta name="description" content="Deploy serverless JavaScript/TypeScript functions at the edge with Shipyard." />
</svelte:head>

<div class="page">

	<!-- ── Nav ── -->
	<nav class="topnav">
		<a class="nav-logo" href="/">
			<svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
				<path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>
				<polyline points="9 22 9 12 15 12 15 22"/>
			</svg>
			<span>Shipyard</span>
		</a>
		<div class="nav-links">
			<span class="nav-badge">Docs</span>
			<span class="nav-sep">·</span>
			<span class="nav-cur">Edge Functions</span>
		</div>
		<a class="nav-app-btn" href="/orgs">Open App</a>
	</nav>

	<div class="layout">

		<!-- ── Sidebar ── -->
		<aside class="sidebar">
			<div class="sidebar-section">
				<div class="sidebar-heading">Edge Functions</div>
				<a class="sidebar-link active" href="#overview">Overview</a>
				<a class="sidebar-link" href="#quickstart">Quick Start</a>
				<a class="sidebar-link" href="#repo-structure">Repo Structure</a>
				<a class="sidebar-link" href="#shipyard-json">shipyard.json</a>
				<a class="sidebar-link" href="#invoking">Invoking Functions</a>
				<a class="sidebar-link" href="#custom-domains">Custom Domains</a>
				<a class="sidebar-link" href="#deployments">Deployment History</a>
				<a class="sidebar-link" href="#env-vars">Environment Variables</a>
				<a class="sidebar-link" href="#limits">Limits & Quotas</a>
			</div>
		</aside>

		<!-- ── Content ── -->
		<main class="content">

			<!-- Overview -->
			<section id="overview" class="doc-section">
				<div class="hero-pill"><span class="pill-dot"></span> Edge Functions</div>
				<h1 class="doc-h1">Edge Functions</h1>
				<p class="doc-lead">
					Deploy serverless JavaScript and TypeScript functions that run on-demand inside your
					Shipyard infrastructure. Connect a Git repository, push code, and your functions
					are live in seconds — no containers to manage, no servers to provision.
				</p>

				<div class="feature-grid">
					<div class="feature-card">
						<div class="feature-icon">⚡</div>
						<div class="feature-title">Git-driven deploys</div>
						<div class="feature-desc">Push to your branch and functions redeploy automatically via webhook.</div>
					</div>
					<div class="feature-card">
						<div class="feature-icon">🔀</div>
						<div class="feature-title">Versioned history</div>
						<div class="feature-desc">Every deploy is snapshotted. Browse code from any past deployment and restore in one click.</div>
					</div>
					<div class="feature-card">
						<div class="feature-icon">🌐</div>
						<div class="feature-title">Custom domains</div>
						<div class="feature-desc">Route any hostname to your functions with automatic TLS via Let's Encrypt.</div>
					</div>
					<div class="feature-card">
						<div class="feature-icon">📊</div>
						<div class="feature-title">Invocation logs</div>
						<div class="feature-desc">Rolling 7-day log of every request: method, path, status, duration, and errors.</div>
					</div>
				</div>
			</section>

			<hr class="doc-divider" />

			<!-- Quick Start -->
			<section id="quickstart" class="doc-section">
				<h2 class="doc-h2">Quick Start</h2>

				<div class="steps">
					<div class="step">
						<div class="step-num">1</div>
						<div class="step-body">
							<div class="step-title">Create a function file</div>
							<p class="step-desc">
								In your Git repository, create a <code>functions/</code> directory. Each
								<code>.ts</code> or <code>.js</code> file becomes one function. The file name
								(kebab-cased) is the function's URL slug.
							</p>
							<div class="code-block">
								<div class="code-header">functions/send-email.ts</div>
								<pre class="code-pre">{`export default async function handler(req: Request): Promise<Response> {
  const { to, subject } = await req.json();
  // your logic here
  return Response.json({ sent: true });
}`}</pre>
							</div>
						</div>
					</div>

					<div class="step">
						<div class="step-num">2</div>
						<div class="step-body">
							<div class="step-title">Connect your repository</div>
							<p class="step-desc">
								Open your project canvas in Shipyard, click <strong>Add Resource → Edge Functions</strong>,
								and paste your repository URL and branch. Shipyard auto-registers a webhook so
								every push triggers a redeploy.
							</p>
						</div>
					</div>

					<div class="step">
						<div class="step-num">3</div>
						<div class="step-body">
							<div class="step-title">Invoke your function</div>
							<p class="step-desc">Your function is immediately available at:</p>
							<div class="code-block">
								<div class="code-header">URL format</div>
								<pre class="code-pre">{`{app-url}/fn/{org-slug}/{function-name}`}</pre>
							</div>
							<div class="code-block" style="margin-top:8px">
								<div class="code-header">Example</div>
								<pre class="code-pre">{`curl -X POST "${origin}/fn/acme/send-email" \\
  -H "Content-Type: application/json" \\
  -d '{"to":"user@example.com","subject":"Hello"}'`}</pre>
							</div>
						</div>
					</div>
				</div>
			</section>

			<hr class="doc-divider" />

			<!-- Repo Structure -->
			<section id="repo-structure" class="doc-section">
				<h2 class="doc-h2">Repo Structure</h2>
				<p class="doc-p">
					Shipyard scans your repository at deploy time using this resolution order:
				</p>

				<div class="resolution-list">
					<div class="res-item">
						<div class="res-num">1</div>
						<div class="res-body">
							<code>shipyard.json</code> with an explicit <code>functions.entries</code> map — use these exact files, skip directory scan.
						</div>
					</div>
					<div class="res-item">
						<div class="res-num">2</div>
						<div class="res-body">
							<code>shipyard.json</code> with <code>functions.dir</code> — scan that directory instead of <code>functions/</code>.
						</div>
					</div>
					<div class="res-item">
						<div class="res-num">3</div>
						<div class="res-body">
							Fallback: scan the <code>functions/</code> directory at the repo root.
						</div>
					</div>
				</div>

				<h3 class="doc-h3">Requirements for auto-detection</h3>
				<ul class="doc-list">
					<li>Files must be <code>.ts</code> or <code>.js</code></li>
					<li>Each file must contain <code>export default</code> (any function or object)</li>
					<li>The file stem becomes the function slug: <code>sendEmail.ts</code> → <code>send-email</code></li>
					<li>Subdirectories and non-JS/TS files are ignored</li>
				</ul>

				<h3 class="doc-h3">Example layout</h3>
				<div class="code-block">
					<div class="code-header">Repository root</div>
					<pre class="code-pre">{`your-repo/
├── functions/
│   ├── send-email.ts      → /fn/acme/send-email
│   ├── resize-image.ts    → /fn/acme/resize-image
│   └── webhook-handler.ts → /fn/acme/webhook-handler
├── shipyard.json          (optional — for advanced config)
└── ...other files`}</pre>
				</div>

				<div class="callout callout-info">
					<strong>Handler signature</strong> — Shipyard uses the Deno runtime.
					Your default export receives a standard <code>Request</code> and must return a <code>Response</code>
					or <code>Promise&lt;Response&gt;</code>.
				</div>
			</section>

			<hr class="doc-divider" />

			<!-- shipyard.json -->
			<section id="shipyard-json" class="doc-section">
				<h2 class="doc-h2">shipyard.json</h2>
				<p class="doc-p">
					Place a <code>shipyard.json</code> file at your repo root for full control over
					function discovery and configuration.
				</p>

				<h3 class="doc-h3">Scan a custom directory</h3>
				<div class="code-block">
					<div class="code-header">shipyard.json</div>
					<pre class="code-pre">{`{
  "functions": {
    "dir": "src/edge",
    "runtime": "deno"
  }
}`}</pre>
				</div>

				<h3 class="doc-h3">Explicit entry map</h3>
				<p class="doc-p">
					Use <code>entries</code> when you want precise control over names, timeouts,
					and which env vars each function can read:
				</p>
				<div class="code-block">
					<div class="code-header">shipyard.json</div>
					<pre class="code-pre">{`{
  "functions": {
    "runtime": "deno",
    "entries": {
      "send-email": {
        "file": "src/email/handler.ts",
        "timeout": 30,
        "env": ["SMTP_HOST", "SMTP_PORT", "SMTP_USER"]
      },
      "resize-image": {
        "file": "src/media/resize.ts",
        "timeout": 60
      }
    }
  }
}`}</pre>
				</div>

				<h3 class="doc-h3">Schema reference</h3>
				<div class="schema-table">
					<div class="schema-row schema-head">
						<span>Field</span><span>Type</span><span>Description</span>
					</div>
					<div class="schema-row">
						<code>functions.dir</code>
						<span class="type-badge">string</span>
						<span>Directory to scan. Ignored when <code>entries</code> is set.</span>
					</div>
					<div class="schema-row">
						<code>functions.runtime</code>
						<span class="type-badge">string</span>
						<span>Runtime name. Currently only <code>"deno"</code> is supported.</span>
					</div>
					<div class="schema-row">
						<code>functions.entries</code>
						<span class="type-badge">object</span>
						<span>Map of slug → entry config. Takes precedence over dir scan.</span>
					</div>
					<div class="schema-row">
						<code>entry.file</code>
						<span class="type-badge">string</span>
						<span>Path to the handler file, relative to repo root.</span>
					</div>
					<div class="schema-row">
						<code>entry.timeout</code>
						<span class="type-badge">number</span>
						<span>Max execution time in seconds. Default: 10.</span>
					</div>
					<div class="schema-row">
						<code>entry.env</code>
						<span class="type-badge">string[]</span>
						<span>Allowlist of env var keys this function can access. Empty = allow all org vars.</span>
					</div>
				</div>
			</section>

			<hr class="doc-divider" />

			<!-- Invoking -->
			<section id="invoking" class="doc-section">
				<h2 class="doc-h2">Invoking Functions</h2>

				<h3 class="doc-h3">URL format</h3>
				<div class="code-block">
					<div class="code-header">Pattern</div>
					<pre class="code-pre">{`{app-url}/fn/{org-slug}/{function-name}[/extra/path][?query=params]`}</pre>
				</div>

				<ul class="doc-list">
					<li><strong>org-slug</strong> — your organization slug, shown in the URL when you open the app</li>
					<li><strong>function-name</strong> — the kebab-cased file stem or the key from <code>entries</code></li>
					<li>Any path suffix and query string is forwarded verbatim to your handler</li>
				</ul>

				<h3 class="doc-h3">HTTP methods</h3>
				<p class="doc-p">All HTTP methods are supported. Your handler receives the original method.</p>

				<div class="code-block">
					<div class="code-header">GET request</div>
					<pre class="code-pre">{`curl "${origin}/fn/acme/send-email"`}</pre>
				</div>

				<div class="code-block" style="margin-top:10px">
					<div class="code-header">POST with JSON body</div>
					<pre class="code-pre">{`curl -X POST "${origin}/fn/acme/send-email" \\
  -H "Content-Type: application/json" \\
  -d '{"to": "alice@example.com", "subject": "Hello"}'`}</pre>
				</div>

				<div class="code-block" style="margin-top:10px">
					<div class="code-header">With extra path and query</div>
					<pre class="code-pre">{`curl "${origin}/fn/acme/send-email/bulk?dryRun=true"`}</pre>
				</div>

				<h3 class="doc-h3">Reading the request in your handler</h3>
				<div class="code-block">
					<div class="code-header">functions/send-email.ts</div>
					<pre class="code-pre">{`export default async function handler(req: Request): Promise<Response> {
  const url    = new URL(req.url);
  const dryRun = url.searchParams.get('dryRun') === 'true';
  const path   = url.pathname;           // e.g. "/send-email/bulk"

  if (req.method === 'POST') {
    const body = await req.json();       // parse JSON body
    // ...
  }

  return Response.json({ ok: true });
}`}</pre>
				</div>
			</section>

			<hr class="doc-divider" />

			<!-- Custom Domains -->
			<section id="custom-domains" class="doc-section">
				<h2 class="doc-h2">Custom Domains</h2>
				<p class="doc-p">
					You can bind one or more custom hostnames to an edge function group.
					Requests to that hostname are routed directly to your functions — the path after
					the domain maps to the function name, just like with the built-in URL.
				</p>

				<div class="steps">
					<div class="step">
						<div class="step-num">1</div>
						<div class="step-body">
							<div class="step-title">Add a DNS record</div>
							<p class="step-desc">
								Point your domain's <strong>A record</strong> to the Shipyard server IP,
								or set a <strong>CNAME</strong> to your Shipyard hostname.
							</p>
						</div>
					</div>
					<div class="step">
						<div class="step-num">2</div>
						<div class="step-body">
							<div class="step-title">Add the domain in Shipyard</div>
							<p class="step-desc">
								Open the edge function group panel → <strong>Domains</strong> tab →
								<strong>Add Domain</strong>. Shipyard writes a Traefik route file and TLS
								certificates are provisioned automatically.
							</p>
						</div>
					</div>
					<div class="step">
						<div class="step-num">3</div>
						<div class="step-body">
							<div class="step-title">Invoke via the custom domain</div>
							<div class="code-block" style="margin-top:6px">
								<div class="code-header">Example</div>
								<pre class="code-pre">{`# Built-in URL
curl "${origin}/fn/acme/send-email"

# Same function via custom domain
curl "https://api.acme.com/send-email"`}</pre>
							</div>
						</div>
					</div>
				</div>

				<div class="callout callout-tip">
					<strong>DNS check</strong> — use the <strong>DNS</strong> button in the Domains tab
					to verify your record is propagated. Traefik requests a Let's Encrypt certificate
					automatically once DNS resolves.
				</div>

				<h3 class="doc-h3">TLS options</h3>
				<div class="schema-table">
					<div class="schema-row schema-head">
						<span>Provider</span><span></span><span>When to use</span>
					</div>
					<div class="schema-row">
						<code>letsencrypt</code>
						<span class="badge-green">Recommended</span>
						<span>Free ACME cert. Requires a public domain with valid DNS.</span>
					</div>
					<div class="schema-row">
						<code>selfsigned</code>
						<span class="type-badge">Dev</span>
						<span>Auto-generated cert. Browser will show a security warning.</span>
					</div>
					<div class="schema-row">
						<code>custom</code>
						<span class="type-badge">Advanced</span>
						<span>Named Traefik cert resolver — must match your Traefik config.</span>
					</div>
				</div>
			</section>

			<hr class="doc-divider" />

			<!-- Deployment History -->
			<section id="deployments" class="doc-section">
				<h2 class="doc-h2">Deployment History</h2>
				<p class="doc-p">
					Every deploy (git push, webhook, or manual redeploy) creates a versioned snapshot.
					The panel's <strong>Functions</strong> tab shows per-function history so you can
					audit, inspect, and restore any past deployment.
				</p>

				<div class="feature-grid" style="grid-template-columns:1fr 1fr">
					<div class="feature-card">
						<div class="feature-icon">🔍</div>
						<div class="feature-title">View any snapshot</div>
						<div class="feature-desc">Click <strong>View</strong> next to any past deployment to open the code in a read-only editor with syntax highlighting.</div>
					</div>
					<div class="feature-card">
						<div class="feature-icon">↩️</div>
						<div class="feature-title">Restore in one click</div>
						<div class="feature-desc">Click <strong>Restore</strong> on any non-live deployment to re-activate that snapshot as the new live version.</div>
					</div>
				</div>

				<h3 class="doc-h3">Deployment statuses</h3>
				<div class="schema-table">
					<div class="schema-row schema-head">
						<span>Status</span><span></span><span>Meaning</span>
					</div>
					<div class="schema-row">
						<code>live</code>
						<span class="badge-green">Active</span>
						<span>Currently serving traffic. Only one deployment per function is live at a time.</span>
					</div>
					<div class="schema-row">
						<code>rolled_back</code>
						<span class="type-badge">Inactive</span>
						<span>Superseded by a newer deploy or a restore action. Code is preserved.</span>
					</div>
					<div class="schema-row">
						<code>pending</code>
						<span class="type-badge">Queued</span>
						<span>Deploy queued but not yet executed.</span>
					</div>
					<div class="schema-row">
						<code>error</code>
						<span class="badge-red">Failed</span>
						<span>Deploy failed. Inspect the error field for details.</span>
					</div>
				</div>

				<div class="callout callout-info">
					<strong>Code is always stored.</strong> Shipyard stores the full code bundle for every
					deployment. Rolling back never requires access to your Git repository.
				</div>
			</section>

			<hr class="doc-divider" />

			<!-- Env Vars -->
			<section id="env-vars" class="doc-section">
				<h2 class="doc-h2">Environment Variables</h2>
				<p class="doc-p">
					Set environment variables on individual functions via the function detail panel.
					Variables are encrypted at rest using AES-256.
				</p>

				<h3 class="doc-h3">Env var allowlist</h3>
				<p class="doc-p">
					By default a function can read all org-level env vars. Use <code>env_whitelist</code>
					(or <code>entry.env</code> in <code>shipyard.json</code>) to restrict which keys are
					injected at runtime — useful for isolating sensitive credentials.
				</p>

				<div class="code-block">
					<div class="code-header">Reading env vars in your handler</div>
					<pre class="code-pre">{`export default async function handler(req: Request): Promise<Response> {
  // Deno runtime — env vars are injected into Deno.env
  const apiKey = Deno.env.get('STRIPE_SECRET_KEY');
  // ...
}`}</pre>
				</div>
			</section>

			<hr class="doc-divider" />

			<!-- Limits -->
			<section id="limits" class="doc-section">
				<h2 class="doc-h2">Limits &amp; Quotas</h2>
				<p class="doc-p">Quotas are enforced per organization and vary by billing tier.</p>

				<div class="schema-table">
					<div class="schema-row schema-head">
						<span>Limit</span><span>Free</span><span>Pro / Enterprise</span>
					</div>
					<div class="schema-row">
						<span>Max functions</span>
						<code>3</code>
						<span>Configurable by admin</span>
					</div>
					<div class="schema-row">
						<span>Max code bundle size</span>
						<code>256 KB</code>
						<span>Configurable by admin</span>
					</div>
					<div class="schema-row">
						<span>Default timeout</span>
						<code>10 s</code>
						<span>Up to 300 s per function</span>
					</div>
					<div class="schema-row">
						<span>Invocation log retention</span>
						<code>7 days</code>
						<code>7 days</code>
					</div>
					<div class="schema-row">
						<span>Deployment history</span>
						<span>Unlimited</span>
						<span>Unlimited</span>
					</div>
				</div>

				<div class="callout callout-tip">
					Admins can override quotas under <strong>Settings → Edge Functions</strong>.
				</div>
			</section>

			<div class="doc-footer">
				<a href="/orgs" class="footer-btn">Open Shipyard</a>
				<span class="footer-sep">·</span>
				<span class="footer-copy">Shipyard PaaS</span>
			</div>

		</main>
	</div>
</div>

<style>
	/* ── Reset / base ── */
	*, *::before, *::after { box-sizing: border-box; }

	.page {
		min-height: 100vh;
		background: var(--bg-base);
		color: var(--text-primary);
		font-family: var(--font-sans, system-ui, sans-serif);
	}

	/* ── Top nav ── */
	.topnav {
		display: flex; align-items: center; gap: 12px;
		padding: 0 24px; height: 52px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		position: sticky; top: 0; z-index: 50;
	}
	.nav-logo {
		display: flex; align-items: center; gap: 8px;
		color: var(--text-primary); text-decoration: none;
		font-weight: 700; font-size: 15px;
	}
	.nav-logo:hover { color: var(--accent); }
	.nav-links { display: flex; align-items: center; gap: 6px; flex: 1; }
	.nav-badge {
		font-size: 10px; font-weight: 700; padding: 2px 7px; border-radius: 99px;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent); letter-spacing: 0.06em; text-transform: uppercase;
	}
	.nav-sep { color: var(--border); }
	.nav-cur { font-size: 13px; color: var(--text-muted); }
	.nav-app-btn {
		display: inline-flex; align-items: center;
		padding: 5px 14px; border-radius: var(--radius-sm);
		background: var(--accent); color: #fff;
		font-size: 12px; font-weight: 600; text-decoration: none;
		transition: opacity .15s;
	}
	.nav-app-btn:hover { opacity: 0.88; }

	/* ── Layout ── */
	.layout {
		display: flex; gap: 0;
		max-width: 1100px; margin: 0 auto;
		padding: 0 24px;
	}

	/* ── Sidebar ── */
	.sidebar {
		width: 200px; flex-shrink: 0;
		padding: 32px 0;
		position: sticky; top: 52px;
		height: calc(100vh - 52px); overflow-y: auto;
	}
	.sidebar-section { display: flex; flex-direction: column; gap: 2px; }
	.sidebar-heading {
		font-size: 10px; font-weight: 700; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em;
		padding: 0 12px 8px;
	}
	.sidebar-link {
		display: block; padding: 5px 12px; border-radius: 6px;
		font-size: 13px; color: var(--text-muted); text-decoration: none;
		transition: all .12s;
	}
	.sidebar-link:hover { color: var(--text-primary); background: var(--bg-surface); }
	.sidebar-link.active { color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, transparent); }

	/* ── Content ── */
	.content {
		flex: 1; min-width: 0;
		padding: 40px 0 80px 48px;
		border-left: 1px solid var(--border);
	}

	.doc-section { margin-bottom: 8px; scroll-margin-top: 72px; }
	.doc-divider { border: none; border-top: 1px solid var(--border); margin: 40px 0; }

	.hero-pill {
		display: inline-flex; align-items: center; gap: 7px;
		padding: 4px 12px; border-radius: 99px; margin-bottom: 16px;
		background: color-mix(in srgb, var(--accent) 8%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
		font-size: 11px; font-weight: 600; color: var(--accent);
		text-transform: uppercase; letter-spacing: 0.06em;
	}
	.pill-dot {
		width: 6px; height: 6px; border-radius: 50%; background: var(--accent);
		animation: pulse 2s ease-in-out infinite;
	}
	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50% { opacity: 0.4; }
	}

	.doc-h1 {
		font-size: 32px; font-weight: 800; line-height: 1.2;
		color: var(--text-primary); margin: 0 0 16px;
	}
	.doc-h2 {
		font-size: 22px; font-weight: 700; color: var(--text-primary);
		margin: 0 0 14px;
	}
	.doc-h3 {
		font-size: 14px; font-weight: 700; color: var(--text-primary);
		margin: 24px 0 10px;
	}
	.doc-lead {
		font-size: 15px; color: var(--text-muted); line-height: 1.7;
		margin: 0 0 28px; max-width: 640px;
	}
	.doc-p {
		font-size: 14px; color: var(--text-muted); line-height: 1.7;
		margin: 0 0 16px;
	}
	.doc-list {
		font-size: 14px; color: var(--text-muted); line-height: 1.8;
		margin: 0 0 16px; padding-left: 20px;
	}
	.doc-list li { margin-bottom: 4px; }
	.doc-list code, .doc-p code, .step-desc code {
		font-family: var(--font-mono); font-size: 12px;
		background: var(--bg-elevated); border: 1px solid var(--border);
		padding: 1px 5px; border-radius: 4px; color: var(--text-primary);
	}

	/* ── Feature grid ── */
	.feature-grid {
		display: grid; grid-template-columns: 1fr 1fr;
		gap: 12px; margin-bottom: 0;
	}
	.feature-card {
		padding: 16px; border-radius: var(--radius-sm);
		background: var(--bg-elevated); border: 1px solid var(--border);
	}
	.feature-icon { font-size: 20px; margin-bottom: 8px; }
	.feature-title { font-size: 13px; font-weight: 700; color: var(--text-primary); margin-bottom: 4px; }
	.feature-desc { font-size: 12px; color: var(--text-muted); line-height: 1.6; }

	/* ── Steps ── */
	.steps { display: flex; flex-direction: column; gap: 0; }
	.step {
		display: flex; gap: 16px; padding: 20px 0;
		border-bottom: 1px solid var(--border);
	}
	.step:last-child { border-bottom: none; }
	.step-num {
		width: 26px; height: 26px; border-radius: 50%; flex-shrink: 0;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
		color: var(--accent); font-size: 12px; font-weight: 700;
		display: flex; align-items: center; justify-content: center;
		margin-top: 2px;
	}
	.step-body { flex: 1; min-width: 0; }
	.step-title { font-size: 14px; font-weight: 700; color: var(--text-primary); margin-bottom: 6px; }
	.step-desc { font-size: 13px; color: var(--text-muted); line-height: 1.7; margin: 0 0 10px; }

	/* ── Code blocks ── */
	.code-block {
		border-radius: var(--radius-sm); overflow: hidden;
		border: 1px solid var(--border);
	}
	.code-header {
		padding: 6px 14px; font-size: 10px; font-weight: 600;
		color: var(--text-dim); background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		text-transform: uppercase; letter-spacing: 0.05em;
	}
	.code-pre {
		margin: 0; padding: 14px 16px;
		font-family: var(--font-mono, monospace); font-size: 12.5px;
		color: var(--text-primary); background: var(--bg-base);
		white-space: pre; overflow-x: auto; line-height: 1.65;
	}

	/* ── Callouts ── */
	.callout {
		padding: 12px 16px; border-radius: var(--radius-sm);
		font-size: 13px; line-height: 1.6; margin: 16px 0;
	}
	.callout-info {
		background: color-mix(in srgb, var(--accent) 6%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
		color: var(--text-muted);
	}
	.callout-info strong { color: var(--accent); }
	.callout-tip {
		background: color-mix(in srgb, #22c55e 6%, transparent);
		border: 1px solid color-mix(in srgb, #22c55e 20%, transparent);
		color: var(--text-muted);
	}
	.callout-tip strong { color: #22c55e; }

	/* ── Schema table ── */
	.schema-table { border: 1px solid var(--border); border-radius: var(--radius-sm); overflow: hidden; }
	.schema-row {
		display: grid; grid-template-columns: minmax(140px,auto) auto 1fr;
		gap: 16px; align-items: start;
		padding: 9px 14px; border-bottom: 1px solid var(--border);
		font-size: 13px; color: var(--text-muted);
	}
	.schema-row:last-child { border-bottom: none; }
	.schema-head {
		background: var(--bg-elevated); font-size: 10px; font-weight: 700;
		color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em;
	}
	.schema-row code {
		font-family: var(--font-mono); font-size: 12px;
		color: var(--text-primary);
	}
	.type-badge {
		display: inline-block; font-size: 10px; font-weight: 600; padding: 1px 7px;
		border-radius: 99px; background: var(--bg-elevated); border: 1px solid var(--border);
		color: var(--text-dim); white-space: nowrap;
	}
	.badge-green {
		display: inline-block; font-size: 10px; font-weight: 600; padding: 1px 7px;
		border-radius: 99px;
		background: color-mix(in srgb, #22c55e 10%, transparent);
		border: 1px solid color-mix(in srgb, #22c55e 25%, transparent);
		color: #22c55e; white-space: nowrap;
	}
	.badge-red {
		display: inline-block; font-size: 10px; font-weight: 600; padding: 1px 7px;
		border-radius: 99px;
		background: color-mix(in srgb, #ef4444 10%, transparent);
		border: 1px solid color-mix(in srgb, #ef4444 25%, transparent);
		color: #ef4444; white-space: nowrap;
	}

	/* ── Resolution list ── */
	.resolution-list { display: flex; flex-direction: column; gap: 8px; margin-bottom: 20px; }
	.res-item {
		display: flex; gap: 12px; align-items: flex-start;
		padding: 10px 14px; border-radius: var(--radius-sm);
		background: var(--bg-elevated); border: 1px solid var(--border);
		font-size: 13px; color: var(--text-muted); line-height: 1.6;
	}
	.res-num {
		font-size: 11px; font-weight: 700; color: var(--accent);
		width: 18px; flex-shrink: 0; padding-top: 1px;
	}
	.res-body code {
		font-family: var(--font-mono); font-size: 12px;
		background: var(--bg-base); border: 1px solid var(--border);
		padding: 1px 5px; border-radius: 3px; color: var(--text-primary);
	}

	/* ── Footer ── */
	.doc-footer {
		margin-top: 48px; padding-top: 32px;
		border-top: 1px solid var(--border);
		display: flex; align-items: center; gap: 12px;
	}
	.footer-btn {
		display: inline-flex; align-items: center;
		padding: 7px 18px; border-radius: var(--radius-sm);
		background: var(--accent); color: #fff;
		font-size: 13px; font-weight: 600; text-decoration: none;
		transition: opacity .15s;
	}
	.footer-btn:hover { opacity: 0.88; }
	.footer-sep { color: var(--border); }
	.footer-copy { font-size: 12px; color: var(--text-dim); }
</style>
