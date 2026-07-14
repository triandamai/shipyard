<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface ServiceSummary {
		id: string; name: string; image: string;
		replicas_running: number; replicas_desired: number;
		mode: string; ports: string[]; labels: Record<string, string>;
		created_at: string | null; updated_at: string | null;
	}

	let services = $state<ServiceSummary[]>([]);
	let loading = $state(false);
	let error = $state('');
	let search = $state('');
	let page = $state(0);
	const LIMIT = 25;

	let expanded = $state(new Set<string>());
	function toggleExpand(id: string) {
		if (expanded.has(id)) expanded.delete(id);
		else expanded.add(id);
		expanded = new Set(expanded);
	}

	async function load() {
		loading = true; error = '';
		const r = await api.get<ServiceSummary[]>('/admin/docker/services');
		if (r.data) services = r.data;
		else error = r.error?.message ?? 'Failed to load services';
		loading = false;
	}

	let filtered = $derived(
		services.filter(s => !search || s.name.toLowerCase().includes(search.toLowerCase()) || s.image.toLowerCase().includes(search.toLowerCase()))
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
			<input type="text" placeholder="Search services…" bind:value={search} />
		</label>
		<button class="refresh-btn" onclick={load} title="Refresh">
			<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
		</button>
	</div>
</div>

{#if loading}
	<div class="tbl">{#each [0,1,2,3] as _}<div class="sk-row"><div class="sk" style="width:140px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div><div class="sk" style="width:70px;height:18px;border-radius:999px"></div></div>{/each}</div>
{:else if error}
	<div class="err">{error}</div>
{:else if paged.length === 0}
	<div class="empty">No services found.</div>
{:else}
	<div class="tbl">
		<div class="thead">
			<span style="flex:2.2">Name</span>
			<span style="flex:3">Image</span>
			<span style="flex:0.8">Mode</span>
			<span style="flex:0.9">Replicas</span>
		</div>
		{#each paged as s}
			<!-- svelte-ignore a11y_click_events_have_key_events -->
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<div class="trow-wrapper">
				<div class="trow" onclick={() => toggleExpand(s.id)}>
					<div class="mono trunc expand-cell" style="flex:2.2;font-size:12px">
						<span class="chevron" class:expanded={expanded.has(s.id)}>▶</span>
						{s.name}
					</div>
					<div class="cell img trunc" style="flex:3">{s.image}</div>
					<div class="cell" style="flex:0.8">{s.mode}</div>
					<div style="flex:0.9">
						<span class="rep-pill" class:rep-ok={s.replicas_running >= s.replicas_desired} class:rep-warn={s.replicas_running < s.replicas_desired}>
							{s.replicas_running}/{s.replicas_desired}
						</span>
					</div>
				</div>
				{#if expanded.has(s.id)}
					<div class="details-panel">
						<div class="details-grid">
							<div><strong>Service ID:</strong> <span class="mono">{s.id}</span></div>
							<div><strong>Mode:</strong> {s.mode}</div>
							<div style="grid-column: 1 / -1"><strong>Image:</strong> <span class="mono">{s.image}</span></div>
							{#if s.ports && s.ports.length > 0}
								<div style="grid-column: 1 / -1"><strong>Published Ports:</strong> <span class="mono">{s.ports.join(', ')}</span></div>
							{/if}
							{#if s.created_at}
								<div><strong>Created At:</strong> {new Date(s.created_at).toLocaleString()}</div>
							{/if}
							{#if s.updated_at}
								<div><strong>Updated At:</strong> {new Date(s.updated_at).toLocaleString()}</div>
							{/if}
							{#if Object.keys(s.labels).length > 0}
								<div style="grid-column: 1 / -1">
									<strong>Labels:</strong>
									<div class="labels-box mono">
										{#each Object.entries(s.labels) as [k, val]}
											<div><span class="lbl-key">{k}:</span> {val}</div>
										{:else}
											<!-- no-op -->
										{/each}
									</div>
								</div>
							{/if}
						</div>
					</div>
				{/if}
			</div>
		{/each}
	</div>

	<div class="card-list">
		{#each paged as s}
			<div class="m-card">
				<div class="m-card-title mono">{s.name}</div>
				<div class="m-card-row"><span class="m-key">Replicas</span><span class="rep-pill" class:rep-ok={s.replicas_running >= s.replicas_desired} class:rep-warn={s.replicas_running < s.replicas_desired}>{s.replicas_running}/{s.replicas_desired}</span></div>
				<div class="m-card-row"><span class="m-key">Mode</span><span>{s.mode}</span></div>
				<div class="m-card-row"><span class="m-key">Image</span><span class="mono" style="font-size:11px;word-break:break-all">{s.image}</span></div>
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
	.trow-wrapper { border-bottom:1px solid var(--border); display:flex; flex-direction:column; }
	.trow-wrapper:last-child { border-bottom:none; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:none; transition:background .1s; cursor:pointer; }
	.trow:hover { background:var(--row-hover); }
	.expand-cell { display:flex; align-items:center; gap:6px; min-width:0; }
	.chevron { display:inline-block; font-size:8px; color:var(--text-3); transition:transform .2s; margin-right:2px; flex-shrink:0; }
	.chevron.expanded { transform:rotate(90deg); }
	.details-panel { padding:14px 20px 16px; background:var(--surface-2); border-top:1px solid var(--border); font-size:12.5px; border-bottom:1px solid var(--border); }
	.details-grid { display:grid; grid-template-columns:1fr 1fr; gap:8px; }
	.labels-box { margin-top:6px; padding:8px 12px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:11px; max-height:120px; overflow-y:auto; }
	.lbl-key { color:var(--accent); font-weight:500; }

	.cell { font-size:12.5px; color:var(--text-2); }
	.mono { font-family:var(--mono); color:var(--text); }
	.trunc { text-overflow:ellipsis; white-space:nowrap; overflow:hidden; min-width:0; }
	.img { font-size:11.5px; font-family:var(--mono); }
	.rep-pill { display:inline-flex; padding:2px 8px; border-radius:999px; font-size:11px; font-weight:700; }
	.rep-ok { background:var(--ok-soft); color:var(--ok); }
	.rep-warn { background:var(--warn-soft); color:var(--warn); }

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

	@media (max-width: 860px) {
		.tbl { display:none; }
		.card-list { display:block; }
		.inner-toolbar { flex-direction:column; align-items:flex-start; gap:8px; }
		.right-controls { width:100%; justify-content:space-between; }
		.search { flex:1; }
		.search input { width:100%; }
	}
</style>
