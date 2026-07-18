<script lang="ts">
	import { onDestroy } from 'svelte';
	import { X } from '@lucide/svelte';
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
								<button
									class={monitorTarget?.id === c.id ? 'mvo-replica-btn active' : 'mvo-replica-btn'}
									onclick={() => selectMonitorTarget(c)}
								>
									replica-{c.replica_index ?? '?'}
								</button>
							{/each}
						</div>
					{/if}
				</div>
				<div class="mvo-controls">
					<button class="mvo-close-btn" onclick={onClose} title="Close"><X size={15} /></button>
				</div>
			</div>

			<!-- Body -->
			<div class="mvo-body">
				{#if loadingContainers}
					<div class="mvo-loading"><div class="spinner-sm"></div> Loading containers…</div>

				{:else if runningContainers.length === 0}
					<div class="mvo-empty">No running replicas to monitor.</div>

				{:else if monitorError}
					<div class="mvo-error">{monitorError}</div>

				{:else if monitorLoading && !currentStats}
					<div class="mvo-loading"><div class="spinner-sm"></div> Fetching metrics…</div>

				{:else}
					<!-- 2×2 metric grid -->
					<div class="metric-grid">

						<!-- CPU -->
						<div class="metric-card">
							<div class="metric-header">
								<span class="metric-label">CPU</span>
								<span class="metric-value cpu">{currentStats ? `${currentStats.cpu_percent.toFixed(1)}%` : '—'}</span>
							</div>
							{#each [sparklinePaths(cpuHistory)] as cpu}
								<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
									{#if cpu.line}
										<path d={cpu.area} fill="rgba(59,130,246,0.15)" />
										<path d={cpu.line} fill="none" stroke="#3B82F6" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
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
							{#each [sparklinePaths(memHistory)] as mem}
								<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
									{#if mem.line}
										<path d={mem.area} fill="rgba(16,185,129,0.15)" />
										<path d={mem.line} fill="none" stroke="#10B981" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
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
										<path d={netRx.area} fill="rgba(99,102,241,0.12)" />
										<path d={netRx.line} fill="none" stroke="#6366F1" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								{/each}
								{#each [sparklinePaths(netTxHistory)] as netTx}
									{#if netTx.line}
										<path d={netTx.area} fill="rgba(244,114,182,0.10)" />
										<path d={netTx.line} fill="none" stroke="#F472B6" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								{/each}
							</svg>
							<div class="metric-net-row">
								<span class="net-chip rx">↓ {formatBytes(netRxDeltaPerSec)}/s</span>
								<span class="net-chip tx">↑ {formatBytes(netTxDeltaPerSec)}/s</span>
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
										<path d={blkR.area} fill="rgba(251,191,36,0.12)" />
										<path d={blkR.line} fill="none" stroke="#FBBF24" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								{/each}
								{#each [sparklinePaths(blkWriteHistory)] as blkW}
									{#if blkW.line}
										<path d={blkW.area} fill="rgba(249,115,22,0.10)" />
										<path d={blkW.line} fill="none" stroke="#F97316" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
									{/if}
								{/each}
							</svg>
							<div class="metric-net-row">
								<span class="net-chip blk-r">R {formatBytes(currentStats?.block_read_bytes ?? 0)}</span>
								<span class="net-chip blk-w">W {formatBytes(currentStats?.block_write_bytes ?? 0)}</span>
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
		background: #0d1117;
		border: 1px solid rgba(255, 255, 255, 0.08);
		border-bottom: none;
		border-radius: 10px 10px 0 0;
		overflow: hidden;
		box-shadow: 0 -8px 40px rgba(0, 0, 0, 0.6);
	}

	/* ── Header ── */
	.mvo-header {
		display: flex; align-items: center;
		padding: 10px 14px; gap: 10px;
		background: #161b22;
		border-bottom: 1px solid rgba(255, 255, 255, 0.07);
		flex-shrink: 0;
	}

	.mvo-title-group {
		display: flex; align-items: center; gap: 10px; flex: 1; min-width: 0;
	}

	.mvo-title {
		font-size: 13px; font-weight: 700; color: #e6edf3;
		white-space: nowrap;
	}

	.mvo-controls {
		display: flex; align-items: center; gap: 8px; flex-shrink: 0;
	}

	.mvo-close-btn {
		display: flex; align-items: center; justify-content: center;
		width: 28px; height: 28px; border-radius: 6px;
		background: none; border: none; cursor: pointer;
		color: #8b949e; transition: all 0.12s;
	}
	.mvo-close-btn:hover { background: rgba(255, 255, 255, 0.08); color: #e6edf3; }

	/* Replica selector */
	.mvo-replica-group {
		display: flex; flex-wrap: wrap; gap: 3px;
		background: rgba(255, 255, 255, 0.04);
		border: 1px solid rgba(255, 255, 255, 0.08);
		border-radius: 5px;
		padding: 2px 4px;
	}

	.mvo-replica-btn {
		font-size: 10px; font-weight: 600; font-family: var(--font-mono, monospace);
		padding: 2px 7px; border-radius: 3px;
		background: none; border: none; cursor: pointer;
		color: #8b949e; transition: all 0.12s;
	}
	.mvo-replica-btn:hover { color: #e6edf3; background: rgba(255, 255, 255, 0.06); }
	.mvo-replica-btn.active { background: rgba(99, 102, 241, 0.2); color: #818cf8; }

	/* ── Body ── */
	.mvo-body {
		flex: 1; min-height: 0;
		display: flex; flex-direction: column;
		overflow: hidden;
	}

	.mvo-loading {
		display: flex; align-items: center; gap: 10px;
		padding: 32px; color: #8b949e; font-size: 13px;
	}

	.mvo-empty {
		display: flex; align-items: center; justify-content: center;
		flex: 1; min-height: 200px;
		font-size: 13px; color: #484f58;
		font-family: var(--font-mono, monospace);
	}

	.mvo-error {
		margin: 16px; padding: 10px 12px;
		font-size: 12px; color: #f85149;
		background: rgba(248, 81, 73, 0.08);
		border: 1px solid rgba(248, 81, 73, 0.2);
		border-radius: 6px;
	}

	/* ── Metric grid ── */
	.metric-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1px;
		background: rgba(255, 255, 255, 0.06);
		flex: 1;
		align-content: start;
	}

	.metric-card {
		background: #0d1117;
		padding: 14px 16px 12px;
		display: flex; flex-direction: column; gap: 8px;
	}

	.metric-header {
		display: flex; align-items: baseline;
		justify-content: space-between; gap: 6px;
	}

	.metric-label {
		font-size: 10px; font-weight: 600; color: #8b949e;
		text-transform: uppercase; letter-spacing: 0.07em; flex-shrink: 0;
	}

	.metric-value {
		font-size: 20px; font-weight: 700;
		font-family: var(--font-mono, monospace); line-height: 1;
	}
	.metric-value.cpu { color: #3B82F6; }
	.metric-value.mem { color: #10B981; }

	.spark {
		width: 100%; height: 46px; display: block;
		border-radius: 4px;
		background: rgba(255, 255, 255, 0.03);
		overflow: visible;
	}

	.metric-sub {
		font-size: 10px; color: #8b949e;
		font-family: var(--font-mono, monospace);
	}

	.metric-net-row {
		display: flex; align-items: center; gap: 5px; flex-wrap: wrap;
	}

	.net-chip {
		font-size: 10px; font-weight: 600;
		font-family: var(--font-mono, monospace);
		padding: 2px 7px; border-radius: 99px;
	}
	.net-chip.rx    { background: rgba(99,102,241,0.15);  color: #818cf8; border: 1px solid rgba(99,102,241,0.3); }
	.net-chip.tx    { background: rgba(236,72,153,0.12);  color: #f472b6; border: 1px solid rgba(236,72,153,0.25); }
	.net-chip.blk-r { background: rgba(251,191,36,0.12);  color: #fbbf24; border: 1px solid rgba(251,191,36,0.25); }
	.net-chip.blk-w { background: rgba(249,115,22,0.12);  color: #fb923c; border: 1px solid rgba(249,115,22,0.25); }

	/* ── Footer ── */
	.mvo-footer {
		display: flex; align-items: center; justify-content: space-between;
		padding: 7px 16px;
		border-top: 1px solid rgba(255, 255, 255, 0.07);
		background: #161b22;
		flex-shrink: 0;
	}

	.mvo-footer-pids {
		font-size: 10px; font-weight: 600; color: #8b949e;
		font-family: var(--font-mono, monospace);
	}

	.mvo-footer-ts {
		font-size: 10px; color: #484f58;
	}

	@media (max-width: 639px) {
		.metric-grid { grid-template-columns: 1fr; }
	}
</style>
