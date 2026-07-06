<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import { page } from '$app/stores';
	import {
		Globe, Save, Check, AlertCircle, Loader2,
		RefreshCw, Terminal, Zap, PackageOpen
	} from '@lucide/svelte';
	interface PlatformSettings {
		main_domain?: string;
		traefik_network?: string;
		traefik_entrypoint_http?: string;
		traefik_entrypoint_https?: string;
		traefik_cert_resolver?: string;
		max_parallel_deployments?: number;
	}

	// ── Version info ────────────────────────────────────────────────────────────
	interface VersionInfo {
		current: string;
		git_sha: string;
		build_date: string;
		update_available: boolean;
		remote_sha: string | null;
	}
	let versionInfo    = $state<VersionInfo | null>(null);
	let loadingVersion = $state(false);
	let checkingUpdate = $state(false);

	function formatBuildDate(iso: string): string {
		if (!iso || iso === 'unknown') return '';
		try {
			return new Date(iso).toLocaleString('en-US', {
				year: 'numeric', month: 'short', day: 'numeric',
				hour: '2-digit', minute: '2-digit', timeZoneName: 'short'
			});
		} catch { return iso; }
	}

	async function checkForUpdates() {
		checkingUpdate = true;
		const res = await api.get<VersionInfo>('/admin/version?force=true');
		if (res.data) versionInfo = res.data;
		checkingUpdate = false;
	}

	// ── Update state ────────────────────────────────────────────────────────────
	type UpdateStatus = 'idle' | 'running' | 'done' | 'error' | 'disconnected';
	let updateStatus   = $state<UpdateStatus>('idle');
	let updateLog      = $state<string[]>([]);
	let updateLogEl    = $state<HTMLDivElement | null>(null);
	let updateSource: EventSource | null = null;

	function startUpdate() {
		if (!canUpdate || updateStatus === 'running') return;
		updateLog = [];
		updateStatus = 'running';

		updateSource?.close();
		updateSource = new EventSource('/api/admin/update/stream');

		updateSource.onmessage = (e) => {
			if (!e.data?.trim()) return;
			updateLog = [...updateLog, e.data];
			if (updateLogEl) requestAnimationFrame(() => {
				if (updateLogEl) updateLogEl.scrollTop = updateLogEl.scrollHeight;
			});
		};

		updateSource.addEventListener('done', (e: MessageEvent) => {
			updateLog = [...updateLog, `✓ ${e.data}`];
			updateStatus = 'done';
			updateSource?.close();
			updateSource = null;
		});

		updateSource.addEventListener('error', (e: MessageEvent) => {
			if (e.data) {
				updateLog = [...updateLog, `✗ ${e.data}`];
				updateStatus = 'error';
				updateSource?.close();
				updateSource = null;
			}
		});

		// onerror fires when the SSE connection drops — expected when backend restarts
		updateSource.onerror = () => {
			if (updateStatus === 'running') {
				updateLog = [...updateLog, '⟳ Connection lost — services are restarting. Reload the page when ready.'];
				updateStatus = 'disconnected';
			}
			updateSource?.close();
			updateSource = null;
		};
	}

	function clearUpdateLog() {
		updateLog = [];
		updateStatus = 'idle';
	}

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let canUpdate = $derived(can(myRole, myPerms, perm(orgId, 'system', 'update')));

	let settings    = $state<PlatformSettings>({});
	let loading     = $state(true);
	let saving      = $state(false);
	let saved       = $state(false);
	let saveError   = $state('');

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true; saved = false; saveError = '';
		try {
			const res = await api.put<PlatformSettings>('/settings', settings);
			if (res.error) saveError = res.error.message;
			else { saved = true; setTimeout(() => (saved = false), 3000); }
		} finally { saving = false; }
	}

	onMount(async () => {
		const res = await api.get<PlatformSettings>('/settings');
		if (res.data) settings = res.data;
		loading = false;

		loadingVersion = true;
		const vRes = await api.get<VersionInfo>('/admin/version');
		if (vRes.data) versionInfo = vRes.data;
		loadingVersion = false;
	});
</script>

