<script lang="ts">
	import { onMount } from 'svelte';
	import { SHIPYARD_VERSION } from '$lib/version';
	import '$lib/styles/tokens.css';

	const GITHUB = 'https://github.com/triandamai/shipyard';
	const DESCRIPTION =
		'Shipyard is an open-source platform you install on your own server. Deploy from Git or any Docker image, get HTTPS on your domain, roll back in one click, and see every service on a live canvas.';

	let installCmd = $state('curl -fsSL https://shipyard.trian.space/install.sh | sudo bash');
	let copied = $state(false);

	// Hero: one deploy plays out on the canvas once, then rests in its final state.
	const DEPLOY_STEPS = ['Pull image', 'Apply variables', 'Mount volumes', 'Update service'];
	let doneSteps = $state(0);
	let live = $state(false);

	// The canvas is drawn at 600×500 and scaled to fit its column.
	let canvasWrap: HTMLDivElement | undefined = $state();
	let scale = $state(1);

	onMount(() => {
		installCmd = `curl -fsSL ${location.origin}/install.sh | sudo bash`;

		const ro = new ResizeObserver(([entry]) => {
			scale = Math.min(1, entry.contentRect.width / 600);
		});
		if (canvasWrap) ro.observe(canvasWrap);

		const timers: ReturnType<typeof setTimeout>[] = [];
		if (matchMedia('(prefers-reduced-motion: reduce)').matches) {
			doneSteps = DEPLOY_STEPS.length;
			live = true;
		} else {
			DEPLOY_STEPS.forEach((_, i) => timers.push(setTimeout(() => (doneSteps = i + 1), 900 + i * 700)));
			timers.push(setTimeout(() => (live = true), 900 + DEPLOY_STEPS.length * 700));
		}
		return () => {
			ro.disconnect();
			timers.forEach(clearTimeout);
		};
	});

	async function copyInstall() {
		try {
			await navigator.clipboard.writeText(installCmd);
			copied = true;
			setTimeout(() => (copied = false), 1600);
		} catch {
			/* clipboard unavailable: the command stays selectable */
		}
	}

	const builtOn = ['Docker Swarm', 'Traefik', "Let's Encrypt", 'PostgreSQL', 'Rust', 'SvelteKit'];

	const featureGroups = [
		{
			title: 'Deploy',
			items: [
				['From Git', 'Push to deploy. Shipyard builds from your Dockerfile or with Nixpacks on your server.'],
				['From any image', 'Run anything from Docker Hub, GHCR or Shipyard’s own built-in registry.'],
				['Docker Compose import', 'Paste a Compose file and get managed services, networks and volumes.'],
				['One-click rollback', 'Every deploy records its exact image digest, so going back is instant.'],
				['API keys and webhooks', 'Trigger deploys from GitHub Actions, scripts or another platform.']
			]
		},
		{
			title: 'Run',
			items: [
				['Automatic HTTPS', 'Add a domain and Traefik issues a Let’s Encrypt certificate for it.'],
				['Persistent volumes', 'Databases keep their data across redeploys, restarts and updates.'],
				['Resource limits', 'Cap CPU and memory per service so one container can’t starve the rest.'],
				['Multi-node Swarm', 'Join more servers with one script. Swarm spreads and reschedules work.'],
				['Edge functions and sandboxes', 'Run TypeScript functions on Deno, or build small apps in the browser.']
			]
		},
		{
			title: 'Operate',
			items: [
				['Live topology canvas', 'Services, volumes and domains on one canvas, updated as they change.'],
				['Logs, terminal and metrics', 'Stream logs, open a shell in a container, watch CPU and memory.'],
				['Roles and permissions', 'Owner, admin, member and viewer, per organization and per project.'],
				['Audit log', 'Who deployed what, when and from where, for every action.'],
				['Docs for people and LLMs', 'Every docs page is also plain Markdown, indexed by llms.txt.']
			]
		}
	];
</script>

<svelte:head>
	<title>Shipyard · Deploy apps to servers you own</title>
	<meta name="description" content={DESCRIPTION} />
	<link rel="canonical" href="https://shipyard.trian.space/" />
	<meta property="og:url" content="https://shipyard.trian.space/" />
	<meta property="og:title" content="Shipyard · Deploy apps to servers you own" />
	<meta property="og:description" content={DESCRIPTION} />
	<meta name="twitter:title" content="Shipyard · Deploy apps to servers you own" />
	<meta name="twitter:description" content={DESCRIPTION} />
