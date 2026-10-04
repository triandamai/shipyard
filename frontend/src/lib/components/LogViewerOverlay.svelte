<script lang="ts" module>
	export interface LogColumn {
		key: string;
		label: string;
		width?: string;
		mono?: boolean;
		color?: (row: any) => string;
		format?: (val: any, row: any) => string | number;
	}

	export interface ParsedLine {
		ts?: string;
		level?: 'error' | 'warn' | 'info' | 'debug' | 'trace';
		content: string;
	}
</script>

<script lang="ts">
	import { onDestroy } from 'svelte';
	import type { Snippet } from 'svelte';
	import { X, RefreshCw, Play, Square } from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import SearchInput from '$lib/components/ui/SearchInput.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';

	interface Props {
		/** Controlled visibility */
		open: boolean;
		/** Panel title */
		title: string;
		/** Secondary title info (container ID, function name, …) */
		subtitle?: string;
		/** Called when the user closes the panel */
		onClose: () => void;

		/**
		 * Fetch function — called with the current tail value on open/tail-change.
		 * Return `string[]` for stream mode, or `any[]` for table mode.
		 */
		fetchFn: (tail: number) => Promise<string[] | any[]>;

		/**
		 * SSE endpoint URL for live streaming (stream mode only).
		 * If omitted the panel is fetch-only (table mode can also omit it).
		 */
		streamUrl?: string;

		/**
		 * Column definitions — enables table mode.
		 * When omitted the panel uses terminal/stream mode.
		 */
		columns?: LogColumn[];

		/**
		 * Changing this value re-initialises the overlay (useful when the
		 * caller switches the underlying resource, e.g. container replica).
		 */
		resetKey?: string;

		/** Tail-line options shown in the header (stream mode) */
		tailOptions?: number[];
		/** Initial tail value */
		initialTail?: number;

		/** Stream mode: optional structured line parser */
		parseLine?: (raw: string) => ParsedLine;

		/** Table mode: empty state message */
		emptyMessage?: string;

		/**
		 * Extra content rendered right of the title (replica selector, etc.).
		 * Svelte 5 snippet.
		 */
		headerControls?: Snippet;
	}

	let {
		open,
		title,
		subtitle,
		onClose,
		fetchFn,
		streamUrl,
		columns,
		resetKey = '',
		tailOptions = [100, 200, 500, 1000],
		initialTail = 200,
		parseLine,
		emptyMessage = 'No logs yet.',
		headerControls,
	}: Props = $props();

	// ── Mode ──────────────────────────────────────────────────────────────────

	const isTable = $derived(!!columns?.length);

	// ── Shared ────────────────────────────────────────────────────────────────

	let search = $state('');
	// eslint-disable-next-line svelte/reactivity-svelte-5 -- intentional: initialTail is used only as initial value
	let tail   = $state(initialTail); // $effect resets this to initialTail on open

	// ── Stream mode ───────────────────────────────────────────────────────────

	let initLines  = $state<string[]>([]);
	let liveLines  = $state<string[]>([]);
	let fetching   = $state(false);
	let fetchError = $state('');

	type StreamStatus = 'idle' | 'connecting' | 'connected' | 'error';
	let streamStatus = $state<StreamStatus>('idle');
	let streamError  = $state('');
	let esSource: EventSource | null = null;
	let consoleEl = $state<HTMLElement | null>(null);

	let allLines = $derived([...initLines, ...liveLines]);
	let filteredLines = $derived(
		search
			? allLines.filter(l => l.toLowerCase().includes(search.toLowerCase()))
			: allLines
	);

	// ── Table mode ────────────────────────────────────────────────────────────

	let rows        = $state<any[]>([]);
	let tableLoading = $state(false);
	let tableError   = $state('');

	let filteredRows = $derived(
		search
			? rows.filter(r =>
				Object.values(r).some(v => String(v ?? '').toLowerCase().includes(search.toLowerCase()))
			)
			: rows
	);

	// ── Helpers ───────────────────────────────────────────────────────────────

	function scrollBottom() {
		if (consoleEl) requestAnimationFrame(() => {
			if (consoleEl) consoleEl.scrollTop = consoleEl.scrollHeight;
		});
	}

	async function doFetch() {
		if (isTable) {
			tableLoading = true;
			tableError = '';
		} else {
			fetching = true;
			fetchError = '';
			initLines = [];
		}
		try {
			const data = await fetchFn(tail);
			if (isTable) {
				rows = data as any[];
			} else {
				initLines = data as string[];
				scrollBottom();
			}
		} catch (e: any) {
			if (isTable) tableError = e.message ?? 'Failed to load';
			else fetchError = e.message ?? 'Failed to load';
		} finally {
			fetching = false;
			tableLoading = false;
		}
	}

	function connectStream() {
		if (!streamUrl || esSource) return;
		streamStatus = 'connecting';
		streamError = '';
		liveLines = [];
		const sep = streamUrl.includes('?') ? '&' : '?';
		const url = `${streamUrl}${sep}tail=${tail}`;
		const es = new EventSource(url);
		esSource = es;

		es.onopen = () => { streamStatus = 'connected'; };
		es.onmessage = (e) => {
			if (!e.data?.trim()) return;
			liveLines = [...liveLines, e.data];
			scrollBottom();
		};
		es.addEventListener('error', (e: MessageEvent) => {
			streamError = e.data ?? 'Stream error';
			streamStatus = 'error';
		});
		es.onerror = () => {
			if (streamStatus === 'connecting') {
				streamError = 'Could not connect';
				streamStatus = 'error';
				es.close();
				esSource = null;
			}
		};
	}

	function disconnectStream() {
		esSource?.close();
		esSource = null;
		streamStatus = 'idle';
	}

	async function changeTail(n: number) {
		tail = n;
		disconnectStream();
		liveLines = [];
		await doFetch();
	}

	// ── Lifecycle ─────────────────────────────────────────────────────────────

	$effect(() => {
		// react to open, and to resetKey so the panel re-initialises when the
		// caller switches resources (e.g. different container replica)
		void resetKey;

		if (open) {
			search = '';
			tail = initialTail;
			if (isTable) {
				rows = [];
				tableError = '';
				doFetch();
			} else {
				initLines = [];
				liveLines = [];
				fetchError = '';
				streamStatus = 'idle';
				streamError = '';
				doFetch();
			}
		} else {
			disconnectStream();
			initLines = [];
			liveLines = [];
			rows = [];
		}
	});

	onDestroy(() => { disconnectStream(); });

	function handleClose() {
		disconnectStream();
		onClose();
	}

	// ── Default line parser (no-op raw) ──────────────────────────────────────

	const parse = $derived(parseLine ?? ((l: string): ParsedLine => ({ content: l })));

	const levelTone = {
		info: 'blue', warn: 'yellow', error: 'red', debug: 'neutral', trace: 'neutral'
	} as const;
