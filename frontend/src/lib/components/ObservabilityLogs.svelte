<script lang="ts">
	import { onMount } from 'svelte';
	import type { StoredLog, LogPage, LogStatsResult, LogQueryParams, ApiResponse } from '$lib/api/types';
	import {
		Search, RefreshCw, X, ChevronRight, Hash, Layers, ShieldAlert,
		Clock, Filter, ArrowUpDown, ChevronDown
	} from '@lucide/svelte';
	import { Button } from '$lib/components/ui';

	interface Props {
		scope: 'admin' | 'org' | 'project' | 'service';
		scopeId?: string;
		fetchLogs: (params: LogQueryParams) => Promise<ApiResponse<LogPage>>;
		fetchStats: (params: LogQueryParams) => Promise<ApiResponse<LogStatsResult>>;
		allowStackTabs?: boolean;
	}

	let {
		scope,
		scopeId,
		fetchLogs,
		fetchStats,
		allowStackTabs = true
	}: Props = $props();

	// ─── Filter State ──────────────────────────────────────────────────────────

	let currentSource = $state<string>('all');
	let selectedLevel = $state<string>('all');
	let keyword = $state<string>('');
	let traceId = $state<string>('');
	let selectedRange = $state<'1h' | '24h' | '7d' | '30d' | '90d'>('24h');
	let previousReplicas = $state<boolean>(false);

	let loading = $state<boolean>(false);
	let errorMsg = $state<string | null>(null);

	let logEntries = $state<StoredLog[]>([]);
	let nextCursor = $state<string | null>(null);
	let stats = $state<LogStatsResult | null>(null);

	let selectedLog = $state<StoredLog | null>(null);

	const STACK_TABS = [
		{ id: 'all', label: 'All Stacks' },
		{ id: 'traefik', label: 'Traefik' },
		{ id: 'postgres', label: 'Database' },
		{ id: 'backend', label: 'Backend' },
		{ id: 'frontend', label: 'Frontend' },
		{ id: 'app', label: 'App Containers' },
	];

	const LEVELS = ['all', 'error', 'warn', 'info', 'debug', 'trace'];

	const RANGES: Array<'1h' | '24h' | '7d' | '30d' | '90d'> = ['1h', '24h', '7d', '30d', '90d'];

	function calculateDateRange(r: string): { from: string; to: string } {
		const now = new Date();
		let pastMs = 24 * 3600 * 1000;
		switch (r) {
			case '1h': pastMs = 3600 * 1000; break;
			case '24h': pastMs = 24 * 3600 * 1000; break;
			case '7d': pastMs = 7 * 24 * 3600 * 1000; break;
			case '30d': pastMs = 30 * 24 * 3600 * 1000; break;
			case '90d': pastMs = 90 * 24 * 3600 * 1000; break;
		}
		const fromDate = new Date(now.getTime() - pastMs);
		return { from: fromDate.toISOString(), to: now.toISOString() };
	}

	async function loadData(reset: boolean = true) {
		loading = true;
		errorMsg = null;
		if (reset) {
			nextCursor = null;
		}

		const { from, to } = calculateDateRange(selectedRange);
		const params: LogQueryParams = {
			from,
			to,
			source: currentSource === 'all' ? undefined : currentSource,
			level: selectedLevel === 'all' ? undefined : selectedLevel,
			q: keyword.trim() || undefined,
			trace_id: traceId.trim() || undefined,
			cursor: reset ? undefined : (nextCursor || undefined),
			previous: previousReplicas || undefined,
			limit: 100
		};

		try {
			const [logRes, statsRes] = await Promise.all([
				fetchLogs(params),
				fetchStats({ from, to, source: params.source, level: params.level })
			]);

			if (logRes.error) {
				errorMsg = logRes.error.message;
			} else if (logRes.data) {
				if (reset) {
					logEntries = logRes.data.entries;
				} else {
					logEntries = [...logEntries, ...logRes.data.entries];
				}
				nextCursor = logRes.data.next_cursor;
			}

			if (statsRes.data) {
				stats = statsRes.data;
			}
		} catch (e: any) {
			errorMsg = e.message || 'Failed to load logs';
		} finally {
			loading = false;
		}
	}

	let searchTimeout: ReturnType<typeof setTimeout>;
	function onSearchInput() {
		clearTimeout(searchTimeout);
		searchTimeout = setTimeout(() => {
			loadData(true);
		}, 300);
	}

	function setTraceFilter(tid: string) {
		traceId = tid;
		loadData(true);
	}

	function clearTraceFilter() {
		traceId = '';
		loadData(true);
	}

	onMount(() => {
		loadData(true);
	});

	function formatTimestamp(ts: string) {
		try {
			const d = new Date(ts);
			return d.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3 });
		} catch {
			return ts;
		}
	}

	function renderMasked(text: string) {
		// Replace "••• [kind]" with styled spans
		const parts = text.split(/(••• \[[a-z0-9_]+\])/g);
		return parts;
	}
