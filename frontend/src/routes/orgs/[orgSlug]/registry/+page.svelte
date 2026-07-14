<script lang="ts">
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { Package, ChevronRight, RefreshCw, FolderOpen } from '@lucide/svelte';
	import { onMount } from 'svelte';

	let orgId   = $derived($orgStore.activeOrg?.id ?? '');
	let orgSlug = $derived(page.params.orgSlug ?? '');

	type Namespace = {
		id: string;
		slug: string;
		artifact_count: number;
		total_size: number;
		last_pushed: string | null;
	};

	let namespaces: Namespace[] = $state([]);
	let loading = $state(false);
	let page_n  = $state(1);
	let perPage = $state(20);
	let total   = $state(0);

	async function load() {
		if (!orgId) return;
		loading = true;
		const res = await api.get(`/orgs/${orgId}/registry/namespaces?page=${page_n}&per_page=${perPage}`);
		namespaces = res.data?.items ?? res.data ?? [];
		total      = res.data?.total ?? namespaces.length;
		loading    = false;
	}

	onMount(load);
	$effect(() => { if (orgId) load(); });

	let pageCount = $derived(Math.max(1, Math.ceil(total / perPage)));

	function fmtBytes(n: number) {
		if (!n) return '0 B';
		if (n < 1024) return `${n} B`;
		if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
		if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
		return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
	}

	function timeAgo(d: string | null) {
		if (!d) return '—';
		const s = Math.floor((Date.now() - new Date(d).getTime()) / 1000);
		if (s < 60)    return 'just now';
		if (s < 3600)  return `${Math.floor(s / 60)}m ago`;
		if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
		return `${Math.floor(s / 86400)}d ago`;
	}
</script>

<div class="browser">
	<div class="topbar">
		<nav class="breadcrumb" aria-label="Registry navigation">
			<span class="bc-item bc-active"><FolderOpen size={13} /> Namespaces</span>
		</nav>
		<button class="icon-btn" onclick={load} title="Refresh" aria-label="Refresh">
			<RefreshCw size={13} />
		</button>
	</div>

	{#if loading}
		<div class="skel-list">
			{#each [1,2,3,4,5] as _}<div class="skel"></div>{/each}
		</div>
	{:else if namespaces.length === 0}
		<div class="empty">
			<Package size={32} />
			<p>No namespaces yet.</p>
			<span>Deploy a project or push an image to create your first namespace.</span>
		</div>
	{:else}
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr>
						<th>Namespace</th>
						<th class="num-col">Artifacts</th>
						<th class="num-col">Size</th>
						<th class="num-col">Last pushed</th>
						<th class="action-col"></th>
					</tr>
				</thead>
				<tbody>
					{#each namespaces as ns}
						<tr class="clickable" role="button" tabindex="0"
							onclick={() => (location.href = `/orgs/${orgSlug}/registry/${ns.id}`)}
							onkeydown={(e) => e.key === 'Enter' && (location.href = `/orgs/${orgSlug}/registry/${ns.id}`)}>
							<td>
								<div class="ns-name">
									<div class="ns-icon"><FolderOpen size={13} /></div>
									<span class="mono">{ns.slug}</span>
								</div>
							</td>
							<td class="num-col muted">{ns.artifact_count}</td>
							<td class="num-col muted">{fmtBytes(ns.total_size)}</td>
							<td class="num-col muted">{timeAgo(ns.last_pushed)}</td>
							<td class="action-col"><ChevronRight size={14} class="row-arrow" /></td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		{#if pageCount > 1}
			<div class="pagination">
				<button class="page-btn" disabled={page_n <= 1}
					onclick={() => { page_n--; load(); }}>← Prev</button>
				<span class="page-info">Page {page_n} of {pageCount}</span>
				<button class="page-btn" disabled={page_n >= pageCount}
					onclick={() => { page_n++; load(); }}>Next →</button>
			</div>
		{/if}
	{/if}
</div>

<style>
.browser { padding: 20px 32px 40px; display: flex; flex-direction: column; gap: 16px; max-width: 1000px; }
.topbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.breadcrumb { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
.bc-item { display: flex; align-items: center; gap: 5px; font-size: 13px; font-weight: 500; color: var(--text-muted); padding: 3px 5px; border-radius: 5px; }
.bc-item.bc-active { color: var(--text-primary); }
.icon-btn { display: flex; align-items: center; justify-content: center; width: 30px; height: 30px; border-radius: 7px; border: 1px solid var(--border); background: var(--surface); color: var(--text-muted); cursor: pointer; flex-shrink: 0; transition: background var(--transition-fast), color var(--transition-fast); }
.icon-btn:hover { background: var(--surface-2); color: var(--text-primary); }
.table-wrap { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; }
.table { width: 100%; border-collapse: collapse; font-size: 13px; }
.table th { background: var(--surface-2); border-bottom: 1px solid var(--border); padding: 8px 14px; text-align: left; font-size: 11px; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; white-space: nowrap; }
.table td { padding: 10px 14px; border-bottom: 1px solid var(--border); color: var(--text-primary); vertical-align: middle; }
.table tr:last-child td { border-bottom: none; }
.table tr.clickable { cursor: pointer; }
.table tr.clickable:hover td { background: var(--surface-2); }
:global(.row-arrow) { color: var(--text-muted); }
.num-col { text-align: right; white-space: nowrap; }
.action-col { width: 32px; text-align: right; }
.muted { color: var(--text-muted); font-size: 12px; }
.mono { font-family: var(--font-mono, monospace); font-size: 12px; }
.ns-name { display: flex; align-items: center; gap: 8px; }
.ns-icon { width: 26px; height: 26px; border-radius: 6px; display: flex; align-items: center; justify-content: center; background: var(--surface-2); color: var(--text-muted); flex-shrink: 0; }
.pagination { display: flex; align-items: center; justify-content: center; gap: 12px; }
.page-btn { font-size: 12px; font-weight: 500; padding: 5px 14px; border-radius: 7px; border: 1px solid var(--border); background: var(--surface); color: var(--text-muted); cursor: pointer; transition: background var(--transition-fast), color var(--transition-fast); }
.page-btn:hover:not(:disabled) { background: var(--surface-2); color: var(--text-primary); }
.page-btn:disabled { opacity: 0.4; cursor: not-allowed; }
.page-info { font-size: 12px; color: var(--text-muted); }
.empty { display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 60px 20px; color: var(--text-muted); background: var(--surface); border: 1px dashed var(--border); border-radius: 10px; text-align: center; }
.empty p { font-size: 14px; font-weight: 600; margin: 0; color: var(--text-primary); }
.empty span { font-size: 13px; color: var(--text-muted); max-width: 340px; }
.skel-list { display: flex; flex-direction: column; gap: 6px; }
.skel { height: 44px; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; animation: pulse 1.5s ease-in-out infinite; }
@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:0.4} }
</style>
