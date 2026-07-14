<script lang="ts">
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { Package, ChevronRight, RefreshCw, FolderOpen, Tag } from '@lucide/svelte';
	import { onMount } from 'svelte';

	let orgId   = $derived($orgStore.activeOrg?.id ?? '');
	let orgSlug = $derived(page.params.orgSlug ?? '');
	let nsId    = $derived(page.params.nsId ?? '');

	type Repo = {
		repo: string;
		kind: string;
		tag_count: number;
		total_size: number;
		last_pushed: string | null;
	};

	let nsSlug  = $state('');
	let repos: Repo[] = $state([]);
	let loading = $state(false);

	async function load() {
		if (!orgId || !nsId) return;
		loading = true;

		// Load namespace slug for display
		const nsRes = await api.get(`/orgs/${orgId}/registry/namespaces?page=1&per_page=200`);
		const nsList = nsRes.data?.items ?? nsRes.data ?? [];
		const ns = nsList.find((n: { id: string; slug: string }) => n.id === nsId);
		if (ns) nsSlug = ns.slug;

		const res = await api.get(`/orgs/${orgId}/registry/namespaces/${nsId}/repos`);
		repos   = res.data ?? [];
		loading = false;
	}

	onMount(load);
	$effect(() => { if (orgId && nsId) load(); });

	const kindMeta: Record<string, { label: string; color: string }> = {
		docker_image:  { label: 'Image',   color: '#3b82f6' },
		static_bundle: { label: 'Static',  color: '#8b5cf6' },
		edge_function: { label: 'Edge Fn', color: '#f59e0b' },
		build_cache:   { label: 'Cache',   color: '#6b7280' },
	};
	function kindLabel(k: string) { return kindMeta[k]?.label ?? k; }
	function kindColor(k: string) { return kindMeta[k]?.color ?? '#6b7280'; }

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

	function repoHref(repo: string) {
		return `/orgs/${orgSlug}/registry/${nsId}/${encodeURIComponent(repo)}`;
	}
</script>

<div class="browser">
	<div class="topbar">
		<nav class="breadcrumb" aria-label="Registry navigation">
			<a class="bc-item" href="/orgs/{orgSlug}/registry">
				<FolderOpen size={13} /> Namespaces
			</a>
			<ChevronRight size={12} class="bc-sep" />
			<span class="bc-item bc-active">{nsSlug || nsId}</span>
		</nav>
		<button class="icon-btn" onclick={load} title="Refresh" aria-label="Refresh">
			<RefreshCw size={13} />
		</button>
	</div>

	{#if loading}
		<div class="skel-list">
			{#each [1,2,3] as _}<div class="skel"></div>{/each}
		</div>
	{:else if repos.length === 0}
		<div class="empty">
			<Package size={32} />
			<p>No repositories in this namespace.</p>
		</div>
	{:else}
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr>
						<th>Repository</th>
						<th>Kind</th>
						<th class="num-col">Tags</th>
						<th class="num-col">Size</th>
						<th class="num-col">Last pushed</th>
						<th class="action-col"></th>
					</tr>
				</thead>
				<tbody>
					{#each repos as repo}
						<tr class="clickable" role="button" tabindex="0"
							onclick={() => (location.href = repoHref(repo.repo))}
							onkeydown={(e) => e.key === 'Enter' && (location.href = repoHref(repo.repo))}>
							<td>
								<div class="repo-name">
									<div class="repo-icon" style="color:{kindColor(repo.kind)}">
										<Tag size={13} />
									</div>
									<span class="mono">{repo.repo}</span>
								</div>
							</td>
							<td>
								<span class="kind-badge" style="--kc:{kindColor(repo.kind)}">
									{kindLabel(repo.kind)}
								</span>
							</td>
							<td class="num-col muted">{repo.tag_count}</td>
							<td class="num-col muted">{fmtBytes(repo.total_size)}</td>
							<td class="num-col muted">{timeAgo(repo.last_pushed)}</td>
							<td class="action-col"><ChevronRight size={14} class="row-arrow" /></td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>
	{/if}
</div>

<style>
.browser { padding: 20px 32px 40px; display: flex; flex-direction: column; gap: 16px; max-width: 1000px; }
.topbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.breadcrumb { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
.bc-item { display: flex; align-items: center; gap: 5px; font-size: 13px; font-weight: 500; color: var(--text-muted); padding: 3px 5px; border-radius: 5px; text-decoration: none; background: none; border: none; cursor: pointer; transition: color var(--transition-fast), background var(--transition-fast); }
.bc-item:hover { color: var(--text-primary); background: var(--surface-2); }
.bc-item.bc-active { color: var(--text-primary); cursor: default; }
.bc-item.bc-active:hover { background: none; }
:global(.bc-sep) { color: var(--border); flex-shrink: 0; }
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
.repo-name { display: flex; align-items: center; gap: 8px; }
.repo-icon { width: 26px; height: 26px; border-radius: 6px; display: flex; align-items: center; justify-content: center; background: var(--surface-2); flex-shrink: 0; }
.kind-badge { display: inline-flex; align-items: center; font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: 999px; background: color-mix(in srgb, var(--kc) 12%, transparent); color: var(--kc); border: 1px solid color-mix(in srgb, var(--kc) 25%, transparent); white-space: nowrap; }
.empty { display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 60px 20px; color: var(--text-muted); background: var(--surface); border: 1px dashed var(--border); border-radius: 10px; text-align: center; }
.empty p { font-size: 14px; font-weight: 600; margin: 0; color: var(--text-primary); }
.skel-list { display: flex; flex-direction: column; gap: 6px; }
.skel { height: 44px; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; animation: pulse 1.5s ease-in-out infinite; }
@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:0.4} }
</style>
