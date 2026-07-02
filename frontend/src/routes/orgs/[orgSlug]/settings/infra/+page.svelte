<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api/client';
	import { Cpu, MemoryStick, HardDrive, Network, RefreshCw, Activity } from '@lucide/svelte';

	interface DiskInfo {
		mount: string;
		total_gb: number;
		used_gb: number;
		used_pct: number;
	}

	interface NetInfo {
		iface: string;
		rx_bytes: number;
		tx_bytes: number;
	}

	interface SystemInfo {
		cpu_usage_pct: number;
		memory_total_mb: number;
		memory_used_mb: number;
		memory_used_pct: number;
		swap_total_mb: number;
		swap_used_mb: number;
		uptime_secs: number;
		disks: DiskInfo[];
		networks: NetInfo[];
	}

	let info    = $state<SystemInfo | null>(null);
	let loading = $state(false);
	let error   = $state('');
	let intervalId: ReturnType<typeof setInterval>;

	async function load() {
		loading = true;
		error = '';
		const res = await api.get<SystemInfo>('/admin/system');
		if (res.data) info = res.data;
		else error = res.error?.message ?? 'Failed to load system info';
		loading = false;
	}

	function formatUptime(secs: number): string {
		const d = Math.floor(secs / 86400);
		const h = Math.floor((secs % 86400) / 3600);
		const m = Math.floor((secs % 3600) / 60);
		const parts = [];
		if (d) parts.push(`${d}d`);
		if (h) parts.push(`${h}h`);
		parts.push(`${m}m`);
		return parts.join(' ');
	}

	function formatBytes(b: number): string {
		if (b >= 1_099_511_627_776) return `${(b / 1_099_511_627_776).toFixed(2)} TB`;
		if (b >= 1_073_741_824)     return `${(b / 1_073_741_824).toFixed(2)} GB`;
		if (b >= 1_048_576)         return `${(b / 1_048_576).toFixed(1)} MB`;
		if (b >= 1024)              return `${(b / 1024).toFixed(1)} KB`;
		return `${b} B`;
	}

	function gaugeColor(pct: number): string {
		if (pct >= 90) return '#ef4444';
		if (pct >= 70) return '#f97316';
		return 'var(--accent)';
	}

	// System-level network interfaces to hide (not user services)
	const HIDE_IFACES = new Set(['lo', 'docker0', 'docker_gwbridge']);
	function visibleNets(nets: NetInfo[]): NetInfo[] {
		return nets.filter(n => !HIDE_IFACES.has(n.iface) && !n.iface.startsWith('br-') && !n.iface.startsWith('veth'));
	}

	onMount(() => {
		load();
		intervalId = setInterval(load, 10_000);
	});
	onDestroy(() => clearInterval(intervalId));
</script>

