<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface ProvisioningJob {
		id: string;
		org_name: string;
		org_slug: string;
		name: string;
		provider: string;
		region: string;
		status: string;
		created_at: string;
	}
	let jobs = $state<ProvisioningJob[]>([]);
	let loading = $state(false);
	let error = $state('');
	let page = $state(0);
	let total = $state(0);
	const LIMIT = 30;

	onMount(() => load());

	async function load() {
		loading = true; error = '';
		const params = new URLSearchParams({ page: String(page), limit: String(LIMIT) });
		const res = await api.get<{ items: ProvisioningJob[]; total: number }>(`/admin/deployments/provisioning?${params}`);
		if (res.data) { jobs = res.data.items ?? []; total = res.data.total ?? 0; }
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	}

	let totalPages = $derived(Math.ceil(total / LIMIT));

	function statusColor(s: string): string {
		if (s === 'active') return 'var(--ok)';
		if (s === 'failed') return 'var(--danger)';
		if (s === 'provisioning') return 'var(--accent)';
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
			<h1 class="ttl">Tenant Provisioning</h1>
			<p class="sub">Compute nodes being provisioned for tenant organizations.</p>
		</div>
		<div style="display:flex;gap:10px;align-items:center">
			<a class="back-link" href="/admin/deployments">← App Deployments</a>
			<button class="refresh-btn" onclick={() => { page = 0; load(); }}>
				<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
				Refresh
			</button>
		</div>
	</header>

	{#if loading}
		<div class="tbl">{#each [0,1,2,3] as _}<div class="sk-row"><div class="sk" style="width:140px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div></div>{/each}</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if jobs.length === 0}
		<div class="empty">No provisioning jobs in progress.</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:2">Organization</span>
				<span style="flex:1.5">Node</span>
				<span style="flex:1">Provider</span>
				<span style="flex:1">Region</span>
				<span style="flex:1">Status</span>
				<span style="flex:1">Started</span>
			</div>
			{#each jobs as j}
				<div class="trow">
					<div style="flex:2;min-width:0">
						<a class="link" href="/orgs/{j.org_slug}" target="_blank">{j.org_name}</a>
					</div>
					<div class="mono cell" style="flex:1.5">{j.name}</div>
					<div class="cell" style="flex:1">{j.provider}</div>
					<div class="cell" style="flex:1">{j.region}</div>
					<div style="flex:1">
						<span class="dot" style="background:{statusColor(j.status)}"></span>
						<span style="font-size:12px;color:{statusColor(j.status)};font-weight:500">{j.status}</span>
					</div>
					<div class="muted" style="flex:1;font-size:11.5px">{relTime(j.created_at)}</div>
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
	.back-link { font-size:12.5px; font-weight:600; color:var(--accent); text-decoration:none; }
	.back-link:hover { text-decoration:underline; }
	.refresh-btn { display:flex; align-items:center; gap:6px; padding:6px 12px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.refresh-btn:hover { background:var(--surface-2); }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.mono { font-family:var(--mono); }
	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.dot { display:inline-block; width:6px; height:6px; border-radius:50%; margin-right:5px; }
	.link { font-size:12.5px; font-weight:500; color:var(--text); text-decoration:none; }
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
</style>
