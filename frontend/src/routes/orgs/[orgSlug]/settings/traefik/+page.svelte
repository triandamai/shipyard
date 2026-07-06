<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import type { TraefikFileResponse, TraefikDynamicResponse } from '$lib/api/types';
	import type { LogLevel } from '$lib/api/types';
	import LogViewer from '$lib/components/LogViewer.svelte';
	import {
		Server, Save, Check, AlertCircle, Copy, CheckCheck,
		FileCode2, FolderOpen, RefreshCw, FileX, Loader2,
		ScrollText, Play, Square, Wifi, WifiOff
	} from '@lucide/svelte';

	let orgId            = $derived($orgStore.activeOrg?.id ?? '');
	let myRole           = $derived($orgStore.myMembership?.role ?? null);
	let myPerms          = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let canSettingsRead  = $derived(can(myRole, myPerms, perm(orgId, 'settings', 'read')));

	interface TraefikSettings {
		main_domain?: string;
		traefik_network?: string;
		traefik_entrypoint_http?: string;
		traefik_entrypoint_https?: string;
		traefik_cert_resolver?: string;
	}

	let settings  = $state<TraefikSettings>({});
	let loading   = $state(true);
	let saving    = $state(false);
	let saved     = $state(false);
	let saveError = $state('');

	// ── Explorer tab ─────────────────────────────────────────────────
	type ExplorerTab = 'static' | 'dynamic' | 'template';
	let activeTab = $state<ExplorerTab>('static');

	// static file state
	let staticFile  = $state<TraefikFileResponse | null>(null);
	let staticLoading = $state(false);

	// dynamic dir state
	let dynamicDir    = $state<TraefikDynamicResponse | null>(null);
	let dynamicLoading = $state(false);
	let selectedFile  = $state<string | null>(null);
	let selectedContent = $state<TraefikFileResponse | null>(null);
	let fileLoading   = $state(false);

	// template tab (generated YAML — kept as reference)
	type TplTab = 'traefik' | 'stack';
	let activeTpl = $state<TplTab>('traefik');

	let network      = $derived(settings.traefik_network       || 'platform_proxy');
	let httpEp       = $derived(settings.traefik_entrypoint_http  || 'web');
	let httpsEp      = $derived(settings.traefik_entrypoint_https || 'websecure');
	let certResolver = $derived(settings.traefik_cert_resolver  || 'letsencrypt');
	let domain       = $derived(settings.main_domain           || 'example.com');

	let traefikYaml = $derived(`# Traefik v3 — Static Configuration
# Place this file at /etc/traefik/traefik.yml on your Traefik host.
# Regenerate from Shipyard whenever you change proxy settings.

api:
  dashboard: true
  insecure: false

entryPoints:
  ${httpEp}:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: ${httpsEp}
          scheme: https
  ${httpsEp}:
    address: ":443"

providers:
  docker:
    swarmMode: true
    exposedByDefault: false
    network: ${network}
    endpoint: "unix:///var/run/docker.sock"
    watch: true

certificatesResolvers:
  ${certResolver}:
    acme:
      email: "admin@${domain}"
      storage: /letsencrypt/acme.json
      httpChallenge:
        entryPoint: ${httpEp}

log:
  level: INFO

accessLog: {}`);

	let stackYaml = $derived(`# Docker Swarm Stack — deploy Traefik as a global manager service
# Run: docker stack deploy -c docker-stack.yml traefik
#
# Prerequisites:
#   1. Create the overlay network first:
#      docker network create --driver overlay --attachable ${network}
#   2. Copy traefik.yml to /etc/traefik/traefik.yml on every manager node.

services:
  traefik:
    image: traefik:v3.0
    command:
      - "--configFile=/etc/traefik/traefik.yml"
    ports:
      - target: 80
        published: 80
        protocol: tcp
        mode: host
      - target: 443
        published: 443
        protocol: tcp
        mode: host
    volumes:
      - /var/run/docker.sock:/var/run/docker.sock:ro
      - /etc/traefik:/etc/traefik:ro
      - traefik-certs:/letsencrypt
    networks:
      - ${network}
    deploy:
      mode: global
      placement:
        constraints:
          - node.role == manager
      restart_policy:
        condition: on-failure
        delay: 5s
      update_config:
        parallelism: 1
        delay: 10s

networks:
  ${network}:
    external: true

volumes:
  traefik-certs:
    driver: local`);

	// ── Log streaming ─────────────────────────────────────────────────
	interface LogLine { timestamp: string; level: LogLevel; message: string; }

	type LogStatus = 'idle' | 'connecting' | 'connected' | 'error';
	let logStatus  = $state<LogStatus>('idle');
	let logs       = $state<LogLine[]>([]);
	let logError   = $state('');
	let logSource: EventSource | null = null;

	function parseTraefikLine(raw: string): LogLine {
		const now = new Date().toISOString();
		// JSON format: {"level":"info","msg":"...","time":"..."}
		try {
			const j = JSON.parse(raw);
			if (j.level && (j.msg || j.message)) {
				return { timestamp: j.time ?? j.timestamp ?? now, level: normalizeLevel(j.level), message: j.msg ?? j.message };
			}
		} catch {}
		// Text format: time="..." level=info msg="..."
		const timeMatch = raw.match(/time="([^"]+)"/);
		const levelMatch = raw.match(/\blevel=(\w+)/);
		const msgMatch = raw.match(/\bmsg="([^"]+)"/);
		if (levelMatch || msgMatch) {
			return { timestamp: timeMatch?.[1] ?? now, level: normalizeLevel(levelMatch?.[1] ?? 'info'), message: msgMatch?.[1] ?? raw };
		}
		return { timestamp: now, level: 'info', message: raw };
	}

	function normalizeLevel(raw: string): LogLevel {
		const l = raw.toLowerCase();
		if (l === 'debug') return 'debug';
		if (l === 'warn' || l === 'warning') return 'warn';
		if (l === 'error' || l === 'err' || l === 'fatal' || l === 'panic') return 'error';
		return 'info';
	}

	function connectLogs() {
		if (logSource) return;
		logStatus = 'connecting';
		logError = '';
		logs = [];

		const es = new EventSource('/api/settings/traefik/logs/stream');
		logSource = es;

		es.onopen = () => { logStatus = 'connected'; };

		es.onmessage = (e) => {
			if (!e.data?.trim()) return;
			logs = [...logs, parseTraefikLine(e.data)];
		};

		es.addEventListener('error', (e: MessageEvent) => {
			logError = e.data ?? 'Stream error';
			logStatus = 'error';
		});

		es.onerror = () => {
			if (logStatus === 'connecting') {
				logError = 'Could not connect to log stream';
				logStatus = 'error';
				es.close();
				logSource = null;
			}
		};
	}

	function disconnectLogs() {
		logSource?.close();
		logSource = null;
		logStatus = 'idle';
	}

	onDestroy(() => { logSource?.close(); });

	// ── Copy state ────────────────────────────────────────────────────
	let copied = $state(false);

	function currentCopyContent(): string {
		if (activeTab === 'static')  return staticFile?.content ?? '';
		if (activeTab === 'dynamic') return selectedContent?.content ?? '';
		return activeTpl === 'traefik' ? traefikYaml : stackYaml;
	}

	async function copyContent() {
		const text = currentCopyContent();
		if (!text) return;
		await navigator.clipboard.writeText(text);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	// ── YAML syntax highlighting ──────────────────────────────────────
	function highlightYaml(yaml: string): string {
		return yaml
			.split('\n')
			.map(line => {
				const escaped = line
					.replace(/&/g, '&amp;')
					.replace(/</g, '&lt;')
					.replace(/>/g, '&gt;');
				if (/^\s*#/.test(line)) {
					return `<span class="y-comment">${escaped}</span>`;
				}
				return escaped.replace(
					/^(\s*)([\w-]+)(\s*:\s*)(.*)/,
					(_, indent, key, sep, val) => {
						const styledVal = val
							.replace(/(&quot;[^&]*&quot;|'[^']*')/g, '<span class="y-string">$1</span>')
							.replace(/\b(true|false)\b/g, '<span class="y-bool">$1</span>')
							.replace(/\b(\d+)\b/g, '<span class="y-num">$1</span>');
						return `${indent}<span class="y-key">${key}</span>${sep}${styledVal}`;
					}
				);
			})
			.join('\n');
	}

	// ── Data loading ──────────────────────────────────────────────────
	async function loadStaticFile() {
		staticLoading = true;
		const res = await api.getTraefikStatic();
		if (res.data) staticFile = res.data;
		staticLoading = false;
	}

	async function loadDynamicDir() {
		dynamicLoading = true;
		selectedFile = null;
		selectedContent = null;
		const res = await api.getTraefikDynamic();
		if (res.data) {
			dynamicDir = res.data;
			if (res.data.files.length > 0) {
				await selectDynamicFile(res.data.files[0].name);
			}
		}
		dynamicLoading = false;
	}

	async function selectDynamicFile(name: string) {
		selectedFile = name;
		fileLoading = true;
		selectedContent = null;
		const res = await api.getTraefikDynamicFile(name);
		if (res.data) selectedContent = res.data;
		fileLoading = false;
	}

	async function switchTab(tab: ExplorerTab) {
		activeTab = tab;
		if (tab === 'static' && staticFile === null && !staticLoading) {
			await loadStaticFile();
		}
		if (tab === 'dynamic' && dynamicDir === null && !dynamicLoading) {
			await loadDynamicDir();
		}
	}

	// ── Config form save ──────────────────────────────────────────────
	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true; saved = false; saveError = '';
		try {
			const res = await api.put<TraefikSettings>('/settings', settings);
			if (res.error) saveError = res.error.message;
			else {
				saved = true;
				setTimeout(() => (saved = false), 3000);
				// Invalidate cached file results so refresh picks up new paths
				staticFile = null;
				dynamicDir = null;
				selectedContent = null;
			}
		} finally { saving = false; }
	}

	onMount(async () => {
		const res = await api.get<TraefikSettings>('/settings');
		if (res.data) settings = res.data;
		loading = false;
		// Eagerly load the static file (default tab)
		await loadStaticFile();
	});
</script>

<PermissionDeniedDialog open={membershipLoaded && !!orgId && !canSettingsRead} />

{#if loading}
	<div class="loading"><div class="spinner"></div><span>Loading…</span></div>
{:else if canSettingsRead}
	<div class="traefik-page">

		<!-- ── Config form ────────────────────────────────────────── -->
		<form class="config-form" onsubmit={save}>
			<section class="settings-section">
				<div class="section-header">
					<div class="section-icon"><Server size={16} /></div>
					<div>
						<h2 class="section-title">Traefik Configuration</h2>
						<p class="section-desc">Proxy settings used when generating service labels. Must match your Traefik deployment.</p>
					</div>
				</div>
				<div class="fields-grid">
					<div class="field">
						<label class="field-label" for="traefik-network">Docker Network</label>
						<input id="traefik-network" class="field-input font-mono" type="text"
							bind:value={settings.traefik_network} placeholder="platform_proxy" />
						<span class="field-hint">Overlay network shared between Traefik and all services.</span>
					</div>
					<div class="field">
						<label class="field-label" for="traefik-resolver">Cert Resolver</label>
						<input id="traefik-resolver" class="field-input font-mono" type="text"
							bind:value={settings.traefik_cert_resolver} placeholder="letsencrypt" />
						<span class="field-hint">Must match the key under <code>certificatesResolvers</code> in traefik.yml.</span>
					</div>
					<div class="field">
						<label class="field-label" for="traefik-http">HTTP Entrypoint</label>
						<input id="traefik-http" class="field-input font-mono" type="text"
							bind:value={settings.traefik_entrypoint_http} placeholder="web" />
						<span class="field-hint">Port 80 — redirects to HTTPS.</span>
					</div>
					<div class="field">
						<label class="field-label" for="traefik-https">HTTPS Entrypoint</label>
						<input id="traefik-https" class="field-input font-mono" type="text"
							bind:value={settings.traefik_entrypoint_https} placeholder="websecure" />
						<span class="field-hint">Port 443 with TLS.</span>
					</div>
	</div>

				{#if saveError}
					<div class="error-banner"><AlertCircle size={13} />{saveError}</div>
				{/if}

				<div class="form-footer">
					<span class="footer-hint">
						<AlertCircle size={12} />
						Traefik must be deployed on the same Swarm and connected to the network above.
					</span>
					<button class="btn btn-primary save-btn" type="submit" disabled={saving}>
						{#if saving}<div class="btn-spinner"></div>Saving…
						{:else if saved}<Check size={14} />Saved
						{:else}<Save size={14} />Save
						{/if}
					</button>
				</div>
			</section>
		</form>

		<!-- ── File Explorer ──────────────────────────────────────── -->
		<section class="explorer-section">
			<div class="explorer-header">
				<div class="explorer-tabs">
					<button
						class="explorer-tab"
						class:active={activeTab === 'static'}
						onclick={() => switchTab('static')}
					>
						<FileCode2 size={13} />
						traefik.yml
					</button>
					<button
						class="explorer-tab"
						class:active={activeTab === 'dynamic'}
						onclick={() => switchTab('dynamic')}
					>
						<FolderOpen size={13} />
						dynamic/
					</button>
					<button
						class="explorer-tab"
						class:active={activeTab === 'template'}
						onclick={() => switchTab('template')}
					>
						<FileCode2 size={13} />
						Templates
					</button>
				</div>
				<div class="explorer-actions">
					{#if activeTab === 'static'}
						<span class="path-chip">/etc/traefik/traefik.yml</span>
						<button class="icon-btn" onclick={loadStaticFile} disabled={staticLoading} title="Refresh">
							<RefreshCw size={13} class={staticLoading ? 'spin' : ''} />
						</button>
					{:else if activeTab === 'dynamic'}
						<span class="path-chip">/etc/traefik/dynamic/</span>
						<button class="icon-btn" onclick={loadDynamicDir} disabled={dynamicLoading} title="Refresh">
							<RefreshCw size={13} class={dynamicLoading ? 'spin' : ''} />
						</button>
					{:else}
						<div class="tpl-tabs">
							<button class="tpl-tab" class:active={activeTpl === 'traefik'} onclick={() => (activeTpl = 'traefik')}>traefik.yml</button>
							<button class="tpl-tab" class:active={activeTpl === 'stack'} onclick={() => (activeTpl = 'stack')}>docker-stack.yml</button>
						</div>
					{/if}
					<button class="copy-btn" class:copied onclick={copyContent} disabled={!currentCopyContent()}>
						{#if copied}<CheckCheck size={13} />Copied{:else}<Copy size={13} />Copy{/if}
					</button>
				</div>
			</div>

			<!-- Static file view -->
			{#if activeTab === 'static'}
				{#if staticLoading}
					<div class="file-loading"><Loader2 size={16} class="spin" /><span>Reading file…</span></div>
				{:else if staticFile?.error || (staticFile && !staticFile.exists)}
					<div class="file-error">
						<FileX size={16} />
						<span>{staticFile?.error ?? 'Cannot read /etc/traefik/traefik.yml'}</span>
					</div>
				{:else if staticFile?.content}
					{@const content = staticFile.content}
					<div class="yaml-body">
						<div class="line-numbers" aria-hidden="true">
							{#each content.split('\n') as _, i}<span>{i + 1}</span>{/each}
						</div>
						<pre class="yaml-pre">{@html highlightYaml(content)}</pre>
					</div>
				{:else}
					<div class="file-loading"><Loader2 size={16} class="spin" /><span>Loading…</span></div>
				{/if}

			<!-- Dynamic dir view -->
			{:else if activeTab === 'dynamic'}
				{#if dynamicLoading}
					<div class="file-loading"><Loader2 size={16} class="spin" /><span>Reading directory…</span></div>
				{:else if dynamicDir === null}
					<div class="file-loading"><span>Press refresh to load.</span></div>
				{:else if dynamicDir.error}
					<div class="file-error"><FileX size={16} /><span>{dynamicDir.error}</span></div>
				{:else if dynamicDir.files.length === 0}
					<div class="file-missing">
						<FolderOpen size={32} />
						<p>No dynamic config files</p>
						<span>/etc/traefik/dynamic/ is empty</span>
					</div>
				{:else}
					<div class="dynamic-pane">
						<div class="file-list">
							{#each dynamicDir.files as f}
								<button
									class="file-item"
									class:active={selectedFile === f.name}
									onclick={() => selectDynamicFile(f.name)}
								>
									<FileCode2 size={12} />
									{f.name}
								</button>
							{/each}
						</div>
						<div class="file-content">
							{#if fileLoading}
								<div class="file-loading"><Loader2 size={16} class="spin" /><span>Reading…</span></div>
							{:else if selectedContent?.error}
								<div class="file-error"><FileX size={16} /><span>{selectedContent.error}</span></div>
							{:else if selectedContent?.content}
								{@const content = selectedContent.content}
								<div class="yaml-body">
									<div class="line-numbers" aria-hidden="true">
										{#each content.split('\n') as _, i}<span>{i + 1}</span>{/each}
									</div>
									<pre class="yaml-pre">{@html highlightYaml(content)}</pre>
								</div>
							{:else}
								<div class="file-loading"><span>Select a file.</span></div>
							{/if}
						</div>
					</div>
				{/if}

			<!-- Template view -->
			{:else}
				{@const tplContent = activeTpl === 'traefik' ? traefikYaml : stackYaml}
				<div class="yaml-body">
					<div class="line-numbers" aria-hidden="true">
						{#each tplContent.split('\n') as _, i}<span>{i + 1}</span>{/each}
					</div>
					<pre class="yaml-pre">{@html highlightYaml(tplContent)}</pre>
				</div>
			{/if}
		</section>

		<!-- ── Traefik Logs ───────────────────────────────────────── -->
		<section class="log-section">
			<div class="log-section-header">
				<div class="log-title">
					<div class="section-icon"><ScrollText size={16} /></div>
					<div>
						<h2 class="section-title">Traefik Logs</h2>
						<p class="section-desc">Real-time log stream from the <code>shipyard-traefik</code> container.</p>
					</div>
				</div>
				<div class="log-controls">
					{#if logStatus === 'connected'}
						<span class="status-dot connected"></span>
						<span class="status-label">Live</span>
						<button class="btn btn-ghost btn-sm log-btn" onclick={disconnectLogs}>
							<Square size={12} />Stop
						</button>
					{:else if logStatus === 'connecting'}
						<Loader2 size={14} class="spin" />
						<span class="status-label muted">Connecting…</span>
					{:else if logStatus === 'error'}
						<WifiOff size={14} style="color:#EF4444" />
						<span class="status-label error">{logError}</span>
						<button class="btn btn-ghost btn-sm log-btn" onclick={connectLogs}>
							<Play size={12} />Retry
						</button>
					{:else}
						<button class="btn btn-primary btn-sm log-btn" onclick={connectLogs}>
							<Play size={12} />Connect
						</button>
					{/if}
				</div>
			</div>

			{#if logStatus === 'idle'}
				<div class="log-placeholder">
					<Wifi size={28} />
					<p>Press <strong>Connect</strong> to start streaming logs</p>
				</div>
			{:else}
				<div class="log-viewer-wrap">
					<LogViewer {logs} follow={true} maxHeight="420px" />
				</div>
			{/if}
		</section>

	</div>
{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }
	.spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.7s linear infinite; }

	.traefik-page {
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	/* ── Config form ── */
	.config-form { display: contents; }

	.settings-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	.section-header {
		display: flex; gap: 14px; padding: 18px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.section-icon {
		width: 32px; height: 32px; border-radius: var(--radius-md);
		background: rgba(37,99,235,0.1); color: var(--accent);
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0; margin-top: 1px;
	}

	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc  { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.fields-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 16px;
		padding: 18px 20px;
	}

	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-full { grid-column: 1 / -1; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }

	.field-input {
		background: var(--bg-base); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary);
		font-size: 13px; font-family: var(--font-sans);
		padding: 8px 10px; outline: none;
		transition: border-color var(--transition-fast);
	}
	.field-input.font-mono { font-family: var(--font-mono); }
	.field-input:focus { border-color: var(--accent); }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }

	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 20px; background: rgba(239,68,68,0.08); color: #EF4444; font-size: 13px; border-top: 1px solid rgba(239,68,68,0.2); }

	.form-footer {
		display: flex; align-items: center; justify-content: space-between; gap: 16px;
		padding: 12px 20px; border-top: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.footer-hint { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-dim); }
	.save-btn { display: flex; align-items: center; gap: 6px; min-width: 100px; justify-content: center; }
	.btn-spinner { width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite; }

	/* ── File Explorer ── */
	.explorer-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
		display: flex;
		flex-direction: column;
		min-height: 320px;
	}

	.explorer-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 0 12px 0 0;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		flex-shrink: 0;
		gap: 8px;
	}

	.explorer-tabs { display: flex; }

	.explorer-tab {
		display: flex; align-items: center; gap: 6px;
		padding: 11px 16px;
		font-size: 12px; font-weight: 500; font-family: var(--font-sans);
		background: transparent; border: none; border-bottom: 2px solid transparent;
		color: var(--text-dim); cursor: pointer;
		transition: color var(--transition-fast), border-color var(--transition-fast);
		white-space: nowrap;
	}
	.explorer-tab:hover { color: var(--text-primary); }
	.explorer-tab.active { color: var(--accent); border-bottom-color: var(--accent); }

	.explorer-actions { display: flex; align-items: center; gap: 8px; margin-left: auto; }

	.path-chip {
		font-size: 11px; font-family: var(--font-mono);
		color: var(--text-dim);
		background: var(--bg-base);
		padding: 3px 8px; border-radius: 4px;
		border: 1px solid var(--border);
		max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	.icon-btn {
		display: flex; align-items: center; justify-content: center;
		width: 28px; height: 28px; border-radius: var(--radius-sm);
		background: transparent; border: 1px solid var(--border);
		color: var(--text-muted); cursor: pointer;
		transition: all var(--transition-fast);
	}
	.icon-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.icon-btn:disabled { opacity: 0.5; cursor: default; }

	.tpl-tabs { display: flex; gap: 2px; }
	.tpl-tab {
		font-size: 11px; font-weight: 500; font-family: var(--font-mono);
		padding: 4px 10px; border-radius: 4px;
		background: transparent; border: 1px solid transparent;
		color: var(--text-dim); cursor: pointer;
		transition: all var(--transition-fast);
	}
	.tpl-tab:hover { color: var(--text-primary); }
	.tpl-tab.active { background: var(--bg-base); border-color: var(--border); color: var(--text-primary); }

	.copy-btn {
		display: flex; align-items: center; gap: 5px;
		font-size: 12px; font-weight: 500; font-family: var(--font-sans);
		padding: 5px 10px; border-radius: var(--radius-sm);
		background: transparent; border: 1px solid var(--border);
		color: var(--text-muted); cursor: pointer;
		transition: all var(--transition-fast);
	}
	.copy-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.copy-btn.copied { border-color: #22C55E; color: #22C55E; }
	.copy-btn:disabled { opacity: 0.4; cursor: default; }

	/* ── File loading / empty / error states ── */
	.file-loading {
		display: flex; align-items: center; justify-content: center;
		gap: 10px; padding: 60px 20px;
		color: var(--text-muted); font-size: 13px;
		flex: 1;
	}

	.file-error {
		display: flex; align-items: flex-start; gap: 10px; padding: 20px;
		color: #F87171; font-size: 13px; font-family: var(--font-mono);
		background: rgba(239,68,68,0.06);
		border-top: 1px solid rgba(239,68,68,0.15);
	}

	.file-missing {
		display: flex; flex-direction: column; align-items: center;
		justify-content: center; gap: 6px; padding: 48px 24px;
		color: var(--text-dim); text-align: center;
		flex: 1;
	}
	.file-missing p { font-size: 14px; font-weight: 600; color: var(--text-muted); margin: 4px 0 0; }
	.file-missing span { font-size: 12px; font-family: var(--font-mono); }

	/* ── Dynamic two-pane view ── */
	.dynamic-pane { display: flex; flex: 1; overflow: hidden; min-height: 280px; }

	.file-list {
		width: 180px; flex-shrink: 0;
		border-right: 1px solid var(--border);
		overflow-y: auto;
		padding: 8px 0;
		background: var(--bg-elevated);
	}

	.file-item {
		display: flex; align-items: center; gap: 8px;
		width: 100%; padding: 7px 14px;
		font-size: 12px; font-family: var(--font-mono); font-weight: 400;
		color: var(--text-muted); background: transparent; border: none;
		cursor: pointer; text-align: left;
		transition: background var(--transition-fast), color var(--transition-fast);
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}
	.file-item:hover { background: var(--bg-surface); color: var(--text-primary); }
	.file-item.active { background: rgba(37,99,235,0.1); color: var(--accent); }

	.file-content { flex: 1; overflow: hidden; display: flex; flex-direction: column; }

	/* ── YAML viewer ── */
	.yaml-body {
		display: flex;
		overflow: auto;
		font-family: var(--font-mono);
		font-size: 12.5px;
		line-height: 1.65;
		flex: 1;
	}

	.line-numbers {
		display: flex; flex-direction: column;
		padding: 16px 12px 16px 16px;
		text-align: right;
		color: var(--text-dim);
		background: var(--bg-elevated);
		border-right: 1px solid var(--border);
		user-select: none;
		flex-shrink: 0;
		font-size: 11.5px;
		line-height: 1.65;
		opacity: 0.6;
		min-width: 36px;
	}
	.line-numbers span { display: block; }

	.yaml-pre {
		margin: 0;
		padding: 16px 20px;
		white-space: pre;
		color: var(--text-secondary);
		flex: 1;
	}

	/* YAML token colours */
	:global(.y-comment) { color: var(--text-dim); font-style: italic; }
	:global(.y-key)     { color: #60A5FA; }
	:global(.y-string)  { color: #86EFAC; }
	:global(.y-bool)    { color: #F9A8D4; }
	:global(.y-num)     { color: #FCD34D; }

	/* ── Log section ── */
	.log-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	.log-section-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 14px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		gap: 16px;
		flex-shrink: 0;
	}

	.log-title { display: flex; align-items: flex-start; gap: 14px; }

	.log-controls { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }

	.status-dot {
		width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0;
	}
	.status-dot.connected { background: #22C55E; box-shadow: 0 0 6px #22C55E; animation: pulse 2s ease-in-out infinite; }
	@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.5; } }

	.status-label { font-size: 12px; font-weight: 500; color: var(--text-muted); }
	.status-label.muted { color: var(--text-dim); }
	.status-label.error { color: #EF4444; max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.log-btn { display: flex; align-items: center; gap: 5px; }

	.log-placeholder {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; padding: 48px 24px;
		color: var(--text-dim); font-size: 13px;
	}
	.log-placeholder p { margin: 0; color: var(--text-muted); }
	.log-placeholder strong { color: var(--text-primary); }

	.log-viewer-wrap { flex: 1; }

	@media (max-width: 639px) {
		.traefik-page { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.fields-grid { grid-template-columns: 1fr; padding: 14px 16px; }
		.form-footer { flex-direction: column; align-items: flex-start; gap: 12px; padding: 12px 16px; }
		.save-btn { width: 100%; justify-content: center; }
		.explorer-header { flex-wrap: wrap; padding: 0; gap: 0; }
		.explorer-tabs { overflow-x: auto; flex-wrap: nowrap; -webkit-overflow-scrolling: touch; scrollbar-width: none; width: 100%; border-bottom: 1px solid var(--border); }
		.explorer-tabs::-webkit-scrollbar { display: none; }
		.explorer-actions { width: 100%; padding: 8px 12px; border-bottom: 1px solid var(--border); overflow-x: auto; }
		.path-chip { max-width: 160px; }
		.dynamic-pane { flex-direction: column; min-height: 0; }
		.file-list { width: 100%; border-right: none; border-bottom: 1px solid var(--border); display: flex; flex-direction: row; overflow-x: auto; padding: 4px 8px; gap: 4px; }
		.file-item { width: auto; white-space: nowrap; padding: 6px 10px; border-radius: var(--radius-sm); border: 1px solid var(--border); }
		.file-item.active { border-color: var(--accent); }
		.log-section-header { flex-wrap: wrap; gap: 10px; padding: 12px 16px; }
		.log-controls { width: 100%; justify-content: flex-end; }
	}
</style>
