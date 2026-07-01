<script lang="ts">
	import { onMount } from 'svelte';
	import { Anchor, Container, Globe, Shield, Zap, ArrowRight, Terminal, Copy, Check, ChevronRight } from '@lucide/svelte';
	import { SHIPYARD_VERSION } from '$lib/version';

	let copied = $state(false);
	let installCmd = $state('curl -fsSL https://shipyard.trian.space/install.sh | bash');

	onMount(() => {
		installCmd = `curl -fsSL ${window.location.protocol}//${window.location.host}/install.sh | bash`;
	});

	async function copyInstall() {
		await navigator.clipboard.writeText(installCmd);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	const steps = [
		{ num: '01', title: 'Run the install script', desc: 'One command sets up Docker, generates secrets, and starts all services.' },
		{ num: '02', title: 'Open the dashboard', desc: 'Shipyard is running at your server IP. Create your admin account.' },
		{ num: '03', title: 'Deploy your first app', desc: 'Point to a Git repo or Docker image and hit Deploy — Shipyard handles the rest.' },
	];

	const features = [
		{ icon: Container, title: 'Container Orchestration', desc: 'Run Docker services as single containers or scaled replicas across your fleet.' },
		{ icon: Globe, title: 'Automatic HTTPS', desc: 'Traefik handles TLS termination and Let\'s Encrypt certificates out of the box.' },
		{ icon: Zap, title: 'Live Topology Canvas', desc: 'Visual graph of services, networks, and volumes with real-time MQTT updates.' },
		{ icon: Shield, title: 'Role-based Access', desc: 'Owner, admin, member, and viewer roles with fine-grained permission grants.' },
	];

	const stack = [
		{
			name: 'Rust',
			role: 'Backend',
			desc: 'Axum web framework powers the API — safe, fast, and zero-cost abstractions.',
			color: '#f97316',
			logo: `<svg viewBox="0 0 106 106" fill="currentColor" xmlns="http://www.w3.org/2000/svg"><path d="M51.9 3.1a2.2 2.2 0 0 1 2.2 0l46.8 27a2.2 2.2 0 0 1 1.1 1.9v54a2.2 2.2 0 0 1-1.1 1.9l-46.8 27a2.2 2.2 0 0 1-2.2 0L5.1 87.9A2.2 2.2 0 0 1 4 86V32a2.2 2.2 0 0 1 1.1-1.9L51.9 3.1z"/></svg>`,
		},
		{
			name: 'SvelteKit',
			role: 'Frontend',
			desc: 'Svelte 5 Runes with fine-grained reactivity for the dashboard and this landing page.',
			color: '#ff3e00',
			logo: `<svg viewBox="0 0 98.1 118" fill="none" xmlns="http://www.w3.org/2000/svg"><path d="M91.8 15.6C80.9-.1 59.2-4.7 43.6 5.5L16.1 22.8A29.6 29.6 0 0 0 3.4 49.2a31 31 0 0 0 4.2 16.4 29.6 29.6 0 0 0-4.4 13.9 31.3 31.3 0 0 0 5.3 18.5c10.9 15.7 32.6 20.3 48.2 10.1l27.5-17.3a29.6 29.6 0 0 0 12.7-26.4 31 31 0 0 0-4.2-16.4 29.6 29.6 0 0 0 4.4-13.9 31.3 31.3 0 0 0-5.3-18.5" fill="currentColor"/></svg>`,
		},
		{
			name: 'PostgreSQL',
			role: 'Database',
			desc: 'All platform state — orgs, services, deployments — lives in Postgres via SQLx.',
			color: '#336791',
			logo: `<svg viewBox="0 0 32 32" fill="currentColor" xmlns="http://www.w3.org/2000/svg"><path d="M16 2C8.3 2 2 8.3 2 16s6.3 14 14 14 14-6.3 14-14S23.7 2 16 2zm0 2c6.6 0 12 5.4 12 12s-5.4 12-12 12S4 22.6 4 16 9.4 4 16 4z"/></svg>`,
		},
		{
			name: 'Docker',
			role: 'Runtime',
			desc: 'Containers are orchestrated via the Docker Engine API — Swarm mode for replicas.',
			color: '#2496ed',
			logo: `<svg viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg"><path d="M13.98 11.08h2.12v-2h-2.12v2zm-3.06 0h2.12v-2h-2.12v2zm-3.07 0h2.12v-2H7.85v2zM4.78 11.08H6.9v-2H4.78v2zm3.07-3.07h2.12v-2H7.85v2zm3.07 0h2.12v-2h-2.12v2zm3.06 0h2.12v-2h-2.12v2zM23.7 11.5a4.35 4.35 0 0 0-3.7-1.35 5.1 5.1 0 0 0-1.7-3.27l-.34-.27-.3.32a4.82 4.82 0 0 0-.92 3.12 4.4 4.4 0 0 0 .5 1.83 5.84 5.84 0 0 1-2.13.4H.3l-.06.38a9.4 9.4 0 0 0 .47 4.57l.2.53.06.15a7.05 7.05 0 0 0 6.35 3.6 13.62 13.62 0 0 0 6.56-1.67 11.44 11.44 0 0 0 4.44-4.5 8.1 8.1 0 0 0 3.76-.9c1.01-.55 1.76-1.4 2.1-2.42l.12-.37-.6-.15z"/></svg>`,
		},
		{
			name: 'Traefik',
			role: 'Reverse Proxy',
			desc: 'Automatic routing and TLS via Traefik — domain configuration written dynamically.',
			color: '#24a1c1',
			logo: `<svg viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>`,
		},
		{
			name: 'MQTT',
			role: 'Real-time Events',
			desc: 'RMQTT broker pushes topology and log events to the dashboard without polling.',
			color: '#660066',
			logo: `<svg viewBox="0 0 24 24" fill="currentColor" xmlns="http://www.w3.org/2000/svg"><path d="M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm0 18a8 8 0 1 1 8-8 8 8 0 0 1-8 8zm-1-13h2v6h-2zm0 8h2v2h-2z"/></svg>`,
		},
	];
</script>

<svelte:head>
	<title>Shipyard — Weigh anchor. Ship your app.</title>
	<meta name="description" content="Self-hosted container platform. Deploy, manage, and scale Docker services with automatic HTTPS and a live topology canvas." />
	<meta property="og:title" content="Shipyard" />
	<meta property="og:description" content="Weigh anchor. Ship your app." />
</svelte:head>

<!-- ─── Page ──────────────────────────────────────────────────────────────── -->
<div class="root">

	<!-- ── Nav ── -->
	<header class="nav">
		<nav class="nav-inner">
			<a href="/" class="brand">
				<Anchor size={20} strokeWidth={2.5} />
				<span>Shipyard</span>
			</a>
			<ul class="nav-links" role="list">
				<li><a href="#install" class="nav-link">Install</a></li>
				<li><a href="#features" class="nav-link">Features</a></li>
				<li>
					<a
						href="https://github.com/triandamai/shipyard"
						target="_blank"
						rel="noopener noreferrer"
						class="nav-link"
					>
						<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
							<path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"/>
						</svg>
						GitHub
					</a>
				</li>
			</ul>
			<a href="#install" class="btn btn-nav">
				Get started
				<ChevronRight size={14} />
			</a>
		</nav>
	</header>

	<!-- ── Hero ── -->
	<section class="hero">
		<!-- Decorative grid -->
		<div class="hero-grid" aria-hidden="true"></div>
		<!-- Glow orbs -->
		<div class="orb orb-1" aria-hidden="true"></div>
		<div class="orb orb-2" aria-hidden="true"></div>

		<div class="hero-content">
			<div class="hero-badge">
				<span class="badge-dot"></span>
				Open-source · Self-hosted · MIT License
				<span class="badge-sep">·</span>
				<a
					href="https://github.com/triandamai/shipyard/releases/tag/v{SHIPYARD_VERSION}"
					target="_blank"
					rel="noopener noreferrer"
					class="badge-version"
				>v{SHIPYARD_VERSION}</a>
			</div>

			<h1 class="hero-title">
				Weigh anchor.<br />
				<em>Ship your app.</em>
			</h1>

			<p class="hero-sub">
				Shipyard is a self-hosted PaaS. Deploy containers, manage domains,
				scale replicas, and monitor your stack — from one dashboard you own.
			</p>

			<div class="hero-ctas">
				<a href="#install" class="btn btn-primary">
					Install Shipyard
					<ArrowRight size={16} />
				</a>
				<a
					href="https://github.com/triandamai/shipyard"
					target="_blank"
					rel="noopener noreferrer"
					class="btn btn-outline"
				>
					<svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
						<path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"/>
					</svg>
					View on GitHub
				</a>
			</div>

			<!-- Install card -->
			<div class="install-card" id="install">
				<div class="install-card-header">
					<Terminal size={14} />
					<span>Quick install — Linux / macOS with Docker</span>
				</div>
				<div class="install-card-body">
					<code class="install-cmd">{installCmd}</code>
					<button class="copy-btn" onclick={copyInstall} aria-label="Copy install command">
						{#if copied}
							<Check size={14} />
						{:else}
							<Copy size={14} />
						{/if}
					</button>
				</div>
				<div class="install-card-footer">
					Requires Docker ≥ 24 and Docker Compose v2. Tested on Ubuntu 22+, Debian 12+.
				</div>
			</div>
		</div>
	</section>

	<!-- ── How it works ── -->
	<section class="steps-section" id="install-steps">
		<div class="wrap">
			<div class="section-label">How it works</div>
			<h2 class="section-title">Up and running in three steps</h2>
			<div class="steps-grid">
				{#each steps as step}
					<div class="step-card">
						<div class="step-num">{step.num}</div>
						<h3 class="step-title">{step.title}</h3>
						<p class="step-desc">{step.desc}</p>
					</div>
				{/each}
			</div>

			<!-- Detailed install block -->
			<div class="install-detail">
				<div class="install-detail-col">
					<h3 class="install-detail-title">What the script does</h3>
					<ul class="install-checklist">
						<li><Check size={14} /> Checks Docker & Docker Compose prerequisites</li>
						<li><Check size={14} /> Generates secure random secrets</li>
						<li><Check size={14} /> Writes <code>/opt/shipyard/.env</code> and <code>docker-compose.yml</code></li>
						<li><Check size={14} /> Configures Traefik with optional HTTPS</li>
						<li><Check size={14} /> Pulls images and starts the stack</li>
					</ul>
				</div>
				<div class="terminal-block">
					<div class="terminal-bar">
						<span class="t-dot t-red"></span>
						<span class="t-dot t-yellow"></span>
						<span class="t-dot t-green"></span>
						<span class="t-title">install.sh</span>
					</div>
					<pre class="terminal-body"><span class="t-dim">$</span> <span class="t-cmd">{installCmd}</span>

<span class="t-green-txt">✔</span> Docker 26.1.4 found
<span class="t-green-txt">✔</span> Docker Compose v2.27 found
<span class="t-dim">?</span> <span class="t-white">Domain (e.g. shipyard.example.com):</span> <span class="t-blue">ship.acme.io</span>
<span class="t-dim">?</span> <span class="t-white">Enable HTTPS? [Y/n]:</span> <span class="t-blue">Y</span>
<span class="t-dim">?</span> <span class="t-white">Admin email for Let's Encrypt:</span> <span class="t-blue">ops@acme.io</span>

<span class="t-green-txt">✔</span> Secrets generated
<span class="t-green-txt">✔</span> Config written to /opt/shipyard/
<span class="t-green-txt">✔</span> Images pulled
<span class="t-green-txt">✔</span> Stack started

<span class="t-white">Open https://ship.acme.io to finish setup.</span></pre>
				</div>
			</div>
		</div>
	</section>

	<!-- ── Features ── -->
	<section class="features-section" id="features">
		<div class="wrap">
			<div class="section-label">Features</div>
			<h2 class="section-title">Everything in one place</h2>
			<div class="features-grid">
				{#each features as f}
					<div class="feature-card">
						<div class="feature-icon-wrap">
							<f.icon size={20} strokeWidth={1.75} />
						</div>
						<h3 class="feature-title">{f.title}</h3>
						<p class="feature-desc">{f.desc}</p>
					</div>
				{/each}
			</div>
		</div>
	</section>

	<!-- ── Tech stack ── -->
	<section class="stack-section">
		<div class="wrap">
			<div class="section-label">Built with</div>
			<h2 class="section-title">Open-source, all the way down</h2>
			<p class="stack-intro">
				Shipyard is built entirely on production-grade open-source technology.
				No proprietary runtime, no hidden dependency.
			</p>
			<div class="stack-grid">
				{#each stack as tech}
					<div class="stack-card">
						<div class="stack-card-top">
							<div class="stack-logo" style="color: {tech.color}; background: {tech.color}1a; border-color: {tech.color}33">
								{@html tech.logo}
							</div>
							<div class="stack-meta">
								<span class="stack-name">{tech.name}</span>
								<span class="stack-role">{tech.role}</span>
							</div>
						</div>
						<p class="stack-desc">{tech.desc}</p>
					</div>
				{/each}
			</div>
		</div>
	</section>

	<!-- ── CTA band ── -->
	<section class="cta-section">
		<div class="wrap cta-inner">
			<div class="cta-bg-lines" aria-hidden="true"></div>
			<Anchor size={36} class="cta-anchor-icon" strokeWidth={1.5} />
			<h2 class="cta-title">Your infra, your rules.</h2>
			<p class="cta-sub">No vendor lock-in. No per-seat pricing. One install command.</p>
			<div class="cta-btns">
				<a href="#install" class="btn btn-primary btn-lg">
					Install now
					<ArrowRight size={16} />
				</a>
				<a
					href="https://github.com/triandamai/shipyard"
					target="_blank"
					rel="noopener noreferrer"
					class="btn btn-outline btn-lg"
				>
					Star on GitHub
				</a>
			</div>
		</div>
	</section>

	<!-- ── Footer ── -->
	<footer class="footer">
		<div class="footer-inner">
			<span class="footer-brand">
				<Anchor size={14} strokeWidth={2.5} />
				Shipyard
			</span>
			<span class="footer-sep"></span>
			<span class="footer-copy">Open-source container platform — MIT License</span>
			<a
				href="https://github.com/triandamai/shipyard/releases/tag/v{SHIPYARD_VERSION}"
				target="_blank"
				rel="noopener noreferrer"
				class="footer-version"
			>v{SHIPYARD_VERSION}</a>
			<a
				href="https://github.com/triandamai/shipyard"
				target="_blank"
				rel="noopener noreferrer"
				class="footer-gh"
			>
				<svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
					<path d="M12 2C6.477 2 2 6.484 2 12.017c0 4.425 2.865 8.18 6.839 9.504.5.092.682-.217.682-.483 0-.237-.008-.868-.013-1.703-2.782.605-3.369-1.343-3.369-1.343-.454-1.158-1.11-1.466-1.11-1.466-.908-.62.069-.608.069-.608 1.003.07 1.531 1.032 1.531 1.032.892 1.53 2.341 1.088 2.91.832.092-.647.35-1.088.636-1.338-2.22-.253-4.555-1.113-4.555-4.951 0-1.093.39-1.988 1.029-2.688-.103-.253-.446-1.272.098-2.65 0 0 .84-.27 2.75 1.026A9.564 9.564 0 0112 6.844c.85.004 1.705.115 2.504.337 1.909-1.296 2.747-1.027 2.747-1.027.546 1.379.202 2.398.1 2.651.64.7 1.028 1.595 1.028 2.688 0 3.848-2.339 4.695-4.566 4.943.359.309.678.92.678 1.855 0 1.338-.012 2.419-.012 2.747 0 .268.18.58.688.482A10.019 10.019 0 0022 12.017C22 6.484 17.522 2 12 2z"/>
				</svg>
				GitHub
			</a>
		</div>
	</footer>

</div>

<style>
	/* ── Reset & base ────────────────────────────────────────────── */
	:global(*, *::before, *::after) { box-sizing: border-box; margin: 0; padding: 0; }
	:global(html) { scroll-behavior: smooth; }
	:global(body) {
		font-family: 'Inter', system-ui, -apple-system, 'Segoe UI', sans-serif;
		background: #0a0a0f;
		color: #e2e8f0;
		line-height: 1.6;
		-webkit-font-smoothing: antialiased;
	}

	.root { min-height: 100vh; display: flex; flex-direction: column; }
	.wrap { max-width: 1120px; margin: 0 auto; padding: 0 24px; }

	/* ── Nav ────────────────────────────────────────────────────── */
	.nav {
		position: sticky;
		top: 0;
		z-index: 100;
		background: rgba(10,10,15,0.8);
		backdrop-filter: blur(16px) saturate(1.4);
		border-bottom: 1px solid rgba(255,255,255,0.06);
	}
	.nav-inner {
		max-width: 1120px;
		margin: 0 auto;
		padding: 0 24px;
		height: 60px;
		display: flex;
		align-items: center;
		gap: 32px;
	}
	.brand {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 15px;
		font-weight: 700;
		color: #fff;
		text-decoration: none;
		letter-spacing: -0.01em;
		flex-shrink: 0;
	}
	.brand :global(svg) { color: #3b82f6; }
	.nav-links {
		display: flex;
		align-items: center;
		gap: 4px;
		list-style: none;
		flex: 1;
	}
	.nav-link {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		font-size: 14px;
		font-weight: 500;
		color: rgba(255,255,255,0.55);
		text-decoration: none;
		border-radius: 6px;
		transition: color 0.15s, background 0.15s;
	}
	.nav-link:hover { color: #fff; background: rgba(255,255,255,0.06); }

	/* ── Buttons ─────────────────────────────────────────────────── */
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		border-radius: 8px;
		font-family: inherit;
		font-weight: 600;
		text-decoration: none;
		cursor: pointer;
		transition: all 0.18s;
		white-space: nowrap;
		border: none;
		font-size: 14px;
		padding: 8px 18px;
	}
	.btn-lg { font-size: 15px; padding: 11px 24px; }

	.btn-nav {
		background: #1d4ed8;
		color: #fff;
		margin-left: auto;
		font-size: 13px;
		padding: 7px 16px;
	}
	.btn-nav:hover { background: #2563eb; transform: translateY(-1px); }

	.btn-primary {
		background: linear-gradient(135deg, #2563eb 0%, #1d4ed8 100%);
		color: #fff;
		box-shadow: 0 1px 3px rgba(37,99,235,0.3), inset 0 1px 0 rgba(255,255,255,0.1);
	}
	.btn-primary:hover {
		background: linear-gradient(135deg, #3b82f6 0%, #2563eb 100%);
		transform: translateY(-2px);
		box-shadow: 0 6px 20px rgba(37,99,235,0.4);
	}

	.btn-outline {
		background: transparent;
		color: rgba(255,255,255,0.7);
		border: 1px solid rgba(255,255,255,0.15);
	}
	.btn-outline:hover {
		color: #fff;
		border-color: rgba(255,255,255,0.35);
		background: rgba(255,255,255,0.05);
	}

	/* ── Hero ────────────────────────────────────────────────────── */
	.hero {
		position: relative;
		overflow: hidden;
		padding: 96px 24px 80px;
		display: flex;
		justify-content: center;
	}

	/* Angular-style dot grid */
	.hero-grid {
		position: absolute;
		inset: 0;
		background-image:
			radial-gradient(circle, rgba(255,255,255,0.06) 1px, transparent 1px);
		background-size: 32px 32px;
		mask-image: radial-gradient(ellipse 80% 60% at 50% 0%, black 30%, transparent 100%);
		pointer-events: none;
	}

	.orb {
		position: absolute;
		border-radius: 50%;
		filter: blur(80px);
		pointer-events: none;
	}
	.orb-1 {
		width: 600px; height: 600px;
		top: -200px; left: 50%;
		transform: translateX(-50%);
		background: radial-gradient(circle, rgba(37,99,235,0.22) 0%, transparent 70%);
	}
	.orb-2 {
		width: 300px; height: 300px;
		top: 60px; right: 5%;
		background: radial-gradient(circle, rgba(99,102,241,0.14) 0%, transparent 70%);
	}

	.hero-content {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 24px;
		max-width: 760px;
		width: 100%;
	}

	.hero-badge {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		padding: 5px 14px;
		background: rgba(37,99,235,0.12);
		border: 1px solid rgba(59,130,246,0.25);
		border-radius: 999px;
		font-size: 12px;
		font-weight: 600;
		color: #93c5fd;
		letter-spacing: 0.02em;
		text-transform: uppercase;
	}
	.badge-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #3b82f6;
		box-shadow: 0 0 6px #3b82f6;
		animation: pulse 2s ease-in-out infinite;
	}
	.badge-sep { opacity: 0.4; }
	.badge-version {
		color: #93c5fd;
		text-decoration: none;
		font-weight: 700;
		transition: color 0.15s;
	}
	.badge-version:hover { color: #fff; }
	@keyframes pulse {
		0%, 100% { opacity: 1; }
		50%       { opacity: 0.4; }
	}

	.hero-title {
		font-size: clamp(2.4rem, 6vw, 4rem);
		font-weight: 800;
		line-height: 1.08;
		letter-spacing: -0.04em;
		color: #fff;
	}
	.hero-title em {
		font-style: normal;
		background: linear-gradient(135deg, #60a5fa 0%, #818cf8 50%, #a78bfa 100%);
		-webkit-background-clip: text;
		-webkit-text-fill-color: transparent;
		background-clip: text;
	}

	.hero-sub {
		font-size: 17px;
		line-height: 1.65;
		color: rgba(255,255,255,0.5);
		max-width: 540px;
	}

	.hero-ctas {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
		justify-content: center;
	}

	/* ── Install card ─────────────────────────────────────────────── */
	.install-card {
		width: 100%;
		max-width: 620px;
		background: #111118;
		border: 1px solid rgba(255,255,255,0.1);
		border-radius: 12px;
		overflow: hidden;
		box-shadow: 0 0 0 1px rgba(59,130,246,0.08), 0 20px 48px rgba(0,0,0,0.5);
		text-align: left;
		margin-top: 8px;
	}
	.install-card-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 16px;
		background: rgba(255,255,255,0.03);
		border-bottom: 1px solid rgba(255,255,255,0.07);
		font-size: 12px;
		color: rgba(255,255,255,0.4);
	}
	.install-card-body {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 14px 16px;
	}
	.install-cmd {
		flex: 1;
		font-family: 'Fira Code', 'Cascadia Code', 'JetBrains Mono', ui-monospace, monospace;
		font-size: 13px;
		color: #93c5fd;
		word-break: break-all;
		user-select: all;
	}
	.copy-btn {
		flex-shrink: 0;
		width: 32px;
		height: 32px;
		display: flex;
		align-items: center;
		justify-content: center;
		background: rgba(255,255,255,0.05);
		border: 1px solid rgba(255,255,255,0.1);
		border-radius: 6px;
		cursor: pointer;
		color: rgba(255,255,255,0.5);
		transition: all 0.15s;
	}
	.copy-btn:hover { background: rgba(255,255,255,0.1); color: #fff; }
	.install-card-footer {
		padding: 8px 16px;
		background: rgba(255,255,255,0.02);
		border-top: 1px solid rgba(255,255,255,0.06);
		font-size: 11.5px;
		color: rgba(255,255,255,0.3);
	}

	/* ── Steps section ───────────────────────────────────────────── */
	.steps-section {
		padding: 96px 0;
		border-top: 1px solid rgba(255,255,255,0.06);
	}
	.section-label {
		font-size: 12px;
		font-weight: 700;
		letter-spacing: 0.1em;
		text-transform: uppercase;
		color: #3b82f6;
		margin-bottom: 12px;
	}
	.section-title {
		font-size: clamp(1.5rem, 3vw, 2.25rem);
		font-weight: 700;
		letter-spacing: -0.03em;
		color: #fff;
		margin-bottom: 48px;
	}
	.steps-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 1px;
		background: rgba(255,255,255,0.06);
		border: 1px solid rgba(255,255,255,0.06);
		border-radius: 12px;
		overflow: hidden;
		margin-bottom: 64px;
	}
	.step-card {
		padding: 32px 28px;
		background: #0a0a0f;
		display: flex;
		flex-direction: column;
		gap: 10px;
		transition: background 0.2s;
	}
	.step-card:hover { background: #0f0f1a; }
	.step-num {
		font-size: 12px;
		font-weight: 700;
		letter-spacing: 0.08em;
		color: #3b82f6;
		font-variant-numeric: tabular-nums;
	}
	.step-title {
		font-size: 16px;
		font-weight: 650;
		color: #f1f5f9;
		letter-spacing: -0.01em;
	}
	.step-desc {
		font-size: 14px;
		color: rgba(255,255,255,0.45);
		line-height: 1.6;
	}

	/* ── Install detail ──────────────────────────────────────────── */
	.install-detail {
		display: grid;
		grid-template-columns: 1fr 1.4fr;
		gap: 40px;
		align-items: start;
	}
	.install-detail-title {
		font-size: 16px;
		font-weight: 650;
		color: #f1f5f9;
		margin-bottom: 20px;
		letter-spacing: -0.01em;
	}
	.install-checklist {
		list-style: none;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}
	.install-checklist li {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		font-size: 14px;
		color: rgba(255,255,255,0.6);
		line-height: 1.5;
	}
	.install-checklist li :global(svg) {
		color: #22c55e;
		flex-shrink: 0;
		margin-top: 2px;
	}
	.install-checklist code {
		font-family: ui-monospace, monospace;
		font-size: 12px;
		color: #93c5fd;
		background: rgba(59,130,246,0.1);
		padding: 1px 5px;
		border-radius: 4px;
	}

	/* ── Terminal block ──────────────────────────────────────────── */
	.terminal-block {
		background: #0d1017;
		border: 1px solid rgba(255,255,255,0.08);
		border-radius: 10px;
		overflow: hidden;
		box-shadow: 0 16px 40px rgba(0,0,0,0.4);
	}
	.terminal-bar {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 10px 14px;
		background: rgba(255,255,255,0.03);
		border-bottom: 1px solid rgba(255,255,255,0.06);
	}
	.t-dot {
		width: 10px;
		height: 10px;
		border-radius: 50%;
	}
	.t-red    { background: #ff5f57; }
	.t-yellow { background: #febc2e; }
	.t-green  { background: #28c840; }
	.t-title {
		margin-left: 8px;
		font-size: 12px;
		color: rgba(255,255,255,0.3);
		font-family: ui-monospace, monospace;
	}
	.terminal-body {
		padding: 20px;
		font-family: 'Fira Code', 'Cascadia Code', ui-monospace, monospace;
		font-size: 12.5px;
		line-height: 1.75;
		white-space: pre;
		overflow-x: auto;
		color: rgba(255,255,255,0.7);
	}
	.t-dim       { color: rgba(255,255,255,0.25); }
	.t-cmd       { color: #93c5fd; }
	.t-green-txt { color: #4ade80; }
	.t-white     { color: rgba(255,255,255,0.85); }
	.t-blue      { color: #7dd3fc; }

	/* ── Features ────────────────────────────────────────────────── */
	.features-section {
		padding: 96px 0;
		border-top: 1px solid rgba(255,255,255,0.06);
		background: linear-gradient(180deg, rgba(15,15,26,0.8) 0%, #0a0a0f 100%);
	}
	.features-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
		gap: 1px;
		background: rgba(255,255,255,0.06);
		border: 1px solid rgba(255,255,255,0.06);
		border-radius: 12px;
		overflow: hidden;
	}
	.feature-card {
		padding: 32px 28px;
		background: #0a0a0f;
		display: flex;
		flex-direction: column;
		gap: 12px;
		transition: background 0.2s;
	}
	.feature-card:hover { background: #0f0f1a; }
	.feature-icon-wrap {
		width: 40px;
		height: 40px;
		border-radius: 10px;
		background: rgba(37,99,235,0.12);
		border: 1px solid rgba(59,130,246,0.2);
		display: flex;
		align-items: center;
		justify-content: center;
		color: #60a5fa;
	}
	.feature-title {
		font-size: 15px;
		font-weight: 650;
		color: #f1f5f9;
		letter-spacing: -0.01em;
	}
	.feature-desc {
		font-size: 13.5px;
		line-height: 1.6;
		color: rgba(255,255,255,0.42);
	}

	/* ── CTA section ─────────────────────────────────────────────── */
	.cta-section {
		padding: 96px 24px;
		border-top: 1px solid rgba(255,255,255,0.06);
		overflow: hidden;
	}
	.cta-inner {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		gap: 20px;
	}
	/* Angular-style diagonal line decoration */
	.cta-bg-lines {
		position: absolute;
		inset: -60px;
		background-image:
			repeating-linear-gradient(
				-45deg,
				transparent,
				transparent 40px,
				rgba(59,130,246,0.04) 40px,
				rgba(59,130,246,0.04) 41px
			);
		pointer-events: none;
		border-radius: 12px;
	}
	:global(.cta-anchor-icon) { color: #3b82f6; position: relative; }
	.cta-title {
		font-size: clamp(1.75rem, 4vw, 2.75rem);
		font-weight: 800;
		color: #fff;
		letter-spacing: -0.04em;
		position: relative;
	}
	.cta-sub {
		font-size: 16px;
		color: rgba(255,255,255,0.45);
		max-width: 400px;
		position: relative;
	}
	.cta-btns {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
		justify-content: center;
		position: relative;
		margin-top: 4px;
	}

	/* ── Footer ──────────────────────────────────────────────────── */
	.footer {
		border-top: 1px solid rgba(255,255,255,0.06);
		padding: 20px 24px;
	}
	.footer-inner {
		max-width: 1120px;
		margin: 0 auto;
		display: flex;
		align-items: center;
		gap: 12px;
		font-size: 13px;
		flex-wrap: wrap;
	}
	.footer-brand {
		display: flex;
		align-items: center;
		gap: 6px;
		font-weight: 700;
		color: rgba(255,255,255,0.7);
	}
	.footer-sep {
		width: 1px;
		height: 14px;
		background: rgba(255,255,255,0.1);
	}
	.footer-copy { color: rgba(255,255,255,0.3); flex: 1; }
	.footer-gh {
		display: flex;
		align-items: center;
		gap: 5px;
		color: rgba(255,255,255,0.35);
		text-decoration: none;
		transition: color 0.15s;
	}
	.footer-gh:hover { color: rgba(255,255,255,0.8); }
	.footer-version {
		font-size: 12px;
		font-weight: 600;
		font-family: ui-monospace, monospace;
		color: rgba(255,255,255,0.3);
		text-decoration: none;
		padding: 2px 8px;
		border: 1px solid rgba(255,255,255,0.08);
		border-radius: 999px;
		transition: color 0.15s, border-color 0.15s;
	}
	.footer-version:hover { color: #93c5fd; border-color: rgba(96,165,250,0.3); }

	/* ── Tech stack ─────────────────────────────────────────────── */
	.stack-section {
		padding: 96px 0;
		border-top: 1px solid rgba(255,255,255,0.06);
	}
	.stack-intro {
		font-size: 15px;
		color: rgba(255,255,255,0.42);
		line-height: 1.65;
		max-width: 520px;
		margin-bottom: 48px;
	}
	.stack-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 1px;
		background: rgba(255,255,255,0.06);
		border: 1px solid rgba(255,255,255,0.06);
		border-radius: 12px;
		overflow: hidden;
	}
	.stack-card {
		padding: 28px;
		background: #0a0a0f;
		display: flex;
		flex-direction: column;
		gap: 14px;
		transition: background 0.2s;
	}
	.stack-card:hover { background: #0f0f1a; }
	.stack-card-top {
		display: flex;
		align-items: center;
		gap: 14px;
	}
	.stack-logo {
		width: 44px;
		height: 44px;
		border-radius: 10px;
		border: 1px solid;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		padding: 10px;
	}
	.stack-logo :global(svg) { width: 100%; height: 100%; }
	.stack-meta {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.stack-name {
		font-size: 15px;
		font-weight: 650;
		color: #f1f5f9;
		letter-spacing: -0.01em;
	}
	.stack-role {
		font-size: 11px;
		font-weight: 600;
		letter-spacing: 0.07em;
		text-transform: uppercase;
		color: rgba(255,255,255,0.3);
	}
	.stack-desc {
		font-size: 13.5px;
		line-height: 1.6;
		color: rgba(255,255,255,0.42);
	}

	/* ── Responsive ──────────────────────────────────────────────── */
	@media (max-width: 768px) {
		.hero { padding: 64px 20px 56px; }
		.install-detail { grid-template-columns: 1fr; }
		.terminal-block { display: none; }
		.nav-links { display: none; }
		.steps-section, .features-section { padding: 64px 0; }
		.section-title { margin-bottom: 32px; }
	}
	@media (max-width: 480px) {
		.hero-title { font-size: 2rem; }
		.hero-ctas { flex-direction: column; align-items: stretch; }
		.hero-ctas .btn { justify-content: center; }
		.install-cmd { font-size: 11px; }
	}
</style>
