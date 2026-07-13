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
	import { X, RefreshCw, Play, Square, Search } from '@lucide/svelte';

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
								{#each tailOptions as n}
									<button
										class="lvo-tail-btn"
										class:active={tail === n}
										onclick={() => changeTail(n)}
									>{n}</button>
								{/each}
							</div>
						{/if}

						<!-- Stream controls -->
						{#if !isTable && streamUrl}
							<div class="lvo-stream-ctrl">
								{#if streamStatus === 'connected'}
									<span class="lvo-live-dot"></span>
									<span class="lvo-live-label">Live</span>
									<button class="lvo-ctrl-btn" onclick={disconnectStream}>
										<Square size={10} /> Stop
									</button>
								{:else if streamStatus === 'connecting'}
									<span class="lvo-status-dim">Connecting…</span>
								{:else if streamStatus === 'error'}
									<span class="lvo-status-err">{streamError}</span>
									<button class="lvo-ctrl-btn" onclick={connectStream}>
										<Play size={10} /> Retry
									</button>
								{:else}
									<button class="lvo-ctrl-btn primary" onclick={connectStream}>
										<Play size={10} /> Connect
									</button>
								{/if}
							</div>
						{/if}

						<!-- Table refresh -->
						{#if isTable}
							<button
								class="lvo-ctrl-btn"
								onclick={() => doFetch()}
								disabled={tableLoading}
								title="Refresh"
							>
								<RefreshCw size={11} />
							</button>
						{/if}

						<button class="lvo-close-btn" onclick={handleClose} title="Close">
							<X size={15} />
						</button>
					</div>
				</div>

				<!-- Search bar -->
				<div class="lvo-search-row">
					<Search size={11} class="lvo-search-icon" />
					<input
						class="lvo-search-input"
						type="text"
						placeholder="Search…"
						bind:value={search}
					/>
					{#if search}
						<span class="lvo-search-count">
							{isTable ? filteredRows.length : filteredLines.length} match{(isTable ? filteredRows.length : filteredLines.length) !== 1 ? 'es' : ''}
						</span>
						<button class="lvo-search-clear" onclick={() => search = ''}>✕</button>
					{/if}
				</div>
			</div>

			<!-- ── Content ── -->
			<div class="lvo-body">

				<!-- Stream / terminal mode -->
				{#if !isTable}
					{#if fetching}
						<div class="lvo-loading"><div class="spinner-sm"></div> Loading…</div>
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
											<span class="lvo-badge lvo-badge-{p.level ?? 'info'}">{(p.level ?? 'info').toUpperCase()}</span>
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
												<span class="lvo-badge lvo-badge-{p.level ?? 'info'}">{(p.level ?? 'info').toUpperCase()}</span>
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
						<div class="lvo-loading"><div class="spinner-sm"></div> Loading…</div>
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
		background: #0d1117;
		border: 1px solid rgba(255, 255, 255, 0.08);
		border-bottom: none;
		border-radius: 10px 10px 0 0;
		overflow: hidden;
		box-shadow: 0 -8px 40px rgba(0, 0, 0, 0.6);
	}

	/* ── Header ── */
	.lvo-header {
		display: flex; flex-direction: column;
		background: #161b22;
		border-bottom: 1px solid rgba(255, 255, 255, 0.07);
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
		font-size: 13px; font-weight: 700; color: #e6edf3;
		white-space: nowrap;
	}

	.lvo-subtitle {
		font-size: 11px; color: #8b949e;
		font-family: var(--font-mono, monospace);
		white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
	}

	.lvo-controls {
		display: flex; align-items: center; gap: 8px; flex-shrink: 0;
	}

	/* Tail selector */
	.lvo-tail-group {
		display: flex; align-items: center; gap: 3px;
		background: rgba(255, 255, 255, 0.04);
		border: 1px solid rgba(255, 255, 255, 0.08);
		border-radius: 5px;
		padding: 2px 4px;
	}

	.lvo-tail-label {
		font-size: 10px; color: #8b949e;
		padding: 0 4px;
	}

	.lvo-tail-btn {
		font-size: 10px; font-weight: 600;
		padding: 2px 7px; border-radius: 3px;
		background: none; border: none; cursor: pointer;
		color: #8b949e;
		transition: all 0.12s;
	}
	.lvo-tail-btn:hover { color: #e6edf3; background: rgba(255,255,255,0.06); }
	.lvo-tail-btn.active {
		background: rgba(99, 102, 241, 0.2);
		color: #818cf8;
	}

	/* Stream controls */
	.lvo-stream-ctrl {
		display: flex; align-items: center; gap: 6px;
	}

	.lvo-live-dot {
		width: 7px; height: 7px; border-radius: 50%;
		background: #22c55e;
		box-shadow: 0 0 6px #22c55e88;
		animation: pulse 2s ease-in-out infinite;
	}

	.lvo-live-label { font-size: 11px; color: #22c55e; font-weight: 600; }
	.lvo-status-dim { font-size: 11px; color: #8b949e; }
	.lvo-status-err { font-size: 11px; color: #f85149; max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.lvo-ctrl-btn {
		display: inline-flex; align-items: center; gap: 4px;
		font-size: 11px; font-weight: 600; font-family: inherit;
		padding: 4px 9px; border-radius: 5px;
		background: rgba(255,255,255,0.06); color: #8b949e;
		border: 1px solid rgba(255,255,255,0.1); cursor: pointer;
		transition: all 0.12s;
	}
	.lvo-ctrl-btn:hover:not(:disabled) {
		background: rgba(255,255,255,0.1); color: #e6edf3;
		border-color: rgba(255,255,255,0.18);
	}
	.lvo-ctrl-btn.primary {
		background: rgba(99,102,241,0.15); color: #818cf8;
		border-color: rgba(99,102,241,0.3);
	}
	.lvo-ctrl-btn.primary:hover:not(:disabled) {
		background: rgba(99,102,241,0.25); color: #a5b4fc;
	}
	.lvo-ctrl-btn:disabled { opacity: 0.4; cursor: not-allowed; }

	.lvo-close-btn {
		display: flex; align-items: center; justify-content: center;
		width: 28px; height: 28px; border-radius: 6px;
		background: none; border: none; cursor: pointer;
		color: #8b949e; transition: all 0.12s;
	}
	.lvo-close-btn:hover { background: rgba(255,255,255,0.08); color: #e6edf3; }

	/* Search bar */
	.lvo-search-row {
		display: flex; align-items: center; gap: 8px;
		padding: 0 14px 9px;
		position: relative;
	}

	:global(.lvo-search-icon) {
		color: #8b949e; flex-shrink: 0;
	}

	.lvo-search-input {
		flex: 1; background: rgba(255,255,255,0.04); border: 1px solid rgba(255,255,255,0.07);
		border-radius: 5px; padding: 5px 8px;
		font-size: 12px; font-family: var(--font-mono, monospace);
		color: #e6edf3; outline: none;
		transition: border-color 0.12s;
	}
	.lvo-search-input:focus { border-color: rgba(99,102,241,0.4); }
	.lvo-search-input::placeholder { color: #484f58; }

	.lvo-search-count { font-size: 10px; color: #8b949e; white-space: nowrap; }

	.lvo-search-clear {
		background: none; border: none; cursor: pointer;
		font-size: 10px; color: #8b949e; padding: 2px 4px;
		border-radius: 3px; transition: color 0.12s;
	}
	.lvo-search-clear:hover { color: #e6edf3; }

	/* ── Body ── */
	.lvo-body {
		flex: 1; min-height: 0; display: flex; flex-direction: column;
		overflow: hidden;
	}

	/* Loading / error / empty */
	.lvo-loading {
		display: flex; align-items: center; gap: 10px;
		padding: 32px; color: #8b949e; font-size: 13px;
	}

	.lvo-error {
		margin: 16px; padding: 10px 12px;
		font-size: 12px; color: #f85149;
		background: rgba(248, 81, 73, 0.08);
		border: 1px solid rgba(248, 81, 73, 0.2);
		border-radius: 6px;
	}

	.lvo-empty {
		display: flex; align-items: center; justify-content: center;
		flex: 1; min-height: 200px;
		font-size: 13px; color: #484f58;
		font-family: var(--font-mono, monospace);
	}

	/* ── Terminal / stream console ── */
	.lvo-console {
		flex: 1; overflow-y: auto; padding: 10px 0;
		scrollbar-width: thin; scrollbar-color: #30363d transparent;
	}
	.lvo-console::-webkit-scrollbar { width: 5px; }
	.lvo-console::-webkit-scrollbar-thumb { background: #30363d; border-radius: 3px; }

	.lvo-line {
		font-family: var(--font-mono, monospace); font-size: 12px;
		line-height: 1.55; color: #c9d1d9;
		padding: 1px 14px;
		white-space: pre-wrap; word-break: break-all;
	}

	.lvo-line:hover { background: rgba(255,255,255,0.025); }

	/* Parsed lines (with level badges) */
	.lvo-ts {
		color: #8b949e; margin-right: 6px; user-select: none;
	}

	.lvo-badge {
		display: inline-block;
		font-size: 9px; font-weight: 700; letter-spacing: 0.05em;
		padding: 1px 5px; border-radius: 3px; margin-right: 6px;
		user-select: none;
	}

	.lvo-badge-info  { background: rgba(56,189,248,0.15);  color: #38bdf8; }
	.lvo-badge-warn  { background: rgba(251,191,36,0.15);  color: #fbbf24; }
	.lvo-badge-error { background: rgba(248,81,73,0.15);   color: #f85149; }
	.lvo-badge-debug { background: rgba(99,102,241,0.12);  color: #818cf8; }
	.lvo-badge-trace { background: rgba(163,163,163,0.12); color: #8b949e; }

	.lvo-lvl-error { background: rgba(248,81,73,0.04); }
	.lvo-lvl-warn  { background: rgba(251,191,36,0.03); }

	.lvo-msg { color: #c9d1d9; }

	.lvo-live { opacity: 0.93; }

	.lvo-stream-divider {
		text-align: center; font-size: 10px; color: #30363d;
		padding: 6px 0; font-family: var(--font-mono, monospace);
		letter-spacing: 0.1em;
	}

	/* ── Table mode ── */
	.lvo-table-wrap {
		flex: 1; overflow: auto;
		scrollbar-width: thin; scrollbar-color: #30363d transparent;
	}
	.lvo-table-wrap::-webkit-scrollbar { width: 5px; height: 5px; }
	.lvo-table-wrap::-webkit-scrollbar-thumb { background: #30363d; border-radius: 3px; }

	.lvo-table {
		width: 100%; border-collapse: collapse;
		font-size: 12px; font-family: var(--font-sans, inherit);
	}

	.lvo-table thead {
		position: sticky; top: 0; z-index: 1;
		background: #161b22;
	}

	.lvo-table th {
		padding: 8px 14px; text-align: left;
		font-size: 10px; font-weight: 700; color: #8b949e;
		text-transform: uppercase; letter-spacing: 0.07em;
		border-bottom: 1px solid rgba(255,255,255,0.07);
		white-space: nowrap;
	}

	.lvo-table td {
		padding: 7px 14px;
		color: #c9d1d9;
		border-bottom: 1px solid rgba(255,255,255,0.04);
		vertical-align: middle;
	}

	.lvo-table tr:hover td { background: rgba(255,255,255,0.02); }
	.lvo-table tr:last-child td { border-bottom: none; }

	.lvo-table td.mono { font-family: var(--font-mono, monospace); font-size: 11px; }

	/* ── Spinner ── */
	.spinner-sm {
		width: 16px; height: 16px; flex-shrink: 0;
		border: 2px solid rgba(255,255,255,0.12);
		border-top-color: #818cf8;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	@keyframes spin { to { transform: rotate(360deg); } }
	@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }
</style>
