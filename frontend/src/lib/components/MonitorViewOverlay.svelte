<script lang="ts">
	import { onDestroy } from 'svelte';
	import { X } from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import ProgressBar from '$lib/components/ui/ProgressBar.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import { formatDistanceToNow } from 'date-fns';
	import { api } from '$lib/api/client';
	import type { Container, ContainerStats } from '$lib/api/types';

	interface Props {
		open:      boolean;
		onClose:   () => void;
		serviceId: string;
	}

	let { open, onClose, serviceId }: Props = $props();

	const HISTORY_LEN = 30;

	let containers         = $state<Container[]>([]);
	let loadingContainers  = $state(false);

	let monitorTarget    = $state<Container | null>(null);
	let statsSource: EventSource | null = null;
	let currentStats     = $state<ContainerStats | null>(null);
	let monitorLoading   = $state(false);
	let monitorError     = $state('');
	let netRxDeltaPerSec = $state(0);
	let netTxDeltaPerSec = $state(0);

	let cpuHistory      = $state<number[]>([]);
	let memHistory      = $state<number[]>([]);
	let netRxHistory    = $state<number[]>([]);
	let netTxHistory    = $state<number[]>([]);
	let blkReadHistory  = $state<number[]>([]);
	let blkWriteHistory = $state<number[]>([]);

	let runningContainers = $derived(containers.filter(c => c.status === 'running'));

	function addToHistory(hist: number[], val: number): number[] {
		const next = [...hist, val];
		return next.length > HISTORY_LEN ? next.slice(-HISTORY_LEN) : next;
	}

	function formatBytes(bytes: number, decimals = 1): string {
		if (bytes <= 0) return '0 B';
		const k = 1024;
		const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
		const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
		return `${(bytes / Math.pow(k, i)).toFixed(decimals)} ${sizes[i]}`;
	}

	function sparklinePaths(data: number[]): { line: string; area: string } {
		if (data.length < 2) return { line: '', area: '' };
		const max = Math.max(...data, 0.001);
		const W = 200, H = 50, PAD = 4;
		const pts = data.map((v, i) => ({
			x: (i / (data.length - 1)) * W,
			y: (H - PAD) - (v / max) * (H - PAD * 2) + PAD
		}));
		const line = pts.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ');
		const area = `${line} L${W},${H} L0,${H} Z`;
		return { line, area };
	}

	function formatTime(ts: string | null | undefined): string {
		if (!ts) return '–';
		try { return formatDistanceToNow(new Date(ts), { addSuffix: true }); }
		catch { return ts; }
	}

	function disconnectStats() {
		statsSource?.close();
		statsSource = null;
	}

	function connectStats(c: Container) {
		disconnectStats();
		monitorLoading = true;
		monitorError   = '';

		const cid = c.docker_container_id;
		const es  = new EventSource(`/api/services/${serviceId}/containers/${cid}/stats`);
		statsSource = es;

		es.onmessage = (e) => {
			if (!e.data?.trim()) return;
			let stats: ContainerStats;
			try { stats = JSON.parse(e.data); } catch { return; }

			const prev = currentStats;
			const rxDelta = prev ? Math.max(0, stats.net_rx_bytes - prev.net_rx_bytes) : 0;
			const txDelta = prev ? Math.max(0, stats.net_tx_bytes - prev.net_tx_bytes) : 0;

			cpuHistory      = addToHistory(cpuHistory,      stats.cpu_percent);
			memHistory      = addToHistory(memHistory,      stats.memory_percent);
			netRxHistory    = addToHistory(netRxHistory,    rxDelta);
			netTxHistory    = addToHistory(netTxHistory,    txDelta);
			blkReadHistory  = addToHistory(blkReadHistory,  stats.block_read_bytes);
			blkWriteHistory = addToHistory(blkWriteHistory, stats.block_write_bytes);

			netRxDeltaPerSec = rxDelta;
			netTxDeltaPerSec = txDelta;
			currentStats     = stats;
			monitorLoading   = false;
			monitorError     = '';
		};

		es.addEventListener('error', (e: MessageEvent) => {
			monitorError   = (e as any).data ?? 'Stats stream error';
			monitorLoading = false;
		});

		es.addEventListener('remote', (e: MessageEvent) => {
			monitorError   = (e as any).data ?? 'Container is on a remote Swarm node — live stats unavailable';
			monitorLoading = false;
			es.close();
			statsSource = null;
		});

		es.onerror = () => {
			if (monitorLoading) {
				monitorError   = 'Could not connect to stats stream';
				monitorLoading = false;
				es.close();
				statsSource = null;
			}
		};
	}

	function resetMetrics() {
		currentStats     = null;
		cpuHistory       = [];
		memHistory       = [];
		netRxHistory     = [];
		netTxHistory     = [];
		blkReadHistory   = [];
		blkWriteHistory  = [];
		netRxDeltaPerSec = 0;
		netTxDeltaPerSec = 0;
		monitorError     = '';
	}

	function selectMonitorTarget(c: Container) {
		monitorTarget = c;
		resetMetrics();
		connectStats(c);
	}

	async function loadAndConnect() {
		loadingContainers = true;
		monitorTarget = null;
		containers = [];
		resetMetrics();
		try {
			const res = await api.getServiceContainers(serviceId);
			if (res.data) {
				containers = res.data;
				const first = res.data.find(c => c.status === 'running');
				if (first) selectMonitorTarget(first);
			}
		} finally {
			loadingContainers = false;
		}
	}

	$effect(() => {
		if (open) {
			void loadAndConnect();
		} else {
			disconnectStats();
		}
	});

	onDestroy(() => disconnectStats());