{#if loading}
	<div class="loading">
		<div class="spinner"></div>
		<span>Loading settings…</span>
	</div>
{:else}
	<form class="settings-form" onsubmit={save}>

		<!-- Main Domain -->
		<section class="settings-section">
			<div class="section-header">
				<div class="section-icon"><Globe size={16} /></div>
				<div>
					<h2 class="section-title">Main Domain</h2>
					<p class="section-desc">Base domain for all deployed services (e.g. <code>example.com</code>). Services get subdomains like <code>my-service.example.com</code>.</p>
				</div>
			</div>
			<div class="fields">
				<div class="field">
					<label class="field-label" for="main-domain">Base Domain</label>
					<input id="main-domain" class="field-input" type="text" bind:value={settings.main_domain} placeholder="example.com" />
					<span class="field-hint">Leave blank to use manual domain assignments per service.</span>
				</div>
			</div>
		</section>

{#if saveError}
			<div class="error-banner"><AlertCircle size={14} />{saveError}</div>
		{/if}

		<div class="save-bar">
			<button class="btn btn-primary save-btn" type="submit" disabled={saving}>
				{#if saving}<div class="btn-spinner"></div>Saving…
				{:else if saved}<Check size={14} />Saved
				{:else}<Save size={14} />Save Settings
				{/if}
			</button>
		</div>
	</form>

	<!-- Platform Update -->
	{#if canUpdate}
	<section class="settings-section update-section">
		<div class="section-header">
			<div class="section-icon update-icon"><PackageOpen size={16} /></div>
			<div>
				<h2 class="section-title">Platform Update</h2>
				<p class="section-desc">
					Pull the latest Docker images from the registry and restart all Shipyard services.
					The connection will drop briefly while the backend restarts — that's expected.
				</p>
			</div>
		</div>

		<!-- Version info bar -->
		<div class="version-info-bar">
			{#if loadingVersion}
				<span class="version-loading">Checking version…</span>
			{:else if versionInfo}
				<div class="version-chip">
					<span class="version-label">Running</span>
					<code class="version-sha">{versionInfo.git_sha}</code>
					{#if versionInfo.build_date && versionInfo.build_date !== 'unknown'}
						<span class="version-date">{formatBuildDate(versionInfo.build_date)}</span>
					{/if}
				</div>
				{#if versionInfo.update_available && versionInfo.remote_sha}
					<span class="version-badge update-avail">Update available → <code>{versionInfo.remote_sha}</code></span>
				{:else}
					<span class="version-badge up-to-date">Up to date</span>
				{/if}
				<button class="refresh-version-btn" disabled={checkingUpdate} onclick={checkForUpdates}>
					{#if checkingUpdate}<Loader2 size={11} class="spin" />Checking…{:else}<RefreshCw size={11} />Check{/if}
				</button>
			{/if}
		</div>

		<div class="update-body">
			<div class="update-actions">
				<button
					class="btn btn-update"
					disabled={updateStatus === 'running'}
					onclick={startUpdate}
				>
					{#if updateStatus === 'running'}
						<Loader2 size={14} class="spin-icon" />Running update…
					{:else}
						<RefreshCw size={14} />Pull &amp; Restart
					{/if}
				</button>

				{#if updateStatus === 'done'}
					<span class="update-badge done"><Check size={11} />Done</span>
				{:else if updateStatus === 'error'}
					<span class="update-badge error"><AlertCircle size={11} />Failed</span>
				{:else if updateStatus === 'disconnected'}
					<span class="update-badge restarting"><Zap size={11} />Restarting…</span>
				{/if}

				{#if updateLog.length > 0 && updateStatus !== 'running'}
					<button class="clear-log-btn" onclick={clearUpdateLog}>Clear</button>
				{/if}
			</div>

			{#if updateLog.length > 0}
				<div class="update-log" bind:this={updateLogEl}>
					<div class="update-log-header">
						<Terminal size={11} />
						<span>Update output</span>
					</div>
					{#each updateLog as line, i (i)}
						<div class="update-log-line" class:log-done={line.startsWith('✓')} class:log-error={line.startsWith('✗')} class:log-restart={line.startsWith('⟳')}>
							{line}
						</div>
					{/each}
					{#if updateStatus === 'running'}
						<div class="update-log-cursor">▊</div>
					{/if}
				</div>
			{/if}

			{#if updateStatus === 'disconnected'}
				<div class="update-reconnect-hint">
					Services are restarting. Reload this page in a few seconds to confirm the update completed.
					<button class="btn btn-sm btn-secondary" onclick={() => window.location.reload()}>
						<RefreshCw size={12} />Reload now
					</button>
				</div>
			{/if}
		</div>
	</section>
	{/if}
{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }
	.spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.settings-form { display: flex; flex-direction: column; gap: 20px; }

	.settings-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	.section-header {
		display: flex;
		gap: 14px;
		padding: 18px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.section-icon {
		width: 32px; height: 32px;
		border-radius: var(--radius-md);
		background: rgba(37, 99, 235, 0.1);
		color: var(--accent);
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0;
		margin-top: 1px;
	}

	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }
	.fields-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; padding: 18px 20px; }
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }

	.field-input {
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		padding: 8px 10px;
		outline: none;
		transition: border-color var(--transition-fast);
	}
	.field-input.font-mono { font-family: var(--font-mono); }
	.field-input:focus { border-color: var(--accent); }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }

	code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 12px; }

	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	.save-bar { display: flex; justify-content: flex-end; padding: 4px 0 8px; }
	.save-btn { display: flex; align-items: center; gap: 6px; min-width: 140px; justify-content: center; }
	.btn-spinner { width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite; }

	/* ── Platform Update ── */
	.update-section { margin-top: 8px; }
	.update-icon { background: rgba(139, 92, 246, 0.12); color: #8B5CF6; }

	.version-info-bar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-base);
		flex-wrap: wrap;
		min-height: 42px;
	}
	.version-chip { display: flex; align-items: center; gap: 6px; }
	.version-label { font-size: 10px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.07em; }
	.version-sha {
		font-family: var(--font-mono);
		font-size: 12px;
		background: var(--bg-elevated);
		color: var(--text-primary);
		padding: 2px 7px;
		border-radius: 4px;
		border: 1px solid var(--border);
	}
	.version-date { font-size: 11px; color: var(--text-dim); }
	.version-loading { font-size: 12px; color: var(--text-dim); }
	.version-badge {
		font-size: 11px;
		font-weight: 600;
		padding: 2px 9px;
		border-radius: 99px;
	}
	.version-badge code { font-family: var(--font-mono); font-size: 11px; }
	.version-badge.update-avail { background: rgba(245,158,11,0.12); color: #D97706; border: 1px solid rgba(245,158,11,0.3); }
	.version-badge.up-to-date   { background: rgba(16,185,129,0.10); color: #10B981; border: 1px solid rgba(16,185,129,0.25); }
	.refresh-version-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-left: auto;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-muted);
		font-size: 11px;
		font-family: var(--font-sans);
		padding: 3px 9px;
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.refresh-version-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.refresh-version-btn:disabled { opacity: 0.5; cursor: default; }

	.update-body {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 18px 20px;
	}

	.update-actions {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	.btn-update {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		padding: 8px 16px;
		background: #8B5CF6;
		color: white;
		border: none;
		border-radius: var(--radius-sm);
		font-size: 13px;
		font-weight: 500;
		font-family: var(--font-sans);
		cursor: pointer;
		transition: opacity var(--transition-fast), background var(--transition-fast);
	}
	.btn-update:hover:not(:disabled) { background: #7C3AED; }
	.btn-update:disabled { opacity: 0.55; cursor: default; }

	:global(.spin-icon) { animation: spin 0.8s linear infinite; }

	.update-badge {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 10px;
		border-radius: 99px;
		font-size: 11px;
		font-weight: 600;
	}
	.update-badge.done       { background: rgba(16,185,129,0.12); color: #10B981; border: 1px solid rgba(16,185,129,0.3); }
	.update-badge.error      { background: rgba(239,68,68,0.10);  color: #EF4444; border: 1px solid rgba(239,68,68,0.3); }
	.update-badge.restarting { background: rgba(245,158,11,0.12); color: #F59E0B; border: 1px solid rgba(245,158,11,0.3); }

	.clear-log-btn {
		margin-left: auto;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-muted);
		font-size: 11px;
		padding: 3px 10px;
		cursor: pointer;
		font-family: var(--font-sans);
		transition: all var(--transition-fast);
	}
	.clear-log-btn:hover { border-color: var(--accent); color: var(--accent); }

	.update-log {
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: var(--radius-md);
		overflow-y: auto;
		max-height: 340px;
		font-family: var(--font-mono);
		font-size: 12px;
	}

	.update-log-header {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 7px 12px;
		border-bottom: 1px solid #21262d;
		color: #8b949e;
		font-size: 11px;
		font-family: var(--font-sans);
	}

	.update-log-line {
		padding: 2px 14px;
		color: #e6edf3;
		white-space: pre-wrap;
		word-break: break-all;
		line-height: 1.6;
	}
	.update-log-line.log-done    { color: #3fb950; }
	.update-log-line.log-error   { color: #f85149; }
	.update-log-line.log-restart { color: #d29922; }

	.update-log-cursor {
		padding: 2px 14px 6px;
		color: #e6edf3;
		animation: blink 1s step-end infinite;
	}
	@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }

	.update-reconnect-hint {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 14px;
		background: rgba(245,158,11,0.08);
		border: 1px solid rgba(245,158,11,0.25);
		border-radius: var(--radius-md);
		color: #D97706;
		font-size: 12px;
		flex-wrap: wrap;
	}

	/* ── Responsive ── */
	@media (max-width: 639px) {
		.settings-form { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.fields { padding: 14px 16px; }
		.fields-grid { grid-template-columns: 1fr; padding: 14px 16px; }
		.save-bar { padding: 0 0 4px; }
	}
</style>
