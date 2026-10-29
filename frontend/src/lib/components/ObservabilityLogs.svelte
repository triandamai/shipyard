<script lang="ts">
	import { onMount } from 'svelte';
	import type { StoredLog, LogPage, LogStatsResult, LogQueryParams, ApiResponse } from '$lib/api/types';
	import { Search, RefreshCw, X, Layers, ShieldAlert, Hash } from '@lucide/svelte';

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
			if (isNaN(d.getTime())) return ts;
			const timeStr = d.toLocaleTimeString([], {
				hour12: false,
				hour: '2-digit',
				minute: '2-digit',
				second: '2-digit'
			});
			const ms = String(d.getMilliseconds()).padStart(3, '0');
			return `${timeStr}.${ms}`;
		} catch {
			return ts;
		}
	}

	function renderMasked(text: string) {
		return text.split(/(••• \[[a-z0-9_]+\])/g);
	}
</script>

<div class="obs-root">
	<!-- Top Toolbar: Stack tabs + Range Selector + Refresh -->
	<div class="obs-topbar">
		{#if allowStackTabs}
			<div class="obs-segmented-tabs" role="tablist" aria-label="Stack tabs">
				{#each STACK_TABS as tab}
					<button
						type="button"
						role="tab"
						aria-selected={currentSource === tab.id}
						class="obs-tab-btn"
						class:active={currentSource === tab.id}
						onclick={() => { currentSource = tab.id; loadData(true); }}
					>
						{tab.label}
					</button>
				{/each}
			</div>
		{/if}

		<div class="obs-topbar-right">
			<div class="obs-range-group" role="group" aria-label="Time range selector">
				{#each RANGES as r}
					<button
						type="button"
						class="obs-range-chip"
						class:active={selectedRange === r}
						onclick={() => { selectedRange = r; loadData(true); }}
					>
						{r}
					</button>
				{/each}
			</div>

			<button
				type="button"
				class="obs-refresh-btn"
				onclick={() => loadData(true)}
				title="Refresh logs"
				aria-label="Refresh logs"
			>
				<RefreshCw size={13} class={loading ? 'animate-spin' : ''} />
			</button>
		</div>
	</div>

	<!-- Filter Controls: Search Box + Level Chips + Crashed Toggle -->
	<div class="obs-controls-row">
		<div class="obs-search-field">
			<Search size={14} class="obs-search-icon" />
			<input
				type="text"
				placeholder="Filter masked logs by keyword..."
				bind:value={keyword}
				oninput={onSearchInput}
				class="obs-search-input"
			/>
			{#if keyword}
				<button
					type="button"
					class="obs-clear-btn"
					onclick={() => { keyword = ''; loadData(true); }}
					aria-label="Clear keyword search"
				>
					<X size={13} />
				</button>
			{/if}
		</div>

		<!-- Status Level Chips with Geometric Glyphs (Drydock 04) -->
		<div class="obs-level-chips" role="group" aria-label="Log level filter">
			<button
				type="button"
				class="obs-level-chip chip-all"
				class:active={selectedLevel === 'all'}
				onclick={() => { selectedLevel = 'all'; loadData(true); }}
			>
				All
			</button>

			<button
				type="button"
				class="obs-level-chip chip-error"
				class:active={selectedLevel === 'error'}
				onclick={() => { selectedLevel = 'error'; loadData(true); }}
			>
				<span class="glyph-diamond"></span>
				Error
			</button>

			<button
				type="button"
				class="obs-level-chip chip-warn"
				class:active={selectedLevel === 'warn'}
				onclick={() => { selectedLevel = 'warn'; loadData(true); }}
			>
				<span class="glyph-square"></span>
				Warn
			</button>

			<button
				type="button"
				class="obs-level-chip chip-info"
				class:active={selectedLevel === 'info'}
				onclick={() => { selectedLevel = 'info'; loadData(true); }}
			>
				<span class="glyph-dot-green"></span>
				Info
			</button>

			<button
				type="button"
				class="obs-level-chip chip-debug"
				class:active={selectedLevel === 'debug'}
				onclick={() => { selectedLevel = 'debug'; loadData(true); }}
			>
				<span class="glyph-dot-blue"></span>
				Debug
			</button>

			<button
				type="button"
				class="obs-level-chip chip-trace"
				class:active={selectedLevel === 'trace'}
				onclick={() => { selectedLevel = 'trace'; loadData(true); }}
			>
				<span class="glyph-ring"></span>
				Trace
			</button>
		</div>

		{#if scope === 'service'}
			<label class="obs-checkbox-label">
				<input
					type="checkbox"
					bind:checked={previousReplicas}
					onchange={() => loadData(true)}
				/>
				<span>Include crashed replicas</span>
			</label>
		{/if}
	</div>

	<!-- Active Trace Filter Banner -->
	{#if traceId}
		<div class="obs-trace-bar">
			<span class="obs-trace-chip">
				<Hash size={12} class="trace-hash-icon" />
				<span class="trace-chip-label">TRACE</span>
				<code>{traceId}</code>
				<button
					type="button"
					class="trace-chip-close"
					onclick={clearTraceFilter}
					title="Clear trace filter"
				>
					<X size={12} />
				</button>
			</span>
			<span class="obs-trace-hint">Correlated trace lines across all platform stacks and containers</span>
		</div>
	{/if}

	<!-- Stats & Rollup Summary (Drydock 01 & 03) -->
	{#if stats}
		<div class="obs-stats-grid">
			<div class="obs-stat-card">
				<span class="obs-stat-label">Total Volume</span>
				<span class="obs-stat-value">{stats.total.toLocaleString()}</span>
				<span class="obs-stat-sub">lines in active window</span>
			</div>
			<div class="obs-stat-card" class:has-errs={stats.errors > 0}>
				<span class="obs-stat-label">Errors (5xx / Failed)</span>
				<span class="obs-stat-value error-val">{stats.errors.toLocaleString()}</span>
				<span class="obs-stat-sub">{stats.errors > 0 ? 'attention required' : 'clean stream'}</span>
			</div>
			<div class="obs-stat-card">
				<span class="obs-stat-label">Retention Policy</span>
				<span class="obs-stat-value">90 Days</span>
				<span class="obs-stat-sub">partitioned durable store</span>
			</div>
		</div>
	{/if}

	{#if errorMsg}
		<div class="obs-alert-error" role="alert">
			<ShieldAlert size={16} />
			<span>{errorMsg}</span>
		</div>
	{/if}

	<!-- Terminal Console (Drydock 06 — Logs) -->
	<div class="obs-terminal">
		<!-- Terminal Header Status Bar -->
		<div class="obs-terminal-header">
			<div class="terminal-meta-left">
				<span class="terminal-live-dot"></span>
				<span>Live Edge Log Stream</span>
				<span class="terminal-divider">/</span>
				<span>Masked Ingestion Active</span>
			</div>
			<div class="terminal-meta-right">
				<span>{logEntries.length} lines loaded</span>
			</div>
		</div>

		<!-- Log Stream Lines -->
		{#if logEntries.length === 0 && !loading}
			<div class="obs-terminal-empty">
				<Layers size={28} class="empty-icon" />
				<p>No log records found for the selected stack, level, or time range.</p>
			</div>
		{:else}
			<div class="obs-terminal-stream">
				{#each logEntries as entry (entry.id)}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<div
						role="button"
						tabindex="0"
						class="obs-line"
						class:selected={selectedLog?.id === entry.id}
						onclick={() => selectedLog = entry}
					>
						<span class="line-ts">{formatTimestamp(entry.ts)}</span>

						<span class="line-lvl lvl-{entry.level}">
							{#if entry.level === 'error'}
								<span class="shape-diamond"></span>
								<span class="lvl-txt">ERR</span>
							{:else if entry.level === 'warn'}
								<span class="shape-square"></span>
								<span class="lvl-txt">WRN</span>
							{:else if entry.level === 'info'}
								<span class="shape-dot-green"></span>
								<span class="lvl-txt">INF</span>
							{:else if entry.level === 'debug'}
								<span class="shape-dot-blue"></span>
								<span class="lvl-txt">DBG</span>
							{:else}
								<span class="shape-ring"></span>
								<span class="lvl-txt">TRC</span>
							{/if}
						</span>

						<span class="line-source" title={entry.source}>{entry.source}</span>

						{#if entry.replica !== null}
							<span class="line-replica">r{entry.replica}</span>
						{/if}

						<div class="line-msg">
							{#each renderMasked(entry.message) as part}
								{#if part.startsWith('••• [')}
									<span class="obs-mask-token">{part}</span>
								{:else}
									<span>{part}</span>
								{/if}
							{/each}
						</div>

						{#if entry.trace_id}
							<button
								type="button"
								class="line-trace-btn"
								title="Filter same trace"
								onclick={(e) => { e.stopPropagation(); setTraceFilter(entry.trace_id!); }}
							>
								<Hash size={10} />
								<span>{entry.trace_id.slice(0, 7)}</span>
							</button>
						{/if}
					</div>
				{/each}
			</div>

			{#if nextCursor}
				<div class="obs-terminal-footer">
					<button
						type="button"
						class="obs-more-btn"
						onclick={() => loadData(false)}
						disabled={loading}
					>
						{loading ? 'Retrieving lines...' : 'Load older records'}
					</button>
				</div>
			{/if}
		{/if}
	</div>

	<!-- Detail Drawer (Drydock 05: 8px panel with 0 8px 24px rgba(20,22,25,0.08) shadow) -->
	{#if selectedLog}
		<div class="obs-drawer">
			<div class="drawer-header">
				<div class="drawer-header-left">
					<h3>Log Record</h3>
					<span class="drawer-subtitle">Masked ingestion metadata</span>
				</div>
				<button
					type="button"
					class="drawer-close-btn"
					onclick={() => selectedLog = null}
					aria-label="Close detail panel"
				>
					<X size={15} />
				</button>
			</div>

			<div class="drawer-body">
				<div class="drawer-prop">
					<span class="prop-label">Timestamp (ISO)</span>
					<span class="prop-val mono-val">{selectedLog.ts}</span>
				</div>

				<div class="drawer-prop">
					<span class="prop-label">Source & Stream</span>
					<div class="prop-val-row">
						<span class="mono-badge">{selectedLog.source}</span>
						<span class="stream-badge">{selectedLog.stream}</span>
					</div>
				</div>

				<div class="drawer-prop">
					<span class="prop-label">Severity Level</span>
					<span class="obs-level-chip chip-{selectedLog.level}">
						{#if selectedLog.level === 'error'}
							<span class="glyph-diamond"></span>
						{:else if selectedLog.level === 'warn'}
							<span class="glyph-square"></span>
						{:else if selectedLog.level === 'info'}
							<span class="glyph-dot-green"></span>
						{:else if selectedLog.level === 'debug'}
							<span class="glyph-dot-blue"></span>
						{:else}
							<span class="glyph-ring"></span>
						{/if}
						{selectedLog.level.toUpperCase()}
					</span>
				</div>

				{#if selectedLog.container_id}
					<div class="drawer-prop">
						<span class="prop-label">Container ID</span>
						<span class="prop-val mono-val">{selectedLog.container_id}</span>
					</div>
				{/if}

				{#if selectedLog.replica !== null}
					<div class="drawer-prop">
						<span class="prop-label">Replica Slot</span>
						<span class="prop-val mono-val">slot #{selectedLog.replica}</span>
					</div>
				{/if}

				{#if selectedLog.trace_id}
					<div class="drawer-prop">
						<span class="prop-label">Trace Parent ID</span>
						<div class="trace-action-row">
							<code class="trace-code-box">{selectedLog.trace_id}</code>
							<button
								type="button"
								class="drawer-filter-btn"
								onclick={() => { setTraceFilter(selectedLog!.trace_id!); selectedLog = null; }}
							>
								Filter Trace
							</button>
						</div>
					</div>
				{/if}

				{#if selectedLog.span_id}
					<div class="drawer-prop">
						<span class="prop-label">Span ID</span>
						<span class="prop-val mono-val">{selectedLog.span_id}</span>
					</div>
				{/if}

				<div class="drawer-prop">
					<span class="prop-label">Masked Message</span>
					<div class="drawer-msg-box">{selectedLog.message}</div>
				</div>

				{#if selectedLog.fields && Object.keys(selectedLog.fields).length > 0}
					<div class="drawer-prop">
						<span class="prop-label">Structured Fields</span>
						<pre class="drawer-json-box">{JSON.stringify(selectedLog.fields, null, 2)}</pre>
					</div>
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	/* ─── Drydock Foundations Styling ────────────────────────────────────────── */

	.obs-root {
		display: flex;
		flex-direction: column;
		gap: 16px;
		font-family: var(--font-sans, 'IBM Plex Sans', sans-serif);
		position: relative;
		height: 100%;
		color: var(--text-primary, #16181B);
	}

	/* ─── Top Toolbar ─────────────────────────────────────────────────────────── */
	.obs-topbar {
		display: flex;
		justify-content: space-between;
		align-items: center;
		flex-wrap: wrap;
		gap: 12px;
		background: var(--bg-surface, #FFFFFF);
		border: 1px solid var(--border, #DCDCD6);
		border-radius: var(--radius-lg, 8px);
		padding: 10px 14px;
	}

	.obs-segmented-tabs {
		display: inline-flex;
		align-items: center;
		background: var(--bg-elevated, #F7F7F5);
		border: 1px solid var(--border, #DCDCD6);
		border-radius: var(--radius-md, 6px);
		padding: 2px;
		gap: 2px;
	}

	.obs-tab-btn {
		height: 28px;
		padding: 0 12px;
		border: 0;
		border-radius: var(--radius-sm, 4px);
		font-family: var(--font-sans, 'IBM Plex Sans', sans-serif);
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		background: transparent;
		color: var(--text-muted, #5F656C);
		transition: background var(--transition-fast, 120ms), color var(--transition-fast, 120ms);
		white-space: nowrap;
	}

	.obs-tab-btn:hover {
		color: var(--text-primary, #16181B);
		background: var(--bg-hover, #ECECE8);
	}

	.obs-tab-btn.active {
		background: var(--rail-bg, #141619);
		color: #FFFFFF;
		font-weight: 600;
	}

	.obs-topbar-right {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.obs-range-group {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.obs-range-chip {
		height: 28px;
		padding: 0 10px;
		border-radius: var(--radius-sm, 4px);
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
		font-weight: 500;
		border: 1px solid var(--border-hover, #C4C4BC);
		background: var(--bg-surface, #FFFFFF);
		color: var(--text-muted, #5F656C);
		cursor: pointer;
		transition: all var(--transition-fast, 120ms);
	}

	.obs-range-chip:hover {
		border-color: var(--text-primary, #16181B);
		color: var(--text-primary, #16181B);
	}

	.obs-range-chip.active {
		background: var(--rail-bg, #141619);
		border-color: var(--rail-bg, #141619);
		color: #FFFFFF;
	}

	.obs-refresh-btn {
		height: 28px;
		width: 28px;
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border: 1px solid var(--border-hover, #C4C4BC);
		border-radius: var(--radius-sm, 4px);
		background: var(--bg-surface, #FFFFFF);
		color: var(--text-muted, #5F656C);
		cursor: pointer;
		transition: all var(--transition-fast, 120ms);
	}

	.obs-refresh-btn:hover {
		border-color: var(--text-primary, #16181B);
		color: var(--text-primary, #16181B);
	}

	/* ─── Controls & Level Filters ────────────────────────────────────────────── */
	.obs-controls-row {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}

	.obs-search-field {
		position: relative;
		flex: 1;
		min-width: 260px;
		display: flex;
		align-items: center;
	}

	:global(.obs-search-icon) {
		position: absolute;
		left: 12px;
		color: var(--text-muted, #5F656C);
		pointer-events: none;
	}

	.obs-search-input {
		width: 100%;
		height: 36px;
		box-sizing: border-box;
		padding: 0 32px 0 34px;
		border: 1px solid var(--border-hover, #C4C4BC);
		border-radius: var(--radius-md, 6px);
		background: var(--bg-surface, #FFFFFF);
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 13px;
		color: var(--text-primary, #16181B);
		outline: none;
		transition: border-color var(--transition-fast, 120ms), box-shadow var(--transition-fast, 120ms);
	}

	.obs-search-input:focus {
		border-color: var(--accent, #C2410C);
		box-shadow: 0 0 0 1px var(--accent, #C2410C), 0 0 0 3px var(--accent-muted, rgba(194, 65, 12, 0.09));
	}

	.obs-clear-btn {
		position: absolute;
		right: 10px;
		background: transparent;
		border: 0;
		color: var(--text-muted, #5F656C);
		cursor: pointer;
		display: flex;
		align-items: center;
		padding: 4px;
	}

	.obs-clear-btn:hover {
		color: var(--text-primary, #16181B);
	}

	/* Level Chips (Drydock 04 — Status never relies on colour alone) */
	.obs-level-chips {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-wrap: wrap;
	}

	.obs-level-chip {
		height: 28px;
		padding: 0 10px;
		border-radius: var(--radius-sm, 4px);
		font-family: var(--font-sans, 'IBM Plex Sans', sans-serif);
		font-size: 12px;
		font-weight: 500;
		display: inline-flex;
		align-items: center;
		gap: 6px;
		cursor: pointer;
		transition: all var(--transition-fast, 120ms);
		white-space: nowrap;
	}

	.chip-all {
		border: 1px solid var(--border-hover, #C4C4BC);
		background: var(--bg-surface, #FFFFFF);
		color: var(--text-muted, #5F656C);
	}
	.chip-all.active {
		background: var(--rail-bg, #141619);
		border-color: var(--rail-bg, #141619);
		color: #FFFFFF;
	}

	.chip-error {
		background: #FBE9E9;
		color: #7F1D1D;
		border: 1px solid #FBE9E9;
	}
	.chip-error.active {
		border-color: #B91C1C;
		box-shadow: 0 0 0 1px #B91C1C;
	}

	.chip-warn {
		background: #F7F0DE;
		color: #713F12;
		border: 1px solid #F7F0DE;
	}
	.chip-warn.active {
		border-color: #A16207;
		box-shadow: 0 0 0 1px #A16207;
	}

	.chip-info {
		background: #E7F3EC;
		color: #14532D;
		border: 1px solid #E7F3EC;
	}
	.chip-info.active {
		border-color: #15803D;
		box-shadow: 0 0 0 1px #15803D;
	}

	.chip-debug {
		background: #E8EEFC;
		color: #1E3A8A;
		border: 1px solid #E8EEFC;
	}
	.chip-debug.active {
		border-color: #1D4ED8;
		box-shadow: 0 0 0 1px #1D4ED8;
	}

	.chip-trace {
		background: #EEEEEB;
		color: #41464D;
		border: 1px solid #EEEEEB;
	}
	.chip-trace.active {
		border-color: #8A9097;
		box-shadow: 0 0 0 1px #8A9097;
	}

	/* Geometric Status Glyphs */
	.glyph-diamond {
		width: 7px;
		height: 7px;
		background: #B91C1C;
		transform: rotate(45deg);
		flex-shrink: 0;
	}

	.glyph-square {
		width: 6px;
		height: 6px;
		border-radius: 1px;
		background: #A16207;
		flex-shrink: 0;
	}

	.glyph-dot-green {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #15803D;
		flex-shrink: 0;
	}

	.glyph-dot-blue {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #1D4ED8;
		flex-shrink: 0;
	}

	.glyph-ring {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		border: 1.5px solid #8A9097;
		box-sizing: border-box;
		flex-shrink: 0;
	}

	.obs-checkbox-label {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		color: var(--text-secondary, #41464D);
		cursor: pointer;
		white-space: nowrap;
	}

	/* ─── Active Trace Bar ────────────────────────────────────────────────────── */
	.obs-trace-bar {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	.obs-trace-chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		background: var(--bg-surface, #FFFFFF);
		border: 1px solid var(--border-hover, #C4C4BC);
		border-radius: var(--radius-sm, 4px);
		padding: 4px 10px;
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
	}

	:global(.trace-hash-icon) {
		color: var(--accent, #C2410C);
	}

	.trace-chip-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--accent, #C2410C);
		letter-spacing: 0.05em;
	}

	.trace-chip-close {
		background: transparent;
		border: 0;
		color: var(--text-muted, #5F656C);
		cursor: pointer;
		display: flex;
		align-items: center;
		padding: 2px;
		margin-left: 2px;
	}

	.trace-chip-close:hover {
		color: var(--text-primary, #16181B);
	}

	.obs-trace-hint {
		font-size: 12px;
		color: var(--text-muted, #5F656C);
	}

	/* ─── Stats Banner (Drydock 01 & 03) ──────────────────────────────────────── */
	.obs-stats-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
		gap: 12px;
	}

	.obs-stat-card {
		background: var(--bg-surface, #FFFFFF);
		border: 1px solid var(--border, #DCDCD6);
		border-radius: var(--radius-md, 6px);
		padding: 14px 20px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.obs-stat-card.has-errs {
		border-color: color-mix(in srgb, var(--accent-red, #B91C1C) 30%, var(--border, #DCDCD6));
	}

	.obs-stat-label {
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
		font-weight: 500;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-muted, #5F656C);
	}

	.obs-stat-value {
		font-size: 24px;
		font-weight: 600;
		letter-spacing: -0.015em;
		font-family: var(--font-sans, 'IBM Plex Sans', sans-serif);
		color: var(--text-primary, #16181B);
		font-variant-numeric: tabular-nums;
	}

	.obs-stat-value.error-val {
		color: var(--accent-red, #B91C1C);
	}

	.obs-stat-sub {
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
		color: var(--text-muted, #5F656C);
	}

	.obs-alert-error {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 14px;
		background: var(--accent-red-muted, rgba(185, 28, 28, 0.09));
		color: var(--accent-red-text, #7F1D1D);
		border: 1px solid color-mix(in srgb, var(--accent-red, #B91C1C) 30%, transparent);
		border-radius: var(--radius-md, 6px);
		font-size: 13px;
	}

	/* ─── Terminal (Drydock 06 — Logs) ────────────────────────────────────────── */
	.obs-terminal {
		background: #141619;
		border: 1px solid #2A2D32;
		border-radius: var(--radius-lg, 8px);
		padding: 18px 20px;
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12.5px;
		line-height: 1.7;
		color: #C3C6CA;
		flex: 1;
		min-height: 480px;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.obs-terminal-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding-bottom: 12px;
		margin-bottom: 8px;
		border-bottom: 1px solid #2A2D32;
		font-size: 11px;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: #6B7178;
		user-select: none;
	}

	.terminal-meta-left {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.terminal-live-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #4ADE80;
		box-shadow: 0 0 6px rgba(74, 222, 128, 0.6);
	}

	.terminal-divider {
		color: #41464D;
	}

	.terminal-meta-right {
		color: #8B9097;
	}

	.obs-terminal-stream {
		flex: 1;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 1px;
		padding-right: 4px;
	}

	.obs-line {
		display: flex;
		align-items: baseline;
		gap: 12px;
		padding: 2px 8px;
		border-radius: var(--radius-sm, 4px);
		cursor: pointer;
		transition: background 80ms;
		outline: none;
	}

	.obs-line:hover {
		background: rgba(255, 255, 255, 0.04);
	}

	.obs-line.selected {
		background: rgba(194, 65, 12, 0.16);
		border-left: 2px solid #C2410C;
		padding-left: 6px;
	}

	.line-ts {
		color: #6B7178;
		width: 90px;
		flex-shrink: 0;
		font-variant-numeric: tabular-nums;
		user-select: none;
	}

	.line-lvl {
		width: 48px;
		flex-shrink: 0;
		display: inline-flex;
		align-items: center;
		gap: 5px;
		user-select: none;
	}

	.line-lvl .shape-diamond {
		width: 6px;
		height: 6px;
		background: #F87171;
		transform: rotate(45deg);
		flex-shrink: 0;
	}

	.line-lvl .shape-square {
		width: 6px;
		height: 6px;
		border-radius: 1px;
		background: #FBBF24;
		flex-shrink: 0;
	}

	.line-lvl .shape-dot-green {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #4ADE80;
		flex-shrink: 0;
	}

	.line-lvl .shape-dot-blue {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #60A5FA;
		flex-shrink: 0;
	}

	.line-lvl .shape-ring {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		border: 1.2px solid #8B9097;
		box-sizing: border-box;
		flex-shrink: 0;
	}

	.line-lvl .lvl-txt {
		font-size: 11px;
		font-weight: 500;
	}

	.lvl-error .lvl-txt { color: #F87171; }
	.lvl-warn .lvl-txt  { color: #FBBF24; }
	.lvl-info .lvl-txt  { color: #4ADE80; }
	.lvl-debug .lvl-txt { color: #60A5FA; }
	.lvl-trace .lvl-txt { color: #8B9097; }

	.line-source {
		color: #8B9097;
		width: 76px;
		flex-shrink: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.line-replica {
		color: #6B7178;
		border: 1px solid #2A2D32;
		border-radius: 3px;
		padding: 0 4px;
		font-size: 11px;
		flex-shrink: 0;
	}

	.line-msg {
		flex: 1;
		word-break: break-all;
		color: #C3C6CA;
	}

	/* Inline Mask Token (Drydock 4px chip style) */
	.obs-mask-token {
		display: inline-flex;
		align-items: center;
		border: 1px solid #41464D;
		background: rgba(255, 255, 255, 0.06);
		color: #C3C6CA;
		border-radius: 4px;
		padding: 0 6px;
		font-size: 11px;
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		margin: 0 3px;
		vertical-align: baseline;
	}

	.line-trace-btn {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 11px;
		color: #6B7178;
		background: rgba(255, 255, 255, 0.04);
		border: 1px solid #2A2D32;
		border-radius: 3px;
		padding: 1px 6px;
		cursor: pointer;
		flex-shrink: 0;
		transition: all 120ms;
	}

	.line-trace-btn:hover {
		color: #FB923C;
		border-color: #FB923C;
		background: rgba(251, 146, 60, 0.08);
	}

	.obs-terminal-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		flex: 1;
		min-height: 280px;
		color: #6B7178;
		gap: 8px;
	}

	.empty-icon {
		opacity: 0.4;
	}

	.obs-terminal-footer {
		padding: 14px 0 4px;
		display: flex;
		justify-content: center;
		border-top: 1px solid #2A2D32;
		margin-top: 8px;
	}

	.obs-more-btn {
		height: 34px;
		padding: 0 16px;
		border: 1px solid #2A2D32;
		border-radius: var(--radius-md, 6px);
		background: rgba(255, 255, 255, 0.04);
		color: #C3C6CA;
		font-family: var(--font-sans, 'IBM Plex Sans', sans-serif);
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		transition: all var(--transition-fast, 120ms);
	}

	.obs-more-btn:hover:not(:disabled) {
		background: rgba(255, 255, 255, 0.08);
		color: #FFFFFF;
	}

	/* ─── Detail Drawer (Drydock 05) ──────────────────────────────────────────── */
	.obs-drawer {
		position: absolute;
		right: 0;
		top: 0;
		bottom: 0;
		width: 500px;
		max-width: 90vw;
		background: var(--bg-surface, #FFFFFF);
		border-left: 1px solid var(--border, #DCDCD6);
		box-shadow: 0 8px 24px rgba(20, 22, 25, 0.08);
		border-top-left-radius: var(--radius-lg, 8px);
		border-bottom-left-radius: var(--radius-lg, 8px);
		display: flex;
		flex-direction: column;
		z-index: 50;
		overflow: hidden;
	}

	.drawer-header {
		display: flex;
		justify-content: space-between;
		align-items: center;
		padding: 18px 24px;
		border-bottom: 1px solid var(--border, #DCDCD6);
		background: var(--bg-elevated, #F7F7F5);
	}

	.drawer-header h3 {
		margin: 0;
		font-size: 16px;
		font-weight: 600;
		color: var(--text-primary, #16181B);
		letter-spacing: -0.015em;
	}

	.drawer-subtitle {
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
		color: var(--text-muted, #5F656C);
	}

	.drawer-close-btn {
		width: 32px;
		height: 32px;
		display: flex;
		align-items: center;
		justify-content: center;
		border: 1px solid var(--border-hover, #C4C4BC);
		border-radius: var(--radius-md, 6px);
		background: var(--bg-surface, #FFFFFF);
		color: var(--text-muted, #5F656C);
		cursor: pointer;
		transition: all var(--transition-fast, 120ms);
	}

	.drawer-close-btn:hover {
		background: var(--bg-hover, #ECECE8);
		color: var(--text-primary, #16181B);
	}

	.drawer-body {
		padding: 24px;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 18px;
	}

	.drawer-prop {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.prop-label {
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
		font-weight: 500;
		letter-spacing: 0.08em;
		text-transform: uppercase;
		color: var(--text-muted, #5F656C);
	}

	.prop-val {
		font-size: 13px;
		color: var(--text-primary, #16181B);
	}

	.mono-val {
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
	}

	.prop-val-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.mono-badge {
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
		background: var(--bg-hover, #ECECE8);
		border: 1px solid var(--border-hover, #C4C4BC);
		padding: 2px 8px;
		border-radius: var(--radius-sm, 4px);
		color: var(--text-primary, #16181B);
	}

	.stream-badge {
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
		color: var(--text-muted, #5F656C);
	}

	.trace-action-row {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	.trace-code-box {
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12.5px;
		background: var(--bg-hover, #ECECE8);
		border: 1px solid var(--border-hover, #C4C4BC);
		padding: 4px 8px;
		border-radius: var(--radius-sm, 4px);
		color: var(--text-primary, #16181B);
	}

	.drawer-filter-btn {
		height: 30px;
		padding: 0 12px;
		border: 1px solid var(--border-hover, #C4C4BC);
		border-radius: var(--radius-md, 6px);
		background: var(--bg-surface, #FFFFFF);
		color: var(--text-primary, #16181B);
		font-family: var(--font-sans, 'IBM Plex Sans', sans-serif);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		transition: all var(--transition-fast, 120ms);
	}

	.drawer-filter-btn:hover {
		border-color: var(--accent, #C2410C);
		color: var(--accent, #C2410C);
	}

	.drawer-msg-box {
		background: var(--bg-elevated, #F7F7F5);
		border: 1px solid var(--border, #DCDCD6);
		border-radius: var(--radius-md, 6px);
		padding: 12px 14px;
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12.5px;
		line-height: 1.6;
		color: var(--text-primary, #16181B);
		white-space: pre-wrap;
		word-break: break-all;
	}

	.drawer-json-box {
		background: #141619;
		border: 1px solid #2A2D32;
		border-radius: var(--radius-md, 6px);
		padding: 14px;
		font-family: var(--font-mono, 'IBM Plex Mono', monospace);
		font-size: 12px;
		line-height: 1.6;
		color: #C3C6CA;
		white-space: pre-wrap;
		word-break: break-all;
		margin: 0;
		overflow-x: auto;
	}

	.animate-spin {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}
</style>
