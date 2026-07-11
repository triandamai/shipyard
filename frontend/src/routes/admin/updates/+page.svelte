<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface VersionInfo {
		current: string;
		git_sha: string;
		build_date: string;
		update_available: boolean;
		remote_sha: string | null;
	}

	let info           = $state<VersionInfo | null>(null);
	let loadingVersion = $state(true);
	let checkingUpdate = $state(false);
	let versionError   = $state('');

	async function loadVersion(force = false) {
		if (force) checkingUpdate = true; else loadingVersion = true;
		versionError = '';
		const res = await api.get<VersionInfo>(`/admin/version${force ? '?force=true' : ''}`);
		if (res.data) info = res.data;
		else versionError = res.error?.message ?? 'Failed to load version info';
		if (force) checkingUpdate = false; else loadingVersion = false;
	}

	// ── Update stream ────────────────────────────────────────────────────────────

	type UpdateStatus = 'idle' | 'running' | 'done' | 'error' | 'disconnected';
	let updateStatus = $state<UpdateStatus>('idle');
	let updateLog    = $state<string[]>([]);
	let logEl        = $state<HTMLDivElement | null>(null);
	let eventSource: EventSource | null = null;

	function startUpdate() {
		if (updateStatus === 'running') return;
		updateLog    = [];
		updateStatus = 'running';

		eventSource?.close();
		eventSource = new EventSource('/api/admin/update/stream');

		eventSource.onmessage = (e) => {
			if (!e.data?.trim()) return;
			updateLog = [...updateLog, e.data];
			if (logEl) requestAnimationFrame(() => {
				if (logEl) logEl.scrollTop = logEl.scrollHeight;
			});
		};

		eventSource.addEventListener('done', (e: MessageEvent) => {
			updateLog    = [...updateLog, `✓ ${e.data}`];
			updateStatus = 'done';
			eventSource?.close();
			eventSource = null;
			// Refresh version info after successful update
			loadVersion(true);
		});

		eventSource.addEventListener('error', (e: MessageEvent) => {
			if (e.data) {
				updateLog    = [...updateLog, `✗ ${e.data}`];
				updateStatus = 'error';
				eventSource?.close();
				eventSource = null;
			}
		});

		// onerror = connection dropped (expected when backend restarts after update)
		eventSource.onerror = () => {
			if (updateStatus === 'running') {
				updateLog    = [...updateLog, '⟳ Connection lost — services are restarting. Reload the page when ready.'];
				updateStatus = 'disconnected';
			}
			eventSource?.close();
			eventSource = null;
		};
	}

	function clearLog() {
		updateLog    = [];
		updateStatus = 'idle';
	}

	function formatDate(iso: string): string {
		if (!iso || iso === 'unknown') return '';
		try {
			return new Date(iso).toLocaleString('en-US', {
				year: 'numeric', month: 'short', day: 'numeric',
				hour: '2-digit', minute: '2-digit', timeZoneName: 'short',
			});
		} catch { return iso; }
	}

	onMount(() => loadVersion());
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Platform Updates</h1>
			<p class="sub">Check for newer images and apply a rolling update to all Shipyard services.</p>
		</div>
	</header>

	<!-- Version card -->
	<div class="card version-card">
		<div class="card-hdr">
			<span class="card-title">Current Version</span>
			<button class="check-btn" disabled={checkingUpdate || loadingVersion} onclick={() => loadVersion(true)}>
				{#if checkingUpdate}
					<span class="spin-dot"></span>Checking…
				{:else}
					<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
					Check for updates
				{/if}
			</button>
		</div>

		{#if loadingVersion}
			<div class="version-loading">
				<div class="sk" style="width:120px;height:13px"></div>
				<div class="sk" style="width:200px;height:11px;margin-top:6px"></div>
			</div>
		{:else if versionError}
			<div class="err-inline">{versionError}</div>
		{:else if info}
			<div class="version-body">
				<div class="v-row">
					<span class="v-label">Version</span>
					<code class="v-sha">{info.git_sha}</code>
					{#if info.build_date && info.build_date !== 'unknown'}
						<span class="v-date">{formatDate(info.build_date)}</span>
					{/if}
				</div>
				{#if info.update_available && info.remote_sha}
					<div class="update-avail-banner">
						<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-7-4a1 1 0 11-2 0 1 1 0 012 0zM9 9a1 1 0 000 2v3a1 1 0 001 1h1a1 1 0 100-2v-3a1 1 0 00-1-1H9z" clip-rule="evenodd"/></svg>
						Update available — <code>{info.remote_sha}</code>
					</div>
				{:else if info.remote_sha}
					<div class="up-to-date-row">
						<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd"/></svg>
						Up to date
					</div>
				{/if}
			</div>
		{/if}
	</div>

	<!-- Update card -->
	<div class="card update-card">
		<div class="card-hdr">
			<span class="card-title">Pull &amp; Restart</span>
			{#if updateStatus === 'done'}
				<span class="status-badge badge-ok">
					<svg viewBox="0 0 20 20" fill="currentColor" width="11" height="11"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zm3.707-9.293a1 1 0 00-1.414-1.414L9 10.586 7.707 9.293a1 1 0 00-1.414 1.414l2 2a1 1 0 001.414 0l4-4z" clip-rule="evenodd"/></svg>
					Done
				</span>
			{:else if updateStatus === 'error'}
				<span class="status-badge badge-err">
					<svg viewBox="0 0 20 20" fill="currentColor" width="11" height="11"><path fill-rule="evenodd" d="M10 18a8 8 0 100-16 8 8 0 000 16zM8.707 7.293a1 1 0 00-1.414 1.414L8.586 10l-1.293 1.293a1 1 0 101.414 1.414L10 11.414l1.293 1.293a1 1 0 001.414-1.414L11.414 10l1.293-1.293a1 1 0 00-1.414-1.414L10 8.586 8.707 7.293z" clip-rule="evenodd"/></svg>
					Failed
				</span>
			{:else if updateStatus === 'disconnected'}
				<span class="status-badge badge-warn">
					<svg viewBox="0 0 20 20" fill="currentColor" width="11" height="11"><path fill-rule="evenodd" d="M11.3 1.046A1 1 0 0112 2v5h4a1 1 0 01.82 1.573l-7 10A1 1 0 018 18v-5H4a1 1 0 01-.82-1.573l7-10a1 1 0 011.12-.38z" clip-rule="evenodd"/></svg>
					Restarting…
				</span>
			{/if}
		</div>

		<p class="update-desc">
			Pulls the latest Docker images from the registry and restarts all Shipyard services.
			The connection will drop briefly while the backend restarts — that's expected.
		</p>

		<div class="update-actions">
			<button
				class="btn-update"
				disabled={updateStatus === 'running'}
				onclick={startUpdate}
			>
				{#if updateStatus === 'running'}
					<span class="spin-dot"></span>Running update…
				{:else}
					<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
					Pull &amp; Restart
				{/if}
			</button>

			{#if updateLog.length > 0 && updateStatus !== 'running'}
				<button class="btn-ghost" onclick={clearLog}>Clear log</button>
			{/if}
		</div>

		{#if updateLog.length > 0}
			<div class="log" bind:this={logEl}>
				<div class="log-hdr">
					<svg viewBox="0 0 20 20" fill="currentColor" width="11" height="11"><path fill-rule="evenodd" d="M2 5a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2V5zm14 1a1 1 0 11-2 0 1 1 0 012 0zM2 13a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2v-2zm14 1a1 1 0 11-2 0 1 1 0 012 0z" clip-rule="evenodd"/></svg>
					Update output
				</div>
				{#each updateLog as line, i (i)}
					<div
						class="log-line"
						class:log-ok={line.startsWith('✓')}
						class:log-err={line.startsWith('✗')}
						class:log-warn={line.startsWith('⟳')}
					>{line}</div>
				{/each}
				{#if updateStatus === 'running'}
					<div class="log-cursor">▊</div>
				{/if}
			</div>
		{/if}

		{#if updateStatus === 'disconnected'}
			<div class="reconnect-hint">
				Services are restarting. Reload this page in a few seconds to confirm the update completed.
				<button class="btn-ghost btn-sm" onclick={() => window.location.reload()}>Reload now</button>
			</div>
		{/if}
	</div>
</div>

<style>
	.p { max-width: 680px; margin: 0 auto; padding: 40px 36px; display: flex; flex-direction: column; gap: 16px; }
	.hdr { margin-bottom: 4px; }
	.ttl { font-size: 18px; font-weight: 700; color: var(--text); margin: 0 0 4px; letter-spacing: -0.02em; }
	.sub { font-size: 12.5px; color: var(--text-3); margin: 0; }

	.card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		overflow: hidden;
		box-shadow: var(--shadow-sm);
	}

	.card-hdr {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 18px;
		border-bottom: 1px solid var(--border);
		background: var(--surface-2);
	}
	.card-title { font-size: 12.5px; font-weight: 700; color: var(--text); flex: 1; }

	/* ── Version card ── */
	.version-loading { padding: 16px 18px; display: flex; flex-direction: column; gap: 8px; }
	.err-inline { padding: 12px 18px; font-size: 12px; color: var(--danger); }

	.version-body { padding: 14px 18px; display: flex; flex-direction: column; gap: 10px; }
	.v-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
	.v-label { font-size: 10.5px; font-weight: 700; color: var(--text-3); text-transform: uppercase; letter-spacing: .07em; }
	.v-sha {
		font-family: var(--mono);
		font-size: 12px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		padding: 2px 8px;
		border-radius: 5px;
		color: var(--text);
	}
	.v-date { font-size: 11px; color: var(--text-3); }

	.update-avail-banner {
		display: flex; align-items: center; gap: 6px;
		padding: 8px 12px;
		background: rgba(245,158,11,0.08);
		border: 1px solid rgba(245,158,11,0.25);
		border-radius: var(--radius-sm);
		font-size: 12px; color: #b45309;
	}
	.update-avail-banner code {
		font-family: var(--mono);
		font-size: 11px;
		background: rgba(245,158,11,0.12);
		padding: 1px 5px;
		border-radius: 3px;
	}
	.up-to-date-row {
		display: flex; align-items: center; gap: 6px;
		font-size: 12px; color: var(--ok);
	}

	.check-btn {
		display: inline-flex; align-items: center; gap: 5px;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-2);
		font-size: 11.5px; font-family: var(--font);
		padding: 4px 10px; cursor: pointer;
		transition: all .15s;
	}
	.check-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.check-btn:disabled { opacity: .5; cursor: default; }

	/* ── Update card ── */
	.update-desc { margin: 0; padding: 12px 18px 0; font-size: 12px; color: var(--text-3); line-height: 1.5; }
	.update-actions { display: flex; align-items: center; gap: 10px; padding: 14px 18px; flex-wrap: wrap; }

	.btn-update {
		display: inline-flex; align-items: center; gap: 7px;
		padding: 8px 18px;
		background: var(--accent);
		color: #fff;
		border: none;
		border-radius: var(--radius-sm);
		font-size: 12.5px; font-weight: 600; font-family: var(--font);
		cursor: pointer;
		transition: opacity .15s;
	}
	.btn-update:hover:not(:disabled) { opacity: .88; }
	.btn-update:disabled { opacity: .5; cursor: default; }

	.btn-ghost {
		padding: 7px 14px;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-2);
		font-size: 11.5px; font-family: var(--font);
		cursor: pointer;
		transition: background .15s;
	}
	.btn-ghost:hover { background: var(--surface-2); }
	.btn-sm { padding: 5px 11px; font-size: 11px; }

	.status-badge {
		display: inline-flex; align-items: center; gap: 4px;
		font-size: 11px; font-weight: 700;
		padding: 3px 9px; border-radius: 999px;
	}
	.badge-ok   { background: var(--ok-soft);     color: var(--ok);     border: 1px solid rgba(22,163,74,0.25); }
	.badge-err  { background: var(--danger-soft);  color: var(--danger); border: 1px solid rgba(220,38,38,0.25); }
	.badge-warn { background: var(--warn-soft);    color: var(--warn);   border: 1px solid rgba(180,83,9,0.25); }

	/* ── Log ── */
	.log {
		margin: 0 18px 18px;
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: var(--radius-sm);
		overflow-y: auto;
		max-height: 400px;
		font-family: var(--mono);
		font-size: 12px;
	}
	.log-hdr {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px;
		border-bottom: 1px solid #21262d;
		color: #8b949e; font-size: 11px;
		font-family: var(--font);
	}
	.log-line {
		padding: 2px 14px;
		color: #e6edf3;
		white-space: pre-wrap;
		word-break: break-all;
		line-height: 1.6;
	}
	.log-ok   { color: #3fb950; }
	.log-err  { color: #f85149; }
	.log-warn { color: #d29922; }
	.log-cursor { padding: 2px 14px 8px; color: #e6edf3; animation: blink 1s step-end infinite; }
	@keyframes blink { 0%,100%{opacity:1} 50%{opacity:0} }

	.reconnect-hint {
		display: flex; align-items: center; gap: 12px; flex-wrap: wrap;
		margin: 0 18px 18px;
		padding: 10px 14px;
		background: var(--warn-soft);
		border: 1px solid rgba(180,83,9,0.25);
		border-radius: var(--radius-sm);
		color: var(--warn); font-size: 12px;
	}

	/* ── Skeletons ── */
	.sk { background: var(--border); border-radius: 4px; animation: sk 1.3s ease-in-out infinite; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	/* ── Spinner ── */
	.spin-dot {
		display: inline-block;
		width: 11px; height: 11px;
		border: 2px solid rgba(255,255,255,0.3);
		border-top-color: #fff;
		border-radius: 50%;
		animation: spin .7s linear infinite;
		flex-shrink: 0;
	}
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
