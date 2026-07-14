<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface AppDeployment {
		id: string;
		org_name: string;
		org_slug: string;
		service_name: string;
		status: string;
		triggered_by: string | null;
		created_at: string;
		finished_at: string | null;
	}
	let deps = $state<AppDeployment[]>([]);
	let loading = $state(false);
	let error = $state('');
	let page = $state(0);
	let total = $state(0);
	let orgFilter = $state('');
	let statusFilter = $state('');
	const LIMIT = 30;

	onMount(() => load());

	async function load() {
		loading = true; error = '';
		const params = new URLSearchParams({
			page: String(page), limit: String(LIMIT),
			...(orgFilter ? { org: orgFilter } : {}),
			...(statusFilter ? { status: statusFilter } : {}),
		});
		const res = await api.get<{ items: AppDeployment[]; total: number }>(`/admin/deployments/app?${params}`);
		if (res.data) { deps = res.data.items ?? []; total = res.data.total ?? 0; }
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	}

	let totalPages = $derived(Math.ceil(total / LIMIT));

	function statusColor(s: string): string {
		if (s === 'success' || s === 'done') return 'var(--ok)';
		if (s === 'failed' || s === 'error') return 'var(--danger)';
		if (s === 'running') return 'var(--accent)';
		return 'var(--text-3)';
	}

	function relTime(iso: string): string {
		const diff = Date.now() - new Date(iso).getTime();
		const m = Math.floor(diff / 60000);
		if (m < 1) return 'just now';
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		return `${Math.floor(h / 24)}d ago`;
	}
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">App Deployments</h1>
			<p class="sub">All service deployments across organizations.</p>
		</div>
		<a class="prov-link" href="/admin/deployments/provisioning">Tenant Provisioning →</a>
	</header>

	<div class="toolbar">
		<input class="filter-inp" placeholder="Filter by org…" bind:value={orgFilter} oninput={() => { page = 0; load(); }} />
		<select class="filter-sel" bind:value={statusFilter} onchange={() => { page = 0; load(); }}>
			<option value="">All Statuses</option>
			<option value="running">Running</option>
			<option value="success">Success</option>
			<option value="failed">Failed</option>
			<option value="queued">Queued</option>
			<option value="cancelled">Cancelled</option>
		</select>
		<button class="refresh-btn" onclick={() => { page = 0; load(); }}>
			<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
			Refresh
		</button>
	</div>

	{#if loading}
		<div class="tbl">{#each [0,1,2,3] as _}<div class="sk-row"><div class="sk" style="width:120px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div></div>{/each}</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if deps.length === 0}
		<div class="empty">No app deployments found.</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:1.5">Org</span>
				<span style="flex:1.5">Service</span>
				<span style="flex:1">Status</span>
				<span style="flex:1">Triggered</span>
				<span style="flex:1">Started</span>
			</div>
			{#each deps as d}
				<div class="trow">
					<div style="flex:1.5;min-width:0">
						<div class="name"><a class="link" href="/orgs/{d.org_slug}" target="_blank">{d.org_name}</a></div>
					</div>
					<div class="name" style="flex:1.5;min-width:0">{d.service_name}</div>
					<div style="flex:1">
						<span class="dot" style="background:{statusColor(d.status)}"></span>
						<span style="font-size:12px;color:{statusColor(d.status)};font-weight:500">{d.status}</span>
					</div>
					<div class="cell" style="flex:1">{d.triggered_by ?? '—'}</div>
					<div class="muted" style="flex:1;font-size:11.5px">{relTime(d.created_at)}</div>
				</div>
			{/each}
		</div>

		<div class="card-list">
			{#each deps as d}
				<div class="m-card">
					<div class="m-card-title"><a class="link" href="/orgs/{d.org_slug}" target="_blank">{d.org_name}</a></div>
					<div class="m-card-row"><span class="m-card-key">Service</span><span class="cell">{d.service_name}</span></div>
					<div class="m-card-row">
						<span class="m-card-key">Status</span>
						<span>
							<span class="dot" style="background:{statusColor(d.status)}"></span>
							<span style="font-size:12px;color:{statusColor(d.status)};font-weight:500">{d.status}</span>
						</span>
					</div>
					<div class="m-card-row"><span class="m-card-key">Triggered</span><span class="cell">{d.triggered_by ?? '—'}</span></div>
					<div class="m-card-row"><span class="m-card-key">Started</span><span class="muted" style="font-size:11.5px">{relTime(d.created_at)}</span></div>
				</div>
			{/each}
		</div>

		{#if totalPages > 1}
			<div class="pager">
				<button class="pg-btn" disabled={page === 0} onclick={() => { page--; load(); }}>Prev</button>
				<span class="pg-info">Page {page + 1} of {totalPages} &bull; {total} total</span>
				<button class="pg-btn" disabled={page >= totalPages - 1} onclick={() => { page++; load(); }}>Next</button>
			</div>
		{/if}
	{/if}
</div>

<style>
	.p { max-width:1100px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:20px; flex-wrap:wrap; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }
	.prov-link { font-size:12.5px; font-weight:600; color:var(--accent); text-decoration:none; white-space:nowrap; }
	.prov-link:hover { text-decoration:underline; }

	.toolbar { display:flex; align-items:center; gap:8px; margin-bottom:14px; flex-wrap:wrap; }
	.filter-inp { height:32px; padding:0 10px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12px; color:var(--text); outline:none; width:160px; font-family:var(--font); }
	.filter-inp:focus { border-color:var(--accent); }
	.filter-sel { height:32px; padding:0 8px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12px; color:var(--text); outline:none; font-family:var(--font); cursor:pointer; }
	.refresh-btn { display:flex; align-items:center; gap:6px; padding:6px 12px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); margin-left:auto; }
	.refresh-btn:hover { background:var(--surface-2); }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.name { font-size:12.5px; font-weight:500; color:var(--text); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.dot { display:inline-block; width:6px; height:6px; border-radius:50%; margin-right:5px; }
	.link { color:var(--text); text-decoration:none; }
	.link:hover { text-decoration:underline; color:var(--accent); }

	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-row { display:flex; align-items:center; gap:12px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.pager { display:flex; align-items:center; gap:10px; padding:12px 0 4px; justify-content:center; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }

	.card-list { display:none; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-title { font-size:13px; font-weight:600; color:var(--text); margin-bottom:8px; }
	.m-card-row { display:flex; justify-content:space-between; align-items:center; padding:4px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); }
	.m-card-row:last-child { border-bottom:none; }
	.m-card-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; }

	@media (max-width: 640px) {
		.p { padding:20px 12px; }
		.tbl { display:none; }
		.card-list { display:block; }
		.toolbar { gap:6px; }
		.filter-inp { width:100%; }
		.filter-sel { width:100%; }
		.refresh-btn { width:100%; margin-left:0; }
	}
</style>