</script>

{#if open}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div
		class="mvo-backdrop"
		role="presentation"
		onclick={(e) => { if (e.target === e.currentTarget) onClose(); }}
		onkeydown={() => {}}
	>
		<div class="mvo-panel">

			<!-- Header -->
			<div class="mvo-header">
				<div class="mvo-title-group">
					<span class="mvo-title">Container Monitor</span>
					{#if runningContainers.length > 1}
						<div class="mvo-replica-group">
							{#each runningContainers as c (c.id)}
								<Button
									variant={monitorTarget?.id === c.id ? 'primary' : 'ghost'}
									size="sm"
									onclick={() => selectMonitorTarget(c)}
								>
									replica-{c.replica_index ?? '?'}
								</Button>
							{/each}
						</div>
					{/if}
				</div>
				<div class="mvo-controls">
					<Button variant="ghost" size="icon" onclick={onClose} title="Close" aria-label="Close"><X size={15} /></Button>
				</div>
			</div>

			<!-- Body -->
			<div class="mvo-body">
				{#if loadingContainers}
					<div class="mvo-loading"><Spinner size={16} /> Loading containers…</div>

				{:else if runningContainers.length === 0}
					<div class="mvo-empty">No running replicas to monitor.</div>

				{:else if monitorError}
					<div class="mvo-error">{monitorError}</div>

				{:else if monitorLoading && !currentStats}
					<div class="mvo-loading"><Spinner size={16} /> Fetching metrics…</div>

				{:else}
					<!-- 2×2 metric grid -->
					<div class="metric-grid">

						<!-- CPU -->
						<div class="metric-card">
							<div class="metric-header">
								<span class="metric-label">CPU</span>
								<span class="metric-value cpu">{currentStats ? `${currentStats.cpu_percent.toFixed(1)}%` : '—'}</span>
							</div>
							<ProgressBar value={currentStats?.cpu_percent ?? 0} tone="blue" />
							{#each [sparklinePaths(cpuHistory)] as cpu}
								<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
									{#if cpu.line}
										<path d={cpu.area} style="fill:var(--accent);fill-opacity:0.15" />
										<path d={cpu.line} style="stroke:var(--accent)" fill="none" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								</svg>
							{/each}
							<div class="metric-sub">
								{cpuHistory.length > 1
									? `avg ${(cpuHistory.reduce((a, b) => a + b, 0) / cpuHistory.length).toFixed(1)}%`
									: 'collecting…'}
							</div>
						</div>

						<!-- Memory -->
						<div class="metric-card">
							<div class="metric-header">
								<span class="metric-label">Memory</span>
								<span class="metric-value mem">{currentStats ? `${currentStats.memory_percent.toFixed(1)}%` : '—'}</span>
							</div>
							<ProgressBar value={currentStats?.memory_percent ?? 0} tone="green" />
							{#each [sparklinePaths(memHistory)] as mem}
								<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
									{#if mem.line}
										<path d={mem.area} style="fill:var(--accent-green);fill-opacity:0.15" />
										<path d={mem.line} style="stroke:var(--accent-green)" fill="none" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								</svg>
							{/each}
							<div class="metric-sub">
								{currentStats
									? `${formatBytes(currentStats.memory_usage_bytes)} / ${formatBytes(currentStats.memory_limit_bytes)}`
									: 'collecting…'}
							</div>
						</div>

						<!-- Network I/O -->
						<div class="metric-card">
							<div class="metric-header">
								<span class="metric-label">Network I/O</span>
							</div>
							<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
								{#each [sparklinePaths(netRxHistory)] as netRx}
									{#if netRx.line}
										<path d={netRx.area} style="fill:var(--accent);fill-opacity:0.12" />
										<path d={netRx.line} style="stroke:var(--accent)" fill="none" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								{/each}
								{#each [sparklinePaths(netTxHistory)] as netTx}
									{#if netTx.line}
										<path d={netTx.area} style="fill:var(--accent-green);fill-opacity:0.10" />
										<path d={netTx.line} style="stroke:var(--accent-green)" fill="none" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								{/each}
							</svg>
							<div class="metric-net-row">
								<Badge tone="blue">↓ {formatBytes(netRxDeltaPerSec)}/s</Badge>
								<Badge tone="green">↑ {formatBytes(netTxDeltaPerSec)}/s</Badge>
							</div>
						</div>

						<!-- Block I/O -->
						<div class="metric-card">
							<div class="metric-header">
								<span class="metric-label">Block I/O</span>
							</div>
							<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
								{#each [sparklinePaths(blkReadHistory)] as blkR}
									{#if blkR.line}
										<path d={blkR.area} style="fill:var(--accent-yellow);fill-opacity:0.12" />
										<path d={blkR.line} style="stroke:var(--accent-yellow)" fill="none" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								{/each}
								{#each [sparklinePaths(blkWriteHistory)] as blkW}
									{#if blkW.line}
										<path d={blkW.area} style="fill:var(--accent-red);fill-opacity:0.10" />
										<path d={blkW.line} style="stroke:var(--accent-red)" fill="none" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								{/each}
							</svg>
							<div class="metric-net-row">
								<Badge tone="yellow">R {formatBytes(currentStats?.block_read_bytes ?? 0)}</Badge>
								<Badge tone="red">W {formatBytes(currentStats?.block_write_bytes ?? 0)}</Badge>
							</div>
						</div>
					</div>

					<!-- Footer -->
					<div class="mvo-footer">
						{#if currentStats}
							<span class="mvo-footer-pids">{currentStats.pids} PID{currentStats.pids !== 1 ? 's' : ''}</span>
							<span class="mvo-footer-ts">Updated {formatTime(currentStats.timestamp)}</span>
						{/if}
					</div>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	/* ── Backdrop — matches LogViewerOverlay ── */
	.mvo-backdrop {
		position: fixed; inset: 0;
		background: rgba(0, 0, 0, 0.65);
		display: flex; align-items: flex-end; justify-content: center;
		z-index: 500;
		padding: 0;
	}

	/* ── Panel ── */
	.mvo-panel {
		width: 100%; max-width: 1100px;
		height: 58vh; min-height: 360px;
		display: flex; flex-direction: column;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-bottom: none;
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
		overflow: hidden;
		box-shadow: var(--shadow-lg);
	}

	/* ── Header ── */
	.mvo-header {
		display: flex; align-items: center;
		padding: 10px 14px; gap: 10px;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.mvo-title-group {
		display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0;
	}

	.mvo-title {
		font-size: 13px; font-weight: 700; color: var(--text-primary);
		white-space: nowrap;
	}

	.mvo-controls {
		display: flex; align-items: center; gap: 8px; flex-shrink: 0;
	}

	.mvo-replica-group { display: flex; flex-wrap: wrap; gap: 4px; }

	/* ── Body ── */
	.mvo-body {
		flex: 1; min-height: 0;
		display: flex; flex-direction: column;
		overflow: hidden;
	}

	.mvo-loading {
		display: flex; align-items: center; gap: 10px;
		padding: 32px; color: var(--text-muted); font-size: 13px;
	}

	.mvo-empty {
		display: flex; align-items: center; justify-content: center;
		flex: 1; min-height: 200px;
		font-size: 13px; color: var(--text-dim);
		font-family: var(--font-mono);
	}

	.mvo-error {
		margin: 16px; padding: 10px 12px;
		font-size: 12px; color: var(--accent-red);
		background: var(--accent-red-muted);
		border: 1px solid color-mix(in srgb, var(--accent-red) 25%, transparent);
		border-radius: var(--radius-md);
	}

	/* ── Metric grid ── */
	.metric-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1px;
		background: var(--border);
		flex: 0 1 auto;
		min-height: 0;
		align-content: start;
		overflow-y: auto;
	}

	.metric-card {
		background: var(--bg-surface);
		padding: 14px 16px 12px;
		display: flex; flex-direction: column; gap: 8px;
	}

	.metric-header {
		display: flex; align-items: baseline;
		justify-content: space-between; gap: 6px;
	}

	.metric-label {
		font-size: 10px; font-weight: 600; color: var(--text-muted);
		text-transform: uppercase; letter-spacing: 0.07em; flex-shrink: 0;
	}

	.metric-value {
		font-size: 20px; font-weight: 700;
		font-family: var(--font-mono); line-height: 1;
	}
	.metric-value.cpu { color: var(--accent); }
	.metric-value.mem { color: var(--accent-green); }

	/* Sparkline <svg> — documented chart-geometry exception (colors via tokens) */
	.spark {
		width: 100%; height: 46px; display: block;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		overflow: visible;
	}

	.metric-sub {
		font-size: 10px; color: var(--text-muted);
		font-family: var(--font-mono);
	}

	.metric-net-row {
		display: flex; align-items: center; gap: 5px; flex-wrap: wrap;
	}

	/* ── Footer ── */
	.mvo-footer {
		display: flex; align-items: center; justify-content: space-between;
		padding: 7px 16px;
		border-top: 1px solid var(--border);
		background: var(--bg-elevated);
		flex-shrink: 0;
		margin-top: auto;
	}

	.mvo-footer-pids {
		font-size: 10px; font-weight: 600; color: var(--text-muted);
		font-family: var(--font-mono);
	}

	.mvo-footer-ts {
		font-size: 10px; color: var(--text-dim);
	}

	@media (max-width: 639px) {
		.metric-grid { grid-template-columns: 1fr; }
	}
</style>
