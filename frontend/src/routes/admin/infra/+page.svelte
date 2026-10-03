<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api/client';
	import type { SwarmNode, SwarmJoinTokens } from '$lib/api/types';
	import { Cpu, MemoryStick, ArrowRightLeft, Clock, HardDrive } from '@lucide/svelte';
	import {
		PageHeader,
		SectionLabel,
		StatCard,
		ProgressBar,
		DataTable,
		StatusDot,
		Badge,
		Card,
		Button,
		InlineAlert,
		Skeleton
	} from '$lib/components/ui';

	interface DiskInfo { mount: string; total_gb: number; used_gb: number; used_pct: number; }
	interface NetInfo  { iface: string; rx_bytes: number; tx_bytes: number; }
	interface SystemInfo {
		cpu_usage_pct:    number;
		memory_total_mb:  number;
		memory_used_mb:   number;
		memory_used_pct:  number;
		swap_total_mb:    number;
		swap_used_mb:     number;
		uptime_secs:      number;
		disks:            DiskInfo[];
		networks:         NetInfo[];
		container_stats?: Record<string, ContainerRes>;
	}

	let sysInfo    = $state<SystemInfo | null>(null);
	let sysError   = $state('');
	let sysLoading = $state(true);
	let connected  = $state(false);

	let nodes        = $state<SwarmNode[]>([]);
	let nodesLoading = $state(true);
	let nodesError   = $state('');

	interface CoreServiceStats {
		id: string;
		name: string;
		image: string;
		status: string;
		state: string;
	}
	interface ContainerRes { cpu_pct: number; mem_used_mb: number; mem_limit_mb: number; mem_pct: number; }

	let coreServices  = $state<CoreServiceStats[]>([]);
	let coreLoading   = $state(true);
	let coreResStats  = $state<Record<string, ContainerRes>>({});

	let tokens       = $state<SwarmJoinTokens | null>(null);
	let showWorker   = $state(false);
	let showManager  = $state(false);
	let copiedWorker  = $state(false);
	let copiedManager = $state(false);

	let es: EventSource | null = null;

	function openStream() {
		if (es) { es.close(); es = null; }
		sysError = '';
		connected = false;

		const source = new EventSource('/api/admin/system/stream');
		source.onopen = () => { connected = true; sysError = ''; };
		function applyFrame(raw: string) {
			try {
				const parsed = JSON.parse(raw) as SystemInfo & { container_stats?: Record<string, ContainerRes> };
				sysInfo = parsed;
				if (parsed.container_stats && Object.keys(parsed.container_stats).length > 0) {
					coreResStats = parsed.container_stats;
				}
				connected = true;
				sysLoading = false;
				sysError = '';
			} catch { /* ignore malformed frame */ }
		}
		source.onmessage = (ev) => applyFrame(ev.data);
		source.addEventListener('system', (ev) => applyFrame((ev as MessageEvent).data));
		source.onerror = () => {
			connected = false;
			if (!sysInfo) { sysError = 'Unable to connect to metrics stream — retrying…'; sysLoading = false; }
		};
		es = source;
	}

	async function loadNodes() {
		const r = await api.get<SwarmNode[]>('/admin/docker/nodes');
		if (r.data) nodes = r.data;
		else nodesError = r.error?.message ?? 'Failed';
		nodesLoading = false;
	}

	async function loadTokens() {
		const r = await api.get<SwarmJoinTokens>('/admin/docker/swarm/join-tokens');
		if (r.data) tokens = r.data;
	}

	async function copyText(text: string, which: 'worker' | 'manager') {
		await navigator.clipboard.writeText(text);
		if (which === 'worker')  { copiedWorker  = true; setTimeout(() => (copiedWorker  = false), 2000); }
		if (which === 'manager') { copiedManager = true; setTimeout(() => (copiedManager = false), 2000); }
	}

	function fmtUptime(s: number): string {
		const d = Math.floor(s / 86400);
		const h = Math.floor((s % 86400) / 3600);
		const m = Math.floor((s % 3600) / 60);
		return d > 0 ? `${d}d ${h}h` : h > 0 ? `${h}h ${m}m` : `${m}m`;
	}
	function fmtBytes(b: number): string {
		if (b < 1024) return `${b} B`;
		if (b < 1048576) return `${(b/1024).toFixed(1)} KB`;
		if (b < 1073741824) return `${(b/1048576).toFixed(1)} MB`;
		return `${(b/1073741824).toFixed(2)} GB`;
	}
	// Same thresholds as the old hand-rolled barColor() (danger >85, warn >65,
	// else ok) — now expressed as ProgressBar's tone enum instead of a CSS color.
	function barTone(pct: number): 'red' | 'yellow' | 'green' {
		if (pct > 85) return 'red';
		if (pct > 65) return 'yellow';
		return 'green';
	}
	// Same three-way state semantics as the old nodeStateColor() (ready=ok,
	// down/disconnected=danger, else=muted) — now expressed as StatusDot's
	// status enum. StatusDot has no dedicated "unknown/muted" status; "stopped"
	// (static --text-dim gray) is the closer match to the old muted-gray
	// fallback than "pending" would be — pending's pulse implies an active
	// transition, which "unknown" explicitly is not (same pulse-vs-static
	// reasoning the nodes page already applies to its own degraded→failed
	// mapping).
	function nodeStatusDot(state: string): 'running' | 'stopped' | 'pending' {
		if (state === 'ready') return 'running';
		if (state === 'down' || state === 'disconnected') return 'stopped';
		return 'stopped';
	}

	interface NodeRow {
		id: string;
		hostname: string;
		ip: string;
		role: 'manager' | 'worker';
		state: string;
		availability: string;
		engine: string;
	}
	// Defensive field extraction preserved exactly from the old template —
	// `/admin/docker/nodes` returns the raw Docker Swarm node shape (nested
	// spec/status/description), not the flat `SwarmNode` type this file
	// imports for typing convenience, hence the `as any` casts and fallbacks.
	let nodeRows = $derived<NodeRow[]>(
		nodes.map((n) => {
			const node = n as any;
			return {
				id: node.id ?? '—',
				hostname: node.description?.hostname ?? node.hostname ?? '—',
				ip: node.addr ?? node.status?.addr ?? '—',
				role: (node.spec?.role === 'manager' || node.role === 'manager') ? 'manager' : 'worker',
				state: node.status?.state ?? node.state ?? 'unknown',
				availability: node.spec?.availability ?? node.availability ?? '—',
				engine: node.description?.engine?.engine_version ?? '—'
			};
		})
	);

	async function loadCoreServices() {
		const r = await api.get<CoreServiceStats[]>('/admin/infra/core-services');
		if (r.data) coreServices = Array.isArray(r.data) ? r.data : (r.data as any).items ?? [];
		coreLoading = false;
	}
	let coreTableItems = $derived(
		coreServices.map((svc) => ({ ...svc, name: svc.name.replace(/^\//, '') }))
	);

	onMount(() => { openStream(); loadNodes(); loadTokens(); loadCoreServices(); });
	onDestroy(() => { if (es) { es.close(); es = null; } });
</script>

<div class="page-wrap">
	<PageHeader title="System" subtitle="Live platform metrics, swarm nodes, and join tokens.">
		{#snippet actions()}
			<div class="hdr-status">
				<span class="conn-dot" class:conn-ok={connected} class:conn-err={!connected && !sysLoading} title={connected ? 'Streaming' : 'Connecting…'}></span>
				<span class="conn-label">{connected ? 'Live' : sysLoading ? 'Connecting…' : 'Disconnected'}</span>
			</div>
		{/snippet}
	</PageHeader>

	<!-- Core Services — shown first, above disk/system stats -->
	<div class="sec-head">
		<SectionLabel>Core Services</SectionLabel>
		{#if !coreLoading}<Badge tone="neutral">{coreServices.length}</Badge>{/if}
	</div>
	{#if coreLoading}
		<div class="skel-stack"><Skeleton variant="row" /><Skeleton variant="row" /><Skeleton variant="row" /></div>
	{:else}
		<DataTable
			items={coreTableItems}
			rowKey={(c) => c.id}
			searchFields={['name', 'image', 'status']}
			columns={[
				{ key: 'name', label: 'Service', width: '24%' },
				{ key: 'state', label: 'State', width: '14%' },
				{ key: 'cpu', label: 'CPU', width: '14%' },
				{ key: 'memory', label: 'Memory', width: '16%' },
				{ key: 'status', label: 'Status', width: '32%' }
			]}
			emptyMessage="No core service containers detected."
		>
			{#snippet row(svc)}
				{@const res = coreResStats[svc.name]}
				<tr>
					<td class="mono">{svc.name}</td>
					<td><StatusDot status={svc.state === 'running' ? 'running' : 'stopped'} /> {svc.state}</td>
					<td>{#if res}<span class="res-val">{res.cpu_pct.toFixed(1)}%</span>{:else}<span class="muted">—</span>{/if}</td>
					<td>{#if res}<span class="res-val">{res.mem_used_mb.toFixed(0)} <span class="muted">MB</span></span>{:else}<span class="muted">—</span>{/if}</td>
					<td class="muted">{svc.status}</td>
				</tr>
			{/snippet}
		</DataTable>
	{/if}

	<!-- System Metrics -->
	<div class="sec-head sec-head-top"><SectionLabel>Host Metrics</SectionLabel></div>
	{#if sysLoading}
		<div class="metrics-grid">
			{#each [0,1,2,3] as _}<Skeleton variant="card" height="112px" />{/each}
		</div>
	{:else if sysInfo}
		<div class="metrics-grid">
			<div class="metric-card">
				<StatCard tone="blue" value="{sysInfo.cpu_usage_pct.toFixed(1)}%" label="CPU Usage">
					{#snippet icon()}<Cpu size={16} />{/snippet}
				</StatCard>
				<div class="metric-foot">
					<ProgressBar value={sysInfo.cpu_usage_pct} tone={barTone(sysInfo.cpu_usage_pct)} />
					<span class="metric-sub">Updated live</span>
				</div>
			</div>
			<div class="metric-card">
				<StatCard tone="blue" value="{sysInfo.memory_used_pct.toFixed(1)}%" label="Memory">
					{#snippet icon()}<MemoryStick size={16} />{/snippet}
				</StatCard>
				<div class="metric-foot">
					<ProgressBar value={sysInfo.memory_used_pct} tone={barTone(sysInfo.memory_used_pct)} />
					<span class="metric-sub">{sysInfo.memory_used_mb.toFixed(0)} / {sysInfo.memory_total_mb.toFixed(0)} MB</span>
				</div>
			</div>
			{#if sysInfo.swap_total_mb > 0}
				{@const swapPct = sysInfo.swap_used_mb / Math.max(sysInfo.swap_total_mb, 1) * 100}
				<div class="metric-card">
					<StatCard tone="blue" value="{swapPct.toFixed(1)}%" label="Swap">
						{#snippet icon()}<ArrowRightLeft size={16} />{/snippet}
					</StatCard>
					<div class="metric-foot">
						<ProgressBar value={swapPct} tone={barTone(swapPct)} />
						<span class="metric-sub">{sysInfo.swap_used_mb} / {sysInfo.swap_total_mb} MB</span>
					</div>
				</div>
			{/if}
			<div class="metric-card">
				<StatCard tone="blue" value={fmtUptime(sysInfo.uptime_secs)} label="Uptime">
					{#snippet icon()}<Clock size={16} />{/snippet}
				</StatCard>
				<div class="metric-foot metric-foot-text">
					<span class="metric-sub">Platform running time</span>
				</div>
			</div>
			{#if sysInfo.disks[0]}
				<div class="metric-card">
					<StatCard tone="blue" value="{sysInfo.disks[0].used_pct.toFixed(1)}%" label="Disk ({sysInfo.disks[0].mount})">
						{#snippet icon()}<HardDrive size={16} />{/snippet}
					</StatCard>
					<div class="metric-foot">
						<ProgressBar value={sysInfo.disks[0].used_pct} tone={barTone(sysInfo.disks[0].used_pct)} />
						<span class="metric-sub">{sysInfo.disks[0].used_gb.toFixed(1)} / {sysInfo.disks[0].total_gb.toFixed(1)} GB</span>
					</div>
				</div>
			{/if}
		</div>

		{#if sysInfo.disks.length > 1}
			<div class="sec-head sec-head-top"><SectionLabel>All Disks</SectionLabel></div>
			<Card padding="4px 16px">
				{#each sysInfo.disks as d}
					<div class="disk-row">
						<span class="mono disk-mount">{d.mount}</span>
						<div class="disk-bar"><ProgressBar value={d.used_pct} tone={barTone(d.used_pct)} /></div>
						<span class="cell disk-pct">{d.used_pct.toFixed(1)}%</span>
						<span class="muted disk-size">{d.used_gb.toFixed(1)}/{d.total_gb.toFixed(1)} GB</span>
					</div>
				{/each}
			</Card>
		{/if}

		{#if sysInfo.networks.length > 0}
			<div class="sec-head sec-head-top"><SectionLabel>Network Interfaces</SectionLabel></div>
			<div class="net-wrap">
				<div class="net-scroll">
					<table class="net-table">
						<thead>
							<tr><th>Interface</th><th>RX</th><th>TX</th></tr>
						</thead>
						<tbody>
							{#each sysInfo.networks as n}
								<tr>
									<td class="mono">{n.iface}</td>
									<td class="cell">{fmtBytes(n.rx_bytes)}</td>
									<td class="cell">{fmtBytes(n.tx_bytes)}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</div>
		{/if}
	{:else if sysError}
		<InlineAlert tone="error">{sysError}</InlineAlert>
	{/if}

	<!-- Swarm Nodes -->
	<div class="sec-head sec-head-top">
		<SectionLabel>Swarm Nodes</SectionLabel>
		<Badge tone="neutral">{nodes.length}</Badge>
	</div>
	{#if nodesLoading}
		<div class="skel-stack"><Skeleton variant="row" /><Skeleton variant="row" /></div>
	{:else if nodesError}
		<InlineAlert tone="error">{nodesError}</InlineAlert>
	{:else}
		<DataTable
			items={nodeRows}
			rowKey={(n) => n.id}
			searchFields={['hostname', 'id', 'ip']}
			columns={[
				{ key: 'id', label: 'Node ID', width: '14%' },
				{ key: 'hostname', label: 'Hostname', width: '20%' },
				{ key: 'ip', label: 'IP', width: '14%' },
				{ key: 'role', label: 'Role', width: '10%' },
				{ key: 'state', label: 'State', width: '14%' },
				{ key: 'availability', label: 'Availability', width: '14%' },
				{ key: 'engine', label: 'Engine', width: '14%' }
			]}
			emptyMessage="No swarm nodes found."
		>
			{#snippet row(n)}
				<tr>
					<td class="mono muted">{n.id.slice(0, 12)}</td>
					<td class="mono">{n.hostname}</td>
					<td class="mono cell">{n.ip}</td>
					<td><Badge tone={n.role === 'manager' ? 'blue' : 'neutral'}>{n.role === 'manager' ? 'Manager' : 'Worker'}</Badge></td>
					<td><StatusDot status={nodeStatusDot(n.state)} /> {n.state}</td>
					<td class="cell">{n.availability}</td>
					<td class="mono muted">{n.engine}</td>
				</tr>
			{/snippet}
		</DataTable>
	{/if}

	<!-- Join Tokens -->
	{#if tokens}
		<div class="sec-head sec-head-top"><SectionLabel>Swarm Join Tokens</SectionLabel></div>
		<Card padding="0">
			<div class="token-row">
				<div class="token-label">Worker</div>
				<div class="token-body">
					<code class="token-val">{showWorker ? tokens.worker : '••••••••••••••••••••••••••••••••'}</code>
					<div class="token-actions">
						<Button variant="secondary" size="sm" onclick={() => (showWorker = !showWorker)}>{showWorker ? 'Hide' : 'Show'}</Button>
						<Button variant="secondary" size="sm" onclick={() => copyText(tokens!.worker, 'worker')}>{copiedWorker ? 'Copied!' : 'Copy'}</Button>
					</div>
				</div>
			</div>
			<div class="token-divider"></div>
			<div class="token-row">
				<div class="token-label">Manager</div>
				<div class="token-body">
					<code class="token-val">{showManager ? tokens.manager : '••••••••••••••••••••••••••••••••'}</code>
					<div class="token-actions">
						<Button variant="secondary" size="sm" onclick={() => (showManager = !showManager)}>{showManager ? 'Hide' : 'Show'}</Button>
						<Button variant="secondary" size="sm" onclick={() => copyText(tokens!.manager, 'manager')}>{copiedManager ? 'Copied!' : 'Copy'}</Button>
					</div>
				</div>
			</div>
		</Card>
	{/if}
</div>

<style>
	.page-wrap { max-width: 980px; margin: 0 auto; padding: 40px 36px; }

	.hdr-status { display: flex; align-items: center; gap: 6px; }
	.conn-dot { display: inline-block; width: 7px; height: 7px; border-radius: 50%; background: var(--text-dim); flex-shrink: 0; }
	.conn-dot.conn-ok { background: var(--accent-green); box-shadow: 0 0 0 2px var(--accent-green-muted); }
	.conn-dot.conn-err { background: var(--accent-red); }
	.conn-label { font-size: 11.5px; color: var(--text-muted); }

	.sec-head { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
	.sec-head :global(.ui-section-label) { margin-bottom: 0; }
	.sec-head-top { margin-top: 28px; }

	.skel-stack { display: flex; flex-direction: column; gap: 8px; margin-bottom: 8px; }

	.metrics-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: 12px; margin-bottom: 8px; }
	.metric-card { display: flex; flex-direction: column; }
	.metric-card :global(.ui-stat-card) { border-bottom-left-radius: 0; border-bottom-right-radius: 0; border-bottom: none; }
	.metric-foot { background: var(--bg-surface); border: 1px solid var(--border); border-top: none; border-bottom-left-radius: var(--radius-lg); border-bottom-right-radius: var(--radius-lg); padding: 9px 14px 11px; display: flex; flex-direction: column; gap: 6px; }
	.metric-foot-text { padding-top: 11px; }
	.metric-sub { font-size: 11px; color: var(--text-muted); }

	.disk-row { display: flex; align-items: center; gap: 12px; padding: 10px 0; border-bottom: 1px solid var(--border); }
	.disk-row:last-child { border-bottom: none; }
	.disk-mount { flex: 1.5; font-size: 12px; }
	.disk-bar { flex: 3; }
	.disk-pct { flex: 1; text-align: right; }
	.disk-size { flex: 1.5; text-align: right; font-size: 11.5px; }

	.net-wrap { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; margin-bottom: 8px; }
	.net-scroll { overflow-x: auto; }
	.net-table { width: 100%; border-collapse: collapse; font-size: 12.5px; }
	.net-table thead th { text-align: left; padding: 9px 16px; background: var(--bg-elevated); border-bottom: 1px solid var(--border); font-size: 10.5px; font-weight: 700; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; white-space: nowrap; }
	.net-table tbody td { padding: 10px 16px; border-bottom: 1px solid var(--border); }
	.net-table tbody tr:last-child td { border-bottom: none; }
	.net-table tbody tr:hover { background: var(--bg-hover); }

	.mono { font-family: var(--font-mono); color: var(--text-primary); }
	.muted { color: var(--text-muted); }
	.cell { color: var(--text-secondary); }
	.res-val { font-size: 12px; font-weight: 600; font-family: var(--font-mono); color: var(--text-primary); }

	.token-row { padding: 14px 16px; display: flex; flex-direction: column; gap: 8px; }
	.token-divider { height: 1px; background: var(--border); }
	.token-label { font-size: 11px; font-weight: 700; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.06em; }
	.token-body { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
	.token-val { font-size: 11.5px; font-family: var(--font-mono); color: var(--text-secondary); background: var(--bg-elevated); padding: 4px 8px; border-radius: var(--radius-sm); word-break: break-all; flex: 1; border: 1px solid var(--border); }
	.token-actions { display: flex; gap: 6px; }

	@media (max-width: 768px) {
		.page-wrap { padding: 20px 14px; }
	}
	@media (max-width: 640px) {
		.page-wrap { padding: 16px 12px; }
		.metrics-grid { grid-template-columns: 1fr 1fr; }
		.disk-row { flex-wrap: wrap; gap: 6px; }
		.token-body { flex-direction: column; align-items: flex-start; }
		.token-val { width: 100%; }
	}
</style>