<div class="infra-page">

	<div class="page-toolbar">
		<div class="toolbar-left">
			<Activity size={15} />
			<span class="toolbar-title">Infrastructure</span>
			{#if info}
				<span class="uptime-chip">Up {formatUptime(info.uptime_secs)}</span>
			{/if}
		</div>
		<button class="refresh-btn" onclick={load} disabled={loading}>
			<RefreshCw size={14} class={loading ? 'spin' : ''} />
			Refresh
		</button>
	</div>

	{#if error}
		<div class="error-banner">{error}</div>
	{/if}

	{#if !info && loading}
		<div class="empty-state"><div class="spinner"></div> Loading system info…</div>
	{:else if info}

		<!-- Top stat cards -->
		<div class="stat-grid">

			<!-- CPU -->
			<div class="stat-card">
				<div class="stat-header">
					<Cpu size={15} />
					<span class="stat-label">CPU Usage</span>
				</div>
				<div class="stat-value" style="color: {gaugeColor(info.cpu_usage_pct)}">
					{info.cpu_usage_pct.toFixed(1)}%
				</div>
				<div class="gauge-track">
					<div class="gauge-fill" style="width: {Math.min(info.cpu_usage_pct, 100)}%; background: {gaugeColor(info.cpu_usage_pct)}"></div>
				</div>
			</div>

			<!-- Memory -->
			<div class="stat-card">
				<div class="stat-header">
					<MemoryStick size={15} />
					<span class="stat-label">Memory</span>
				</div>
				<div class="stat-value" style="color: {gaugeColor(info.memory_used_pct)}">
					{info.memory_used_pct.toFixed(1)}%
				</div>
				<div class="gauge-track">
					<div class="gauge-fill" style="width: {Math.min(info.memory_used_pct, 100)}%; background: {gaugeColor(info.memory_used_pct)}"></div>
				</div>
				<div class="stat-sub">
					{info.memory_used_mb.toLocaleString()} MB / {info.memory_total_mb.toLocaleString()} MB
				</div>
			</div>

			<!-- Swap -->
			{#if info.swap_total_mb > 0}
				{@const swapPct = info.swap_total_mb > 0 ? (info.swap_used_mb / info.swap_total_mb * 100) : 0}
				<div class="stat-card">
					<div class="stat-header">
						<MemoryStick size={15} />
						<span class="stat-label">Swap</span>
					</div>
					<div class="stat-value" style="color: {gaugeColor(swapPct)}">
						{swapPct.toFixed(1)}%
					</div>
					<div class="gauge-track">
						<div class="gauge-fill" style="width: {Math.min(swapPct, 100)}%; background: {gaugeColor(swapPct)}"></div>
					</div>
					<div class="stat-sub">
						{info.swap_used_mb.toLocaleString()} MB / {info.swap_total_mb.toLocaleString()} MB
					</div>
				</div>
			{/if}

		</div>

		<!-- Disk section -->
		<div class="section">
			<div class="section-head">
				<HardDrive size={14} />
				<span>Disk</span>
			</div>
			<div class="disk-list">
				{#each info.disks as disk}
					<div class="disk-row">
						<div class="disk-meta">
							<span class="disk-mount mono">{disk.mount}</span>
							<span class="disk-size">{disk.used_gb.toFixed(1)} GB / {disk.total_gb.toFixed(1)} GB</span>
						</div>
						<div class="disk-gauge-wrap">
							<div class="gauge-track disk-track">
								<div class="gauge-fill" style="width: {Math.min(disk.used_pct, 100)}%; background: {gaugeColor(disk.used_pct)}"></div>
							</div>
							<span class="disk-pct" style="color: {gaugeColor(disk.used_pct)}">{disk.used_pct.toFixed(1)}%</span>
						</div>
					</div>
				{/each}
			</div>
		</div>

		<!-- Network section -->
		<div class="section">
			<div class="section-head">
				<Network size={14} />
				<span>Network</span>
				<span class="section-hint">cumulative since boot</span>
			</div>
			<div class="table-wrap">
				<table class="data-table">
					<thead>
						<tr>
							<th>Interface</th>
							<th>Received</th>
							<th>Transmitted</th>
						</tr>
					</thead>
					<tbody>
						{#each visibleNets(info.networks) as net}
							<tr>
								<td class="mono iface">{net.iface}</td>
								<td class="rx">{formatBytes(net.rx_bytes)}</td>
								<td class="tx">{formatBytes(net.tx_bytes)}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		</div>

	{/if}
</div>

<style>
	.infra-page { display: flex; flex-direction: column; gap: 20px; }

	.page-toolbar {
		display: flex; align-items: center; justify-content: space-between;
	}
	.toolbar-left { display: flex; align-items: center; gap: 8px; color: var(--text-secondary); font-size: 13px; }
	.toolbar-title { font-weight: 600; color: var(--text-primary); }
	.uptime-chip {
		padding: 2px 8px;
		background: var(--bg-muted); border: 1px solid var(--border);
		border-radius: 10px; font-size: 11px; color: var(--text-muted);
	}

	.refresh-btn {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px; font-size: 12px; font-weight: 500;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius); color: var(--text-secondary);
		cursor: pointer; transition: all var(--transition-fast);
	}
	.refresh-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.refresh-btn:disabled { opacity: 0.5; cursor: default; }

	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.error-banner {
		padding: 10px 14px;
		background: #fef2f2; border: 1px solid #fecaca; border-radius: var(--radius);
		color: #dc2626; font-size: 13px;
	}

	.empty-state {
		display: flex; align-items: center; justify-content: center; gap: 10px;
		padding: 60px; color: var(--text-muted); font-size: 13px;
	}
	.spinner {
		width: 18px; height: 18px; border: 2px solid var(--border);
		border-top-color: var(--accent); border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	/* Stat cards */
	.stat-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
		gap: 16px;
	}
	.stat-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 18px 20px;
		display: flex; flex-direction: column; gap: 8px;
	}
	.stat-header { display: flex; align-items: center; gap: 7px; color: var(--text-muted); font-size: 12px; font-weight: 500; }
	.stat-label { font-size: 12px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; }
	.stat-value { font-size: 32px; font-weight: 700; line-height: 1; letter-spacing: -0.02em; color: var(--text-primary); }
	.stat-sub { font-size: 11px; color: var(--text-muted); }

	/* Gauge */
	.gauge-track {
		height: 6px; background: var(--bg-muted); border-radius: 3px; overflow: hidden;
	}
	.gauge-fill { height: 100%; border-radius: 3px; transition: width 0.4s ease; }

	/* Section */
	.section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.section-head {
		display: flex; align-items: center; gap: 8px;
		padding: 12px 16px;
		font-size: 12px; font-weight: 600;
		color: var(--text-secondary);
		border-bottom: 1px solid var(--border);
		background: var(--bg-muted);
		text-transform: uppercase; letter-spacing: 0.05em;
	}
	.section-hint { font-size: 11px; font-weight: 400; color: var(--text-muted); text-transform: none; letter-spacing: 0; margin-left: auto; }

	/* Disk */
	.disk-list { display: flex; flex-direction: column; }
	.disk-row {
		display: flex; align-items: center; gap: 16px;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
	}
	.disk-row:last-child { border-bottom: none; }
	.disk-meta { display: flex; flex-direction: column; gap: 2px; min-width: 160px; }
	.disk-mount { font-size: 13px; color: var(--text-primary); }
	.disk-size { font-size: 11px; color: var(--text-muted); }
	.disk-gauge-wrap { flex: 1; display: flex; align-items: center; gap: 10px; }
	.disk-track { flex: 1; }
	.disk-pct { font-size: 12px; font-weight: 600; min-width: 44px; text-align: right; }

	/* Network table */
	.table-wrap { overflow-x: auto; }
	.data-table { width: 100%; border-collapse: collapse; font-size: 13px; }
	.data-table thead th {
		padding: 10px 16px;
		text-align: left; font-size: 11px; font-weight: 600;
		text-transform: uppercase; letter-spacing: 0.04em;
		color: var(--text-muted); background: var(--bg-muted);
		border-bottom: 1px solid var(--border);
	}
	.data-table tbody tr td { padding: 10px 16px; border-bottom: 1px solid var(--border); color: var(--text-primary); }
	.data-table tbody tr:last-child td { border-bottom: none; }
	.data-table tbody tr:hover td { background: var(--bg-muted); }

	.mono { font-family: var(--font-mono, 'JetBrains Mono', monospace); font-size: 12px; }
	.iface { color: var(--text-secondary); }
	.rx { color: #16a34a; }
	.tx { color: #2563eb; }
</style>
