<script lang="ts">
	import { onMount } from 'svelte';
	import { Copy, ArrowDown } from '@lucide/svelte';
	import { Button, Select } from '$lib/components/ui';
	import type { LogLevel } from '$lib/api/types';

	interface LogLine {
		timestamp: string;
		level: LogLevel;
		message: string;
	}

	interface Props {
		logs: LogLine[];
		follow?: boolean;
		maxHeight?: string;
	}

	let { logs, follow = true, maxHeight = '100%' }: Props = $props();

	const TAIL_OPTIONS = [100, 200, 500, 1000, 0] as const; // 0 = all
	type TailOption = typeof TAIL_OPTIONS[number];

	let levelFilter  = $state<LogLevel | 'all'>('all');
	let tailLimit    = $state<TailOption>(200);
	let isFollowing  = $state(follow);
	let scrollContainer = $state<HTMLDivElement | null>(null);
	let userScrolledUp  = $state(false);
	let prevCount = 0;

	const LEVELS: Array<LogLevel | 'all'> = ['all', 'debug', 'info', 'warn', 'error'];

	let byLevel = $derived(
		levelFilter === 'all' ? logs : logs.filter(l => l.level === levelFilter)
	);

	let filteredLogs = $derived(
		tailLimit === 0 ? byLevel : byLevel.slice(-tailLimit)
	);

	function levelRowClass(level: string) {
		switch (level) {
			case 'error': return 'row-error';
			case 'warn':  return 'row-warn';
			case 'debug': return 'row-debug';
			default:      return '';
		}
	}

	function levelBadgeClass(level: string) {
		switch (level) {
			case 'debug': return 'lvl-debug';
			case 'info':  return 'lvl-info';
			case 'warn':  return 'lvl-warn';
			case 'error': return 'lvl-error';
			default:      return 'lvl-info';
		}
	}

	function levelMsgClass(level: string) {
		switch (level) {
			case 'debug': return 'msg-debug';
			case 'warn':  return 'msg-warn';
			case 'error': return 'msg-error';
			default:      return 'msg-info';
		}
	}

	function formatTs(ts: string): string {
		try {
			return new Date(ts).toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit' });
		} catch {
			return ts.slice(11, 19) || ts;
		}
	}

	function scrollToBottom() {
		if (scrollContainer) scrollContainer.scrollTop = scrollContainer.scrollHeight;
	}

	function handleScroll() {
		if (!scrollContainer) return;
		const { scrollTop, scrollHeight, clientHeight } = scrollContainer;
		userScrolledUp = scrollHeight - scrollTop - clientHeight > 40;
		if (!userScrolledUp) isFollowing = true;
	}

	function toggleFollow() {
		isFollowing = !isFollowing;
		if (isFollowing) { userScrolledUp = false; scrollToBottom(); }
	}

	async function copyLogs() {
		const text = filteredLogs.map(l => `[${l.timestamp}] [${l.level.toUpperCase()}] ${l.message}`).join('\n');
		await navigator.clipboard.writeText(text);
	}

	$effect(() => {
		if (filteredLogs.length !== prevCount) {
			prevCount = filteredLogs.length;
			if (isFollowing && !userScrolledUp) scrollToBottom();
		}
	});

	onMount(() => { if (isFollowing) scrollToBottom(); });
</script>

