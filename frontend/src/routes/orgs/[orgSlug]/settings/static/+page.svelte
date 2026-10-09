<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm, isAdminRole } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { FileText, RefreshCw, ChevronRight, Globe, ScrollText, Square, WifiOff, Play, Wifi } from '@lucide/svelte';
	import { Card, Button, Badge, StatusDot, ListRow, InlineAlert, EmptyState, Spinner } from '$lib/components/ui';
	import type { LogLevel } from '$lib/api/types';
	import LogViewer from '$lib/components/LogViewer.svelte';

	interface NginxConfEntry { name: string }
	interface NginxConfList {
		dir: string;
		files: NginxConfEntry[];
		error?: string;
	}
	interface NginxConfFile {
		name: string;
		content?: string;
		exists: boolean;
		error?: string;
	}

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let isAdmin  = $derived(isAdminRole(myRole));
	let canRead  = $derived(
		isAdmin ||
		can(myRole, myPerms, perm(orgId, 'static', 'read')) ||
		can(myRole, myPerms, perm(orgId, 'infra', 'read')) ||
		can(myRole, myPerms, perm(orgId, 'settings', 'read'))
	);

	let confList   = $state<NginxConfList | null>(null);
	let loading    = $state(true);
	let listError  = $state('');

	let selected    = $state<string | null>(null);
	let fileContent = $state<NginxConfFile | null>(null);
	let loadingFile = $state(false);

	async function loadList() {
		loading = true;
		listError = '';
		const res = await api.get<NginxConfList>(`/admin/nginx-static/confs?org_id=${orgId}`);
		if (res.data) {
			confList = res.data;
			if (res.data.error) listError = res.data.error;
		} else if (res.error) {
			listError = res.error.message;
		}
		loading = false;
	}

	async function selectConf(name: string) {
		if (selected === name) {
			selected = null;
			fileContent = null;
			return;
		}
		selected = name;
		loadingFile = true;
		fileContent = null;
		const res = await api.get<NginxConfFile>(`/admin/nginx-static/confs/${encodeURIComponent(name)}?org_id=${orgId}`);
		if (res.data) fileContent = res.data;
		loadingFile = false;
	}

	function siteIdFromName(name: string): string {
		return name.replace(/\.conf$/, '');
	}

	onMount(() => {
		if (orgId) loadList();
	});

	$effect(() => {
		if (orgId && canRead) loadList();
	});

	// ── Log streaming ─────────────────────────────────────────────────
	interface LogLine { timestamp: string; level: LogLevel; message: string; }
	type LogStatus = 'idle' | 'connecting' | 'connected' | 'error';
	let logStatus  = $state<LogStatus>('idle');
	let logs       = $state<LogLine[]>([]);
	let logError   = $state('');
	let logSource: EventSource | null = null;

	function parseNginxLine(raw: string): LogLine {
		const now = new Date().toISOString();
		
		// Error log format, e.g. 2026/07/08 17:35:11 [emerg] 1#1: ...
		const errorMatch = raw.match(/^(\d{4}\/\d{2}\/\d{2} \d{2}:\d{2}:\d{2}) \[(\w+)\] (.*)$/);
		if (errorMatch) {
			const timeStr = errorMatch[1].replace(/\//g, '-').replace(' ', 'T') + 'Z';
			return {
				timestamp: timeStr,
				level: normalizeLevel(errorMatch[2]),
				message: errorMatch[3]
			};
		}
		
		// Access log format, e.g. 172.18.0.1 - - [09/Jul/2026:06:14:48 +0000] "GET / HTTP/1.1" ...
		const accessMatch = raw.match(/^([\d.]+|[a-fA-F\d:]+) - - \[(.*?)\] (.*)$/);
		if (accessMatch) {
			const dateStr = accessMatch[2];
			let timestamp = now;
			try {
				const parts = dateStr.split(':');
				if (parts.length >= 4) {
					const datePart = parts[0];
					const timePart = `${parts[1]}:${parts[2]}:${parts[3]}`;
					const formatted = datePart.replace(/\//g, ' ') + ' ' + timePart;
					timestamp = new Date(formatted).toISOString();
				}
			} catch {}
			return {
				timestamp,
				level: 'info',
				message: `${accessMatch[1]} - ${accessMatch[3]}`
			};
		}
		
		return { timestamp: now, level: 'info', message: raw };
	}

	function normalizeLevel(raw: string): LogLevel {
		const l = raw.toLowerCase();
		if (l === 'debug') return 'debug';
		if (l === 'warn' || l === 'warning' || l === 'notice') return 'warn';
		if (l === 'error' || l === 'err' || l === 'emerg' || l === 'crit' || l === 'alert') return 'error';
		return 'info';
	}

	function connectLogs() {
		if (logSource) return;
		logStatus = 'connecting';
		logError = '';
		logs = [];

		const es = new EventSource(`/api/admin/nginx-static/logs/stream?org_id=${orgId}`);
		logSource = es;

		es.onopen = () => { logStatus = 'connected'; };

		es.onmessage = (e) => {
			if (!e.data?.trim()) return;
			logs = [...logs, parseNginxLine(e.data)];
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

	onDestroy(() => {
		logSource?.close();
	});
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canRead}
	message="You need the 'View static server' or 'View infrastructure' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if canRead}
<div class="static-page">

	<div class="page-toolbar">
		<div class="toolbar-left">
			<Globe size={15} />
			<span class="toolbar-title">Static Server</span>
			{#if confList}
				<Badge tone="neutral">{confList.files.length} site{confList.files.length === 1 ? '' : 's'}</Badge>
			{/if}
		</div>
		<Button variant="secondary" onclick={loadList} disabled={loading}>
			{#if loading}<Spinner size={14} tone="current" />{:else}<RefreshCw size={14} />{/if}
			Refresh
		</Button>
	</div>

	{#if listError}
		<div role="alert"><InlineAlert tone="error">{listError}</InlineAlert></div>
	{/if}

	{#if loading}
		<div class="loading"><Spinner size={18} /> Loading nginx conf files…</div>
	{:else if !confList || confList.files.length === 0}
		<EmptyState message="No .conf files found in {confList?.dir ?? '/etc/nginx/conf.d'}" />
	{:else}

		<Card padding="0">
		<div class="conf-layout">

			<!-- File list -->
			<div class="conf-list-panel">
				<div class="panel-header">
					<FileText size={13} />
					<span>{confList.dir}</span>
				</div>
				<ul class="conf-list">
					{#each confList.files as entry (entry.name)}
						<li>
							<button
								class={selected === entry.name ? 'conf-item active' : 'conf-item'}
								onclick={() => selectConf(entry.name)}
							>
								<ListRow iconTone="green" title={entry.name} meta={siteIdFromName(entry.name)}>
									{#snippet icon()}<StatusDot status="running" />{/snippet}
									{#snippet trailing()}<ChevronRight size={13} class="conf-chevron" />{/snippet}
								</ListRow>
							</button>
						</li>
					{/each}
				</ul>
			</div>

			<!-- File content -->
			<div class="conf-content-panel">
				{#if !selected}
					<div class="content-empty">
						<FileText size={24} />
						<span>Select a conf file to view its content</span>
					</div>
				{:else if loadingFile}
					<div class="content-empty"><Spinner size={18} /> Loading…</div>
				{:else if fileContent?.error}
					<div class="content-empty"><InlineAlert tone="error">{fileContent.error}</InlineAlert></div>
				{:else if fileContent?.content}
					<div class="content-header">
						<span class="content-filename">{fileContent.name}</span>
					</div>
					<pre class="conf-code">{fileContent.content}</pre>
				{/if}
			</div>

		</div>
		</Card>

	{/if}

	<!-- ── Static Site Logs ───────────────────────────────────────── -->
	<Card padding="0">
		<div class="log-section-header">
			<div class="log-title">
				<div class="section-icon"><ScrollText size={16} /></div>
				<div>
					<h2 class="section-title">Static Server Logs</h2>
					<p class="section-desc">Real-time log stream from the <code>shipyard-nginx-static</code> container.</p>
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
	.static-page { display: flex; flex-direction: column; gap: 16px; }

	.page-toolbar {
		display: flex; align-items: center; justify-content: space-between;
	}
	.toolbar-left { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); font-size: 13px; }
	.toolbar-title { font-weight: 600; color: var(--text-primary); }

	.loading { display: flex; align-items: center; justify-content: center; gap: 10px; padding: 60px; color: var(--text-muted); font-size: 13px; }

	.conf-layout {
		display: grid;
		grid-template-columns: 260px 1fr;
		gap: 0;
		min-height: 400px;
	}

	.conf-list-panel {
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.panel-header {
		display: flex; align-items: center; gap: 7px;
		padding: 10px 14px;
		font-size: 11px; font-weight: 600; color: var(--text-muted);
		background: var(--bg-elevated); border-bottom: 1px solid var(--border);
		font-family: var(--font-mono);
	}
	.conf-list { list-style: none; margin: 0; padding: 0; overflow-y: auto; flex: 1; }

	.conf-item {
		width: 100%;
		display: block;
		padding: 0 14px;
		background: transparent; border: none;
		cursor: pointer; text-align: left;
		font-family: var(--font-sans);
		transition: background var(--transition-fast);
	}
	.conf-item:hover { background: var(--bg-elevated); }
	.conf-item.active { background: color-mix(in srgb, var(--accent) 8%, transparent); }
	.conf-item.active :global(.conf-chevron) { color: var(--accent); }
	.conf-item :global(.ui-list-row-title),
	.conf-item :global(.ui-list-row-meta) { font-family: var(--font-mono); }
	:global(.conf-chevron) { color: var(--text-dim); flex-shrink: 0; transition: color var(--transition-fast); }

	.conf-content-panel {
		display: flex;
		flex-direction: column;
		overflow: hidden;
		min-width: 0;
	}
	.content-empty {
		flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; color: var(--text-muted); font-size: 13px; padding: 16px;
	}
	.content-header {
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.content-filename { font-size: 12px; font-weight: 600; color: var(--text-primary); font-family: var(--font-mono); }
	.conf-code {
		margin: 0;
		padding: 16px;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		color: var(--text-secondary);
		white-space: pre;
		overflow: auto;
		flex: 1;
		background: transparent;
	}

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
	.section-icon { color: var(--text-secondary); margin-top: 2px; }
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 2px 0; }
	.section-desc { font-size: 12px; color: var(--text-muted); margin: 0; }

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

	@media (max-width: 767px) {
		.conf-layout { grid-template-columns: 1fr; }
		.conf-list-panel { border-right: none; border-bottom: 1px solid var(--border); max-height: 220px; }
		.log-section-header { flex-wrap: wrap; gap: 10px; padding: 12px 16px; }
		.log-controls { width: 100%; justify-content: flex-end; }
	}
</style>
