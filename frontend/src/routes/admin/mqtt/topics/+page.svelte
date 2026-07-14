<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	let topics = $state<any[]>([]);
	let loading = $state(true);
	let error = $state('');
	let search = $state('');
	let page = $state(0);
	const LIMIT = 25;

	async function load() {
		loading = true; error = '';
		const r = await api.get<any>('/admin/mqtt/topics');
		if (r.data) {
			topics = Array.isArray(r.data) ? r.data : (r.data.items ?? r.data.data ?? []);
		} else {
			error = r.error?.message ?? 'Failed to load topics';
		}
		loading = false;
	}

	let filtered = $derived(
		topics.filter(t => {
			const q = search.toLowerCase();
			const topicName = (t.topic ?? t.name ?? '').toLowerCase();
			return !q || topicName.includes(q);
		})
	);

	let totalPages = $derived(Math.ceil(filtered.length / LIMIT));
	let paged = $derived(filtered.slice(page * LIMIT, (page + 1) * LIMIT));

	$effect(() => {
		search;
		page = 0;
	});

	onMount(load);
</script>

<div class="search-wrap">
	<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
	<input class="search-inp" placeholder="Filter topics by name…" bind:value={search} />
</div>

{#if loading}
	<div class="tbl">{#each [0,1,2] as _}<div class="sk-row"><div class="sk" style="width:200px;height:12px"></div></div>{/each}</div>
{:else if error}
	<div class="err">{error}</div>
{:else if paged.length === 0}
	<div class="empty">No topics.</div>
{:else}
	<div class="tbl">
		<div class="thead">
			<span>Topic</span>
		</div>
		{#each paged as t}
			<div class="trow">
				<span class="mono" style="font-size:12px">{t.topic ?? t.name ?? '—'}</span>
			</div>
		{/each}
	</div>

	<div class="card-list">
		{#each paged as t}
			<div class="m-card">
				<div class="m-card-title mono">{t.topic ?? t.name ?? '—'}</div>
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
	.search-wrap { position:relative; display:flex; align-items:center; margin-bottom:12px; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search-inp { height:32px; padding:0 10px 0 28px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:260px; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search-inp::placeholder { color:var(--text-3); }
	.search-inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.mono { font-family:var(--mono); color:var(--text); }
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
	.m-card-title { font-size:13px; font-weight:600; color:var(--text); margin-bottom:8px; font-family:var(--mono); }

	@media (max-width: 680px) {
		.tbl { display:none; }
		.card-list { display:block; }
		.search-wrap { width:100%; }
		.search-inp { width:100%; }
	}
</style>