<div class="log-viewer" style="max-height: {maxHeight}">
	<div class="log-toolbar">
		<div class="toolbar-left">
			<!-- Level filter -->
			<div class="filter-group" role="group" aria-label="Filter by level">
				{#each LEVELS as level}
					<button
						type="button"
						class="filter-btn lvl-btn-{level}"
						class:active={levelFilter === level}
						aria-pressed={levelFilter === level}
						onclick={() => { levelFilter = level; }}
					>
						{level === 'all' ? 'All' : level.toUpperCase()}
					</button>
				{/each}
			</div>

			<!-- Tail / lines selector -->
			<div class="tail-group">
				<label class="tail-label" for="log-tail">Lines</label>
				<div class="tail-select">
					<Select
						id="log-tail"
						bind:value={() => String(tailLimit), (v) => { tailLimit = Number(v) as TailOption; }}
						options={TAIL_OPTIONS.map((n) => ({ value: String(n), label: n === 0 ? 'All' : String(n) }))}
					/>
				</div>
			</div>
		</div>

		<div class="toolbar-right">
			<span class="log-count">{filteredLogs.length} lines</span>
			<Button
				variant={isFollowing ? 'primary' : 'secondary'}
				size="sm"
				onclick={toggleFollow}
				title={isFollowing ? 'Unfollow' : 'Follow tail'}
			>
				{isFollowing ? 'Following' : 'Follow'}
			</Button>
			<Button variant="secondary" size="icon" onclick={copyLogs} title="Copy logs" aria-label="Copy logs">
				<Copy size={13} />
			</Button>
		</div>
	</div>

	<div
		class="log-scroller"
		bind:this={scrollContainer}
		onscroll={handleScroll}
	>
		{#if filteredLogs.length === 0}
			<div class="log-empty">No log lines to display.</div>
		{:else}
			{#each filteredLogs as line, i (i)}
				<div class="log-row {levelRowClass(line.level)}">
					<span class="log-ts">{formatTs(line.timestamp)}</span>
					<span class="log-lvl {levelBadgeClass(line.level)}">{line.level.toUpperCase()}</span>
					<span class="log-msg {levelMsgClass(line.level)}">{line.message}</span>
				</div>
			{/each}
		{/if}
	</div>

	{#if userScrolledUp && filteredLogs.length > 0}
		<div class="scroll-to-bottom">
			<Button variant="secondary" size="sm" onclick={() => { isFollowing = true; userScrolledUp = false; scrollToBottom(); }}>
				<ArrowDown size={12} />
				Jump to latest
			</Button>
		</div>
	{/if}
</div>

<style>
	.log-viewer {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
		position: relative;
		font-family: var(--font-mono);
	}

	/* ── Toolbar ── */
	.log-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 10px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		background: var(--bg-elevated);
		gap: 12px;
		flex-wrap: wrap;
	}

	.toolbar-left  { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
	.toolbar-right { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }

	.filter-group, .tail-group { display: flex; align-items: center; gap: 2px; }
	.tail-group { gap: 6px; }
	.tail-select { width: 84px; }
	.tail-select :global(.ui-select) { height: 28px; font-size: 12px; }

	.tail-label {
		font-size: 10px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
		margin-right: 2px; font-family: var(--font-sans);
	}

	.filter-btn {
		padding: 2px 7px;
		font-size: 10px; font-weight: 600;
		font-family: var(--font-mono);
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
		color: var(--text-muted);
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
		letter-spacing: 0.04em;
	}
	.filter-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
	.filter-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
	.filter-btn.active { background: var(--bg-hover); border-color: var(--border-hover); color: var(--text-primary); }

	/* Active level filter colours (class built from `lvl-btn-{level}`) */
	.filter-btn.lvl-btn-debug.active  { background: var(--bg-hover); border-color: var(--border-hover); color: var(--text-muted); }
	.filter-btn.lvl-btn-info.active   { background: var(--accent-muted); border-color: color-mix(in srgb, var(--accent) 40%, transparent); color: var(--accent); }
	.filter-btn.lvl-btn-warn.active   { background: var(--accent-yellow-muted); border-color: color-mix(in srgb, var(--accent-yellow) 40%, transparent); color: var(--accent-yellow); }
	.filter-btn.lvl-btn-error.active  { background: var(--accent-red-muted); border-color: color-mix(in srgb, var(--accent-red) 40%, transparent); color: var(--accent-red); }

	.log-count { font-size: 10px; color: var(--text-dim); font-family: var(--font-sans); }

	/* ── Log rows ── */
	.log-scroller {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}

	.log-row {
		display: flex;
		align-items: baseline;
		gap: 8px;
		padding: 1.5px 12px;
		line-height: 1.65;
		font-size: 11.5px;
		border-left: 2px solid transparent;
	}
	.log-row:hover { background: var(--bg-hover); }

	/* Row level tints (class from levelRowClass) */
	.row-error { background: var(--accent-red-muted);    border-left-color: var(--accent-red); }
	.row-error:hover { background: color-mix(in srgb, var(--accent-red) 14%, transparent); }
	.row-warn  { background: var(--accent-yellow-muted); border-left-color: var(--accent-yellow); }
	.row-warn:hover  { background: color-mix(in srgb, var(--accent-yellow) 14%, transparent); }
	.row-debug { opacity: 0.65; }

	.log-ts {
		color: var(--text-dim);
		flex-shrink: 0;
		min-width: 68px;
		font-size: 10.5px;
	}

	/* Level badge (class from levelBadgeClass) */
	.log-lvl {
		flex-shrink: 0;
		font-size: 9.5px;
		font-weight: 700;
		width: 36px;
		text-align: center;
		padding: 0px 4px;
		border-radius: var(--radius-sm);
		letter-spacing: 0.04em;
		border: 1px solid transparent;
	}
	.lvl-debug { background: var(--bg-hover);            color: var(--text-muted);    border-color: var(--border); }
	.lvl-info  { background: var(--accent-muted);        color: var(--accent);        border-color: color-mix(in srgb, var(--accent) 25%, transparent); }
	.lvl-warn  { background: var(--accent-yellow-muted); color: var(--accent-yellow); border-color: color-mix(in srgb, var(--accent-yellow) 30%, transparent); }
	.lvl-error { background: var(--accent-red-muted);    color: var(--accent-red);    border-color: color-mix(in srgb, var(--accent-red) 30%, transparent); }

	/* Message (class from levelMsgClass) */
	.log-msg {
		flex: 1;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.msg-debug { color: var(--text-dim); }
	.msg-info  { color: var(--text-secondary); }
	.msg-warn  { color: var(--accent-yellow); }
	.msg-error { color: var(--accent-red); }

	.log-empty {
		padding: 24px;
		text-align: center;
		color: var(--text-muted);
		font-size: 12px;
	}

	.scroll-to-bottom {
		position: absolute;
		bottom: 12px;
		left: 50%;
		transform: translateX(-50%);
		box-shadow: var(--shadow-md);
		border-radius: var(--radius-md);
	}
</style>