</script>

<div class="obs-container">
	<!-- Top Controls Toolbar -->
	<div class="toolbar">
		{#if allowStackTabs}
			<div class="stack-tabs">
				{#each STACK_TABS as tab}
					<button
						class="tab-btn"
						class:active={currentSource === tab.id}
						onclick={() => { currentSource = tab.id; loadData(true); }}
					>
						{tab.label}
					</button>
				{/each}
			</div>
		{/if}

		<div class="range-selector">
			{#each RANGES as r}
				<button
					class="range-chip"
					class:active={selectedRange === r}
					onclick={() => { selectedRange = r; loadData(true); }}
				>
					{r}
				</button>
			{/each}
			<button class="refresh-btn" onclick={() => loadData(true)} title="Refresh logs">
				<RefreshCw size={14} class={loading ? 'animate-spin' : ''} />
			</button>
		</div>
	</div>

	<!-- Filter Chips & Search Bar -->
	<div class="filters-bar">
		<div class="search-box">
			<Search size={14} class="search-icon" />
			<input
				type="text"
				placeholder="Search masked logs (keyword)..."
				bind:value={keyword}
				oninput={onSearchInput}
			/>
			{#if keyword}
				<button class="clear-btn" onclick={() => { keyword = ''; loadData(true); }}>
					<X size={13} />
				</button>
			{/if}
		</div>

		<div class="level-chips">
			{#each LEVELS as lvl}
				<button
					class="level-chip lvl-{lvl}"
					class:active={selectedLevel === lvl}
					onclick={() => { selectedLevel = lvl; loadData(true); }}
				>
					{lvl.toUpperCase()}
				</button>
			{/each}
		</div>

		{#if scope === 'service'}
			<label class="previous-toggle">
				<input
					type="checkbox"
					bind:checked={previousReplicas}
					onchange={() => loadData(true)}
				/>
				<span>Include crashed replicas</span>
			</label>
		{/if}
	</div>

	<!-- Active Trace Filter Badge -->
	{#if traceId}
		<div class="active-badge-bar">
			<span class="active-trace-tag">
				<Hash size={13} />
				Trace: <code>{traceId}</code>
				<button class="remove-trace" onclick={clearTraceFilter} title="Clear trace filter">
					<X size={12} />
				</button>
			</span>
			<span class="trace-hint">Showing all lines correlated across stacks for this trace</span>
		</div>
	{/if}

	<!-- Error Summary Tiles / Histogram -->
	{#if stats}
		<div class="stats-banner">
			<div class="stat-tile">
				<span class="stat-label">Total Volume</span>
				<span class="stat-value">{stats.total.toLocaleString()} lines</span>
			</div>
			<div class="stat-tile" class:has-errors={stats.errors > 0}>
				<span class="stat-label">Errors</span>
				<span class="stat-value error-text">{stats.errors.toLocaleString()}</span>
			</div>
			<div class="stat-tile">
				<span class="stat-label">Active Retention</span>
				<span class="stat-value">Up to 90 days durable</span>
			</div>
		</div>
	{/if}

	{#if errorMsg}
		<div class="error-banner">
			<ShieldAlert size={16} />
			<span>{errorMsg}</span>
		</div>
	{/if}

	<!-- Log Table / Virtualized View -->
	<div class="log-table-wrapper">
		{#if logEntries.length === 0 && !loading}
			<div class="empty-state">
				<Layers size={32} class="empty-icon" />
				<p>No log entries found for this scope and time range.</p>
			</div>
		{:else}
			<div class="log-stream">
				{#each logEntries as entry (entry.id)}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<div
						role="button"
						tabindex="0"
						class="log-row lvl-{entry.level}"
						class:selected={selectedLog?.id === entry.id}
						onclick={() => selectedLog = entry}
					>
						<span class="col-ts">{formatTimestamp(entry.ts)}</span>
						<span class="col-lvl lvl-badge lvl-{entry.level}">{entry.level}</span>
						<span class="col-source">{entry.source}</span>
						{#if entry.replica !== null}
							<span class="col-replica">r{entry.replica}</span>
						{/if}
						<div class="col-msg">
							{#each renderMasked(entry.message) as part}
								{#if part.startsWith('••• [')}
									<span class="mask-chip">{part}</span>
								{:else}
									<span>{part}</span>
								{/if}
							{/each}
						</div>
						{#if entry.trace_id}
							<button
								class="col-trace-btn"
								title="Filter same trace"
								onclick={(e) => { e.stopPropagation(); setTraceFilter(entry.trace_id!); }}
							>
								<Hash size={11} />
								<span>trace</span>
							</button>
						{/if}
					</div>
				{/each}
			</div>

			{#if nextCursor}
				<div class="load-more-bar">
					<Button variant="secondary" onclick={() => loadData(false)} disabled={loading}>
						{loading ? 'Loading...' : 'Load more logs'}
					</Button>
				</div>
			{/if}
		{/if}
	</div>

	<!-- Detail Drawer -->
	{#if selectedLog}
		<div class="detail-drawer">
			<div class="drawer-header">
				<h3>Log Line Details</h3>
				<button class="drawer-close" onclick={() => selectedLog = null}>
					<X size={16} />
				</button>
			</div>

			<div class="drawer-body">
				<div class="detail-prop">
					<span class="prop-label">Timestamp</span>
					<span class="prop-val">{selectedLog.ts}</span>
				</div>
				<div class="detail-prop">
					<span class="prop-label">Source / Stream</span>
					<span class="prop-val">{selectedLog.source} ({selectedLog.stream})</span>
				</div>
				<div class="detail-prop">
					<span class="prop-label">Level</span>
					<span class="prop-val lvl-badge lvl-{selectedLog.level}">{selectedLog.level}</span>
				</div>
				{#if selectedLog.container_id}
					<div class="detail-prop">
						<span class="prop-label">Container ID</span>
						<span class="prop-val"><code>{selectedLog.container_id}</code></span>
					</div>
				{/if}
				{#if selectedLog.replica !== null}
					<div class="detail-prop">
						<span class="prop-label">Replica Slot</span>
						<span class="prop-val">Slot #{selectedLog.replica}</span>
					</div>
				{/if}
				{#if selectedLog.trace_id}
					<div class="detail-prop">
						<span class="prop-label">Trace ID</span>
						<div class="prop-val trace-action-val">
							<code>{selectedLog.trace_id}</code>
							<Button size="sm" variant="secondary" onclick={() => { setTraceFilter(selectedLog!.trace_id!); selectedLog = null; }}>
								Filter Same Trace
							</Button>
						</div>
					</div>
				{/if}
				{#if selectedLog.span_id}
					<div class="detail-prop">
						<span class="prop-label">Span ID</span>
						<span class="prop-val"><code>{selectedLog.span_id}</code></span>
					</div>
				{/if}

				<div class="detail-message-block">
					<span class="prop-label">Masked Message</span>
					<div class="msg-box">{selectedLog.message}</div>
				</div>

				{#if selectedLog.fields}
					<div class="detail-fields-block">
						<span class="prop-label">Structured Fields</span>
						<pre class="json-box">{JSON.stringify(selectedLog.fields, null, 2)}</pre>
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.obs-container {
		display: flex;
		flex-direction: column;
		gap: 12px;
		font-family: inherit;
		height: 100%;
		position: relative;
	}

	.toolbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: 8px;
		background: var(--bg-surface, #ffffff);
		padding: 8px 12px;
		border-radius: 8px;
		border: 1px solid var(--border-color, #e5e7eb);
	}

	.stack-tabs {
		display: flex;
		gap: 4px;
	}

	.tab-btn {
		padding: 4px 10px;
		font-size: 13px;
		border: none;
		background: transparent;
		color: var(--text-muted, #6b7280);
		border-radius: 6px;
		cursor: pointer;
		font-weight: 500;
	}

	.tab-btn:hover {
		background: var(--bg-hover, #f3f4f6);
	}

	.tab-btn.active {
		background: var(--primary-bg, #3b82f6);
		color: #ffffff;
	}

	.range-selector {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.range-chip {
		padding: 3px 8px;
		font-size: 12px;
		border: 1px solid var(--border-color, #e5e7eb);
		background: transparent;
		border-radius: 4px;
		cursor: pointer;
	}

	.range-chip.active {
		background: var(--text-primary, #111827);
		color: #ffffff;
		border-color: transparent;
	}

	.refresh-btn {
		padding: 4px;
		background: transparent;
		border: 1px solid var(--border-color, #e5e7eb);
		border-radius: 4px;
		cursor: pointer;
		display: flex;
		align-items: center;
	}

	.filters-bar {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}

	.search-box {
		display: flex;
		align-items: center;
		position: relative;
		flex: 1;
		min-width: 240px;
	}

	.search-box input {
		width: 100%;
		padding: 6px 28px 6px 28px;
		font-size: 13px;
		border: 1px solid var(--border-color, #e5e7eb);
		border-radius: 6px;
		background: var(--bg-surface, #ffffff);
	}

	.search-box .search-icon {
		position: absolute;
		left: 8px;
		color: #9ca3af;
	}

	.search-box .clear-btn {
		position: absolute;
		right: 8px;
		background: none;
		border: none;
		cursor: pointer;
		color: #9ca3af;
	}

	.level-chips {
		display: flex;
		gap: 4px;
	}

	.level-chip {
		padding: 3px 8px;
		font-size: 11px;
		font-weight: 600;
		border-radius: 4px;
		border: 1px solid var(--border-color, #e5e7eb);
		background: var(--bg-surface, #ffffff);
		cursor: pointer;
	}

	.level-chip.active {
		background: var(--text-primary, #111827);
		color: #ffffff;
	}

	.level-chip.lvl-error.active { background: #ef4444; color: #fff; }
	.level-chip.lvl-warn.active { background: #f59e0b; color: #fff; }
	.level-chip.lvl-info.active { background: #3b82f6; color: #fff; }

	.previous-toggle {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted, #6b7280);
		cursor: pointer;
	}

	.active-badge-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
	}

	.active-trace-tag {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		background: #e0e7ff;
		color: #3730a3;
		padding: 2px 8px;
		border-radius: 4px;
		font-size: 12px;
	}

	.remove-trace {
		background: none;
		border: none;
		cursor: pointer;
		color: #3730a3;
		display: flex;
	}

	.trace-hint {
		color: var(--text-muted, #6b7280);
		font-size: 11px;
	}

	.stats-banner {
		display: flex;
		gap: 12px;
	}

	.stat-tile {
		background: var(--bg-surface, #ffffff);
		border: 1px solid var(--border-color, #e5e7eb);
		padding: 8px 14px;
		border-radius: 6px;
		display: flex;
		flex-direction: column;
	}

	.stat-label {
		font-size: 11px;
		color: var(--text-muted, #6b7280);
		text-transform: uppercase;
		font-weight: 500;
	}

	.stat-value {
		font-size: 16px;
		font-weight: 600;
	}

	.error-text {
		color: #ef4444;
	}

	.log-table-wrapper {
		background: #0f172a;
		color: #e2e8f0;
		border-radius: 8px;
		flex: 1;
		min-height: 400px;
		overflow-y: auto;
		padding: 6px 0;
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
		font-size: 12px;
	}

	.log-row {
		display: flex;
		align-items: baseline;
		padding: 3px 12px;
		gap: 10px;
		cursor: pointer;
		line-height: 1.5;
	}

	.log-row:hover {
		background: rgba(255, 255, 255, 0.05);
	}

	.log-row.selected {
		background: rgba(59, 130, 246, 0.2);
	}

	.col-ts {
		color: #94a3b8;
		white-space: nowrap;
	}

	.col-lvl {
		font-weight: 600;
		text-transform: uppercase;
		font-size: 10px;
		padding: 1px 4px;
		border-radius: 3px;
		white-space: nowrap;
	}

	.col-source {
		color: #38bdf8;
		font-size: 11px;
		white-space: nowrap;
	}

	.col-replica {
		color: #fbbf24;
		font-size: 10px;
		white-space: nowrap;
	}

	.col-msg {
		flex: 1;
		word-break: break-all;
	}

	.mask-chip {
		background: rgba(239, 68, 68, 0.2);
		color: #f87171;
		padding: 1px 4px;
		border-radius: 3px;
		font-size: 11px;
		border: 1px solid rgba(239, 68, 68, 0.4);
	}

	.col-trace-btn {
		background: rgba(255, 255, 255, 0.1);
		border: none;
		color: #cbd5e1;
		border-radius: 3px;
		padding: 1px 6px;
		display: flex;
		align-items: center;
		gap: 3px;
		cursor: pointer;
		font-size: 10px;
	}

	.col-trace-btn:hover {
		background: #3b82f6;
		color: #fff;
	}

	.lvl-error .col-lvl { background: rgba(239, 68, 68, 0.3); color: #f87171; }
	.lvl-warn .col-lvl { background: rgba(245, 158, 11, 0.3); color: #fbbf24; }
	.lvl-info .col-lvl { background: rgba(59, 130, 246, 0.3); color: #60a5fa; }
	.lvl-debug .col-lvl { background: rgba(148, 163, 184, 0.2); color: #94a3b8; }
	.lvl-trace .col-lvl { background: rgba(148, 163, 184, 0.1); color: #64748b; }

	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		height: 300px;
		color: #64748b;
	}

	.empty-icon {
		margin-bottom: 8px;
		opacity: 0.5;
	}

	.detail-drawer {
		position: absolute;
		right: 0;
		top: 0;
		bottom: 0;
		width: 480px;
		background: var(--bg-surface, #ffffff);
		border-left: 1px solid var(--border-color, #e5e7eb);
		box-shadow: -4px 0 15px rgba(0, 0, 0, 0.1);
		display: flex;
		flex-direction: column;
		z-index: 50;
	}

	.drawer-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border-color, #e5e7eb);
	}

	.drawer-header h3 {
		margin: 0;
		font-size: 15px;
	}

	.drawer-close {
		background: none;
		border: none;
		cursor: pointer;
	}

	.drawer-body {
		padding: 16px;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 12px;
		font-size: 13px;
	}

	.detail-prop {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.prop-label {
		font-size: 11px;
		color: var(--text-muted, #6b7280);
		text-transform: uppercase;
		font-weight: 500;
	}

	.prop-val {
		font-size: 13px;
	}

	.trace-action-val {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.msg-box, .json-box {
		background: #f8fafc;
		border: 1px solid #e2e8f0;
		padding: 8px;
		border-radius: 4px;
		font-family: monospace;
		font-size: 12px;
		white-space: pre-wrap;
		word-break: break-all;
	}

	.load-more-bar {
		padding: 12px;
		display: flex;
		justify-content: center;
	}

	.animate-spin {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
