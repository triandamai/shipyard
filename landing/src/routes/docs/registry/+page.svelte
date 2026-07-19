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
				{ id: 'overview',    label: 'Overview' },
				{ id: 'quickstart',  label: 'Quick Start' },
			],
		},
		{
			group: 'Authentication',
			items: [
				{ id: 'api-keys',    label: 'API Keys' },
				{ id: 'docker-login', label: 'docker login' },
			],
		},
		{
			group: 'Usage',
			items: [
				{ id: 'image-names',  label: 'Image Naming' },
				{ id: 'push-pull',    label: 'Push & Pull' },
				{ id: 'ci-cd',        label: 'CI/CD Integration' },
			],
		},
		{
			group: 'Configuration',
			items: [
				{ id: 'setup',        label: 'Registry Setup' },
				{ id: 'traefik',      label: 'Traefik Routing' },
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
		dockerLogin:
`docker login registry.your-domain.com \\
  -u anyuser \\
  -p ship_xxxxxxxxxxxxxxxxxxxx`,

		imageFormat:
`# Format
registry.your-domain.com/<org-slug>/<project-slug>/<repo>:<tag>

# Examples
registry.your-domain.com/acme/backend/api:latest
registry.your-domain.com/acme/backend/api:v1.2.3
registry.your-domain.com/acme/frontend/web:sha-abc1234`,

		pushPull:
`# Tag a local image
docker tag myapp:latest registry.your-domain.com/acme/backend/api:latest

# Push
docker push registry.your-domain.com/acme/backend/api:latest

# Pull
docker pull registry.your-domain.com/acme/backend/api:latest`,

		ghActions:
`name: Build and push to Shipyard registry

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
          password: \${{ secrets.SHIPYARD_REGISTRY_KEY }}

      - name: Build and push
        uses: docker/build-push-action@v5
        with:
          push: true
          tags: registry.your-domain.com/acme/backend/api:latest`,

		glCI:
`variables:
  IMAGE: registry.your-domain.com/acme/backend/api

before_script:
  - docker login $CI_REGISTRY -u ci -p $SHIPYARD_REGISTRY_KEY

build:
  script:
    - docker build -t $IMAGE:$CI_COMMIT_SHORT_SHA .
    - docker push $IMAGE:$CI_COMMIT_SHORT_SHA`,

		envConfig:
`SHIPYARD__REGISTRY__HOSTNAME=registry.your-domain.com`,

		serviceImage:
`# In a service's environment variables, override the image at deploy time
__IMAGE__=registry.your-domain.com/acme/backend/api:v1.2.3`,
	};
</script>

<svelte:head>
	<title>Container Registry — Shipyard Docs</title>
	<meta name="description" content="Use Shipyard's built-in OCI-compatible container registry to push, pull, and deploy Docker images with API key authentication." />
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
			<a href="/docs/edge-functions" class="topbar-link">Edge Functions</a>
			<a href="/docs/registry" class="topbar-link active">Registry</a>
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

		<!-- ── Overview ─────────────────────────────────────────────── -->
		<section id="overview">
			<div class="page-eyebrow">Container Registry · OCI Distribution Spec</div>
			<h1>Container Registry</h1>
			<p>
				Shipyard includes a built-in OCI-compatible container registry. Push images from
				your local machine or CI pipeline, then deploy them to your Swarm services with
				zero external dependencies. The registry speaks the standard Docker Distribution
				API — any tool that works with Docker Hub or GHCR works here too.
			</p>

			<div class="quick-cards">
				<div class="quick-card">
					<div class="qc-label">Registry URL</div>
					<code>registry.your-domain.com</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Auth</div>
					<code>API key (ship_...)</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Protocol</div>
					<code>OCI Distribution Spec v1.1</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Image path</div>
					<code>org/project/repo:tag</code>
				</div>
			</div>
		</section>

		<!-- ── Quick Start ──────────────────────────────────────────── -->
		<section id="quickstart">
			<h2>Quick Start</h2>

			<div class="steps-list">
				<div class="step-item"><span class="step-num">1</span><span>Create an API key with <code>registry:manage</code> scope in <strong>Settings → API Keys</strong></span></div>
				<div class="step-item"><span class="step-num">2</span><span>Run <code>docker login registry.your-domain.com -u anyuser -p ship_xxx</code></span></div>
				<div class="step-item"><span class="step-num">3</span><span>Tag your image: <code>docker tag myapp registry.your-domain.com/acme/myproject/myapp:latest</code></span></div>
				<div class="step-item"><span class="step-num">4</span><span>Push: <code>docker push registry.your-domain.com/acme/myproject/myapp:latest</code></span></div>
				<div class="step-item"><span class="step-num">5</span><span>In a Shipyard service, set the image to the full registry URL and deploy</span></div>
			</div>
		</section>

		<!-- ── API Keys ─────────────────────────────────────────────── -->
		<section id="api-keys">
			<h2>API Keys</h2>
			<p>
				The registry uses Shipyard API keys for authentication — not separate registry
				credentials. Keys start with <code>ship_</code> and are scoped to control
				exactly what the holder can do.
			</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Scope</th><th>Allows</th></tr>
					</thead>
					<tbody>
						<tr>
							<td><code>registry:view</code></td>
							<td>Pull images only — read-only access to all repositories in the organization</td>
						</tr>
						<tr>
							<td><code>registry:manage</code></td>
							<td>Pull <em>and</em> push — full read/write access; also includes <code>registry:view</code></td>
						</tr>
					</tbody>
				</table>
			</div>

			<h3>Creating a key</h3>
			<ol>
				<li>Go to <strong>Settings → API Keys</strong></li>
				<li>Click <strong>New API key</strong></li>
				<li>Give the key a name (e.g. <em>CI Push</em> or <em>Read-only pull</em>)</li>
				<li>Select <code>registry:manage</code> (push + pull) or <code>registry:view</code> (pull only)</li>
				<li>Optionally set an expiry date</li>
				<li>Copy the key immediately — it is shown only once</li>
			</ol>

			<div class="callout callout-warn">
				Keys are stored as SHA-256 hashes. If you lose a key, revoke it and create a new one.
				Never commit keys to source control — use CI/CD secrets instead.
			</div>
		</section>

		<!-- ── docker login ─────────────────────────────────────────── -->
		<section id="docker-login">
			<h2>docker login</h2>
			<p>
				The registry implements the Docker token authentication spec. Docker automatically
				negotiates a short-lived JWT when you run <code>docker login</code> — you never
				interact with the token directly.
			</p>

			<h3>Login command</h3>
			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash</span>
					<button class="copy-btn" onclick={() => copyCode('docker-login', snip.dockerLogin)}>
						{#if copied === 'docker-login'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.dockerLogin}</pre>
			</div>

			<div class="callout callout-info">
				The <strong>username is ignored</strong> — pass any non-empty string. Only the
				password (your <code>ship_...</code> API key) is used for authentication.
			</div>

			<h3>How it works</h3>
			<ol>
				<li>Docker sends <code>GET /v2/</code> to check the registry</li>
				<li>The registry returns <code>401 Unauthorized</code> with a <code>WWW-Authenticate</code> header pointing to <code>/auth/registry/token</code></li>
				<li>Docker sends Basic auth (username + API key) to the token endpoint</li>
				<li>Shipyard validates the API key and returns a signed 15-minute JWT</li>
				<li>Docker uses the JWT for all subsequent push and pull requests</li>
			</ol>

			<div class="callout callout-tip">
				Credentials are cached in <code>~/.docker/config.json</code>. You only need to
				run <code>docker login</code> once per machine unless you rotate the key.
			</div>
		</section>

		<!-- ── Image Naming ─────────────────────────────────────────── -->
		<section id="image-names">
			<h2>Image Naming</h2>
			<p>
				Images are namespaced under your organization slug, then a project slug, then a
				repository name. This mirrors the Shipyard project hierarchy and prevents name
				collisions between organizations.
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">format</span>
					<button class="copy-btn" onclick={() => copyCode('image-format', snip.imageFormat)}>
						{#if copied === 'image-format'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.imageFormat}</pre>
			</div>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Segment</th><th>What it maps to</th><th>Example</th></tr>
					</thead>
					<tbody>
						<tr>
							<td><code>org-slug</code></td>
							<td>Your organization's URL slug</td>
							<td><code>acme</code></td>
						</tr>
						<tr>
							<td><code>project-slug</code></td>
							<td>The project containing the service</td>
							<td><code>backend</code></td>
						</tr>
						<tr>
							<td><code>repo</code></td>
							<td>Arbitrary repository name — usually the service name</td>
							<td><code>api</code></td>
						</tr>
						<tr>
							<td><code>tag</code></td>
							<td>Image tag — <code>latest</code>, a version, or a git SHA</td>
							<td><code>v1.2.3</code></td>
						</tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-info">
				The org and project slugs are visible in the Shipyard URL when you navigate to a
				project: <code>https://ship.example.com/orgs/<strong>acme</strong>/projects/<strong>backend</strong></code>
			</div>
		</section>

		<!-- ── Push & Pull ──────────────────────────────────────────── -->
		<section id="push-pull">
			<h2>Push & Pull</h2>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash</span>
					<button class="copy-btn" onclick={() => copyCode('push-pull', snip.pushPull)}>
						{#if copied === 'push-pull'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.pushPull}</pre>
			</div>

			<h3>Deploying a pushed image</h3>
			<p>
				After pushing, point a Shipyard service at the full registry URL. The simplest
				way is to set the <strong>Image</strong> field in the service settings to the
				full registry path and click <strong>Deploy</strong>.
			</p>
			<p>
				You can also override the image per-deployment via the special environment
				variable <code>__IMAGE__</code>:
			</p>
			<div class="code-block">
				<div class="code-header">
					<span class="code-label">service env vars</span>
					<button class="copy-btn" onclick={() => copyCode('service-image', snip.serviceImage)}>
						{#if copied === 'service-image'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.serviceImage}</pre>
			</div>

			<h3>Private registry credentials in Swarm</h3>
			<p>
				For services pulling from the registry at deploy time, Shipyard automatically
				injects the registry credentials as a Docker Swarm secret so worker nodes can
				pull without separate <code>docker login</code> calls. No extra configuration
				is needed on worker nodes.
			</p>
		</section>

		<!-- ── CI/CD Integration ────────────────────────────────────── -->
		<section id="ci-cd">
			<h2>CI/CD Integration</h2>
			<p>
				Store your API key as a CI secret and use it to authenticate during your build
				pipeline. Combine with a webhook trigger to redeploy after every successful push.
			</p>

			<h3>GitHub Actions</h3>
			<p>Add <code>SHIPYARD_REGISTRY_KEY</code> to your repository secrets, then:</p>
			<div class="code-block">
				<div class="code-header">
					<span class="code-label">yaml — .github/workflows/build.yml</span>
					<button class="copy-btn" onclick={() => copyCode('gh-actions', snip.ghActions)}>
						{#if copied === 'gh-actions'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.ghActions}</pre>
			</div>

			<h3>GitLab CI</h3>
			<div class="code-block">
				<div class="code-header">
					<span class="code-label">yaml — .gitlab-ci.yml</span>
					<button class="copy-btn" onclick={() => copyCode('gl-ci', snip.glCI)}>
						{#if copied === 'gl-ci'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.glCI}</pre>
			</div>

			<div class="callout callout-tip">
				After pushing a new image tag, trigger a Shipyard service deployment automatically
				with a <a href="/docs#webhooks" class="inline-link">webhook trigger</a> — no
				manual clicking required.
			</div>
		</section>

		<!-- ── Registry Setup ───────────────────────────────────────── -->
		<section id="setup">
			<h2>Registry Setup</h2>
			<p>
				The registry is bundled with Shipyard and enabled by default. To make it
				reachable via its own hostname (recommended), set one environment variable in
				your <code>/opt/shipyard/.env</code> file:
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">/opt/shipyard/.env</span>
					<button class="copy-btn" onclick={() => copyCode('env-config', snip.envConfig)}>
						{#if copied === 'env-config'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.envConfig}</pre>
			</div>

			<h3>DNS setup</h3>
			<p>
				Point your registry subdomain at the same server IP as your main Shipyard domain.
				An <strong>A record</strong> (or CNAME to your main domain) is sufficient:
			</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Record type</th><th>Name</th><th>Value</th></tr>
					</thead>
					<tbody>
						<tr>
							<td><code>A</code></td>
							<td><code>registry</code></td>
							<td>Your server's public IP</td>
						</tr>
					</tbody>
				</table>
			</div>

			<p>
				TLS is provisioned automatically by Traefik via Let's Encrypt — no manual
				certificate management needed.
			</p>

			<div class="callout callout-info">
				Restart the Shipyard stack after changing <code>.env</code>:
				<code>cd /opt/shipyard &amp;&amp; docker compose restart shipyard-backend</code>
			</div>
		</section>

		<!-- ── Traefik Routing ───────────────────────────────────────── -->
		<section id="traefik">
			<h2>Traefik Routing</h2>
			<p>
				Shipyard auto-generates a Traefik dynamic configuration on every startup.
				The registry gets its own router rule that adds the <code>/registry</code>
				path prefix before forwarding to the backend — so the standard OCI paths
				(<code>/v2/…</code>) work transparently at the registry hostname.
			</p>

			<div class="code-block">
				<div class="code-header"><span class="code-label">generated traefik config (excerpt)</span></div>
				<pre>http:
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
        prefix: "/registry"</pre>
			</div>

			<p>
				This means <code>registry.your-domain.com/v2/…</code> is routed to the backend
				as <code>/registry/v2/…</code>. The Traefik config file is re-written on every
				backend startup, so it is self-healing — if the file is lost the routes are
				restored automatically.
			</p>

			<div class="callout callout-info">
				View the current generated config in the dashboard under
				<strong>Settings → Traefik → Dynamic</strong>.
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

	ul, ol {
		color: rgba(255,255,255,0.6); font-size: 14.5px;
		padding-left: 20px; display: flex; flex-direction: column; gap: 6px;
		margin-bottom: 14px;
	}
	li { line-height: 1.65; }

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

	/* ── Deployment steps list ───────────────────────────────────── */
	.steps-list {
		display: flex; flex-direction: column; gap: 0;
		border: 1px solid rgba(255,255,255,0.08); border-radius: 8px;
		overflow: hidden; margin: 16px 0;
	}
	.step-item {
		display: flex; align-items: center; gap: 12px;
		padding: 10px 16px; font-size: 13.5px; color: rgba(255,255,255,0.6);
		border-bottom: 1px solid rgba(255,255,255,0.05);
	}
	.step-item:last-child { border-bottom: none; }
	.step-num {
		width: 24px; height: 24px; border-radius: 50%;
		background: rgba(59,130,246,0.15); border: 1px solid rgba(59,130,246,0.3);
		color: #60a5fa; font-size: 11px; font-weight: 700;
		display: flex; align-items: center; justify-content: center; flex-shrink: 0;
	}

	/* ── Inline link ─────────────────────────────────────────────── */
	.inline-link {
		color: #60a5fa; text-decoration: underline; text-underline-offset: 3px;
	}
	.inline-link:hover { color: #93c5fd; }

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