</script>

<!-- Portal via use:action — moves node to <body> so z-index always wins -->
{#if open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="lvo-backdrop"
		role="presentation"
		onclick={(e) => { if (e.target === e.currentTarget) handleClose(); }}
		onkeydown={() => {}}
	>
		<div class="lvo-panel">

			<!-- ── Header ── -->
			<div class="lvo-header">
				<div class="lvo-header-row lvo-header-top">
					<div class="lvo-title-group">
						<span class="lvo-title">{title}</span>
						{#if subtitle}<span class="lvo-subtitle">{subtitle}</span>{/if}
						{#if headerControls}{@render headerControls()}{/if}
					</div>

					<div class="lvo-controls">
						<!-- Tail selector (stream mode only) -->
						{#if !isTable && tailOptions.length > 0}
							<div class="lvo-tail-group">
								<span class="lvo-tail-label">Lines</span>
								<div class="lvo-tail-select">
									<Select
										value={String(tail)}
										options={tailOptions.map((n) => ({ value: String(n), label: String(n) }))}
										aria-label="Lines"
										onchange={(e) => changeTail(Number(e.currentTarget.value))}
									/>
								</div>
							</div>
						{/if}

						<!-- Stream controls -->
						{#if !isTable && streamUrl}
							<div class="lvo-stream-ctrl">
								{#if streamStatus === 'connected'}
									<span class="lvo-live-dot"></span>
									<span class="lvo-live-label">Live</span>
									<Button variant="secondary" size="sm" onclick={disconnectStream}>
										<Square size={10} /> Stop
									</Button>
								{:else if streamStatus === 'connecting'}
									<span class="lvo-status-dim">Connecting…</span>
								{:else if streamStatus === 'error'}
									<span class="lvo-status-err">{streamError}</span>
									<Button variant="secondary" size="sm" onclick={connectStream}>
										<Play size={10} /> Retry
									</Button>
								{:else}
									<Button variant="primary" size="sm" onclick={connectStream}>
										<Play size={10} /> Connect
									</Button>
								{/if}
							</div>
						{/if}

						<!-- Table refresh -->
						{#if isTable}
							<Button
								variant="secondary"
								size="icon"
								onclick={() => doFetch()}
								disabled={tableLoading}
								title="Refresh"
								aria-label="Refresh"
							>
								<RefreshCw size={13} />
							</Button>
						{/if}

						<Button variant="ghost" size="icon" onclick={handleClose} title="Close" aria-label="Close">
							<X size={15} />
						</Button>
					</div>
				</div>

				<!-- Search bar -->
				<div class="lvo-search-row">
					<div class="lvo-search-field"><SearchInput bind:value={search} placeholder="Search…" /></div>
					{#if search}
						<span class="lvo-search-count">
							{isTable ? filteredRows.length : filteredLines.length} match{(isTable ? filteredRows.length : filteredLines.length) !== 1 ? 'es' : ''}
						</span>
						<Button variant="ghost" size="icon" onclick={() => search = ''} title="Clear search" aria-label="Clear search">
							<X size={13} />
						</Button>
					{/if}
				</div>
			</div>

			<!-- ── Content ── -->
			<div class="lvo-body">

				<!-- Stream / terminal mode -->
				{#if !isTable}
					{#if fetching}
						<div class="lvo-loading"><Spinner size={16} /> Loading…</div>
					{:else if fetchError}
						<div class="lvo-error">{fetchError}</div>
					{:else}
						<div class="lvo-console" bind:this={consoleEl}>
							{#if filteredLines.length === 0}
								<div class="lvo-empty">
									{#if search}
										No lines match "{search}"
									{:else if streamUrl}
										{streamStatus === 'idle' ? 'Click Connect to stream live logs.' : 'No output yet…'}
									{:else}
										{emptyMessage}
									{/if}
								</div>
							{:else}
								<!-- Historical batch (shown without stream divider if no live lines) -->
								{#each filteredLines.slice(0, initLines.length) as line, i (i)}
									{@const p = parse(line)}
									{#if parseLine}
										<div class="lvo-line lvo-lvl-{p.level ?? 'info'}">
											{#if p.ts}<span class="lvo-ts">{p.ts}</span>{/if}
											<span class="lvo-badge"><Badge tone={levelTone[p.level ?? 'info']}>{(p.level ?? 'info').toUpperCase()}</Badge></span>
											<span class="lvo-msg">{p.content || line}</span>
										</div>
									{:else}
										<div class="lvo-line">{line}</div>
									{/if}
								{/each}

								<!-- Live stream divider + lines -->
								{#if liveLines.length > 0}
									<div class="lvo-stream-divider">── live ──</div>
									{#each filteredLines.slice(initLines.length) as line, i (i)}
										{@const p = parse(line)}
										{#if parseLine}
											<div class="lvo-line lvo-live lvo-lvl-{p.level ?? 'info'}">
												{#if p.ts}<span class="lvo-ts">{p.ts}</span>{/if}
												<span class="lvo-badge"><Badge tone={levelTone[p.level ?? 'info']}>{(p.level ?? 'info').toUpperCase()}</Badge></span>
												<span class="lvo-msg">{p.content || line}</span>
											</div>
										{:else}
											<div class="lvo-line lvo-live">{line}</div>
										{/if}
									{/each}
								{/if}
							{/if}
						</div>
					{/if}
				{/if}

				<!-- Table mode -->
				{#if isTable}
					{#if tableLoading}
						<div class="lvo-loading"><Spinner size={16} /> Loading…</div>
					{:else if tableError}
						<div class="lvo-error">{tableError}</div>
					{:else if filteredRows.length === 0}
						<div class="lvo-empty">{emptyMessage}</div>
					{:else}
						<div class="lvo-table-wrap">
							<table class="lvo-table">
								<thead>
									<tr>
										{#each columns ?? [] as col}
											<th style={col.width ? `width:${col.width}` : ''}>{col.label}</th>
										{/each}
									</tr>
								</thead>
								<tbody>
									{#each filteredRows as row (row.id ?? JSON.stringify(row))}
										<tr>
											{#each columns ?? [] as col}
												<td
													class={col.mono ? 'mono' : ''}
													style={col.color ? `color:${col.color(row)}` : ''}
												>
													{col.format ? col.format(row[col.key], row) : (row[col.key] ?? '')}
												</td>
											{/each}
										</tr>
									{/each}
								</tbody>
							</table>
						</div>
					{/if}
				{/if}

			</div>
		</div>
	</div>
{/if}

<style>
	/* ── Backdrop ── */
	.lvo-backdrop {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.65);
		display: flex; align-items: flex-end; justify-content: center;
		z-index: 500;
		padding: 0;
	}

	/* ── Panel ── */
	.lvo-panel {
		width: 100%; max-width: 1100px;
		height: 68vh; min-height: 400px;
		display: flex; flex-direction: column;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-bottom: none;
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
		overflow: hidden;
		box-shadow: var(--shadow-lg);
	}

	/* ── Header ── */
	.lvo-header {
		display: flex; flex-direction: column;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.lvo-header-row {
		display: flex; align-items: center;
		padding: 10px 14px; gap: 10px;
	}

	.lvo-title-group {
		display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0;
	}

	.lvo-title {
		font-size: 13px; font-weight: 700; color: var(--text-primary);
		white-space: nowrap;
	}

	.lvo-subtitle {
		font-size: 11px; color: var(--text-muted);
		font-family: var(--font-mono);
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.lvo-controls {
		display: flex; align-items: center; gap: 8px; flex-shrink: 0;
	}

	/* Tail selector */
	.lvo-tail-group { display: flex; align-items: center; gap: 6px; }
	.lvo-tail-label { font-size: 11px; color: var(--text-muted); }
	.lvo-tail-select { width: 84px; }
	.lvo-tail-select :global(.ui-select) { height: 30px; font-size: 12px; }

	/* Stream controls */
	.lvo-stream-ctrl {
		display: flex; align-items: center; gap: 6px;
	}

	.lvo-live-dot {
		width: 7px; height: 7px; border-radius: 50%;
		background: var(--accent-green);
		box-shadow: 0 0 6px color-mix(in srgb, var(--accent-green) 55%, transparent);
		animation: pulse 2s ease-in-out infinite;
	}

	.lvo-live-label { font-size: 11px; color: var(--accent-green); font-weight: 600; }
	.lvo-status-dim { font-size: 11px; color: var(--text-muted); }
	.lvo-status-err { font-size: 11px; color: var(--accent-red); max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	/* Search bar */
	.lvo-search-row {
		display: flex; align-items: center; gap: 8px;
		padding: 0 14px 9px;
	}
	.lvo-search-field { flex: 1; min-width: 0; }
	.lvo-search-field :global(.ui-search-input) { font-family: var(--font-mono); }

	.lvo-search-count { font-size: 10px; color: var(--text-muted); white-space: nowrap; }

	/* ── Body ── */
	.lvo-body {
		flex: 1; min-height: 0; display: flex; flex-direction: column;
		overflow: hidden;
	}

	/* Loading / error / empty */
	.lvo-loading {
		display: flex; align-items: center; gap: 10px;
		padding: 32px; color: var(--text-muted); font-size: 13px;
	}

	.lvo-error {
		margin: 16px; padding: 10px 12px;
		font-size: 12px; color: var(--accent-red);
		background: var(--accent-red-muted);
		border: 1px solid color-mix(in srgb, var(--accent-red) 25%, transparent);
		border-radius: var(--radius-md);
	}

	.lvo-empty {
		display: flex; align-items: center; justify-content: center;
		flex: 1; min-height: 200px;
		font-size: 13px; color: var(--text-dim);
		font-family: var(--font-mono);
	}

	/* ── Terminal / stream console ── */
	.lvo-console {
		flex: 1; overflow-y: auto; padding: 10px 0;
		scrollbar-width: thin; scrollbar-color: var(--border) transparent;
	}
	.lvo-console::-webkit-scrollbar { width: 5px; }
	.lvo-console::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }

	.lvo-line {
		font-family: var(--font-mono); font-size: 12px;
		line-height: 1.55; color: var(--text-secondary);
		padding: 1px 14px;
		white-space: pre-wrap; word-break: break-all;
	}

	.lvo-line:hover { background: var(--bg-hover); }

	/* Parsed lines (with level badges) */
	.lvo-ts {
		color: var(--text-muted); margin-right: 6px; user-select: none;
	}

	.lvo-badge { display: inline-block; margin-right: 6px; user-select: none; }

	/* Row tint — class is built from p.level: error | warn (info/debug/trace untinted) */
	.lvo-lvl-error { background: var(--accent-red-muted); }
	.lvo-lvl-warn  { background: var(--accent-yellow-muted); }

	.lvo-msg { color: var(--text-secondary); }

	.lvo-live { opacity: 0.93; }

	.lvo-stream-divider {
		text-align: center; font-size: 10px; color: var(--text-dim);
		padding: 6px 0; font-family: var(--font-mono);
		letter-spacing: 0.1em;
	}

	/* ── Table mode ── */
	.lvo-table-wrap {
		flex: 1; overflow: auto;
		scrollbar-width: thin; scrollbar-color: var(--border) transparent;
	}
	.lvo-table-wrap::-webkit-scrollbar { width: 5px; height: 5px; }
	.lvo-table-wrap::-webkit-scrollbar-thumb { background: var(--border); border-radius: 3px; }

	.lvo-table {
		width: 100%; border-collapse: collapse;
		font-size: 12px; font-family: var(--font-sans);
	}

	.lvo-table thead {
		position: sticky; top: 0; z-index: 1;
		background: var(--bg-elevated);
	}

	.lvo-table th {
		padding: 8px 14px; text-align: left;
		font-size: 10px; font-weight: 700; color: var(--text-muted);
		text-transform: uppercase; letter-spacing: 0.07em;
		border-bottom: 1px solid var(--border);
		white-space: nowrap;
	}

	.lvo-table td {
		padding: 7px 14px;
		color: var(--text-secondary);
		border-bottom: 1px solid var(--border);
		vertical-align: middle;
	}

	.lvo-table tr:hover td { background: var(--bg-hover); }
	.lvo-table tr:last-child td { border-bottom: none; }

	.lvo-table td.mono { font-family: var(--font-mono); font-size: 11px; }

	@media (max-width: 639px) {
		.lvo-header-row { flex-wrap: wrap; }
		.lvo-controls { flex-wrap: wrap; }
	}

	@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }
</style>
