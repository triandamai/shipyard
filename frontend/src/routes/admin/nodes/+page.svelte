<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AdminNode } from '$lib/api/types';

	let nodes = $state<AdminNode[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let search = $state('');

	onMount(async () => {
		const res = await api.getAdminNodes();
		if (res.data) nodes = res.data;
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	});

	let filtered = $derived(
		search.trim()
			? nodes.filter(
					n =>
						n.name.toLowerCase().includes(search.toLowerCase()) ||
						n.org_name.toLowerCase().includes(search.toLowerCase()) ||
						n.provider.toLowerCase().includes(search.toLowerCase())
				)
			: nodes
	);

	const PAGE = 25;
	let page = $state(0);
	let rows = $derived(filtered.slice(page * PAGE, (page + 1) * PAGE));
	let totalPages = $derived(Math.ceil(filtered.length / PAGE));
	$effect(() => { filtered; page = 0; });

	type StatusKey = 'active'|'degraded'|'failed'|'provisioning'|'cloud_init_running'|'wireguard_joined'|'stopped';
	const SM: Record<StatusKey, { label:string; color:string; pulse:boolean }> = {
		active:             { label:'Active',       color:'var(--ok)',       pulse:true  },
		degraded:           { label:'Degraded',     color:'var(--warn)',     pulse:false },
		failed:             { label:'Failed',       color:'var(--danger)',   pulse:false },
		provisioning:       { label:'Provisioning', color:'var(--text-2)',   pulse:true  },
		cloud_init_running: { label:'Init',         color:'var(--text-2)',   pulse:true  },
		wireguard_joined:   { label:'Joining',      color:'var(--text-2)',   pulse:true  },
		stopped:            { label:'Stopped',      color:'var(--text-3)',   pulse:false },
	};
	function getStatus(s:string) {
		return SM[s as StatusKey] ?? { label:s, color:'var(--text-3)', pulse:false };
	}

	const PV: Record<string, string> = {
		hetzner:'#e53e3e', digitalocean:'#1a81c2', aws:'#f59e0b', gcp:'#34a853', vultr:'#007bfc'
	};
	function pvColor(p:string): string {
		return PV[p.toLowerCase()] ?? 'var(--text-3)';
	}

	let summary = $derived({
		active:       nodes.filter(n => n.status === 'active').length,
		provisioning: nodes.filter(n => ['provisioning','cloud_init_running','wireguard_joined'].includes(n.status)).length,
		degraded:     nodes.filter(n => n.status === 'degraded').length,
		stopped:      nodes.filter(n => ['stopped','failed'].includes(n.status)).length,
	});
</script>

<div class="p">
	<header class="hdr">
		<div class="hdr-l">
			<h1 class="ttl">Compute</h1>
			<span class="pill">{nodes.length}</span>
		</div>
		<label class="search">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13">
				<path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/>
			</svg>
			<input type="text" placeholder="Search nodes…" bind:value={search} />
		</label>
	</header>

	<!-- Compact summary row -->
	{#if !loading && nodes.length > 0}
		<div class="summary">
			<div class="sum-item">
				<span class="dot pulse" style="background:var(--ok)"></span>
				<span>{summary.active} active</span>
			</div>
			{#if summary.provisioning > 0}
				<div class="sum-item">
					<span class="dot pulse" style="background:var(--text-3)"></span>
					<span>{summary.provisioning} provisioning</span>
				</div>
			{/if}
			{#if summary.degraded > 0}
				<div class="sum-item">
					<span class="dot" style="background:var(--warn)"></span>
					<span style="color:var(--warn)">{summary.degraded} degraded</span>
				</div>
			{/if}
			{#if summary.stopped > 0}
				<div class="sum-item muted">
					<span class="dot" style="background:var(--text-4)"></span>
					<span>{summary.stopped} stopped</span>
				</div>
			{/if}
		</div>
	{/if}

	{#if loading}
		<div class="tbl">
			{#each [0,1,2,3] as _}
				<div class="sk-row">
					<div style="flex:2;display:flex;flex-direction:column;gap:6px">
						<div class="sk sk-l"></div>
						<div class="sk sk-xs"></div>
					</div>
					<div class="sk sk-chip"></div>
					<div class="sk sk-chip" style="width:70px"></div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if rows.length === 0}
		<div class="empty">
			<svg viewBox="0 0 20 20" fill="currentColor" width="28" height="28"><path fill-rule="evenodd" d="M2 5a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2V5zm14 1a1 1 0 11-2 0 1 1 0 012 0zM2 13a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2v-2zm14 1a1 1 0 11-2 0 1 1 0 012 0z" clip-rule="evenodd"/></svg>
			{search ? 'No results.' : 'No compute nodes yet.'}
		</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:2">Node</span>
				<span style="flex:1.5">Organization</span>
				<span style="flex:0.8">Provider</span>
				<span style="flex:0.9">Region</span>
				<span style="flex:1.1">Status</span>
				<span style="flex:1">Public IP</span>
				<span style="flex:0.9">Created</span>
			</div>
			{#each rows as node}
				{@const st = getStatus(node.status)}
				{@const pc = pvColor(node.provider)}
				<div class="trow">
					<div class="node-c" style="flex:2">
						<span class="node-name">{node.name}</span>
					</div>
					<div class="org-name" style="flex:1.5">{node.org_name}</div>
					<div style="flex:0.8">
						<span class="pv-chip" style="color:{pc}">{node.provider}</span>
					</div>
					<div class="region" style="flex:0.9">{node.region}</div>
					<div style="flex:1.1">
						<span class="status-row" style="color:{st.color}">
							<span class="dot" class:pulse={st.pulse} style="background:{st.color}"></span>
							{st.label}
						</span>
					</div>
					<div class="ip" style="flex:1">{node.public_ip ?? '—'}</div>
					<div class="d" style="flex:0.9">{new Date(node.created_at).toLocaleDateString()}</div>
				</div>
			{/each}
		</div>
		{#if totalPages > 1}
			<div class="pager">
				<button class="pg-btn" disabled={page === 0} onclick={() => page--}>Prev</button>
				<span class="pg-info">Page {page + 1} of {totalPages} &bull; {filtered.length} total</span>
				<button class="pg-btn" disabled={page >= totalPages - 1} onclick={() => page++}>Next</button>
			</div>
		{/if}
		<!-- Mobile cards -->
		<div class="card-list">
			{#each rows as node}
				{@const st = getStatus(node.status)}
				{@const pc = pvColor(node.provider)}
				<div class="m-card">
					<div class="m-card-title mono">{node.name}</div>
					<div class="m-card-row"><span class="m-key">Org</span><span>{node.org_name}</span></div>
					<div class="m-card-row"><span class="m-key">Status</span><span style="color:{st.color}">{st.label}</span></div>
					<div class="m-card-row"><span class="m-key">Provider</span><span style="color:{pc}">{node.provider}</span></div>
					<div class="m-card-row"><span class="m-key">Region</span><span>{node.region}</span></div>
					<div class="m-card-row"><span class="m-key">IP</span><span class="mono">{node.public_ip ?? '—'}</span></div>
				</div>
			{/each}
			{#if totalPages > 1}
				<div class="pager">
					<button class="pg-btn" disabled={page === 0} onclick={() => page--}>Prev</button>
					<span class="pg-info">{page + 1} / {totalPages}</span>
					<button class="pg-btn" disabled={page >= totalPages - 1} onclick={() => page++}>Next</button>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.p { max-width:1080px; margin:0 auto; padding:40px 36px; }

	.hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:16px; gap:12px; }
	.hdr-l { display:flex; align-items:center; gap:8px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0; letter-spacing:-0.02em; }
	.pill { display:inline-flex; align-items:center; justify-content:center; height:20px; padding:0 7px; border-radius:999px; font-size:11px; font-weight:700; background:var(--surface-2); color:var(--text-3); border:1px solid var(--border); }

	.search { position:relative; display:flex; align-items:center; cursor:text; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search input { height:32px; padding:0 10px 0 28px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:190px; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search input::placeholder { color:var(--text-3); }
	.search input:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	/* Summary bar */
	.summary {
		display:flex; align-items:center; gap:20px; flex-wrap:wrap;
		padding:9px 14px; margin-bottom:14px;
		background:var(--surface); border:1px solid var(--border);
		border-radius:var(--radius-sm); box-shadow:var(--shadow-sm);
	}
	.sum-item { display:flex; align-items:center; gap:6px; font-size:12px; color:var(--text-2); }
	.sum-item.muted { color:var(--text-3); }

	/* Dot + pulse */
	.dot { display:inline-block; width:6px; height:6px; border-radius:50%; flex-shrink:0; }
	.dot.pulse { animation:pulse 2s ease-in-out infinite; }
	@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:.3} }

	/* Skeleton */
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-l { width:120px; height:12px; }
	.sk-xs { width:80px; height:10px; }
	.sk-chip { width:60px; height:20px; border-radius:999px; }
	.sk-row { display:flex; align-items:center; gap:14px; padding:14px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:56px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	/* Table */
	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:0.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:11px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }

	.node-c { display:flex; align-items:center; gap:8px; min-width:0; }
	.node-name { font-size:12.5px; font-weight:600; color:var(--text); font-family:var(--mono); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
	.org-name { font-size:12.5px; color:var(--text-2); }
	.pv-chip { font-size:11px; font-weight:700; }
	.region { font-size:11.5px; color:var(--text-3); }
	.ip { font-size:11.5px; color:var(--text-3); font-family:var(--mono); }
	.d { font-size:11.5px; color:var(--text-3); white-space:nowrap; }

	.status-row { display:inline-flex; align-items:center; gap:6px; font-size:12px; font-weight:500; }

	.pager { display:flex; align-items:center; gap:10px; padding:12px 0 4px; justify-content:center; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }

	.card-list { display:none; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-title { font-size:13px; font-weight:600; color:var(--text); margin-bottom:8px; }
	.m-card-row { display:flex; justify-content:space-between; align-items:center; padding:5px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); }
	.m-card-row:last-child { border-bottom:none; }
	.m-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; }
	.mono { font-family:var(--mono); }

	@media (max-width: 768px) {
		.p { padding:20px 14px; }
	}
	@media (max-width: 640px) {
		.p { padding:16px 12px; }
		.tbl { display:none; }
		.card-list { display:block; }
		.hdr { flex-direction:column; align-items:flex-start; }
		.summary { gap:12px; }
	}
</style>
