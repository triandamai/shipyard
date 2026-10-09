<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Card, InlineAlert, Button, Badge } from '$lib/components/ui';
	import { RefreshCw, ScrollText } from '@lucide/svelte';

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
	<Card padding="0">
		<div class="card-hdr">
			<span class="card-title">Current Version</span>
			<Button variant="secondary" size="sm" disabled={checkingUpdate || loadingVersion} onclick={() => loadVersion(true)}>
				{#if checkingUpdate}
					<span class="spin-dot"></span>Checking…
				{:else}
					<RefreshCw size={12} />
					Check for updates
				{/if}
			</Button>
		</div>

		{#if loadingVersion}
			<div class="version-loading">
				<div class="sk" style="width:120px;height:13px"></div>
				<div class="sk" style="width:200px;height:11px;margin-top:6px"></div>
			</div>
		{:else if versionError}
			<div class="version-error"><InlineAlert tone="error">{versionError}</InlineAlert></div>
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
					<InlineAlert tone="info">Update available — <code class="v-remote-sha">{info.remote_sha}</code></InlineAlert>
				{:else if info.remote_sha}
					<InlineAlert tone="success">Up to date</InlineAlert>
				{/if}
			</div>
		{/if}
	</Card>

	<!-- Update card -->
	<Card padding="0">
		<div class="card-hdr">
			<span class="card-title">Pull &amp; Restart</span>
			{#if updateStatus === 'done'}
				<Badge tone="green">Done</Badge>
			{:else if updateStatus === 'error'}
				<Badge tone="red">Failed</Badge>
			{:else if updateStatus === 'disconnected'}
				<Badge tone="yellow">Restarting…</Badge>
			{/if}
		</div>

		<p class="update-desc">
			Pulls the latest Docker images from the registry and restarts all Shipyard services.
			The connection will drop briefly while the backend restarts — that's expected.
		</p>

		<div class="update-actions">
			<Button variant="primary" disabled={updateStatus === 'running'} onclick={startUpdate}>
				{#if updateStatus === 'running'}
					<span class="spin-dot"></span>Running update…
				{:else}
					<RefreshCw size={13} />
					Pull &amp; Restart
				{/if}
			</Button>

			{#if updateLog.length > 0 && updateStatus !== 'running'}
				<Button variant="ghost" onclick={clearLog}>Clear log</Button>
			{/if}
		</div>

		{#if updateLog.length > 0}
			<div class="log" bind:this={logEl}>
				<div class="log-hdr">
					<ScrollText size={11} />
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
			<div class="reconnect-wrap">
				<InlineAlert tone="warning">
					<div class="reconnect-content">
						<span>Services are restarting. Reload this page in a few seconds to confirm the update completed.</span>
						<Button variant="ghost" size="sm" onclick={() => window.location.reload()}>Reload now</Button>
					</div>
				</InlineAlert>
			</div>
		{/if}
	</Card>
</div>

<style>
	.p { max-width: 680px; margin: 0 auto; padding: 40px 36px; display: flex; flex-direction: column; gap: 16px; }
	.hdr { margin-bottom: 4px; }
	.ttl { font-size: 20px; font-weight: 700; color: var(--text-primary); margin: 0 0 4px; letter-spacing: -0.02em; }
	.sub { font-size: 12.5px; color: var(--text-muted); margin: 0; }

	.card-hdr {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 18px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.card-title { font-size: 12.5px; font-weight: 700; color: var(--text-primary); flex: 1; }

	/* ── Version card ── */
	.version-loading { padding: 16px 18px; display: flex; flex-direction: column; gap: 8px; }
	.version-error { padding: 14px 18px; }

	.version-body { padding: 14px 18px; display: flex; flex-direction: column; gap: 10px; }
	.v-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
	.v-label { font-size: 10.5px; font-weight: 700; color: var(--text-muted); }
	.v-sha {
		font-family: var(--font-mono);
		font-size: 12px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		padding: 2px 8px;
		border-radius: 5px;
		color: var(--text-primary);
	}
	.v-date { font-size: 11px; color: var(--text-muted); }
	.v-remote-sha { font-family: var(--font-mono); font-size: 11px; }

	/* ── Update card ── */
	.update-desc { margin: 0; padding: 12px 18px 0; font-size: 12px; color: var(--text-muted); line-height: 1.5; }
	.update-actions { display: flex; align-items: center; gap: 10px; padding: 14px 18px; flex-wrap: wrap; }

	/* Spinner — uses currentColor so it stays visible both on the primary
	   button's white text and the secondary button's dark/light text,
	   across both themes (the pre-migration version hardcoded white, which
	   only worked against the primary button's always-colored background). */
	.spin-dot {
		display: inline-block;
		width: 11px; height: 11px;
		border: 2px solid color-mix(in srgb, currentColor 30%, transparent);
		border-top-color: currentColor;
		border-radius: 50%;
		animation: spin .7s linear infinite;
		flex-shrink: 0;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ── Log (intentionally a fixed dark terminal chrome regardless of
	   site theme — only the per-line ok/err/warn categorization below uses
	   theme-aware tokens, per the task brief). ── */
	.log {
		margin: 0 18px 18px;
		background: var(--terminal-bg);
		border: 1px solid var(--terminal-border);
		border-radius: var(--radius-md);
		overflow-y: auto;
		max-height: 400px;
		font-family: var(--font-mono);
		font-size: 12px;
	}
	.log-hdr {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px;
		border-bottom: 1px solid var(--terminal-border);
		color: var(--terminal-dim); font-size: 11px;
		font-family: var(--font-sans);
	}
	.log-line {
		padding: 2px 14px;
		color: var(--terminal-fg);
		white-space: pre-wrap;
		word-break: break-all;
		line-height: 1.6;
	}
	.log-ok   { color: var(--accent-green); }
	.log-err  { color: var(--accent-red); }
	.log-warn { color: var(--accent-yellow); }
	.log-cursor { padding: 2px 14px 8px; color: var(--terminal-fg); animation: blink 1s step-end infinite; }
	@keyframes blink { 0%,100%{opacity:1} 50%{opacity:0} }

	.reconnect-wrap { margin: 0 18px 18px; }
	.reconnect-content { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }

	/* ── Skeletons ── */
	.sk { background: var(--border); border-radius: 4px; animation: sk 1.3s ease-in-out infinite; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }
</style>