</svelte:head>

<div class="site">
	<a class="skip" href="#main">Skip to content</a>

	<header class="nav">
		<div class="wrap nav-inner">
			<a class="brand" href="/" aria-label="Shipyard home">
				<span class="mark" aria-hidden="true">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="5" r="3"></circle><line x1="12" y1="22" x2="12" y2="8"></line><path d="M5 12H2a10 10 0 0 0 20 0h-3"></path></svg>
				</span>
				Shipyard
			</a>
			<nav aria-label="Main">
				<ul class="nav-links">
					<li><a href="#features">Features</a></li>
					<li><a href="#how">How it works</a></li>
					<li><a href="/docs">Docs</a></li>
					<li><a href="/docs/api">API</a></li>
					<li><a href={GITHUB} rel="noopener noreferrer">GitHub</a></li>
				</ul>
			</nav>
			<a class="btn btn-primary btn-sm" href="#install">Install</a>
		</div>
	</header>

	<main id="main">
		<!-- Hero -->
		<section class="hero wrap">
			<div class="hero-copy">
				<h1>Deploy apps to servers you own.</h1>
				<p class="lead">
					Shipyard is an open-source platform you install on your own server. Deploy from Git or any
					Docker image, get HTTPS on your domain, roll back in one click, and see every service on a
					live canvas.
				</p>
				<div class="hero-actions">
					<a class="btn btn-primary" href="#install">Install Shipyard</a>
					<a class="btn btn-secondary" href="/docs">Read the docs</a>
				</div>
				<div class="cmd">
					<code>{installCmd}</code>
					<button type="button" onclick={copyInstall} aria-label="Copy install command">{copied ? 'Copied' : 'Copy'}</button>
				</div>
				<p class="fine">Free and open source under the MIT license. Current release v{SHIPYARD_VERSION}.</p>
			</div>

			<div class="hero-visual" bind:this={canvasWrap} style="height: {500 * scale}px" aria-label="A project on the Shipyard canvas while its api service deploys" role="img">
				<div class="canvas" style="transform: scale({scale})">
					<div class="edge h" style="left:170px; top:66px; width:40px"></div>
					<div class="edge v" style="left:297px; top:116px; height:72px"></div>
					<div class="edge h" style="left:385px; top:232px; width:30px"></div>
					<div class="edge v dashed" style="left:497px; top:96px; height:92px"></div>
					<div class="edge v" style="left:355px; top:282px; height:102px"></div>
					<div class="edge h" style="left:355px; top:384px; width:60px"></div>

					<div class="node portal" style="left:20px; top:40px; width:150px">
						<span class="domain">shop.acme.io</span>
						<span class="sub">HTTPS</span>
					</div>

					<div class="node" style="left:210px; top:24px; width:175px">
						<div class="node-head"><span class="tile">JS</span><span class="name">web<small>git · main</small></span></div>
						<div class="node-foot"><span class="state"><i class="dot run"></i>Running</span><span class="mono">2/2</span></div>
					</div>

					<div class="node" class:selected={!live} style="left:210px; top:188px; width:175px">
						<div class="node-head"><span class="tile">RS</span><span class="name">api<small>git · a91f2c0</small></span></div>
						<div class="node-foot">
							{#if live}
								<span class="state"><i class="dot run"></i>Running</span>
							{:else}
								<span class="state deploying"><i class="dot dep"></i>Deploying</span>
							{/if}
							<span class="mono">:8080</span>
						</div>
						{#if !live}<div class="bar"><span style="width:{(doneSteps / DEPLOY_STEPS.length) * 100}%"></span></div>{/if}
					</div>

					<div class="node volume" style="left:415px; top:40px; width:165px">
						<span class="name">pgdata<small>persistent volume</small></span>
					</div>

					<div class="node" style="left:415px; top:188px; width:165px">
						<div class="node-head"><span class="tile">PG</span><span class="name">postgres<small>postgres:16</small></span></div>
						<div class="node-foot"><span class="state"><i class="dot run"></i>Running</span><span class="mono">1/1</span></div>
					</div>

					<div class="node" style="left:415px; top:340px; width:165px">
						<div class="node-head"><span class="tile">RD</span><span class="name">redis<small>redis:7</small></span></div>
						<div class="node-foot"><span class="state"><i class="dot run"></i>Running</span><span class="mono">1/1</span></div>
					</div>

					<div class="deploy-card">
						<p class="deploy-title">{live ? 'api is live' : 'Deploying api'}</p>
						<ol>
							{#each DEPLOY_STEPS as step, i (step)}
								<li class:done={i < doneSteps}>
									<span class="tick" aria-hidden="true">{i < doneSteps ? '✓' : ''}</span>{step}
								</li>
							{/each}
						</ol>
						<p class="deploy-foot mono">{live ? 'Deployed in 41s · rollback ready' : 'step ' + Math.min(doneSteps + 1, DEPLOY_STEPS.length) + ' of ' + DEPLOY_STEPS.length}</p>
					</div>
				</div>
			</div>
		</section>

		<!-- Built on -->
		<section class="wrap built" aria-labelledby="built-title">
			<p id="built-title">Built on open-source infrastructure you already trust</p>
			<ul>
				{#each builtOn as name (name)}<li>{name}</li>{/each}
			</ul>
		</section>

		<!-- Pillars -->
		<section class="wrap pillars" aria-label="Why Shipyard">
			<div>
				<h2>Runs on your servers</h2>
				<p>Install on any Linux VPS with Docker. Your code, data and logs stay on machines you control, and there’s no per-seat bill. Add servers later and Swarm spreads the work.</p>
			</div>
			<div>
				<h2>Deploys that keep your data</h2>
				<p>Databases get a persistent volume automatically, and stateful services stop before they restart, so a redeploy or a Shipyard update never brings them back empty.</p>
			</div>
			<div>
				<h2>Every service, live</h2>
				<p>Services, volumes and domains sit on one canvas with their status, logs and resource use streamed as they change, so you see a failing deploy before your users do.</p>
			</div>
		</section>

		<!-- How it works -->
		<section class="how" id="how" aria-labelledby="how-title">
			<div class="wrap">
				<div class="how-head">
					<h2 id="how-title">From a fresh server to a live app in three steps</h2>
					<p>No control plane to rent and nothing to sign up for. Shipyard installs next to your apps and manages them from there.</p>
				</div>

				<ol class="steps">
					<li class="step">
						<div class="frag term" aria-hidden="true">
							<div class="term-bar">install.sh</div>
							<pre><span class="dim">$</span> curl -fsSL …/install.sh | sudo bash
<span class="ok">✔</span> Docker 26.1 and Compose v2.27 found
<span class="dim">?</span> Domain: ship.acme.io
<span class="dim">?</span> Enable HTTPS? Y
<span class="ok">✔</span> Secrets generated, config written
<span class="ok">✔</span> Stack started</pre>
						</div>
						<div class="step-text">
							<span class="num">1</span>
							<h3>Install on your server</h3>
							<p>Run one command on any Linux machine with Docker. It writes the config, generates secrets, sets up Traefik and HTTPS, and starts the stack.</p>
						</div>
					</li>
					<li class="step">
						<div class="frag form" aria-hidden="true">
							<p class="frag-title">Create your organization</p>
							<div class="field"><span>Name</span><span class="val">Acme</span></div>
							<div class="field"><span>URL</span><span class="val mono">ship.acme.io/orgs/acme</span></div>
							<div class="field"><span>Invite</span><span class="val">ops@acme.io <em>Admin</em></span></div>
						</div>
						<div class="step-text">
							<span class="num">2</span>
							<h3>Create your organization</h3>
							<p>Open your domain, make the admin account and your first organization, and invite your team with roles from owner to viewer.</p>
						</div>
					</li>
					<li class="step">
						<div class="frag form" aria-hidden="true">
							<p class="frag-title">New service</p>
							<div class="field"><span>Image</span><span class="val mono">ghcr.io/acme/web:latest</span></div>
							<div class="field"><span>Domain</span><span class="val mono">shop.acme.io</span></div>
							<div class="frag-actions"><span class="fake-btn">Deploy</span></div>
						</div>
						<div class="step-text">
							<span class="num">3</span>
							<h3>Deploy your first app</h3>
							<p>Point Shipyard at a Git repo or an image, add a domain and press Deploy. Every deploy is recorded, so rolling back is one click.</p>
						</div>
					</li>
				</ol>
				<a class="btn btn-onDark" href="/docs#installation">Read the install guide</a>
			</div>
		</section>

		<!-- Features -->
		<section class="wrap features" id="features" aria-labelledby="features-title">
			<h2 id="features-title">Everything a small team needs to run production</h2>
			<div class="groups">
				{#each featureGroups as g (g.title)}
					<div class="group">
						<h3>{g.title}</h3>
						<dl>
							{#each g.items as [term, desc] (term)}
								<div><dt>{term}</dt><dd>{desc}</dd></div>
							{/each}
						</dl>
					</div>
				{/each}
			</div>
		</section>

		<!-- Install -->
		<section class="install" id="install" aria-labelledby="install-title">
			<div class="wrap install-inner">
				<div>
					<h2 id="install-title">Install it on your server</h2>
					<p>Needs Docker 24 or later and Docker Compose v2. Tested on Ubuntu 22.04+ and Debian 12+. The script installs Docker for you if it’s missing.</p>
					<div class="cmd cmd-lg">
						<code>{installCmd}</code>
						<button type="button" onclick={copyInstall} aria-label="Copy install command">{copied ? 'Copied' : 'Copy'}</button>
					</div>
					<ul class="does">
						<li>Checks Docker and Docker Compose</li>
						<li>Generates secure random secrets</li>
						<li>Writes <code>/opt/shipyard/.env</code> and <code>docker-compose.yml</code></li>
						<li>Configures Traefik, with HTTPS if you want it</li>
						<li>Pulls the images and starts the stack</li>
					</ul>
					<p class="install-links"><a href="/docs#installation">Installation guide</a> <a href={GITHUB} rel="noopener noreferrer">Source on GitHub</a></p>
				</div>
				<div class="frag term term-lg" aria-hidden="true">
					<div class="term-bar">ship.acme.io: install.sh</div>
					<pre><span class="dim">$</span> curl -fsSL https://shipyard.trian.space/install.sh | sudo bash
<span class="ok">✔</span> Docker 26.1.4 found
<span class="ok">✔</span> Docker Compose v2.27 found
<span class="dim">?</span> Domain (e.g. shipyard.example.com): ship.acme.io
<span class="dim">?</span> Enable HTTPS? [Y/n]: Y
<span class="dim">?</span> Admin email for Let's Encrypt: ops@acme.io
<span class="ok">✔</span> Secrets generated
<span class="ok">✔</span> Config written to /opt/shipyard/
<span class="ok">✔</span> Images pulled
<span class="ok">✔</span> Stack started

Open <span class="link">https://ship.acme.io</span> to finish setup.</pre>
				</div>
			</div>
		</section>
	</main>

	<footer class="footer">
		<div class="wrap footer-inner">
			<div class="footer-brand">
				<a class="brand" href="/">
					<span class="mark" aria-hidden="true">
						<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="5" r="3"></circle><line x1="12" y1="22" x2="12" y2="8"></line><path d="M5 12H2a10 10 0 0 0 20 0h-3"></path></svg>
					</span>
					Shipyard
				</a>
				<p>Open-source platform for deploying apps to servers you own. MIT license, v{SHIPYARD_VERSION}.</p>
			</div>
			<nav aria-label="Documentation">
				<p class="footer-title">Docs</p>
				<ul>
					<li><a href="/docs">Guide</a></li>
					<li><a href="/docs/api">API reference</a></li>
					<li><a href="/docs/edge-functions">Edge functions</a></li>
					<li><a href="/docs/registry">Container registry</a></li>
				</ul>
			</nav>
			<nav aria-label="Project">
				<p class="footer-title">Project</p>
				<ul>
					<li><a href={GITHUB} rel="noopener noreferrer">GitHub</a></li>
					<li><a href="{GITHUB}/releases" rel="noopener noreferrer">Releases</a></li>
					<li><a href="/llms.txt">llms.txt</a></li>
				</ul>
			</nav>
		</div>
	</footer>
</div>

<style>
	:global(body) {
		margin: 0;
		background: var(--bg-base);
	}
	.site {
		background: var(--bg-surface);
		color: var(--text-primary);
		font-family: var(--font-sans);
		font-size: 16px;
		line-height: 1.6;
		-webkit-font-smoothing: antialiased;
	}
	.site :global(*),
	.site :global(*::before),
	.site :global(*::after) {
		box-sizing: border-box;
	}
	.site :focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
	.site a {
		color: var(--link);
	}
	.wrap {
		max-width: 1200px;
		margin: 0 auto;
		padding-inline: 24px;
	}
	.mono {
		font-family: var(--font-mono);
	}
	.skip {
		position: absolute;
		left: 16px;
		top: -48px;
		z-index: 30;
		padding: 8px 12px;
		border-radius: 6px;
		background: var(--accent);
		color: var(--accent-fg) !important;
		text-decoration: none;
	}
	.skip:focus {
		top: 8px;
	}

	/* Buttons */
	.btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		min-height: 44px;
		padding: 0 20px;
		border-radius: 6px;
		border: 1px solid transparent;
		font: 500 15px var(--font-sans);
		text-decoration: none;
		white-space: nowrap;
	}
	.btn-sm {
		min-height: 36px;
		padding: 0 14px;
		font-size: 14px;
	}
	.btn-primary {
		background: var(--accent);
		color: var(--accent-fg) !important;
	}
	.btn-primary:hover {
		background: var(--accent-hover);
	}
	.btn-secondary {
		background: var(--bg-surface);
		border-color: var(--border-hover);
		color: var(--text-primary) !important;
	}
	.btn-secondary:hover {
		background: var(--bg-hover);
	}
	.btn-onDark {
		border-color: #3a3e44;
		color: #ecedee !important;
		margin-top: 8px;
	}
	.btn-onDark:hover {
		background: #22252a;
	}

	/* Nav */
	.nav {
		position: sticky;
		top: 0;
		z-index: 20;
		background: color-mix(in srgb, var(--bg-surface) 92%, transparent);
		backdrop-filter: blur(8px);
		border-bottom: 1px solid var(--border);
	}
	.nav-inner {
		display: flex;
		align-items: center;
		gap: 32px;
		min-height: 64px;
	}
	.brand {
		display: inline-flex;
		align-items: center;
		gap: 10px;
		font-weight: 600;
		font-size: 17px;
		letter-spacing: -0.01em;
		color: var(--text-primary) !important;
		text-decoration: none;
	}
	.mark {
		width: 30px;
		height: 30px;
		border-radius: 6px;
		background: var(--rail-bg);
		color: #f26b1d;
		display: inline-flex;
		align-items: center;
		justify-content: center;
	}
	nav {
		flex: 1;
	}
	.nav-links {
		display: flex;
		justify-content: center;
		gap: 4px;
		margin: 0;
		padding: 0;
		list-style: none;
	}
	.nav-links a {
		display: inline-flex;
		align-items: center;
		min-height: 40px;
		padding: 0 12px;
		border-radius: 6px;
		color: var(--text-secondary);
		text-decoration: none;
		font-size: 15px;
	}
	.nav-links a:hover {
		color: var(--text-primary);
		background: var(--bg-hover);
	}

	/* Hero */
	.hero {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1.05fr);
		gap: 56px;
		align-items: center;
		padding-block: 88px 72px;
	}
	h1 {
		margin: 0;
		font-size: clamp(44px, 6vw, 72px);
		line-height: 1.02;
		font-weight: 500;
		letter-spacing: -0.035em;
		text-wrap: balance;
	}
	.lead {
		margin: 24px 0 0;
		max-width: 52ch;
		font-size: 19px;
		line-height: 1.55;
		color: var(--text-secondary);
	}
	.hero-actions {
		display: flex;
		flex-wrap: wrap;
		gap: 12px;
		margin-top: 32px;
	}
	.cmd {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 20px;
		max-width: 520px;
		padding: 6px 6px 6px 14px;
		border-radius: 8px;
		background: var(--terminal-bg);
		border: 1px solid var(--terminal-border);
	}
	.cmd code {
		flex: 1;
		min-width: 0;
		overflow-x: auto;
		white-space: nowrap;
		font: 400 13px var(--font-mono);
		color: var(--terminal-fg);
		scrollbar-width: none;
	}
	.cmd button {
		min-height: 32px;
		padding: 0 12px;
		border-radius: 4px;
		border: 1px solid #3a3e44;
		background: transparent;
		color: #ecedee;
		font: 500 13px var(--font-sans);
		cursor: pointer;
	}
	.cmd button:hover {
		background: #22252a;
	}
	.fine {
		margin: 12px 0 0;
		font-size: 14px;
		color: var(--text-muted);
	}

	/* Hero canvas: the product's own topology view */
	.hero-visual {
		position: relative;
		overflow: hidden;
	}
	.canvas {
		position: absolute;
		left: 0;
		top: 0;
		width: 600px;
		height: 500px;
		transform-origin: 0 0;
		border-radius: 12px;
		border: 1px solid var(--border);
		background-color: var(--bg-base);
		background-image: radial-gradient(var(--border-hover) 1px, transparent 1px);
		background-size: 20px 20px;
	}
	.edge {
		position: absolute;
		background: var(--border-hover);
	}
	.edge.h {
		height: 1px;
	}
	.edge.v {
		width: 1px;
	}
	.edge.dashed {
		background: none;
		border-left: 1px dashed var(--text-muted);
	}
	.node {
		position: absolute;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 12px;
	}
	.node.selected {
		border-color: var(--blue);
	}
	.node-head {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px;
		border-bottom: 1px solid var(--border);
	}
	.tile {
		width: 26px;
		height: 26px;
		flex-shrink: 0;
		border-radius: 4px;
		background: var(--bg-hover);
		color: var(--text-secondary);
		font: 600 10px var(--font-mono);
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.name {
		display: flex;
		flex-direction: column;
		font-size: 13px;
		font-weight: 600;
		line-height: 1.25;
	}
	.name small {
		font: 400 11px var(--font-mono);
		color: var(--text-muted);
	}
	.node-foot {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 8px 10px;
		color: var(--text-muted);
	}
	.state {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		color: var(--text-secondary);
	}
	.state.deploying {
		color: var(--blue-text);
		font-weight: 500;
	}
	.dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
	}
	.dot.run {
		background: var(--green);
	}
	.dot.dep {
		background: var(--blue);
		animation: pulse 1.2s ease-in-out infinite;
	}
	@keyframes pulse {
		50% {
			opacity: 0.3;
		}
	}
	.bar {
		height: 2px;
		background: var(--bg-hover);
		border-radius: 0 0 6px 6px;
		overflow: hidden;
	}
	.bar span {
		display: block;
		height: 2px;
		background: var(--blue);
		transition: width 600ms ease;
	}
	.node.portal {
		padding: 9px 12px;
		display: flex;
		flex-direction: column;
	}
	.domain {
		font: 500 12px var(--font-mono);
	}
	.sub {
		font-size: 11px;
		color: var(--text-muted);
	}
	.node.volume {
		padding: 10px 12px;
		border-style: dashed;
		border-color: var(--text-muted);
		background: var(--bg-elevated);
	}
	.deploy-card {
		position: absolute;
		left: 20px;
		top: 292px;
		width: 290px;
		padding: 14px 16px;
		border-radius: 8px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		box-shadow: 0 16px 40px rgba(20, 22, 25, 0.14), 0 2px 6px rgba(20, 22, 25, 0.06);
	}
	.deploy-title {
		margin: 0 0 8px;
		font-size: 14px;
		font-weight: 600;
	}
	.deploy-card ol {
		list-style: none;
		margin: 0;
		padding: 0;
		font-size: 13px;
		color: var(--text-muted);
	}
	.deploy-card li {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 2px 0;
	}
	.deploy-card li.done {
		color: var(--text-primary);
	}
	.tick {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		border: 1px solid var(--border-hover);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		font-size: 10px;
		color: var(--green-text);
	}
	li.done .tick {
		border-color: var(--green);
		background: var(--green-muted);
	}
	.deploy-foot {
		margin: 10px 0 0;
		padding-top: 10px;
		border-top: 1px solid var(--border);
		font-size: 12px;
		color: var(--text-muted);
	}

	/* Built on */
	.built {
		padding-block: 8px 72px;
		text-align: center;
	}
	.built p {
		margin: 0 0 20px;
		font-size: 14px;
		color: var(--text-muted);
	}
	.built ul {
		display: flex;
		flex-wrap: wrap;
		justify-content: center;
		gap: 12px 44px;
		margin: 0;
		padding: 0;
		list-style: none;
		font-size: 20px;
		font-weight: 600;
		letter-spacing: -0.01em;
		color: var(--text-muted);
	}

	/* Pillars */
	.pillars {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 48px;
		padding-block: 72px 112px;
		border-top: 1px solid var(--border);
	}
	.pillars h2 {
		margin: 0 0 10px;
		font-size: 22px;
		font-weight: 500;
		letter-spacing: -0.015em;
	}
	.pillars p {
		margin: 0;
		color: var(--text-secondary);
		font-size: 16px;
		line-height: 1.65;
	}

	/* How it works: graphite in both themes */
	.how {
		background: #141619;
		color: #ecedee;
		padding-block: 112px;
	}
	.how-head {
		max-width: 640px;
		margin-bottom: 64px;
	}
	.how h2 {
		margin: 0;
		font-size: clamp(30px, 3.6vw, 44px);
		line-height: 1.1;
		font-weight: 500;
		letter-spacing: -0.025em;
		text-wrap: balance;
	}
	.how-head p {
		margin: 16px 0 0;
		font-size: 18px;
		color: #c3c6ca;
	}
	.steps {
		list-style: none;
		margin: 0 0 40px;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 40px;
	}
	.step {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		gap: 64px;
		align-items: center;
	}
	.step-text {
		position: relative;
		padding-left: 56px;
	}
	.num {
		position: absolute;
		left: 0;
		top: 0;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		border: 1px solid #f26b1d;
		color: #f26b1d;
		font: 500 14px var(--font-mono);
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.step:not(:last-child) .step-text::before {
		content: '';
		position: absolute;
		left: 16px;
		top: 40px;
		bottom: -120px;
		width: 1px;
		background: #2a2d32;
	}
	.step h3 {
		margin: 2px 0 8px;
		font-size: 21px;
		font-weight: 500;
		letter-spacing: -0.01em;
	}
	.step p {
		margin: 0;
		color: #c3c6ca;
		line-height: 1.65;
		max-width: 48ch;
	}
	.frag {
		border-radius: 8px;
		border: 1px solid #2a2d32;
		background: #1b1d21;
	}
	.term {
		background: #0a0b0c;
	}
	.term-bar {
		padding: 9px 14px;
		border-bottom: 1px solid #22252a;
		font: 400 12px var(--font-mono);
		color: #8b9097;
	}
	.term pre {
		margin: 0;
		padding: 14px 16px 16px;
		overflow-x: auto;
		font: 400 13px/1.75 var(--font-mono);
		color: #c3c6ca;
	}
	.term .ok {
		color: #4ade80;
	}
	.term .dim {
		color: #8b9097;
	}
	.term .link {
		color: #fb8b47;
	}
	.form {
		padding: 18px 20px;
		display: flex;
		flex-direction: column;
		gap: 10px;
		font-size: 14px;
	}
	.frag-title {
		margin: 0 0 4px;
		font-weight: 600;
	}
	.field {
		display: grid;
		grid-template-columns: 72px minmax(0, 1fr);
		gap: 12px;
		align-items: center;
		color: #8b9097;
	}
	.val {
		padding: 8px 12px;
		border-radius: 6px;
		border: 1px solid #3a3e44;
		background: #141619;
		color: #ecedee;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.val.mono {
		font-size: 13px;
	}
	.val em {
		margin-left: 8px;
		font-style: normal;
		font-size: 12px;
		color: #8b9097;
	}
	.frag-actions {
		display: flex;
		justify-content: flex-end;
		margin-top: 4px;
	}
	.fake-btn {
		padding: 8px 18px;
		border-radius: 6px;
		background: #f26b1d;
		color: #141619;
		font-weight: 500;
	}

	/* Features */
	.features {
		padding-block: 112px;
	}
	.features > h2 {
		margin: 0 0 56px;
		max-width: 20ch;
		font-size: clamp(30px, 3.6vw, 44px);
		line-height: 1.1;
		font-weight: 500;
		letter-spacing: -0.025em;
	}
	.groups {
		display: grid;
		grid-template-columns: repeat(3, minmax(0, 1fr));
		gap: 48px;
	}
	.group h3 {
		margin: 0 0 8px;
		padding-bottom: 12px;
		border-bottom: 2px solid var(--text-primary);
		font-size: 15px;
		font-weight: 600;
	}
	.group dl {
		margin: 0;
	}
	.group dl > div {
		padding: 16px 0;
		border-bottom: 1px solid var(--border);
	}
	.group dt {
		font-weight: 500;
		margin-bottom: 4px;
	}
	.group dd {
		margin: 0;
		font-size: 15px;
		line-height: 1.55;
		color: var(--text-secondary);
	}

	/* Install */
	.install {
		background: var(--bg-base);
		padding-block: 112px;
		border-top: 1px solid var(--border);
	}
	.install-inner {
		display: grid;
		grid-template-columns: minmax(0, 1fr) minmax(0, 1.1fr);
		gap: 64px;
		align-items: start;
	}
	.install h2 {
		margin: 0;
		font-size: clamp(30px, 3.6vw, 44px);
		line-height: 1.1;
		font-weight: 500;
		letter-spacing: -0.025em;
	}
	.install-inner > div > p {
		margin: 16px 0 0;
		color: var(--text-secondary);
		max-width: 52ch;
	}
	.cmd-lg {
		margin-top: 28px;
		max-width: none;
	}
	.does {
		margin: 28px 0 0;
		padding: 0;
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 10px;
		color: var(--text-secondary);
	}
	.does li {
		position: relative;
		padding-left: 18px;
	}
	.does li::before {
		content: '';
		position: absolute;
		left: 0;
		top: 10px;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: var(--green);
	}
	.does code {
		font: 400 0.88em var(--font-mono);
		padding: 0.1em 0.35em;
		border-radius: 4px;
		background: var(--bg-hover);
	}
	.install-links {
		display: flex;
		gap: 20px;
		margin-top: 28px !important;
	}
	.term-lg pre {
		font-size: 13.5px;
	}

	/* Footer */
	.footer {
		padding-block: 56px;
		border-top: 1px solid var(--border);
	}
	.footer-inner {
		display: grid;
		grid-template-columns: minmax(0, 2fr) minmax(0, 1fr) minmax(0, 1fr);
		gap: 40px;
	}
	.footer-brand p {
		margin: 12px 0 0;
		max-width: 40ch;
		font-size: 14px;
		color: var(--text-muted);
	}
	.footer nav {
		flex: none;
	}
	.footer-title {
		margin: 0 0 10px;
		font-size: 14px;
		font-weight: 600;
	}
	.footer ul {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 6px;
		font-size: 14px;
	}
	.footer ul a {
		color: var(--text-secondary);
		text-decoration: none;
	}
	.footer ul a:hover {
		color: var(--text-primary);
		text-decoration: underline;
	}

	/* Responsive */
	@media (max-width: 960px) {
		.hero,
		.install-inner {
			grid-template-columns: minmax(0, 1fr);
			gap: 48px;
		}
		.hero {
			padding-block: 56px;
		}
		.pillars,
		.groups {
			grid-template-columns: minmax(0, 1fr);
			gap: 36px;
		}
		.step {
			grid-template-columns: minmax(0, 1fr);
			gap: 24px;
		}
		.step .frag {
			order: 2;
		}
		.step:not(:last-child) .step-text::before {
			display: none;
		}
		.footer-inner {
			grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
		}
		.footer-brand {
			grid-column: 1 / -1;
		}
	}
	@media (max-width: 720px) {
		.wrap {
			padding-inline: 16px;
		}
		.nav-inner {
			gap: 12px;
		}
		.nav-links li:not(:nth-child(3)) {
			display: none;
		}
		.how,
		.features,
		.install {
			padding-block: 72px;
		}
		.built ul {
			gap: 8px 24px;
			font-size: 17px;
		}
	}
	@media (prefers-reduced-motion: reduce) {
		.dot.dep {
			animation: none;
		}
		.bar span {
			transition: none;
		}
	}
</style>
