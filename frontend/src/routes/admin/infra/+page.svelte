<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api/client';
	import type { SwarmNode, SwarmJoinTokens } from '$lib/api/types';

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
	function barColor(pct: number): string {
		if (pct > 85) return 'var(--danger)';
		if (pct > 65) return 'var(--warn)';
		return 'var(--ok)';
	}
	function nodeStateColor(state: string): string {
		if (state === 'ready') return 'var(--ok)';
		if (state === 'down' || state === 'disconnected') return 'var(--danger)';
		return 'var(--text-3)';
	}

	let nodePage = $state(0);
	const PAGE = 20;
	let pagedNodes = $derived(nodes.slice(nodePage * PAGE, (nodePage + 1) * PAGE));
	let totalNodePages = $derived(Math.ceil(nodes.length / PAGE));

	async function loadCoreServices() {
		const r = await api.get<CoreServiceStats[]>('/admin/infra/core-services');
		if (r.data) coreServices = Array.isArray(r.data) ? r.data : (r.data as any).items ?? [];
		coreLoading = false;
	}

	onMount(() => { openStream(); loadNodes(); loadTokens(); loadCoreServices(); });
	onDestroy(() => { if (es) { es.close(); es = null; } });
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">System</h1>
			<p class="sub">Live platform metrics, swarm nodes, and join tokens.</p>
		</div>
		<div class="hdr-right">
			<span class="conn-dot" class:conn-ok={connected} class:conn-err={!connected && !sysLoading} title={connected ? 'Streaming' : 'Connecting…'}></span>
			<span class="conn-label">{connected ? 'Live' : sysLoading ? 'Connecting…' : 'Disconnected'}</span>
		</div>
	</header>

	<!-- Core Services — shown first, above disk/system stats -->
	<div class="section-title" style="margin-bottom:10px">
		Core Services
		{#if !coreLoading}<span class="count-pill">{coreServices.length}</span>{/if}
	</div>
	{#if coreLoading}
		<div class="tbl" style="margin-bottom:24px">{#each [0,1,2] as _}<div class="sk-row"><div class="sk" style="width:120px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div></div>{/each}</div>
	{:else if coreServices.length === 0}
		<div class="tbl" style="margin-bottom:24px">
			<div class="trow" style="justify-content:center;color:var(--text-3);font-size:12px;padding:16px">No core service containers detected.</div>
		</div>
	{:else}
		<div class="tbl" style="margin-bottom:24px">
			<div class="thead">
				<span style="flex:2">Service</span>
				<span style="flex:1">State</span>
				<span style="flex:1.2">CPU</span>
				<span style="flex:1.2">Memory</span>
				<span style="flex:2">Status</span>
			</div>
			{#each coreServices as svc}
				{@const svcName = svc.name.replace(/^\//, '')}
				{@const res = coreResStats[svcName]}
				<div class="trow">
					<div class="mono" style="flex:2;font-size:12px">{svcName}</div>
					<div style="flex:1">
						<span class="dot" style="background:{svc.state === 'running' ? 'var(--ok)' : 'var(--danger)'}"></span>
						<span class="cell" style="color:{svc.state === 'running' ? 'var(--ok)' : 'var(--danger)'}">{svc.state}</span>
					</div>
					<div style="flex:1.2">
						{#if res}
							<span class="res-val" style="color:{barColor(res.cpu_pct)}">{res.cpu_pct.toFixed(1)}%</span>
						{:else}
							<span class="muted">—</span>
						{/if}
					</div>
					<div style="flex:1.2">
						{#if res}
							<span class="res-val">{res.mem_used_mb.toFixed(0)}<span class="muted" style="font-size:10px"> MB</span></span>
						{:else}
							<span class="muted">—</span>
						{/if}
					</div>
					<div class="cell muted" style="flex:2;font-size:11.5px">{svc.status}</div>
				</div>
			{/each}
		</div>
		<!-- Mobile cards for Core Services -->
		<div class="card-list" style="margin-bottom:20px">
			{#each coreServices as svc}
				{@const svcName = svc.name.replace(/^\//, '')}
				{@const res = coreResStats[svcName]}
				<div class="m-card">
					<div class="m-card-title mono">{svcName}</div>
					<div class="m-card-row"><span class="m-card-key">State</span><span style="color:{svc.state === 'running' ? 'var(--ok)' : 'var(--danger)'}">{svc.state}</span></div>
					<div class="m-card-row"><span class="m-card-key">CPU</span><span>{res ? res.cpu_pct.toFixed(1) + '%' : '—'}</span></div>
					<div class="m-card-row"><span class="m-card-key">Memory</span><span>{res ? res.mem_used_mb.toFixed(0) + ' MB' : '—'}</span></div>
					<div class="m-card-row"><span class="m-card-key">Status</span><span class="muted" style="font-size:11.5px">{svc.status}</span></div>
				</div>
			{/each}
		</div>
	{/if}

	<!-- System Metrics -->
	<div class="section-title" style="margin-bottom:10px">Host Metrics</div>
	{#if sysLoading}
		<div class="metrics-grid">
			{#each [0,1,2,3] as _}<div class="metric-card sk-card"><div class="sk" style="height:14px;width:60%"></div><div class="sk" style="height:28px;width:40%;margin-top:10px"></div></div>{/each}
		</div>
	{:else if sysInfo}
		<div class="metrics-grid">
			<div class="metric-card">
				<div class="m-label">CPU Usage</div>
				<div class="m-val">{sysInfo.cpu_usage_pct.toFixed(1)}<span class="m-unit">%</span></div>
				<div class="bar-track"><div class="bar-fill" style="width:{sysInfo.cpu_usage_pct}%;background:{barColor(sysInfo.cpu_usage_pct)}"></div></div>
				<div class="m-sub">Updated live</div>
			</div>
			<div class="metric-card">
				<div class="m-label">Memory</div>
				<div class="m-val">{sysInfo.memory_used_pct.toFixed(1)}<span class="m-unit">%</span></div>
				<div class="bar-track"><div class="bar-fill" style="width:{sysInfo.memory_used_pct}%;background:{barColor(sysInfo.memory_used_pct)}"></div></div>
				<div class="m-sub">{sysInfo.memory_used_mb.toFixed(0)} / {sysInfo.memory_total_mb.toFixed(0)} MB</div>
			</div>
			{#if sysInfo.swap_total_mb > 0}
				<div class="metric-card">
					<div class="m-label">Swap</div>
					<div class="m-val">{(sysInfo.swap_used_mb / Math.max(sysInfo.swap_total_mb, 1) * 100).toFixed(1)}<span class="m-unit">%</span></div>
					<div class="bar-track"><div class="bar-fill" style="width:{sysInfo.swap_used_mb / Math.max(sysInfo.swap_total_mb, 1) * 100}%;background:{barColor(sysInfo.swap_used_mb / Math.max(sysInfo.swap_total_mb, 1) * 100)}"></div></div>
					<div class="m-sub">{sysInfo.swap_used_mb} / {sysInfo.swap_total_mb} MB</div>
				</div>
			{/if}
			<div class="metric-card">
				<div class="m-label">Uptime</div>
				<div class="m-val uptime">{fmtUptime(sysInfo.uptime_secs)}</div>
				<div class="m-sub">Platform running time</div>
			</div>
			{#if sysInfo.disks[0]}
				<div class="metric-card">
					<div class="m-label">Disk ({sysInfo.disks[0].mount})</div>
					<div class="m-val">{sysInfo.disks[0].used_pct.toFixed(1)}<span class="m-unit">%</span></div>
					<div class="bar-track"><div class="bar-fill" style="width:{sysInfo.disks[0].used_pct}%;background:{barColor(sysInfo.disks[0].used_pct)}"></div></div>
					<div class="m-sub">{sysInfo.disks[0].used_gb.toFixed(1)} / {sysInfo.disks[0].total_gb.toFixed(1)} GB</div>
				</div>
			{/if}
		</div>

		{#if sysInfo.disks.length > 1}
			<div class="section-title">All Disks</div>
			<div class="disk-list">
				{#each sysInfo.disks as d}
					<div class="disk-row">
						<span class="mono" style="flex:1.5;font-size:12px">{d.mount}</span>
						<div class="bar-track" style="flex:3"><div class="bar-fill" style="width:{d.used_pct}%;background:{barColor(d.used_pct)}"></div></div>
						<span class="cell" style="flex:1;text-align:right">{d.used_pct.toFixed(1)}%</span>
						<span class="muted" style="flex:1.5;text-align:right">{d.used_gb.toFixed(1)}/{d.total_gb.toFixed(1)} GB</span>
					</div>
				{/each}
			</div>
		{/if}

		{#if sysInfo.networks.length > 0}
			<div class="section-title">Network Interfaces</div>
			<div class="tbl">
				<div class="thead">
					<span style="flex:2">Interface</span>
					<span style="flex:1.5">RX</span>
					<span style="flex:1.5">TX</span>
				</div>
				{#each sysInfo.networks as n}
					<div class="trow">
						<div class="mono" style="flex:2;font-size:12px">{n.iface}</div>
						<div class="cell" style="flex:1.5">{fmtBytes(n.rx_bytes)}</div>
						<div class="cell" style="flex:1.5">{fmtBytes(n.tx_bytes)}</div>
					</div>
				{/each}
			</div>
			<!-- Mobile cards for network -->
			<div class="card-list">
				{#each sysInfo.networks as n}
					<div class="m-card">
						<div class="m-card-title mono">{n.iface}</div>
						<div class="m-card-row"><span class="m-card-key">RX</span><span>{fmtBytes(n.rx_bytes)}</span></div>
						<div class="m-card-row"><span class="m-card-key">TX</span><span>{fmtBytes(n.tx_bytes)}</span></div>
					</div>
				{/each}
			</div>
		{/if}
	{:else if sysError}
		<div class="err-banner">{sysError}</div>
	{/if}

	<!-- Swarm Nodes -->
	<div class="section-title" style="margin-top:28px">
		Swarm Nodes
		<span class="count-pill">{nodes.length}</span>
	</div>
	{#if nodesLoading}
		<div class="tbl">{#each [0,1] as _}<div class="sk-row"><div class="sk" style="width:140px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div></div>{/each}</div>
	{:else if nodesError}
		<div class="err-banner">{nodesError}</div>
	{:else if nodes.length === 0}
		<div class="empty">No swarm nodes found.</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:1.5">Node ID</span>
				<span style="flex:2">Hostname</span>
				<span style="flex:1">Role</span>
				<span style="flex:1">State</span>
				<span style="flex:1">Availability</span>
				<span style="flex:1.5">Engine</span>
			</div>
			{#each pagedNodes as node}
				<div class="trow">
					<div class="mono muted" style="flex:1.5;font-size:10.5px">{((node as any).id ?? '—').slice(0, 12)}</div>
					<div class="mono" style="flex:2;font-size:12px">{(node as any).description?.hostname ?? (node as any).hostname ?? '—'}</div>
					<div style="flex:1">
						{#if (node as any).spec?.role === 'manager' || (node as any).role === 'manager'}
							<span class="role-mgr">Manager</span>
						{:else}
							<span class="role-worker">Worker</span>
						{/if}
					</div>
					<div style="flex:1">
						<span class="dot" style="background:{nodeStateColor((node as any).status?.state ?? (node as any).state ?? 'unknown')}"></span>
						<span class="cell" style="color:{nodeStateColor((node as any).status?.state ?? (node as any).state ?? 'unknown')}">{(node as any).status?.state ?? (node as any).state ?? 'unknown'}</span>
					</div>
					<div class="cell" style="flex:1">{(node as any).spec?.availability ?? (node as any).availability ?? '—'}</div>
					<div class="mono muted" style="flex:1.5;font-size:11px">{(node as any).description?.engine?.engine_version ?? '—'}</div>
				</div>
			{/each}
		</div>
		<!-- Mobile cards for nodes -->
		<div class="card-list">
			{#each pagedNodes as node}
				<div class="m-card">
					<div class="m-card-title mono">{(node as any).description?.hostname ?? (node as any).hostname ?? '—'}</div>
					<div class="m-card-row">
						<span class="m-card-key">Role</span>
						{#if (node as any).spec?.role === 'manager' || (node as any).role === 'manager'}
							<span class="role-mgr">Manager</span>
						{:else}
							<span class="role-worker">Worker</span>
						{/if}
					</div>
					<div class="m-card-row"><span class="m-card-key">State</span><span style="color:{nodeStateColor((node as any).status?.state ?? (node as any).state ?? 'unknown')}">{(node as any).status?.state ?? (node as any).state ?? 'unknown'}</span></div>
					<div class="m-card-row"><span class="m-card-key">Avail.</span><span>{(node as any).spec?.availability ?? (node as any).availability ?? '—'}</span></div>
				</div>
			{/each}
		</div>
		{#if totalNodePages > 1}
			<div class="pager">
				<button class="pg-btn" disabled={nodePage === 0} onclick={() => nodePage--}>Prev</button>
				<span class="pg-info">Page {nodePage + 1} of {totalNodePages}</span>
				<button class="pg-btn" disabled={nodePage >= totalNodePages - 1} onclick={() => nodePage++}>Next</button>
			</div>
		{/if}
	{/if}

	<!-- Join Tokens -->
	{#if tokens}
		<div class="section-title" style="margin-top:28px">Swarm Join Tokens</div>
		<div class="tokens-card">
			<div class="token-row">
				<div class="token-label">Worker</div>
				<div class="token-body">
					<code class="token-val">{showWorker ? tokens.worker : '••••••••••••••••••••••••••••••••'}</code>
					<div class="token-actions">
						<button class="t-btn" onclick={() => (showWorker = !showWorker)}>{showWorker ? 'Hide' : 'Show'}</button>
						<button class="t-btn" onclick={() => copyText(tokens!.worker, 'worker')}>{copiedWorker ? 'Copied!' : 'Copy'}</button>
					</div>
				</div>
			</div>
			<div class="token-divider"></div>
			<div class="token-row">
				<div class="token-label">Manager</div>
				<div class="token-body">
					<code class="token-val">{showManager ? tokens.manager : '••••••••••••••••••••••••••••••••'}</code>
					<div class="token-actions">
						<button class="t-btn" onclick={() => (showManager = !showManager)}>{showManager ? 'Hide' : 'Show'}</button>
						<button class="t-btn" onclick={() => copyText(tokens!.manager, 'manager')}>{copiedManager ? 'Copied!' : 'Copy'}</button>
					</div>
				</div>
			</div>
		</div>
	{/if}
</div>

<style>
	.p { max-width:940px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:20px; flex-wrap:wrap; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }
	.hdr-right { display:flex; align-items:center; gap:6px; }
	.conn-dot { display:inline-block; width:7px; height:7px; border-radius:50%; background:var(--text-4); flex-shrink:0; }
	.conn-dot.conn-ok { background:var(--ok); box-shadow:0 0 0 2px var(--ok-soft); }
	.conn-dot.conn-err { background:var(--danger); }
	.conn-label { font-size:11.5px; color:var(--text-3); }

	.metrics-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(200px,1fr)); gap:12px; margin-bottom:20px; }
	.metric-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:18px 16px; box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.sk-card { min-height:100px; }
	.m-label { font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.07em; margin-bottom:6px; }
	.m-val { font-size:26px; font-weight:800; color:var(--text); letter-spacing:-0.03em; line-height:1; margin-bottom:8px; }
	.m-val.uptime { font-size:22px; }
	.m-unit { font-size:14px; font-weight:500; color:var(--text-3); margin-left:1px; }
	.m-sub { font-size:11px; color:var(--text-3); margin-top:4px; }
	.bar-track { height:4px; background:var(--border); border-radius:99px; overflow:hidden; margin-bottom:6px; }
	.bar-fill { height:100%; border-radius:99px; transition:width .5s ease; }

	.section-title { display:flex; align-items:center; gap:8px; font-size:12.5px; font-weight:700; color:var(--text-2); text-transform:uppercase; letter-spacing:.06em; margin-bottom:10px; }
	.count-pill { display:inline-flex; align-items:center; justify-content:center; height:18px; padding:0 6px; border-radius:999px; font-size:10px; font-weight:700; background:var(--surface-2); color:var(--text-3); border:1px solid var(--border); }

	.disk-list { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; margin-bottom:20px; }
	.disk-row { display:flex; align-items:center; gap:12px; padding:10px 16px; border-bottom:1px solid var(--border); }
	.disk-row:last-child { border-bottom:none; }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); margin-bottom:8px; }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.mono { font-family:var(--mono); color:var(--text); }
	.dot { display:inline-block; width:6px; height:6px; border-radius:50%; margin-right:5px; }

	.role-mgr { display:inline-flex; padding:2px 8px; border-radius:999px; font-size:10.5px; font-weight:700; background:var(--accent-soft); color:var(--accent); border:1px solid var(--accent-ring); }
	.role-worker { display:inline-flex; padding:2px 8px; border-radius:999px; font-size:10.5px; font-weight:600; background:var(--surface-2); color:var(--text-3); border:1px solid var(--border); }

	.tokens-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.token-row { padding:14px 16px; display:flex; flex-direction:column; gap:8px; }
	.token-divider { height:1px; background:var(--border); }
	.token-label { font-size:11px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.06em; }
	.token-body { display:flex; align-items:center; gap:10px; flex-wrap:wrap; }
	.token-val { font-size:11.5px; font-family:var(--mono); color:var(--text-2); background:var(--surface-2); padding:4px 8px; border-radius:4px; word-break:break-all; flex:1; border:1px solid var(--border); }
	.token-actions { display:flex; gap:6px; }
	.t-btn { padding:4px 11px; border-radius:var(--radius-sm); font-size:11px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); white-space:nowrap; }
	.t-btn:hover { background:var(--accent); border-color:var(--accent); color:#000; }

	.pager { display:flex; align-items:center; gap:10px; padding:10px 0; justify-content:center; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-row { display:flex; align-items:center; gap:12px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err-banner { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); margin-bottom:16px; }
	.empty { padding:40px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	/* Mobile cards — hidden by default, shown on small screens */
	.card-list { display:none; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-title { font-size:13px; font-weight:600; color:var(--text); margin-bottom:8px; }
	.m-card-row { display:flex; justify-content:space-between; align-items:center; padding:4px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); }
	.m-card-row:last-child { border-bottom:none; }
	.m-card-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; }

	.res-val { font-size:12px; font-weight:600; font-family:var(--mono); color:var(--text); }

	@media (max-width: 768px) {
		.p { padding:20px 14px; }
	}
	@media (max-width: 640px) {
		.p { padding:16px 12px; }
		.metrics-grid { grid-template-columns:1fr 1fr; }
		.tbl { display:none; }
		.card-list { display:block; }
		.disk-row { flex-wrap:wrap; gap:6px; }
		.token-body { flex-direction:column; align-items:flex-start; }
		.token-val { width:100%; }
	}
</style>
