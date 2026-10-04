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
		FileCode2, FolderOpen, RefreshCw, FileX,
		ScrollText, Play, Square, Wifi, WifiOff
	} from '@lucide/svelte';
	import { Card, Button, FormField, TextField, Tabs, StatusDot, InlineAlert, Spinner } from '$lib/components/ui';

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

	// TextField binds a plain string, so the four editable keys are non-optional here
	// (they were `undefined` before load; now ''). Falsy fallbacks below are unchanged.
	let settings  = $state<TraefikSettings & Record<'traefik_network' | 'traefik_cert_resolver' | 'traefik_entrypoint_http' | 'traefik_entrypoint_https', string>>({
		traefik_network: '', traefik_cert_resolver: '', traefik_entrypoint_http: '', traefik_entrypoint_https: ''
	});

	const explorerTabs = [
		{ id: 'static', label: 'traefik.yml', icon: FileCode2 },
		{ id: 'dynamic', label: 'dynamic/', icon: FolderOpen },
		{ id: 'template', label: 'Templates', icon: FileCode2 }
	];
	const tplTabs = [
		{ id: 'traefik', label: 'traefik.yml' },
		{ id: 'stack', label: 'docker-stack.yml' }
	];
	let loading   = $state(true);
	let loaded    = $state(false);
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
		if (!loaded) return;
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
		if (res.data) { settings = { ...settings, ...res.data }; loaded = true; }
		loading = false;
		// Eagerly load the static file (default tab)
		await loadStaticFile();
	});
</script>

<PermissionDeniedDialog open={membershipLoaded && !!orgId && !canSettingsRead} onDismiss={() => history.back()} />

