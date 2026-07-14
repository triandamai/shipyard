<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface NetworkSummary {
		id: string; name: string; driver: string; scope: string;
		internal: boolean; attachable: boolean; ipam_subnet: string | null;
		labels: Record<string, string>; containers: number;
	}

	let networks = $state<NetworkSummary[]>([]);
	let loading = $state(false);
	let error = $state('');
	let search = $state('');
	let page = $state(0);
	const LIMIT = 25;

	async function load() {
		loading = true; error = '';
		const r = await api.get<NetworkSummary[]>('/admin/docker/networks');
		if (r.data) networks = r.data;
		else error = r.error?.message ?? 'Failed to load networks';
		loading = false;
	}

	let filtered = $derived(
		networks.filter(n => !search || n.name.toLowerCase().includes(search.toLowerCase()))
	);

	let totalPages = $derived(Math.ceil(filtered.length / LIMIT));
	let paged = $derived(filtered.slice(page * LIMIT, (page + 1) * LIMIT));

	$effect(() => {
		search;
		page = 0;
	});

	onMount(load);
</script>

<div class="inner-toolbar">
	<div class="left-controls"></div>
	<div class="right-controls">
		<label class="search">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="12" height="12"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
			<input type="text" placeholder="Search networks…" bind:value={search} />
		</label>
		<button class="refresh-btn" onclick={load}>
			<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
		</button>
	</div>
</div>

{#if loading}
	<div class="tbl">{#each [0,1,2,3] as _}<div class="sk-row"><div class="sk" style="width:140px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div><div class="sk" style="width:70px;height:18px;border-radius:999px"></div></div>{/each}</div>
{:else if error}
	<div class="err">{error}</div>
{:else if paged.length === 0}
	<div class="empty">No networks found.</div>
{:else}
	<div class="tbl">
		<div class="thead">
			<span style="flex:2">Name</span>
			<span style="flex:1">Driver</span>
			<span style="flex:1">Scope</span>
			<span style="flex:1.5">Subnet</span>
			<span style="flex:0.6">Containers</span>
		</div>
		{#each paged as n}
			<div class="trow">
				<div class="mono" style="flex:2;font-size:12px">{n.name}</div>
				<div class="cell" style="flex:1">{n.driver}</div>
				<div class="cell" style="flex:1">{n.scope}</div>
				<div class="mono muted" style="flex:1.5;font-size:11.5px">{n.ipam_subnet ?? '—'}</div>
				<div class="cell" style="flex:0.6">{n.containers}</div>
			</div>
		{/each}
	</div>

	<div class="card-list">
		{#each paged as n}
			<div class="m-card">
				<div class="m-card-title mono">{n.name}</div>
				<div class="m-card-row"><span class="m-key">Driver</span><span>{n.driver}</span></div>
				<div class="m-card-row"><span class="m-key">Scope</span><span>{n.scope}</span></div>
				<div class="m-card-row"><span class="m-key">Subnet</span><span class="mono muted">{n.ipam_subnet ?? '—'}</span></div>
				<div class="m-card-row"><span class="m-key">Containers</span><span>{n.containers}</span></div>
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
{/if}

<style>
	.inner-toolbar { display:flex; align-items:center; justify-content:space-between; margin-bottom:14px; gap:12px; }
	.right-controls { display:flex; align-items:center; gap:8px; }
	.refresh-btn { display:flex; align-items:center; justify-content:center; width:32px; height:32px; border-radius:var(--radius-sm); cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; }
	.refresh-btn:hover { background:var(--surface-2); }

	.search { position:relative; display:flex; align-items:center; cursor:text; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search input { height:32px; padding:0 10px 0 27px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12px; color:var(--text); outline:none; width:180px; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search input::placeholder { color:var(--text-3); }
	.search input:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.mono { font-family:var(--mono); color:var(--text); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-row { display:flex; align-items:center; gap:12px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }
	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.pager { display:flex; align-items:center; gap:10px; padding:12px 0 4px; justify-content:center; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }

	.card-list { display:none; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-title { font-size:13px; font-weight:600; color:var(--text); margin-bottom:8px; font-family:var(--mono); }
	.m-card-row { display:flex; justify-content:space-between; align-items:flex-start; padding:5px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); gap:8px; }
	.m-card-row:last-child { border-bottom:none; }
	.m-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; flex-shrink:0; }

	@media (max-width: 680px) {
		.tbl { display:none; }
		.card-list { display:block; }
		.inner-toolbar { flex-direction:column; align-items:flex-start; gap:8px; }
		.right-controls { width:100%; justify-content:space-between; }
		.search { flex:1; }
		.search input { width:100%; }
	}
</style>
