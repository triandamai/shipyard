<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface AdminProject {
		id: string;
		name: string;
		slug: string;
		org_id: string;
		org_name: string;
		org_slug: string;
		service_count: number;
		created_at: string;
	}

	let projects = $state<AdminProject[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let search = $state('');

	const PAGE = 25;
	let page = $state(0);
	let total = $state(0);

	onMount(() => load());

	async function load() {
		loading = true;
		error = null;
		const res = await api.get<{ items: AdminProject[]; total: number }>(
			`/admin/projects?page=${page}&limit=${PAGE}&q=${encodeURIComponent(search)}`
		);
		if (res.data) {
			projects = res.data.items ?? [];
			total = res.data.total ?? 0;
		} else {
			error = res.error?.message ?? 'Failed to load';
		}
		loading = false;
	}

	let totalPages = $derived(Math.ceil(total / PAGE));

	function doSearch() {
		page = 0;
		load();
	}

	const palette = ['#1c1c1c','#252525','#1a1f2e','#1e1a2e','#1a2420'];
	function orgColor(name: string): string {
		return palette[name.charCodeAt(0) % palette.length];
	}
	function initials(name: string): string {
		return name.split(/\s+/).slice(0,2).map(w => w[0]?.toUpperCase() ?? '').join('');
	}
</script>

<div class="p">
	<header class="hdr">
		<div class="hdr-l">
			<h1 class="ttl">Projects</h1>
			<span class="pill">{total}</span>
		</div>
		<form onsubmit={(e) => { e.preventDefault(); doSearch(); }} style="display:flex;gap:8px">
			<label class="search">
				<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13">
					<path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/>
				</svg>
				<input type="text" placeholder="Search projects…" bind:value={search} oninput={doSearch} />
			</label>
		</form>
	</header>

	{#if loading}
		<div class="tbl">
			{#each [0,1,2,3] as _}
				<div class="sk-row">
					<div class="sk sk-ava"></div>
					<div style="display:flex;flex-direction:column;gap:6px;flex:1">
						<div class="sk sk-l"></div>
						<div class="sk sk-xs"></div>
					</div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if projects.length === 0}
		<div class="empty">
			<svg viewBox="0 0 20 20" fill="currentColor" width="28" height="28"><path fill-rule="evenodd" d="M2 6a2 2 0 012-2h4l2 2h4a2 2 0 012 2v1H8a3 3 0 00-3 3v1.5a1.5 1.5 0 01-3 0V6z" clip-rule="evenodd"/><path d="M6 12a2 2 0 012-2h8a2 2 0 012 2v2a2 2 0 01-2 2H2h2a2 2 0 002-2v-2z"/></svg>
			{search ? 'No projects match.' : 'No projects found.'}
		</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:2.2">Project</span>
				<span style="flex:1.8">Organization</span>
				<span class="r" style="flex:0.7">Services</span>
				<span style="flex:1">Created</span>
				<span class="r" style="flex:0.8">Actions</span>
			</div>
			{#each projects as proj}
				{@const oc = orgColor(proj.org_name)}
				<div class="trow">
					<div style="flex:2.2;display:flex;flex-direction:column;gap:1px;min-width:0">
						<span class="name">{proj.name}</span>
						<span class="slug">{proj.slug}</span>
					</div>
					<div style="flex:1.8;display:flex;align-items:center;gap:8px">
						<div class="ava" style="background:{oc}">{initials(proj.org_name)}</div>
						<div style="min-width:0">
							<div class="name">{proj.org_name}</div>
							<div class="slug">{proj.org_slug}</div>
						</div>
					</div>
					<div class="n r" style="flex:0.7">{proj.service_count}</div>
					<div class="d" style="flex:1">{new Date(proj.created_at).toLocaleDateString()}</div>
					<div style="flex:0.8;display:flex;justify-content:flex-end">
						<a class="act" href="/orgs/{proj.org_slug}/projects/{proj.slug}" target="_blank" rel="noopener">Open</a>
					</div>
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
	.p { max-width:1080px; margin:0 auto; padding:40px 36px; }

	.hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:20px; gap:12px; flex-wrap:wrap; }
	.hdr-l { display:flex; align-items:center; gap:8px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0; letter-spacing:-0.02em; }
	.pill { display:inline-flex; align-items:center; justify-content:center; height:20px; padding:0 7px; border-radius:999px; font-size:11px; font-weight:700; background:var(--surface-2); color:var(--text-3); border:1px solid var(--border); }

	.search { position:relative; display:flex; align-items:center; cursor:text; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search input { height:32px; padding:0 10px 0 28px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:220px; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search input::placeholder { color:var(--text-3); }
	.search input:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-ava { width:32px; height:32px; border-radius:8px; flex-shrink:0; }
	.sk-l { width:130px; height:12px; }
	.sk-xs { width:80px; height:10px; }
	.sk-row { display:flex; align-items:center; gap:10px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:56px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:0.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:11px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }

	.ava { width:26px; height:26px; border-radius:6px; flex-shrink:0; display:flex; align-items:center; justify-content:center; font-size:9.5px; font-weight:800; color:rgba(255,255,255,0.7); }
	.name { font-size:12.5px; font-weight:600; color:var(--text); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
	.slug { font-size:10.5px; color:var(--text-3); font-family:var(--mono); }
	.n { font-size:12.5px; font-variant-numeric:tabular-nums; color:var(--text-2); }
	.r { text-align:right; }
	.d { font-size:11.5px; color:var(--text-3); white-space:nowrap; }

	.act { padding:4px 11px; height:28px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface-2); color:var(--text-2); text-decoration:none; display:inline-flex; align-items:center; transition:background .15s; font-family:var(--font); }
	.act:hover { background:var(--accent); color:#000; border-color:var(--accent); }

	.pager { display:flex; align-items:center; gap:10px; padding:12px 0 4px; justify-content:center; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }
</style>
