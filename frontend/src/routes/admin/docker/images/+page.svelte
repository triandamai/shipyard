<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface ImageSummary { id: string; tags: string[]; size: number; created: number; }

	let images = $state<ImageSummary[]>([]);
	let loading = $state(false);
	let error = $state('');
	let search = $state('');
	let page = $state(0);
	const LIMIT = 25;

	let pruning = $state(false);
	let pruneMsg = $state('');

	async function load() {
		loading = true; error = '';
		const r = await api.get<ImageSummary[]>('/admin/docker/images');
		if (r.data) images = r.data;
		else error = r.error?.message ?? 'Failed to load images';
		loading = false;
	}

	async function prune() {
		if (!confirm('Prune unused images? This cannot be undone.')) return;
		pruning = true; pruneMsg = '';
		const r = await api.post<{ message: string }>('/admin/docker/prune/images');
		pruneMsg = r.data?.message ?? r.error?.message ?? 'Done';
		pruning = false;
		setTimeout(() => (pruneMsg = ''), 4000);
		await load();
	}

	let filtered = $derived(
		images.filter(i => !search || i.tags.some(t => t.toLowerCase().includes(search.toLowerCase())))
	);

	let totalPages = $derived(Math.ceil(filtered.length / LIMIT));
	let paged = $derived(filtered.slice(page * LIMIT, (page + 1) * LIMIT));

	$effect(() => {
		search;
		page = 0;
	});

	function fmtBytes(bytes: number): string {
		if (bytes === 0) return '0 B';
		const k = 1024;
		const s = ['B','KB','MB','GB','TB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${(bytes / Math.pow(k, i)).toFixed(1)} ${s[i]}`;
	}

	onMount(load);
</script>

<div class="inner-toolbar">
	<button class="prune-btn" disabled={pruning} onclick={prune}>
		<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd"/></svg>
		{pruning ? 'Pruning…' : 'Prune Unused'}
	</button>
	{#if pruneMsg}<span class="prune-msg">{pruneMsg}</span>{/if}

	<div class="right-controls">
		<label class="search">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="12" height="12"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
			<input type="text" placeholder="Search images…" bind:value={search} />
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
	<div class="empty">No images found.</div>
{:else}
	<div class="tbl">
		<div class="thead">
			<span style="flex:1">ID</span>
			<span style="flex:3">Tags</span>
			<span style="flex:0.8">Size</span>
		</div>
		{#each paged as img}
			<div class="trow">
				<div class="mono muted" style="flex:1;font-size:11px">{img.id.replace('sha256:','').slice(0,12)}</div>
				<div class="cell" style="flex:3">{img.tags.join(', ') || '<none>'}</div>
				<div class="cell" style="flex:0.8">{fmtBytes(img.size)}</div>
			</div>
		{/each}
	</div>

	<div class="card-list">
		{#each paged as img}
			<div class="m-card">
				<div class="m-card-title mono" style="font-size:11.5px">{img.id.replace('sha256:','').slice(0,12)}</div>
				<div class="m-card-row"><span class="m-key">Tags</span><span style="font-size:11.5px">{img.tags.join(', ') || '<none>'}</span></div>
				<div class="m-card-row"><span class="m-key">Size</span><span>{fmtBytes(img.size)}</span></div>
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
	.prune-btn { display:flex; align-items:center; gap:5px; padding:5px 11px; height:32px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:600; cursor:pointer; border:1px solid rgba(220,38,38,0.2); background:var(--danger-soft); color:var(--danger); transition:background .15s; font-family:var(--font); }
	.prune-btn:hover:not(:disabled) { background:rgba(220,38,38,0.14); }
	.prune-btn:disabled { opacity:.45; cursor:not-allowed; }
	.prune-msg { font-size:11.5px; color:var(--ok); font-weight:500; }
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
