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
				{ id: 'overview',     label: 'Overview' },
				{ id: 'quickstart',   label: 'Quick Start' },
				{ id: 'repo-structure', label: 'Repo Structure' },
			],
		},
		{
			group: 'Configuration',
			items: [
				{ id: 'shipyard-json', label: 'shipyard.json' },
				{ id: 'env-vars',      label: 'Environment Variables' },
			],
		},
		{
			group: 'Invocation',
			items: [
				{ id: 'invoking',       label: 'Invoking Functions' },
				{ id: 'custom-domains', label: 'Custom Domains' },
			],
		},
		{
			group: 'Operations',
			items: [
				{ id: 'deploy-history', label: 'Deployment History' },
				{ id: 'limits',         label: 'Limits & Quotas' },
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

	const snip = {
		helloFn: [
			'// functions/hello.ts',
			'export default async function handler(req: Request): Promise<Response> {',
			'  const name = new URL(req.url).searchParams.get("name") ?? "world";',
			'  return new Response(`Hello, ${name}!`, {',
			'    headers: { "Content-Type": "text/plain" },',
			'  });',
			'}',
		].join('\n'),
		shipyardJson: JSON.stringify({
			functions: {
				dir: 'functions',
				runtime: 'deno',
				timeout_ms: 10000,
				env: ['DATABASE_URL', 'STRIPE_KEY'],
				entries: {
					'send-email': 'functions/mailer.ts',
					'resize-image': 'functions/images/resize.ts',
				},
			},
		}, null, 2),
		invokeGet: 'curl https://your-shipyard.example.com/fn/my-org/hello?name=Claude',
		invokePost: [
			'curl -X POST https://your-shipyard.example.com/fn/my-org/send-email \\',
			'  -H "Content-Type: application/json" \\',
			'  -d \'{"to":"user@example.com","subject":"Welcome"}\'',
		].join('\n'),
		invokeCustom: 'curl https://api.acme.com/send-email',
		dnsRecord: [
			'# CNAME your custom domain to the Shipyard edge runtime',
			'api.acme.com.  IN  CNAME  shipyard-edge-<org-short-id>.example.com.',
		].join('\n'),
		envAccess: [
			'export default async function handler(req: Request): Promise<Response> {',
			'  const dbUrl = Deno.env.get("DATABASE_URL");',
			'  // dbUrl is available because "DATABASE_URL" is in the env allowlist',
			'  return new Response("ok");',
			'}',
		].join('\n'),
		repoLayout: [
			'my-repo/',
			'├── functions/',
			'│   ├── hello.ts          ← auto-detected (export default)',
			'│   ├── send-email.ts     ← auto-detected',
			'│   └── utils.ts          ← skipped (no export default)',
			'├── shipyard.json         ← optional overrides',
			'└── ...',
		].join('\n'),
	};
</script>

<svelte:head>
	<title>Edge Functions — Shipyard Docs</title>
	<meta name="description" content="Run serverless TypeScript/JavaScript functions at the edge with Shipyard. Deploy from any Git repository — zero config required." />
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
			<a href="/docs/api" class="topbar-link">API Reference</a>
			<a href="/docs/edge-functions" class="topbar-link active">Edge Functions</a>
			<a href="/docs/registry" class="topbar-link">Registry</a>
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

		<!-- ── Overview ──────────────────────────────────────────────── -->
		<section id="overview">
			<div class="page-eyebrow">Edge Functions · Deno Runtime</div>
			<h1>Edge Functions</h1>
			<p>
				Edge Functions let you run serverless TypeScript or JavaScript handlers alongside
				your other Shipyard services. Drop <code>.ts</code> files into a
				<code>functions/</code> directory in any Git repository — Shipyard detects them
				automatically on every push and deploys them to a Deno runtime scoped to your
				organization.
			</p>

			<div class="quick-cards">
				<div class="quick-card">
					<div class="qc-label">Invoke URL</div>
					<code>/fn/&lt;org-slug&gt;/&lt;fn-name&gt;</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Runtime</div>
					<code>Deno 1.x · TypeScript native</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Deploy trigger</div>
					<code>Git push → auto-detect</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Custom domain</div>
					<code>CNAME → edge runtime</code>
				</div>
			</div>
		</section>

		<!-- ── Quick Start ───────────────────────────────────────────── -->
		<section id="quickstart">
			<h2>Quick Start</h2>
			<p>Three steps from zero to a running function.</p>

			<h3>Step 1 — Write a function</h3>
			<p>
				Create a file inside <code>functions/</code> and export a default async
				function that accepts a <code>Request</code> and returns a <code>Response</code>.
				No framework, no boilerplate.
			</p>
			<div class="code-block">
				<div class="code-header">
					<span class="code-label">functions/hello.ts</span>
					<button class="copy-btn" onclick={() => copyCode('hello-fn', snip.helloFn)}>
						{#if copied === 'hello-fn'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.helloFn}</pre>
			</div>

			<h3>Step 2 — Connect a repo with an edge function group</h3>
			<p>
				In the Shipyard dashboard, open your project → <strong>Edge Functions</strong>
				and create a new function group. Connect it to your Git repository. Shipyard
				will clone the repo and scan the <code>functions/</code> directory on every deploy.
			</p>

			<div class="callout callout-info">
				You can attach one function group per repository branch. Multiple branches in the
				same repo can each have their own group.
			</div>

			<h3>Step 3 — Invoke your function</h3>
			<p>
				Once deployed, your function is reachable at the path-based URL immediately —
				no DNS changes needed.
			</p>
			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash</span>
					<button class="copy-btn" onclick={() => copyCode('invoke-get', snip.invokeGet)}>
						{#if copied === 'invoke-get'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.invokeGet}</pre>
			</div>

			<div class="code-block">
				<div class="code-header"><span class="code-label">response</span></div>
				<pre>Hello, Claude!</pre>
			</div>
		</section>

		<!-- ── Repo Structure ─────────────────────────────────────────── -->
		<section id="repo-structure">
			<h2>Repo Structure</h2>
			<p>
				Shipyard's function detector (<code>edge_fn_detector</code>) scans your repository
				after every deploy. It resolves functions in this order:
			</p>

			<ol class="ordered-list">
				<li>If <code>shipyard.json</code> defines an <code>entries</code> map, use those explicit paths.</li>
				<li>Otherwise scan the configured <code>dir</code> (default <code>functions/</code>) for <code>.ts</code> and <code>.js</code> files.</li>
				<li>A file is registered as a function only if it contains <code>export default</code>.</li>
				<li>The function name is the filename without extension (e.g. <code>send-email.ts</code> → <code>send-email</code>).</li>
			</ol>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">example repo layout</span>
					<button class="copy-btn" onclick={() => copyCode('repo-layout', snip.repoLayout)}>
						{#if copied === 'repo-layout'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.repoLayout}</pre>
			</div>

			<div class="callout callout-warn">
				Files without <code>export default</code> are silently skipped — they will not
				appear in the deploy report and will not be invocable. Check your deploy report
				in the dashboard if a function is missing.
			</div>

			<h3>Handler signature</h3>
			<p>
				Every function must export a default async function with this exact signature.
				Shipyard passes the raw <code>Request</code> object from the Deno HTTP server.
			</p>

			<div class="code-block">
				<div class="code-header"><span class="code-label">typescript</span></div>
				<pre>export default async function handler(req: Request): Promise{'<Response>'} {'{ … }'}</pre>
			</div>
		</section>

		<!-- ── shipyard.json ─────────────────────────────────────────── -->
		<section id="shipyard-json">
			<h2>shipyard.json</h2>
			<p>
				Place a <code>shipyard.json</code> file at the root of your repository to
				override defaults. All fields are optional — an empty file or no file at all
				is valid.
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">shipyard.json</span>
					<button class="copy-btn" onclick={() => copyCode('shipyard-json', snip.shipyardJson)}>
						{#if copied === 'shipyard-json'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.shipyardJson}</pre>
			</div>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Field</th><th>Type</th><th>Default</th><th>Description</th></tr>
					</thead>
					<tbody>
						<tr>
							<td><code>functions.dir</code></td>
							<td>string</td>
							<td><code>"functions"</code></td>
							<td>Directory to scan for function files</td>
						</tr>
						<tr>
							<td><code>functions.runtime</code></td>
							<td>string</td>
							<td><code>"deno"</code></td>
							<td>Only <code>deno</code> is supported currently</td>
						</tr>
						<tr>
							<td><code>functions.timeout_ms</code></td>
							<td>integer</td>
							<td><code>10000</code></td>
							<td>Per-request timeout in milliseconds (max 30 000)</td>
						</tr>
						<tr>
							<td><code>functions.env</code></td>
							<td>string[]</td>
							<td><code>[]</code></td>
							<td>
								Allowlist of environment variable names to inject into the runtime.
								Variables not on this list are not accessible inside functions.
							</td>
						</tr>
						<tr>
							<td><code>functions.entries</code></td>
							<td>object</td>
							<td><code>{'{}'}</code></td>
							<td>
								Explicit name → path mapping. When set, overrides directory scanning.
								Useful for functions nested in subdirectories.
							</td>
						</tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-tip">
				Use <code>entries</code> when your functions live deeper in the repo (e.g.
				<code>src/edge/mailer.ts</code>) or when you want to control the public function
				name independently from the filename.
			</div>
		</section>

		<!-- ── Environment Variables ──────────────────────────────────── -->
		<section id="env-vars">
			<h2>Environment Variables</h2>
			<p>
				Set environment variables for your edge function group in the Shipyard
				dashboard under <strong>Edge Functions → Settings → Environment</strong>.
				Variables are encrypted at rest and injected into the Deno runtime on deploy.
			</p>

			<p>
				Inside your function, access them via <code>Deno.env.get()</code>.
				Only variables listed in <code>functions.env</code> in <code>shipyard.json</code>
				(or all variables if <code>env</code> is omitted) are passed through.
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">typescript — accessing env vars</span>
					<button class="copy-btn" onclick={() => copyCode('env-access', snip.envAccess)}>
						{#if copied === 'env-access'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.envAccess}</pre>
			</div>

			<div class="callout callout-warn">
				Never log or return the values of secrets in your response body. Edge function
				logs are visible to all members of your organization.
			</div>
		</section>

		<!-- ── Invoking Functions ─────────────────────────────────────── -->
		<section id="invoking">
			<h2>Invoking Functions</h2>
			<p>
				Functions can be invoked via two URL patterns: the built-in path-based URL
				(always available) and a custom domain (optional, see next section).
			</p>

			<h3>Path-based URL</h3>
			<p>
				Every function is reachable under <code>/fn/&lt;org-slug&gt;/&lt;fn-name&gt;</code>
				on your Shipyard domain — no configuration required.
			</p>

			<div class="code-block">
				<div class="code-header"><span class="code-label">URL format</span></div>
				<pre>https://&lt;your-shipyard&gt;/fn/&lt;org-slug&gt;/&lt;fn-name&gt;[/extra/path][?query]</pre>
			</div>

			<p>
				All HTTP methods are forwarded verbatim — GET, POST, PUT, PATCH, DELETE, and
				OPTIONS. Query parameters, request headers (excluding hop-by-hop headers), and
				the request body are passed through unchanged.
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash — GET with query params</span>
					<button class="copy-btn" onclick={() => copyCode('invoke-get2', snip.invokeGet)}>
						{#if copied === 'invoke-get2'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.invokeGet}</pre>
			</div>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash — POST with JSON body</span>
					<button class="copy-btn" onclick={() => copyCode('invoke-post', snip.invokePost)}>
						{#if copied === 'invoke-post'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.invokePost}</pre>
			</div>

			<h3>Error responses</h3>
			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Status</th><th>Meaning</th></tr>
					</thead>
					<tbody>
						<tr><td><span class="status-badge s404">404</span></td><td>Organization slug not found or function name not deployed</td></tr>
						<tr><td><span class="status-badge s502">502</span></td><td>Edge runtime is not running for this organization</td></tr>
						<tr><td><span class="status-badge s504">504</span></td><td>Function exceeded its <code>timeout_ms</code> limit</td></tr>
					</tbody>
				</table>
			</div>
		</section>

		<!-- ── Custom Domains ─────────────────────────────────────────── -->
		<section id="custom-domains">
			<h2>Custom Domains</h2>
			<p>
				Attach a custom domain to your edge function group so functions are
				reachable directly at your domain — without the <code>/fn/&lt;org&gt;/</code> prefix.
			</p>

			<h3>Adding a domain</h3>
			<ol class="ordered-list">
				<li>Go to <strong>Edge Functions → Domains → Add Domain</strong> in the dashboard.</li>
				<li>Enter your custom hostname (e.g. <code>api.acme.com</code>).</li>
				<li>Add a CNAME record pointing to the Shipyard edge runtime for your organization.</li>
				<li>TLS is provisioned automatically via Let's Encrypt once DNS propagates.</li>
			</ol>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">DNS — CNAME record</span>
					<button class="copy-btn" onclick={() => copyCode('dns-record', snip.dnsRecord)}>
						{#if copied === 'dns-record'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.dnsRecord}</pre>
			</div>

			<p>
				Once the domain is active, functions are invoked at the root path — the
				function name becomes the first path segment.
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash — invoke via custom domain</span>
					<button class="copy-btn" onclick={() => copyCode('invoke-custom', snip.invokeCustom)}>
						{#if copied === 'invoke-custom'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.invokeCustom}</pre>
			</div>

			<div class="callout callout-info">
				Custom domain routing is handled by Traefik. Each domain gets its own dynamic
				Traefik configuration rule (<code>Host(...)</code>) that routes all traffic
				directly to the edge runtime — bypassing the <code>/fn/</code> path entirely.
			</div>

			<h3>Removing a domain</h3>
			<p>
				Removing a domain from the dashboard immediately removes the Traefik routing rule.
				Existing DNS records are not affected — you must remove the CNAME yourself if
				you no longer need it.
			</p>
		</section>

		<!-- ── Deployment History ─────────────────────────────────────── -->
		<section id="deploy-history">
			<h2>Deployment History</h2>
			<p>
				Every time you click <strong>Deploy</strong> in the dashboard, Shipyard
				creates an immutable deployment record for each function it detects. You can
				browse the full history, view the exact code from any past deployment, and
				restore any previous version in one click.
			</p>

			<h3>Deployment statuses</h3>
			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Status</th><th>Meaning</th></tr>
					</thead>
					<tbody>
						<tr>
							<td><span class="dep-badge dep-live">live</span></td>
							<td>Currently serving traffic — the most recent successful deployment</td>
						</tr>
						<tr>
							<td><span class="dep-badge dep-pending">pending</span></td>
							<td>Deploy in progress (git clone + function scan running)</td>
						</tr>
						<tr>
							<td><span class="dep-badge dep-error">error</span></td>
							<td>Deploy failed (check the error message in the history row)</td>
						</tr>
						<tr>
							<td><span class="dep-badge dep-rolled-back">rolled_back</span></td>
							<td>Superseded by a newer deployment or a manual rollback</td>
						</tr>
					</tbody>
				</table>
			</div>

			<h3>Viewing code from a past deployment</h3>
			<p>
				In the dashboard, expand any function in the Edge Functions panel and click
				<strong>History</strong>. Each row shows the commit SHA, deploy time, and status.
				Click <strong>View</strong> on any row to open a read-only code viewer showing
				the exact code bundle from that deployment.
			</p>

			<h3>Restoring a previous deployment (rollback)</h3>
			<p>
				Click <strong>Restore</strong> on any past deployment row to make it live again.
				Shipyard copies the code bundle from that snapshot into a new deployment record
				with status <code>live</code> and marks the previous live deployment as
				<code>rolled_back</code>. The rollback takes effect immediately — no rebuild or
				git clone required.
			</p>

			<div class="callout callout-tip">
				Rollbacks are instant because the code bundle is stored alongside each deployment
				record — no git history access is needed at restore time.
			</div>

			<h3>Deploy report</h3>
			<p>
				After every deploy, the dashboard shows a summary of what happened:
			</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Field</th><th>Meaning</th></tr>
					</thead>
					<tbody>
						<tr><td><code>deployed</code></td><td>Functions that were detected and successfully deployed</td></tr>
						<tr><td><code>skipped</code></td><td>Files that were scanned but had no <code>export default</code></td></tr>
						<tr><td><code>failed</code></td><td>Files that were detected but failed to compile or load</td></tr>
						<tr><td><code>deleted</code></td><td>Functions present in the previous deploy but no longer in the repo</td></tr>
					</tbody>
				</table>
			</div>
		</section>

		<!-- ── Limits & Quotas ────────────────────────────────────────── -->
		<section id="limits">
			<h2>Limits & Quotas</h2>
			<p>
				Limits are enforced per organization. Contact support to request higher limits
				on paid plans.
			</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Limit</th><th>Free</th><th>Pro</th></tr>
					</thead>
					<tbody>
						<tr>
							<td>Edge function groups</td>
							<td>1</td>
							<td>Unlimited</td>
						</tr>
						<tr>
							<td>Functions per group</td>
							<td>5</td>
							<td>100</td>
						</tr>
						<tr>
							<td>Custom domains per group</td>
							<td>0</td>
							<td>10</td>
						</tr>
						<tr>
							<td>Deployment history retained</td>
							<td>10 per function</td>
							<td>Unlimited</td>
						</tr>
						<tr>
							<td>Request timeout (<code>timeout_ms</code> max)</td>
							<td>10 000 ms</td>
							<td>30 000 ms</td>
						</tr>
						<tr>
							<td>Max request body size</td>
							<td>1 MB</td>
							<td>10 MB</td>
						</tr>
						<tr>
							<td>Concurrent requests per runtime</td>
							<td>50</td>
							<td>500</td>
						</tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-warn">
				The free plan does not support custom domains. Upgrade to Pro to attach your own
				hostname to an edge function group.
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

	.ordered-list {
		margin: 12px 0 14px 20px;
		display: flex; flex-direction: column; gap: 8px;
		color: rgba(255,255,255,0.6); font-size: 14.5px;
	}
	.ordered-list li { padding-left: 4px; }
	.ordered-list li code { font-size: 12.5px; }

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

	/* ── Status code badges ──────────────────────────────────────── */
	.status-badge {
		display: inline-block; padding: 2px 7px;
		font-size: 11px; font-weight: 700; border-radius: 4px;
		font-family: 'Fira Code', monospace;
	}
	.s404 { background: rgba(148,163,184,0.15); color: #94a3b8; }
	.s502 { background: rgba(239,68,68,0.15);   color: #fca5a5; }
	.s504 { background: rgba(234,179,8,0.15);   color: #fde68a; }

	/* ── Deployment status badges ────────────────────────────────── */
	.dep-badge {
		display: inline-block; padding: 2px 8px;
		font-size: 11px; font-weight: 700; border-radius: 999px;
		font-family: 'Fira Code', monospace;
	}
	.dep-live        { background: rgba(34,197,94,0.15);   color: #86efac; }
	.dep-pending     { background: rgba(234,179,8,0.15);   color: #fde68a; }
	.dep-error       { background: rgba(239,68,68,0.15);   color: #fca5a5; }
	.dep-rolled-back { background: rgba(148,163,184,0.15); color: #94a3b8; }

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
	}
	@media (max-width: 480px) {
		h1 { font-size: 1.6rem; }
		h2 { font-size: 1.25rem; }
		.content { padding: 24px 16px 80px; }
	}
</style>