{#if loading}
	<div class="loading"><Spinner size={18} /><span>Loading…</span></div>
{:else if canSettingsRead}
	<div class="traefik-page">

		<!-- ── Config form ────────────────────────────────────────── -->
		<form class="config-form" onsubmit={save}>
			<Card padding="0">
				<div class="section-header">
					<div class="section-icon"><Server size={16} /></div>
					<div>
						<h2 class="section-title">Traefik Configuration</h2>
						<p class="section-desc">Proxy settings used when generating service labels. Must match your Traefik deployment.</p>
					</div>
				</div>
				<div class="fields-grid">
					<FormField label="Docker Network" for="traefik-network" hint="Overlay network shared between Traefik and all services.">
						<TextField id="traefik-network" type="text" bind:value={settings.traefik_network} placeholder="platform_proxy" />
					</FormField>
					<FormField label="Cert Resolver" for="traefik-resolver" hint="Must match the key under certificatesResolvers in traefik.yml.">
						<TextField id="traefik-resolver" type="text" bind:value={settings.traefik_cert_resolver} placeholder="letsencrypt" />
					</FormField>
					<FormField label="HTTP Entrypoint" for="traefik-http" hint="Port 80 — redirects to HTTPS.">
						<TextField id="traefik-http" type="text" bind:value={settings.traefik_entrypoint_http} placeholder="web" />
					</FormField>
					<FormField label="HTTPS Entrypoint" for="traefik-https" hint="Port 443 with TLS.">
						<TextField id="traefik-https" type="text" bind:value={settings.traefik_entrypoint_https} placeholder="websecure" />
					</FormField>
				</div>

				{#if saveError}
					<div class="save-error" role="alert"><InlineAlert tone="error">{saveError}</InlineAlert></div>
				{/if}

				<div class="form-footer">
					<span class="footer-hint">
						<AlertCircle size={12} />
						Traefik must be deployed on the same Swarm and connected to the network above.
					</span>
					<Button type="submit" disabled={saving || !loaded}>
						{#if saving}<Spinner size={12} tone="current" />Saving…
						{:else if saved}<Check size={14} />Saved
						{:else}<Save size={14} />Save
						{/if}
					</Button>
				</div>
			</Card>
		</form>

		<!-- ── File Explorer ──────────────────────────────────────── -->
		<Card padding="0">
			<div class="explorer-header">
				<Tabs tabs={explorerTabs} value={activeTab} onChange={(id) => switchTab(id as ExplorerTab)} ariaLabel="Traefik files" />
				<div class="explorer-actions">
					{#if activeTab === 'static'}
						<span class="path-chip">/etc/traefik/traefik.yml</span>
						<Button variant="secondary" size="icon" onclick={loadStaticFile} disabled={staticLoading} title="Refresh" aria-label="Refresh">
							{#if staticLoading}<Spinner size={13} tone="current" />{:else}<RefreshCw size={13} />{/if}
						</Button>
					{:else if activeTab === 'dynamic'}
						<span class="path-chip">/etc/traefik/dynamic/</span>
						<Button variant="secondary" size="icon" onclick={loadDynamicDir} disabled={dynamicLoading} title="Refresh" aria-label="Refresh">
							{#if dynamicLoading}<Spinner size={13} tone="current" />{:else}<RefreshCw size={13} />{/if}
						</Button>
					{:else}
						<Tabs tabs={tplTabs} value={activeTpl} onChange={(id) => (activeTpl = id as TplTab)} ariaLabel="Templates" />
					{/if}
					<Button variant="secondary" size="sm" onclick={copyContent} disabled={!currentCopyContent()}>
						{#if copied}<CheckCheck size={13} />Copied{:else}<Copy size={13} />Copy{/if}
					</Button>
				</div>
			</div>

			<!-- Static file view -->
			{#if activeTab === 'static'}
				{#if staticLoading}
					<div class="file-loading"><Spinner size={16} /><span>Reading file…</span></div>
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
					<div class="file-loading"><Spinner size={16} /><span>Loading…</span></div>
				{/if}

			<!-- Dynamic dir view -->
			{:else if activeTab === 'dynamic'}
				{#if dynamicLoading}
					<div class="file-loading"><Spinner size={16} /><span>Reading directory…</span></div>
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
									class={selectedFile === f.name ? 'file-item active' : 'file-item'}
									onclick={() => selectDynamicFile(f.name)}
								>
									<FileCode2 size={12} />
									{f.name}
								</button>
							{/each}
						</div>
						<div class="file-content">
							{#if fileLoading}
								<div class="file-loading"><Spinner size={16} /><span>Reading…</span></div>
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
		</Card>

		<!-- ── Traefik Logs ───────────────────────────────────────── -->
		<Card padding="0">
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
						<StatusDot status="running" />
						<span class="status-label">Live</span>
						<Button variant="ghost" size="sm" onclick={disconnectLogs}>
							<Square size={12} />Stop
						</Button>
					{:else if logStatus === 'connecting'}
						<Spinner size={14} />
						<span class="status-label muted">Connecting…</span>
					{:else if logStatus === 'error'}
						<WifiOff size={14} class="log-err-icon" />
						<span class="status-label error">{logError}</span>
						<Button variant="ghost" size="sm" onclick={connectLogs}>
							<Play size={12} />Retry
						</Button>
					{:else}
						<Button size="sm" onclick={connectLogs}>
							<Play size={12} />Connect
						</Button>
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
		</Card>

	</div>
{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }

	.traefik-page {
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	/* ── Config form ── */
	.config-form { display: contents; }

	.section-header {
		display: flex; gap: 14px; padding: 18px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.section-icon {
		width: 32px; height: 32px; border-radius: var(--radius-md);
		background: var(--accent-muted); color: var(--accent);
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
	.fields-grid :global(.ui-textfield) { font-family: var(--font-mono); }

	.save-error { padding: 0 20px 16px; }

	.form-footer {
		display: flex; align-items: center; justify-content: space-between; gap: 16px;
		padding: 12px 20px; border-top: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.footer-hint { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-dim); }

	/* ── File Explorer ── */
	.explorer-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 0 12px 0 0;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		gap: 8px;
	}

	.explorer-actions { display: flex; align-items: center; gap: 8px; margin-left: auto; }

	.path-chip {
		font-size: 11px; font-family: var(--font-mono);
		color: var(--text-dim);
		background: var(--bg-base);
		padding: 3px 8px; border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	/* ── File loading / empty / error states ── */
	.file-loading {
		display: flex; align-items: center; justify-content: center;
		gap: 10px; padding: 60px 20px;
		color: var(--text-muted); font-size: 13px;
		flex: 1;
	}

	.file-error {
		display: flex; align-items: flex-start; gap: 10px; padding: 20px;
		color: var(--accent-red); font-size: 13px; font-family: var(--font-mono);
		background: var(--accent-red-muted);
		border-top: 1px solid color-mix(in srgb, var(--accent-red) 20%, transparent);
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
	.file-item.active { background: var(--accent-muted); color: var(--accent); }

	.file-content { flex: 1; overflow: hidden; display: flex; flex-direction: column; min-width: 0; }

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
	:global(.y-key)     { color: var(--accent); }
	:global(.y-string)  { color: var(--accent-green); }
	:global(.y-bool)    { color: var(--accent-red); }
	:global(.y-num)     { color: var(--accent-yellow); }

	/* ── Log section ── */
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
	:global(.log-err-icon) { color: var(--accent-red); }

	.status-label { font-size: 12px; font-weight: 500; color: var(--text-muted); }
	.status-label.muted { color: var(--text-dim); }
	.status-label.error { color: var(--accent-red); max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.log-placeholder {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; padding: 48px 24px;
		color: var(--text-dim); font-size: 13px;
	}
	.log-placeholder p { margin: 0; color: var(--text-muted); }
	.log-placeholder strong { color: var(--text-primary); }

	@media (max-width: 639px) {
		.traefik-page { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.fields-grid { grid-template-columns: 1fr; padding: 14px 16px; }
		.save-error { padding: 0 16px 14px; }
		.form-footer { flex-direction: column; align-items: flex-start; gap: 12px; padding: 12px 16px; }
		.explorer-header { flex-wrap: wrap; padding: 0; gap: 0; }
		.explorer-actions { width: 100%; padding: 8px 12px; border-top: 1px solid var(--border); overflow-x: auto; }
		.path-chip { max-width: 160px; }
		.dynamic-pane { flex-direction: column; min-height: 0; }
		.file-list { width: 100%; border-right: none; border-bottom: 1px solid var(--border); display: flex; flex-direction: row; overflow-x: auto; padding: 4px 8px; gap: 4px; }
		.file-item { width: auto; white-space: nowrap; padding: 6px 10px; border-radius: var(--radius-sm); border: 1px solid var(--border); }
		.file-item.active { border-color: var(--accent); }
		.log-section-header { flex-wrap: wrap; gap: 10px; padding: 12px 16px; }
		.log-controls { width: 100%; justify-content: flex-end; }
	}
</style>
