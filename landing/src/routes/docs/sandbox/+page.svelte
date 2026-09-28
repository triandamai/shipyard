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
			group: 'Templates',
			items: [
				{ id: 'basic-templates',     label: 'Basic Templates' },
				{ id: 'framework-templates', label: 'Framework Gallery' },
			],
		},
		{
			group: 'Custom Mode',
			items: [
				{ id: 'custom-mode', label: 'Scaffold Your Own' },
				{ id: 'manifest',    label: 'shipyard.json Manifest' },
			],
		},
		{
			group: 'Working in a Sandbox',
			items: [
				{ id: 'editor',   label: 'Editor & Files' },
				{ id: 'terminal', label: 'Terminal' },
				{ id: 'preview',  label: 'Live Preview' },
			],
		},
		{
			group: 'Lifecycle',
			items: [
				{ id: 'idle-and-quota', label: 'Idle Timeout & Quota' },
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
		manifest:
`{
  "app": {
    "runtime": "node",
    "install": "npm install",
    "dev": "npm run dev",
    "port": 3000
  }
}`,

		customFlow:
`# Inside the sandbox's Terminal tab
npx create-react-app .
# or: npm create vite@latest . -- --template svelte
# or literally anything else — it's just a shell

# Then, from the app's canvas panel:
#   Stop  ->  Start
# Shipyard re-detects the real stack and boots its dev server.`,

		previewUrl:
`https://preview-a1b2c3d4.shipyard-apps.dev`,
	};

	const basicTemplates = [
		{ name: 'Node',   runtime: 'node:20-alpine',   install: 'npm install',                    dev: 'npm run dev',      port: 3000 },
		{ name: 'Python', runtime: 'python:3.12-slim', install: 'pip install -r requirements.txt', dev: 'python app.py',    port: 8000 },
		{ name: 'Static', runtime: 'nginx:alpine',     install: '—',                                dev: 'nginx (static)',   port: 8080 },
		{ name: 'Custom', runtime: 'node:22-alpine',   install: '—',                                dev: 'sleep infinity (idle)', port: '—' },
	];

	const frameworkTemplates = [
		{ name: 'React',       variants: 'JS / TS', scaffolder: 'npm create vite@latest',      port: 5173 },
		{ name: 'Vue',         variants: 'JS / TS', scaffolder: 'npm create vite@latest',      port: 5173 },
		{ name: 'SvelteKit',   variants: 'JS / TS', scaffolder: 'npx sv create',               port: 5173 },
		{ name: 'Next.js',     variants: 'JS / TS', scaffolder: 'npx create-next-app@latest',  port: 3000 },
		{ name: 'Nuxt',        variants: 'TS only',  scaffolder: 'npx nuxi@latest init',        port: 3000 },
		{ name: 'Astro',       variants: 'JS / TS', scaffolder: 'npm create astro@latest',      port: 4321 },
	];
</script>

<svelte:head>
	<title>Sandbox Apps — Shipyard Docs</title>
	<meta name="description" content="Spin up disposable coding sandboxes with a browser terminal, live preview, and one-click framework templates for React, Vue, SvelteKit, Next.js, Nuxt, and Astro." />
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
			<a href="/docs/sandbox" class="topbar-link active">Sandbox</a>
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

		<!-- ── Overview ─────────────────────────────────────────────── -->
		<section id="overview">
			<div class="page-eyebrow">Sandbox Apps · Disposable dev environments</div>
			<h1>Sandbox Apps</h1>
			<p>
				A Sandbox App is a disposable, containerized coding environment with a real
				terminal, a file editor, and a live preview URL — no local setup required.
				Pick a framework template and Shipyard scaffolds it with the official CLI,
				installs dependencies, and boots the dev server automatically. Or start from
				a blank <strong>Custom</strong> container and scaffold whatever you want by hand.
			</p>

			<div class="quick-cards">
				<div class="quick-card">
					<div class="qc-label">Preview URL</div>
					<code>preview-&lt;id&gt;.your-domain.dev</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Idle timeout</div>
					<code>20 minutes (default)</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Templates</div>
					<code>4 basic + 11 framework variants</code>
				</div>
				<div class="quick-card">
					<div class="qc-label">Terminal</div>
					<code>Real shell, via WebSocket</code>
				</div>
			</div>
		</section>

		<!-- ── Quick Start ──────────────────────────────────────────── -->
		<section id="quickstart">
			<h2>Quick Start</h2>

			<div class="steps-list">
				<div class="step-item"><span class="step-num">1</span><span>Inside a project, click <strong>New App</strong> and choose a Sandbox App</span></div>
				<div class="step-item"><span class="step-num">2</span><span>Pick a template — a framework (React, Vue, SvelteKit, Next.js, Nuxt, Astro), a basic runtime (Node, Python, Static), or <strong>Custom</strong> for a blank container</span></div>
				<div class="step-item"><span class="step-num">3</span><span>Give it a name — the slug is generated automatically — and click <strong>Create App</strong></span></div>
				<div class="step-item"><span class="step-num">4</span><span>Open the app to reach the editor: on first boot, Shipyard scaffolds the template, installs dependencies, and starts the dev server</span></div>
				<div class="step-item"><span class="step-num">5</span><span>Edit files, use the terminal, and watch the <strong>Preview</strong> tab update live</span></div>
			</div>

			<div class="callout callout-info">
				A framework template's first boot runs a real <code>npm create …</code> scaffold
				and install over the network — it takes noticeably longer than a plain Node
				sandbox's first start. Subsequent stop/start cycles are fast since the
				scaffolded project already lives on the sandbox's persistent volume.
			</div>
		</section>

		<!-- ── Basic Templates ──────────────────────────────────────── -->
		<section id="basic-templates">
			<h2>Basic Templates</h2>
			<p>
				Four built-in starting points beyond the framework gallery below. Each
				resolves to a fully-declared runtime at creation time — nothing to detect.
			</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Template</th><th>Base image</th><th>Install</th><th>Dev command</th><th>Port</th></tr>
					</thead>
					<tbody>
						{#each basicTemplates as t}
							<tr>
								<td><strong>{t.name}</strong></td>
								<td><code>{t.runtime}</code></td>
								<td>{t.install === '—' ? '—' : `${t.install}`}</td>
								<td><code>{t.dev}</code></td>
								<td>{t.port}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			<div class="callout callout-tip">
				<strong>Custom</strong> boots an idle container (<code>sleep infinity</code>)
				with nothing installed yet — see <button class="inline-link" onclick={() => scrollTo('custom-mode')}>Scaffold Your Own</button>
				for the full workflow.
			</div>
		</section>

		<!-- ── Framework Gallery ────────────────────────────────────── -->
		<section id="framework-templates">
			<h2>Framework Gallery</h2>
			<p>
				Each framework template invokes that framework's own official, non-interactive
				scaffolder on first boot — the same command you'd run on your own machine.
				Dependencies install as a separate, explicit step, then the dev server starts
				bound to <code>0.0.0.0</code> and the sandbox's assigned port so the preview
				can actually reach it.
			</p>

			<div class="table-wrap">
				<table>
					<thead>
						<tr><th>Framework</th><th>Variants</th><th>Scaffolder</th><th>Dev server default port</th></tr>
					</thead>
					<tbody>
						{#each frameworkTemplates as f}
							<tr>
								<td><strong>{f.name}</strong></td>
								<td>{f.variants}</td>
								<td><code>{f.scaffolder}</code></td>
								<td>{f.port}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			<div class="callout callout-info">
				Nuxt ships as a single template — its own scaffolder defaults to TypeScript
				with no meaningful plain-JS mode in current tooling, so there's no separate
				JS variant to offer.
			</div>

			<div class="callout callout-warn">
				Astro's own tooling requires a newer Node runtime than the other frameworks,
				so Astro sandboxes run on a different base image under the hood. This is
				handled automatically — nothing to configure.
			</div>
		</section>

		<!-- ── Custom Mode ──────────────────────────────────────────── -->
		<section id="custom-mode">
			<h2>Scaffold Your Own (Custom Mode)</h2>
			<p>
				Choose the <strong>Custom</strong> template to skip the gallery entirely. It
				boots an empty, idle container — nothing installed, no dev server running —
				so you can scaffold exactly what you want by hand from the Terminal tab.
			</p>

			<h3>The workflow</h3>
			<div class="code-block">
				<div class="code-header">
					<span class="code-label">bash — inside the sandbox's Terminal</span>
					<button class="copy-btn" onclick={() => copyCode('custom-flow', snip.customFlow)}>
						{#if copied === 'custom-flow'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.customFlow}</pre>
			</div>

			<p>
				Shipyard only re-checks what's in the container when the sandbox restarts —
				it never watches the filesystem while it's running. That's why the last step
				is always <strong>Stop</strong>, then <strong>Start</strong>, from the app's
				canvas panel: that recreates the container and re-runs stack detection against
				whatever you scaffolded.
			</p>

			<div class="callout callout-info">
				Until a real project is detected, the editor shows a banner: <em>"No project
				detected yet — scaffold one in the Terminal below, then stop and start this
				sandbox from its app panel on the project canvas to apply it."</em> The
				container stays alive and the terminal stays usable the whole time.
			</div>

			<h3>Prefer to declare it yourself?</h3>
			<p>
				A Custom sandbox starts with a starter file at <code>/app/shipyard.json.example</code>.
				Rename it to <code>shipyard.json</code>, fill in your own values, and restart —
				Shipyard trusts an explicit manifest over guessing from files on disk. See the
				<button class="inline-link" onclick={() => scrollTo('manifest')}>manifest format</button> below.
			</p>
		</section>

		<!-- ── shipyard.json Manifest ────────────────────────────────── -->
		<section id="manifest">
			<h2>shipyard.json Manifest</h2>
			<p>
				Every sandbox — template-created or Custom — can be pointed at an explicit
				manifest instead of relying on auto-detection. Drop a <code>shipyard.json</code>
				file in the project root and restart the sandbox.
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">shipyard.json</span>
					<button class="copy-btn" onclick={() => copyCode('manifest', snip.manifest)}>
						{#if copied === 'manifest'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.manifest}</pre>
			</div>

			<div class="table-wrap">
				<table>
					<thead><tr><th>Field</th><th>Meaning</th></tr></thead>
					<tbody>
						<tr><td><code>runtime</code></td><td>Label only — informational, shown in the UI</td></tr>
						<tr><td><code>install</code></td><td>Shell command run once before the dev server starts</td></tr>
						<tr><td><code>dev</code></td><td>Shell command that starts the dev server — must bind <code>0.0.0.0</code>, not just <code>localhost</code></td></tr>
						<tr><td><code>port</code></td><td>The port the dev server listens on — the preview route is wired to this exact port</td></tr>
					</tbody>
				</table>
			</div>

			<div class="callout callout-warn">
				A manifest takes priority over every file-presence heuristic
				(<code>package.json</code>, <code>requirements.txt</code>, <code>index.html</code>,
				etc.). If <code>shipyard.json</code> exists, its values are used as-is — Shipyard
				will not second-guess it.
			</div>
		</section>

		<!-- ── Editor & Files ────────────────────────────────────────── -->
		<section id="editor">
			<h2>Editor & Files</h2>
			<p>
				Opening a sandbox app lands you on its editor page: a file tree on the left,
				a code editor in the center, and Preview / Terminal alongside it.
			</p>

			<h3>File tree</h3>
			<ul>
				<li>Click any file to open it in the editor</li>
				<li>Create new files directly from the tree — folders are created automatically from the path you type</li>
				<li>Changes autosave as you type; a save indicator confirms when a write completes</li>
			</ul>

			<h3>Hot reload</h3>
			<p>
				Saving a file that the running dev server watches (which is every framework
				template — that's exactly what <code>npm run dev</code> is for) reflects in the
				<strong>Preview</strong> tab automatically, the same as running the dev server
				on your own machine.
			</p>

			<h3>Preview sizing</h3>
			<p>
				The Preview tab includes desktop and mobile width presets, plus a free-drag
				handle, so you can sanity-check responsive layouts without leaving the editor.
			</p>
		</section>

		<!-- ── Terminal ──────────────────────────────────────────────── -->
		<section id="terminal">
			<h2>Terminal</h2>
			<p>
				A real shell into the running sandbox container, always available below the
				Editor/Preview panel — not a separate tab you have to switch to. Use it to run
				scaffolding CLIs, install extra packages, inspect logs, or debug directly.
			</p>

			<div class="callout callout-info">
				The terminal connects over a WebSocket authenticated with a short-lived
				(5-minute) token minted specifically for that connection — it is re-issued
				automatically, you never have to think about it.
			</div>
		</section>

		<!-- ── Live Preview ──────────────────────────────────────────── -->
		<section id="preview">
			<h2>Live Preview</h2>
			<p>
				Every sandbox gets its own preview URL, routed through Traefik straight to the
				running dev server:
			</p>

			<div class="code-block">
				<div class="code-header">
					<span class="code-label">preview URL format</span>
					<button class="copy-btn" onclick={() => copyCode('preview-url', snip.previewUrl)}>
						{#if copied === 'preview-url'}<Check size={12} />{:else}<Copy size={12} />{/if}
					</button>
				</div>
				<pre>{snip.previewUrl}</pre>
			</div>

			<p>
				The preview link works for anyone with the URL — no Shipyard login required —
				so you can share a running sandbox with a teammate or a client directly.
			</p>

			<div class="callout callout-tip">
				Visiting a stopped sandbox's preview URL automatically wakes it back up. The
				first request after a cold start waits for the container and dev server to
				come up before the page loads.
			</div>
		</section>

		<!-- ── Idle Timeout & Quota ─────────────────────────────────── -->
		<section id="idle-and-quota">
			<h2>Idle Timeout & Quota</h2>
			<p>
				Sandboxes are meant to be disposable, not long-running services — Shipyard
				stops them automatically when nobody's using them, and caps how many can run
				at once per organization.
			</p>

			<h3>Idle timeout</h3>
			<p>
				While the editor tab is open it sends a periodic heartbeat. A sandbox with no
				heartbeat for <strong>20 minutes</strong> (the default; configurable per
				deployment) is automatically stopped — its files and data are untouched, only
				the running container and dev server are torn down. Opening the app again
				starts it right back up.
			</p>

			<h3>Concurrent sandbox limit</h3>
			<p>
				Each plan sets a maximum number of sandboxes that may be <em>running or
				starting</em> at the same time across the whole organization. Stop an unused
				sandbox — or wait for one to idle out — to free up a slot.
			</p>

			<div class="callout callout-info">
				Stopping a sandbox doesn't delete it. Files on its volume, its assigned
				template, and its preview URL are all preserved — starting it again picks up
				exactly where you left off.
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
		background: none; border: none; padding: 0;
		color: #60a5fa; font-size: inherit; font-family: inherit;
		cursor: pointer; text-decoration: underline; text-underline-offset: 3px;
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
