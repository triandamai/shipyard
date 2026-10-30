<script lang="ts" module>
	export interface LogServiceOption {
		id: string;
		name: string;
		project_id: string;
		project_name: string;
	}
</script>

<script lang="ts">
	import { onMount } from 'svelte';
	import type { StoredLog, LogPage, LogStatsResult, LogQueryParams, ApiResponse } from '$lib/api/types';
	import { RefreshCw, X, ScrollText, Hash, Copy, Check } from '@lucide/svelte';
	import { Button, SearchInput, Select, Toggle, InlineAlert, KeyValueList, Spinner } from '$lib/components/ui';

	type Range = '1h' | '6h' | '24h' | '7d' | '30d';
	type Level = 'all' | 'error' | 'warn' | 'info' | 'debug';

	interface Props {
		scope: 'admin' | 'org';
		fetchLogs: (params: LogQueryParams) => Promise<ApiResponse<LogPage>>;
		fetchStats: (params: LogQueryParams) => Promise<ApiResponse<LogStatsResult>>;
		/** Org scope: services offered in the project/service filters and used to label rows. */
		services?: LogServiceOption[];
		/** Initial filters, e.g. from `?service=` when opened from a service panel. */
		projectId?: string;
		serviceId?: string;
		/** Called when the project/service filter changes so the page can mirror it in the URL. */
		onFilterChange?: (f: { projectId: string; serviceId: string }) => void;
	}

	let { scope, fetchLogs, fetchStats, services = [], projectId = '', serviceId = '', onFilterChange }: Props = $props();

	// ─── Filters ───────────────────────────────────────────────────────────────

	const SOURCES = [
		{ id: 'all', label: 'All' },
		{ id: 'app', label: 'Apps' },
		{ id: 'backend', label: 'Backend' },
		{ id: 'frontend', label: 'Frontend' },
		{ id: 'traefik', label: 'Traefik' },
		{ id: 'postgres', label: 'Postgres' }
	];
	const LEVELS: { id: Level; label: string }[] = [
		{ id: 'all', label: 'All' },
		{ id: 'error', label: 'Error' },
		{ id: 'warn', label: 'Warn' },
		{ id: 'info', label: 'Info' },
		{ id: 'debug', label: 'Debug' }
	];
	const RANGES: { id: Range; ms: number }[] = [
		{ id: '1h', ms: 3600e3 },
		{ id: '6h', ms: 6 * 3600e3 },
		{ id: '24h', ms: 24 * 3600e3 },
		{ id: '7d', ms: 7 * 24 * 3600e3 },
		{ id: '30d', ms: 30 * 24 * 3600e3 }
	];
	const LIVE_INTERVAL_MS = 4000;

	// svelte-ignore state_referenced_locally
	let projectFilter = $state(projectId);
	// svelte-ignore state_referenced_locally
	let serviceFilter = $state(serviceId);
	let source = $state('all');
	let level = $state<Level>('all');
	let range = $state<Range>('24h');
	let keyword = $state('');
	let traceId = $state('');
	let live = $state(false);

	let entries = $state<StoredLog[]>([]);
	let nextCursor = $state<string | null>(null);
	let stats = $state<LogStatsResult | null>(null);
	let loading = $state(false);
	let loadingMore = $state(false);
	let errorMsg = $state<string | null>(null);
	let selected = $state<StoredLog | null>(null);
	let copied = $state(false);
	let windowFrom = $state(new Date());
	let windowTo = $state(new Date());

	let serviceById = $derived(new Map(services.map((s) => [s.id, s])));
	let projectOptions = $derived([
		{ value: '', label: 'All projects' },
		...[...new Map(services.map((s) => [s.project_id, s.project_name])).entries()]
			.sort((a, b) => a[1].localeCompare(b[1]))
			.map(([value, label]) => ({ value, label }))
	]);
	let serviceOptions = $derived([
		{ value: '', label: 'All services' },
		...services
			.filter((s) => !projectFilter || s.project_id === projectFilter)
			.sort((a, b) => a.name.localeCompare(b.name))
			.map((s) => ({ value: s.id, label: s.name }))
	]);

	function levelParam(l: Level): string | undefined {
		if (l === 'all') return undefined;
		// Fatal is shown and filtered together with error.
		return l === 'error' ? 'error,fatal' : l;
	}

	function baseParams(): LogQueryParams {
		return {
			source: scope === 'admin' && source !== 'all' ? source : undefined,
			level: levelParam(level),
			project_id: projectFilter || undefined,
			service_id: serviceFilter || undefined
		};
	}

	// ─── Loading ───────────────────────────────────────────────────────────────

	let requestSeq = 0;

	async function load() {
		const seq = ++requestSeq;
		loading = true;
		errorMsg = null;
		const ms = RANGES.find((r) => r.id === range)!.ms;
		windowTo = new Date();
		windowFrom = new Date(windowTo.getTime() - ms);
		const from = windowFrom.toISOString();
		const to = windowTo.toISOString();
		const base = baseParams();

		const [logRes, statsRes] = await Promise.all([
			fetchLogs({ ...base, from, to, q: keyword.trim() || undefined, trace_id: traceId || undefined, limit: 200 }),
			fetchStats({ ...base, from, to, bucket: ms > 2 * 24 * 3600e3 ? 'day' : 'hour' })
		]);
		if (seq !== requestSeq) return;

		if (logRes.error) {
			errorMsg = logRes.error.message;
			entries = [];
			nextCursor = null;
		} else if (logRes.data) {
			entries = logRes.data.entries;
			nextCursor = logRes.data.next_cursor;
		}
		stats = statsRes.data ?? null;
		loading = false;
	}

	async function loadOlder() {
		if (!nextCursor || loadingMore) return;
		loadingMore = true;
		const res = await fetchLogs({
			...baseParams(),
			from: windowFrom.toISOString(),
			to: windowTo.toISOString(),
			q: keyword.trim() || undefined,
			trace_id: traceId || undefined,
			cursor: nextCursor,
			limit: 200
		});
		if (res.data) {
			entries = [...entries, ...res.data.entries];
			nextCursor = res.data.next_cursor;
		} else if (res.error) {
			errorMsg = res.error.message;
		}
		loadingMore = false;
	}

	/** Live tail: fetch anything newer than the top row and prepend it. */
	async function poll() {
		if (loading) return;
		const newest = entries[0];
		const res = await fetchLogs({
			...baseParams(),
			from: newest ? newest.ts : windowFrom.toISOString(),
			to: new Date().toISOString(),
			q: keyword.trim() || undefined,
			trace_id: traceId || undefined,
			limit: 200
		});
		if (!res.data) return;
		const seen = new Set(entries.map((e) => e.id));
		const fresh = res.data.entries.filter((e) => !seen.has(e.id));
		if (fresh.length) entries = [...fresh, ...entries].slice(0, 2000);
	}

	$effect(() => {
		if (!live) return;
		const t = setInterval(poll, LIVE_INTERVAL_MS);
		return () => clearInterval(t);
	});

	let searchTimer: ReturnType<typeof setTimeout>;
	let searchArmed = false;
	$effect(() => {
		keyword;
		// The first run is the mount; onMount already loads.
		if (!searchArmed) { searchArmed = true; return; }
		clearTimeout(searchTimer);
		searchTimer = setTimeout(load, 300);
		return () => clearTimeout(searchTimer);
	});

	function setProject(v: string) {
		projectFilter = v;
		if (serviceFilter && serviceById.get(serviceFilter)?.project_id !== v) serviceFilter = '';
		onFilterChange?.({ projectId: projectFilter, serviceId: serviceFilter });
		load();
	}

	function setService(v: string) {
		serviceFilter = v;
		if (v) projectFilter = serviceById.get(v)?.project_id ?? projectFilter;
		onFilterChange?.({ projectId: projectFilter, serviceId: serviceFilter });
		load();
	}

	function setTrace(tid: string) {
		traceId = tid;
		selected = null;
		load();
	}

	onMount(load);

	// ─── Volume histogram ──────────────────────────────────────────────────────

	interface Bar { start: number; total: number; errors: number; warns: number }

	let bars = $derived.by<Bar[]>(() => {
		if (!stats) return [];
		const step = RANGES.find((r) => r.id === range)!.ms > 2 * 24 * 3600e3 ? 24 * 3600e3 : 3600e3;
		const first = Math.floor(windowFrom.getTime() / step) * step;
		const out: Bar[] = [];
		for (let t = first; t <= windowTo.getTime(); t += step) out.push({ start: t, total: 0, errors: 0, warns: 0 });
		for (const p of stats.points) {
			const i = Math.floor((new Date(p.bucket_start).getTime() - first) / step);
			const b = out[i];
			if (!b) continue;
			b.total += p.count;
			if (p.level === 'error' || p.level === 'fatal') b.errors += p.count;
			else if (p.level === 'warn') b.warns += p.count;
		}
		return out;
	});
	let barMax = $derived(Math.max(1, ...bars.map((b) => b.total)));
	let errorRate = $derived(stats && stats.total ? (stats.errors / stats.total) * 100 : 0);

	// ─── Formatting ────────────────────────────────────────────────────────────

	let multiDay = $derived(range === '7d' || range === '30d');

	function fmtTime(ts: string): string {
		const d = new Date(ts);
		if (isNaN(d.getTime())) return ts;
		const time = d.toLocaleTimeString([], { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
		const ms = String(d.getMilliseconds()).padStart(3, '0');
		if (!multiDay) return `${time}.${ms}`;
		return `${d.toLocaleDateString([], { month: 'short', day: '2-digit' })} ${time}`;
	}

	function fmtBucket(t: number): string {
		const d = new Date(t);
		return multiDay
			? d.toLocaleDateString([], { month: 'short', day: 'numeric' })
			: d.toLocaleString([], { month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit', hour12: false });
	}

	const LEVEL_TAG: Record<StoredLog['level'], string> = {
		trace: 'TRC', debug: 'DBG', info: 'INF', warn: 'WRN', error: 'ERR', fatal: 'FTL'
	};

	function origin(e: StoredLog): string {
		if (e.service_id) {
			const s = serviceById.get(e.service_id);
			if (s) return s.name;
		}
		return e.source === 'app' ? (e.container_id?.slice(0, 12) ?? 'app') : e.source;
	}

	function splitMasked(text: string): string[] {
		return text.split(/(••• \[[a-z0-9_]+\])/g);
	}

	let detailItems = $derived(
		selected
			? [
					{ key: 'Time', value: selected.ts, mono: true },
					{ key: 'Level', value: selected.level.toUpperCase() },
					{ key: 'Source', value: selected.source },
					{ key: 'Stream', value: selected.stream },
					...(selected.service_id ? [{ key: 'Service', value: serviceById.get(selected.service_id)?.name ?? selected.service_id }] : []),
					...(selected.container_id ? [{ key: 'Container', value: selected.container_id.slice(0, 12), mono: true }] : []),
					...(selected.replica !== null ? [{ key: 'Replica', value: `#${selected.replica}`, mono: true }] : []),
					...(selected.deployment_id ? [{ key: 'Deployment', value: selected.deployment_id.slice(0, 8), mono: true }] : []),
					...(selected.span_id ? [{ key: 'Span', value: selected.span_id, mono: true }] : [])
				]
			: []
	);

	async function copyMessage() {
		if (!selected) return;
		await navigator.clipboard.writeText(selected.message);
		copied = true;
		setTimeout(() => (copied = false), 1200);
	}

	function onRowKey(e: KeyboardEvent, entry: StoredLog) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			selected = selected?.id === entry.id ? null : entry;
		}
	}
</script>

<div class="obs">
	<!-- Filters -->
	<div class="obs-toolbar">
		<div class="obs-search"><SearchInput bind:value={keyword} placeholder="Search messages…" /></div>

		{#if scope === 'org'}
			<div class="obs-select">
				<Select value={projectFilter} options={projectOptions} aria-label="Project" onchange={(e) => setProject((e.currentTarget as HTMLSelectElement).value)} />
			</div>
			<div class="obs-select">
				<Select value={serviceFilter} options={serviceOptions} aria-label="Service" onchange={(e) => setService((e.currentTarget as HTMLSelectElement).value)} />
			</div>
		{/if}

		<div class="obs-toolbar-end">
			<div class="obs-seg" role="group" aria-label="Time range">
				{#each RANGES as r (r.id)}
					<button type="button" class={range === r.id ? 'obs-seg-btn obs-seg-btn--on' : 'obs-seg-btn'} aria-pressed={range === r.id} onclick={() => { range = r.id; load(); }}>{r.id}</button>
				{/each}
			</div>
			<label class="obs-live">
				<Toggle bind:checked={live} label="Live tail" />
				<span class={live ? 'obs-live-text obs-live-text--on' : 'obs-live-text'}>Live</span>
			</label>
			<Button variant="secondary" size="icon" aria-label="Refresh" title="Refresh" onclick={load}>
				<span class={loading ? 'obs-spin' : ''}><RefreshCw size={14} /></span>
			</Button>
		</div>
	</div>

	<div class="obs-subbar">
		{#if scope === 'admin'}
			<div class="obs-seg" role="group" aria-label="Source">
				{#each SOURCES as s (s.id)}
					<button type="button" class={source === s.id ? 'obs-seg-btn obs-seg-btn--on' : 'obs-seg-btn'} aria-pressed={source === s.id} onclick={() => { source = s.id; load(); }}>{s.label}</button>
				{/each}
			</div>
		{/if}
		<div class="obs-seg" role="group" aria-label="Level">
			{#each LEVELS as l (l.id)}
				<button type="button" class={level === l.id ? 'obs-seg-btn obs-seg-btn--on' : 'obs-seg-btn'} aria-pressed={level === l.id} onclick={() => { level = l.id; load(); }}>
					{#if l.id !== 'all'}<span class="obs-glyph obs-glyph--{l.id}" aria-hidden="true"></span>{/if}{l.label}
				</button>
			{/each}
		</div>
		{#if traceId}
			<span class="obs-trace-chip">
				<Hash size={12} />
				<span class="obs-trace-id">{traceId.slice(0, 16)}</span>
				<button type="button" aria-label="Clear trace filter" onclick={() => setTrace('')}><X size={12} /></button>
			</span>
		{/if}
	</div>

	<!-- Volume -->
	<div class="obs-volume">
		<div class="obs-metrics">
			<div class="obs-metric">
				<span class="obs-metric-label">Lines</span>
				<span class="obs-metric-value">{stats ? stats.total.toLocaleString() : '—'}</span>
			</div>
			<div class="obs-metric">
				<span class="obs-metric-label">Errors</span>
				<span class={stats?.errors ? 'obs-metric-value obs-metric-value--bad' : 'obs-metric-value'}>{stats ? stats.errors.toLocaleString() : '—'}</span>
			</div>
			<div class="obs-metric">
				<span class="obs-metric-label">Error rate</span>
				<span class="obs-metric-value">{stats ? `${errorRate.toFixed(errorRate < 1 && errorRate > 0 ? 2 : 1)}%` : '—'}</span>
			</div>
		</div>
		<div class="obs-histogram" aria-label="Log volume over time" role="img">
			{#each bars as b (b.start)}
				{@const h = (b.total / barMax) * 100}
				<div class="obs-bar" title="{fmtBucket(b.start)} · {b.total.toLocaleString()} lines, {b.errors} errors, {b.warns} warnings">
					<div class="obs-bar-stack" style="height:{Math.max(h, b.total ? 4 : 0)}%">
						<span class="obs-bar-seg obs-bar-seg--error" style="flex-grow:{b.errors}"></span>
						<span class="obs-bar-seg obs-bar-seg--warn" style="flex-grow:{b.warns}"></span>
						<span class="obs-bar-seg obs-bar-seg--rest" style="flex-grow:{b.total - b.errors - b.warns}"></span>
					</div>
				</div>
			{/each}
		</div>
	</div>

	{#if errorMsg}
		<InlineAlert tone="error">{errorMsg}</InlineAlert>
	{/if}

	<!-- Stream + detail -->
	<div class="obs-body">
		<div class="obs-console" style:--obs-time-col={multiDay ? '140px' : '104px'}>
			<div class="obs-console-head" role="row">
				<span>Time</span><span>Level</span><span>{scope === 'org' ? 'Service' : 'Source'}</span><span>Message</span>
			</div>

			<div class="obs-console-rows" role="table" aria-label="Log lines">
				{#if loading && entries.length === 0}
					<div class="obs-console-state"><Spinner size={14} /> Loading logs…</div>
				{:else if entries.length === 0}
					<div class="obs-console-state obs-console-state--empty">
						<ScrollText size={26} />
						<span>No log lines in this window.</span>
						<span class="obs-console-hint">Widen the time range or clear filters. New container output appears within a few seconds.</span>
					</div>
				{:else}
					{#each entries as e (e.id)}
						<div
							role="row"
							tabindex="0"
							class={`obs-row obs-row--${e.level}${selected?.id === e.id ? ' obs-row--selected' : ''}`}
							onclick={() => (selected = selected?.id === e.id ? null : e)}
							onkeydown={(ev) => onRowKey(ev, e)}
						>
							<span class="obs-cell-time">{fmtTime(e.ts)}</span>
							<span class="obs-cell-level obs-lvl--{e.level}">{LEVEL_TAG[e.level]}</span>
							<span class="obs-cell-origin" title={origin(e)}>{origin(e)}{#if e.replica !== null}<span class="obs-replica">.{e.replica}</span>{/if}</span>
							<span class="obs-cell-msg">{#each splitMasked(e.message) as part}{#if part.startsWith('••• [')}<span class="obs-mask">{part}</span>{:else}{part}{/if}{/each}{#if e.trace_id}<button type="button" class="obs-trace-link" title="Show all lines for this trace" onclick={(ev) => { ev.stopPropagation(); setTrace(e.trace_id!); }}>#{e.trace_id.slice(0, 7)}</button>{/if}</span>
						</div>
					{/each}
					{#if nextCursor}
						<div class="obs-console-more">
							<button type="button" class="obs-more-btn" onclick={loadOlder} disabled={loadingMore}>
								{loadingMore ? 'Loading…' : 'Load older lines'}
							</button>
						</div>
					{/if}
				{/if}
			</div>

			<div class="obs-console-foot">
				<span class={live ? 'obs-live-dot obs-live-dot--on' : 'obs-live-dot'}></span>
				<span>{live ? 'Tailing' : 'Paused'}</span>
				<span class="obs-foot-sep">·</span>
				<span>{entries.length.toLocaleString()} lines loaded</span>
			</div>
		</div>

		{#if selected}
			<aside class="obs-detail" aria-label="Log line details">
				<div class="obs-detail-head">
					<span class="obs-detail-title">Log line</span>
					<Button variant="ghost" size="icon" aria-label="Close details" onclick={() => (selected = null)}><X size={15} /></Button>
				</div>
				<div class="obs-detail-body">
					<div class="obs-detail-section">
						<div class="obs-detail-label-row">
							<span class="obs-detail-label">Message</span>
							<Button variant="ghost" size="sm" onclick={copyMessage}>
								{#if copied}<Check size={13} /> Copied{:else}<Copy size={13} /> Copy{/if}
							</Button>
						</div>
						<pre class="obs-detail-msg">{selected.message}</pre>
					</div>

					<KeyValueList items={detailItems} keyWidth="92px" />

					{#if selected.trace_id}
						<div class="obs-detail-section">
							<span class="obs-detail-label">Trace</span>
							<div class="obs-detail-trace">
								<code>{selected.trace_id}</code>
								<Button variant="secondary" size="sm" onclick={() => setTrace(selected!.trace_id!)}>Filter</Button>
							</div>
						</div>
					{/if}

					{#if selected.fields && Object.keys(selected.fields).length > 0}
						<div class="obs-detail-section">
							<span class="obs-detail-label">Fields</span>
							<pre class="obs-detail-json">{JSON.stringify(selected.fields, null, 2)}</pre>
						</div>
					{/if}
				</div>
			</aside>
		{/if}
	</div>
</div>

<style>
	.obs {
		display: flex;
		flex-direction: column;
		gap: 12px;
		min-height: 0;
		flex: 1;
	}

	/* ─── Toolbar ─────────────────────────────────────────── */
	.obs-toolbar,
	.obs-subbar {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}
	.obs-search { flex: 1 1 240px; min-width: 200px; max-width: 420px; }
	.obs-select { flex: 0 1 180px; min-width: 140px; }
	.obs-select :global(.ui-select) { height: 34px; font-size: 12.5px; }
	.obs-toolbar-end { display: flex; align-items: center; gap: 8px; margin-left: auto; }

	/* Segmented control: one surface, signal colour marks the selection. */
	.obs-seg {
		display: inline-flex;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 2px;
		gap: 2px;
	}
	.obs-seg-btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		height: 28px;
		padding: 0 10px;
		border: none;
		border-radius: var(--radius-sm);
		background: transparent;
		color: var(--text-muted);
		font: 500 12px var(--font-sans);
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast);
	}
	.obs-seg-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
	.obs-seg-btn--on,
	.obs-seg-btn--on:hover { background: var(--accent-muted); color: var(--accent); font-weight: 600; }

	/* Level glyphs: shape + label, never colour alone. */
	.obs-glyph { width: 7px; height: 7px; flex-shrink: 0; }
	.obs-glyph--error { background: var(--accent-red); transform: rotate(45deg); }
	.obs-glyph--warn { background: var(--accent-yellow); }
	.obs-glyph--info { background: var(--accent-blue); border-radius: 50%; }
	.obs-glyph--debug { border: 1.5px solid var(--text-dim); border-radius: 50%; }

	.obs-live { display: inline-flex; align-items: center; gap: 6px; cursor: pointer; }
	.obs-live-text { font-size: 12px; font-weight: 500; color: var(--text-muted); }
	.obs-live-text--on { color: var(--text-primary); }
	.obs-spin { display: inline-flex; animation: obs-spin 0.8s linear infinite; }
	@keyframes obs-spin { to { transform: rotate(360deg); } }

	.obs-trace-chip {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		height: 30px;
		padding: 0 6px 0 10px;
		border: 1px solid var(--accent);
		background: var(--accent-muted);
		color: var(--accent);
		border-radius: var(--radius-md);
		font-size: 12px;
	}
	.obs-trace-id { font-family: var(--font-mono); }
	.obs-trace-chip button {
		display: inline-flex;
		border: none;
		background: none;
		color: inherit;
		cursor: pointer;
		padding: 3px;
		border-radius: var(--radius-sm);
	}
	.obs-trace-chip button:hover { background: var(--accent-muted); }

	/* ─── Volume ──────────────────────────────────────────── */
	.obs-volume {
		display: flex;
		align-items: stretch;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.obs-metrics { display: flex; border-right: 1px solid var(--border); }
	.obs-metric {
		display: flex;
		flex-direction: column;
		justify-content: center;
		gap: 2px;
		padding: 10px 18px;
		min-width: 104px;
	}
	.obs-metric + .obs-metric { border-left: 1px solid var(--border); }
	.obs-metric-label { font-size: 11px; font-weight: 500; color: var(--text-muted); }
	.obs-metric-value {
		font: 600 18px var(--font-mono);
		color: var(--text-primary);
		letter-spacing: -0.02em;
		font-variant-numeric: tabular-nums;
	}
	.obs-metric-value--bad { color: var(--accent-red); }
	.obs-histogram {
		flex: 1;
		display: flex;
		align-items: flex-end;
		gap: 2px;
		height: 64px;
		padding: 10px 14px;
		min-width: 0;
	}
	.obs-bar { flex: 1; height: 100%; display: flex; align-items: flex-end; min-width: 2px; }
	.obs-bar:hover .obs-bar-stack { opacity: 0.75; }
	.obs-bar-stack {
		width: 100%;
		display: flex;
		flex-direction: column;
		border-radius: 2px 2px 0 0;
		overflow: hidden;
	}
	.obs-bar-seg { flex-basis: 0; display: block; }
	.obs-bar-seg--error { background: var(--accent-red); }
	.obs-bar-seg--warn { background: var(--accent-yellow); }
	.obs-bar-seg--rest { background: var(--border-hover); }

	/* ─── Console ─────────────────────────────────────────── */
	.obs-body { flex: 1; min-height: 320px; display: flex; gap: 12px; }
	.obs-console {
		flex: 1;
		min-width: 0;
		display: flex;
		flex-direction: column;
		background: var(--terminal-bg);
		border: 1px solid var(--terminal-border);
		border-radius: var(--radius-lg);
		overflow: hidden;
		font-family: var(--font-mono);
		font-size: 12px;
	}
	.obs-console-head,
	.obs-row {
		display: grid;
		grid-template-columns: var(--obs-time-col, 104px) 40px minmax(90px, 160px) 1fr;
		column-gap: 14px;
		padding: 0 14px;
	}
	.obs-console-head {
		height: 30px;
		align-items: center;
		border-bottom: 1px solid var(--terminal-border);
		color: var(--terminal-dim);
		font: 500 11px var(--font-sans);
	}
	.obs-console-rows { flex: 1; overflow: auto; padding: 4px 0; }
	.obs-row {
		align-items: baseline;
		padding-top: 3px;
		padding-bottom: 3px;
		line-height: 1.55;
		color: var(--terminal-fg);
		cursor: pointer;
		box-shadow: inset 2px 0 0 transparent;
	}
	.obs-row:hover { background: rgba(255, 255, 255, 0.04); }
	.obs-row:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }
	.obs-row--error, .obs-row--fatal { box-shadow: inset 2px 0 0 var(--terminal-red); background: color-mix(in srgb, var(--terminal-red) 6%, transparent); }
	.obs-row--warn { box-shadow: inset 2px 0 0 var(--terminal-yellow); }
	.obs-row--selected,
	.obs-row--selected:hover { background: color-mix(in srgb, var(--accent) 18%, transparent); box-shadow: inset 2px 0 0 var(--accent); }

	.obs-cell-time { color: var(--terminal-dim); white-space: nowrap; font-variant-numeric: tabular-nums; }
	.obs-cell-level { font-weight: 600; }
	.obs-lvl--trace, .obs-lvl--debug { color: var(--terminal-dim); }
	.obs-lvl--info { color: var(--terminal-blue); }
	.obs-lvl--warn { color: var(--terminal-yellow); }
	.obs-lvl--error, .obs-lvl--fatal { color: var(--terminal-red); }
	.obs-cell-origin {
		color: var(--terminal-orange);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.obs-replica { color: var(--terminal-dim); }
	.obs-cell-msg { white-space: pre-wrap; word-break: break-word; min-width: 0; }
	.obs-mask {
		display: inline-block;
		padding: 0 4px;
		border-radius: 3px;
		background: rgba(255, 255, 255, 0.08);
		color: var(--terminal-dim);
		font-size: 11px;
	}
	.obs-trace-link {
		margin-left: 8px;
		border: none;
		background: none;
		padding: 0;
		color: var(--terminal-blue);
		font: inherit;
		font-size: 11px;
		cursor: pointer;
	}
	.obs-trace-link:hover { text-decoration: underline; }

	.obs-console-state {
		height: 100%;
		min-height: 220px;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		color: var(--terminal-dim);
		font-family: var(--font-sans);
		font-size: 13px;
		text-align: center;
		padding: 24px;
	}
	.obs-console-state--empty { color: var(--terminal-fg); }
	.obs-console-state--empty :global(svg) { color: var(--terminal-dim); }
	.obs-console-hint { color: var(--terminal-dim); font-size: 12px; max-width: 340px; }
	.obs-console-more { display: flex; justify-content: center; padding: 10px; }
	.obs-more-btn {
		height: 28px;
		padding: 0 14px;
		border: 1px solid var(--terminal-border);
		border-radius: var(--radius-md);
		background: transparent;
		color: var(--terminal-fg);
		font: 500 12px var(--font-sans);
		cursor: pointer;
	}
	.obs-more-btn:hover:not(:disabled) { background: rgba(255, 255, 255, 0.05); }
	.obs-more-btn:disabled { opacity: 0.6; cursor: default; }

	.obs-console-foot {
		display: flex;
		align-items: center;
		gap: 8px;
		height: 28px;
		padding: 0 14px;
		border-top: 1px solid var(--terminal-border);
		color: var(--terminal-dim);
		font: 11px var(--font-sans);
	}
	.obs-foot-sep { opacity: 0.6; }
	.obs-live-dot { width: 6px; height: 6px; border-radius: 50%; background: var(--terminal-dim); }
	.obs-live-dot--on { background: var(--terminal-green); animation: obs-pulse 1.6s ease-in-out infinite; }
	@keyframes obs-pulse { 50% { opacity: 0.35; } }

	/* ─── Detail ──────────────────────────────────────────── */
	.obs-detail {
		width: 360px;
		flex-shrink: 0;
		display: flex;
		flex-direction: column;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.obs-detail-head {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 8px 8px 16px;
		border-bottom: 1px solid var(--border);
	}
	.obs-detail-title { font-size: 13px; font-weight: 600; color: var(--text-primary); }
	.obs-detail-body { flex: 1; overflow-y: auto; padding: 14px 16px; display: flex; flex-direction: column; gap: 16px; }
	.obs-detail-section { display: flex; flex-direction: column; gap: 6px; }
	.obs-detail-label-row { display: flex; align-items: center; justify-content: space-between; }
	.obs-detail-label { font-size: 11px; font-weight: 600; color: var(--text-muted); }
	.obs-detail-msg,
	.obs-detail-json {
		margin: 0;
		padding: 10px 12px;
		background: var(--terminal-bg);
		color: var(--terminal-fg);
		border-radius: var(--radius-md);
		font: 12px/1.55 var(--font-mono);
		white-space: pre-wrap;
		word-break: break-word;
		max-height: 260px;
		overflow: auto;
	}
	.obs-detail-trace { display: flex; align-items: center; gap: 8px; }
	.obs-detail-trace code {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		font-size: 11.5px;
		color: var(--text-secondary);
	}

	@media (max-width: 1100px) {
		.obs-body { position: relative; }
		.obs-detail {
			position: absolute;
			right: 0;
			top: 0;
			bottom: 0;
			box-shadow: var(--shadow-lg);
			z-index: 2;
		}
	}
	@media (max-width: 639px) {
		.obs-toolbar-end { margin-left: 0; width: 100%; justify-content: space-between; }
		.obs-search, .obs-select { max-width: none; flex: 1 1 100%; }
		.obs-volume { flex-direction: column; }
		.obs-metrics { border-right: none; border-bottom: 1px solid var(--border); }
		.obs-metric { flex: 1; min-width: 0; padding: 8px 12px; }
		.obs-console-head { display: none; }
		.obs-row { grid-template-columns: auto auto 1fr; row-gap: 2px; }
		.obs-cell-msg { grid-column: 1 / -1; }
		.obs-detail { width: 100%; }
		.obs-subbar { overflow-x: auto; flex-wrap: nowrap; }
	}
</style>
