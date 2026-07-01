<script lang="ts">
	import { onMount } from 'svelte';
	import { Copy, Download, Filter, ChevronDown } from '@lucide/svelte';
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
			<div class="filter-group">
				{#each LEVELS as level}
					<button
						class="filter-btn lvl-btn-{level}"
						class:active={levelFilter === level}
						onclick={() => { levelFilter = level; }}
					>
						{level === 'all' ? 'All' : level.toUpperCase()}
					</button>
				{/each}
			</div>

			<!-- Tail / lines selector -->
			<div class="tail-group">
				<span class="tail-label">Lines</span>
				{#each TAIL_OPTIONS as n}
					<button
						class="filter-btn"
						class:active={tailLimit === n}
						onclick={() => { tailLimit = n; }}
					>
						{n === 0 ? 'All' : n}
					</button>
				{/each}
			</div>
		</div>

		<div class="toolbar-right">
			<span class="log-count">{filteredLogs.length} lines</span>
			<button
				class="action-btn"
				class:follow-active={isFollowing}
				onclick={toggleFollow}
				title={isFollowing ? 'Unfollow' : 'Follow tail'}
			>
				{isFollowing ? 'Following' : 'Follow'}
			</button>
			<button class="action-btn icon-btn" onclick={copyLogs} title="Copy logs">
				<Copy size={13} />
			</button>
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
		<button class="scroll-to-bottom" onclick={() => { isFollowing = true; userScrolledUp = false; scrollToBottom(); }}>
			↓ Jump to latest
		</button>
	{/if}
</div>

<style>
	.log-viewer {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: #0B1120;
		border: 1px solid rgba(255,255,255,0.08);
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
		border-bottom: 1px solid rgba(255,255,255,0.06);
		flex-shrink: 0;
		background: #0F172A;
		gap: 12px;
		flex-wrap: wrap;
	}

	.toolbar-left  { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; }
	.toolbar-right { display: flex; align-items: center; gap: 6px; flex-shrink: 0; }

	.filter-group, .tail-group { display: flex; align-items: center; gap: 2px; }

	.tail-label {
		font-size: 10px; font-weight: 600; color: #4B5563;
		text-transform: uppercase; letter-spacing: 0.06em;
		margin-right: 4px; font-family: var(--font-sans);
	}

	.filter-btn {
		padding: 2px 7px;
		font-size: 10px; font-weight: 600;
		font-family: var(--font-mono);
		background: transparent;
		border: 1px solid transparent;
		border-radius: 3px;
		color: #4B5563;
		cursor: pointer;
		transition: all 0.12s;
		letter-spacing: 0.04em;
	}
	.filter-btn:hover { color: #9CA3AF; background: rgba(255,255,255,0.05); }
	.filter-btn.active { background: rgba(255,255,255,0.08); border-color: rgba(255,255,255,0.15); color: #E5E7EB; }

	/* Active level filter colours */
	.filter-btn.lvl-btn-debug.active  { background: rgba(107,114,128,0.15); border-color: rgba(107,114,128,0.4); color: #9CA3AF; }
	.filter-btn.lvl-btn-info.active   { background: rgba(59,130,246,0.15);  border-color: rgba(59,130,246,0.4);  color: #60A5FA; }
	.filter-btn.lvl-btn-warn.active   { background: rgba(245,158,11,0.15);  border-color: rgba(245,158,11,0.4);  color: #FBBF24; }
	.filter-btn.lvl-btn-error.active  { background: rgba(239,68,68,0.15);   border-color: rgba(239,68,68,0.4);   color: #F87171; }

	.log-count { font-size: 10px; color: #374151; font-family: var(--font-sans); }

	.action-btn {
		padding: 3px 8px;
		font-size: 11px; font-weight: 500; font-family: var(--font-sans);
		background: rgba(255,255,255,0.04);
		border: 1px solid rgba(255,255,255,0.08);
		border-radius: var(--radius-sm);
		color: #6B7280;
		cursor: pointer;
		transition: all 0.12s;
		display: flex; align-items: center; gap: 4px;
	}
	.action-btn:hover { color: #D1D5DB; border-color: rgba(255,255,255,0.2); }
	.action-btn.follow-active { background: rgba(34,197,94,0.12); border-color: rgba(34,197,94,0.35); color: #4ADE80; }
	.icon-btn { padding: 4px 6px; }

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
	.log-row:hover { background: rgba(255,255,255,0.03); }

	/* Row level tints */
	.row-error { background: rgba(239,68,68,0.05);  border-left-color: #7F1D1D; }
	.row-error:hover { background: rgba(239,68,68,0.08); }
	.row-warn  { background: rgba(245,158,11,0.04); border-left-color: #78350F; }
	.row-warn:hover  { background: rgba(245,158,11,0.08); }
	.row-debug { opacity: 0.65; }

	.log-ts {
		color: #374151;
		flex-shrink: 0;
		min-width: 68px;
		font-size: 10.5px;
	}

	/* Level badge */
	.log-lvl {
		flex-shrink: 0;
		font-size: 9.5px;
		font-weight: 700;
		width: 36px;
		text-align: center;
		padding: 0px 4px;
		border-radius: 3px;
		letter-spacing: 0.04em;
		border: 1px solid transparent;
	}
	.lvl-debug { background: rgba(107,114,128,0.12); color: #6B7280;  border-color: rgba(107,114,128,0.25); }
	.lvl-info  { background: rgba(59,130,246,0.12);  color: #60A5FA;  border-color: rgba(59,130,246,0.25); }
	.lvl-warn  { background: rgba(245,158,11,0.15);  color: #FBBF24;  border-color: rgba(245,158,11,0.3); }
	.lvl-error { background: rgba(239,68,68,0.15);   color: #F87171;  border-color: rgba(239,68,68,0.3); }

	/* Message */
	.log-msg {
		flex: 1;
		white-space: pre-wrap;
		word-break: break-all;
	}
	.msg-debug { color: #4B5563; }
	.msg-info  { color: #9CA3AF; }
	.msg-warn  { color: #FBBF24; }
	.msg-error { color: #FCA5A5; }

	.log-empty {
		padding: 24px;
		text-align: center;
		color: #374151;
		font-size: 12px;
	}

	.scroll-to-bottom {
		position: absolute;
		bottom: 12px;
		left: 50%;
		transform: translateX(-50%);
		padding: 5px 14px;
		font-size: 11px; font-weight: 500; font-family: var(--font-sans);
		background: #1E293B;
		border: 1px solid rgba(255,255,255,0.12);
		border-radius: 100px;
		color: #9CA3AF;
		cursor: pointer;
		transition: all 0.15s;
		box-shadow: 0 4px 12px rgba(0,0,0,0.4);
	}
	.scroll-to-bottom:hover { background: var(--accent); border-color: var(--accent); color: white; }
</style>
