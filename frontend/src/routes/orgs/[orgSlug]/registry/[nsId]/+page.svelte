<script lang="ts">
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { Package, ChevronRight, RefreshCw, FolderOpen, Tag } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import { Button, Badge, DataTable, EmptyState, Skeleton } from '$lib/components/ui';

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

	const kindMeta: Record<string, { label: string; tone: 'blue' | 'green' | 'yellow' | 'neutral' }> = {
		docker_image:  { label: 'Image',   tone: 'blue'    },
		static_bundle: { label: 'Static',  tone: 'green'   },
		edge_function: { label: 'Edge Fn', tone: 'yellow'  },
		build_cache:   { label: 'Cache',   tone: 'neutral' },
	};
	function kindLabel(k: string) { return kindMeta[k]?.label ?? k; }
	function kindTone(k: string) { return kindMeta[k]?.tone ?? 'neutral'; }

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
		<Button variant="secondary" size="icon" title="Refresh" aria-label="Refresh" onclick={load}>
			<RefreshCw size={13} />
		</Button>
	</div>

	{#if loading}
		<div class="skel-list">
			{#each [1,2,3] as _}<Skeleton variant="row" />{/each}
		</div>
	{:else if repos.length === 0}
		<EmptyState message="No repositories in this namespace.">
			{#snippet icon()}<Package size={32} />{/snippet}
		</EmptyState>
	{:else}
		<DataTable
			items={repos}
			rowKey={(r: Repo) => r.repo}
			searchable={false}
			columns={[
				{ key: 'repo', label: 'Repository' },
				{ key: 'kind', label: 'Kind' },
				{ key: 'tag_count', label: 'Tags' },
				{ key: 'total_size', label: 'Size' },
				{ key: 'last_pushed', label: 'Last pushed' },
				{ key: 'go', label: '', width: '32px' }
			]}
		>
			{#snippet row(repo: Repo)}
				<tr class="clickable" role="button" tabindex="0"
					onclick={() => (location.href = repoHref(repo.repo))}
					onkeydown={(e) => e.key === 'Enter' && (location.href = repoHref(repo.repo))}>
					<td>
						<div class="repo-name">
							<div class="repo-icon"><Tag size={13} /></div>
							<span class="mono">{repo.repo}</span>
						</div>
					</td>
					<td><Badge tone={kindTone(repo.kind)}>{kindLabel(repo.kind)}</Badge></td>
					<td class="muted">{repo.tag_count}</td>
					<td class="muted">{fmtBytes(repo.total_size)}</td>
					<td class="muted">{timeAgo(repo.last_pushed)}</td>
					<td class="action-col"><ChevronRight size={14} class="row-arrow" /></td>
				</tr>
			{/snippet}
		</DataTable>
	{/if}
</div>

<style>
	.browser { padding: 20px 32px 40px; display: flex; flex-direction: column; gap: 16px; max-width: 1000px; }
	.topbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
	.breadcrumb { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
	.bc-item { display: flex; align-items: center; gap: 5px; font-size: 13px; font-weight: 500; color: var(--text-muted); padding: 3px 5px; border-radius: var(--radius-sm); text-decoration: none; transition: color var(--transition-fast), background var(--transition-fast); }
	.bc-item:is(a):hover { color: var(--text-primary); background: var(--bg-hover); }
	.bc-item.bc-active { color: var(--text-primary); cursor: default; }
	:global(.bc-sep) { color: var(--border); flex-shrink: 0; }
	.skel-list { display: flex; flex-direction: column; gap: 6px; }
	.clickable { cursor: pointer; }
	:global(.row-arrow) { color: var(--text-muted); }
	.action-col { text-align: right; }
	.muted { color: var(--text-muted); font-size: 12px; white-space: nowrap; }
	.mono { font-family: var(--font-mono); font-size: 12px; color: var(--text-primary); }
	.repo-name { display: flex; align-items: center; gap: 8px; }
	.repo-icon { width: 26px; height: 26px; border-radius: var(--radius-sm); display: flex; align-items: center; justify-content: center; background: var(--bg-elevated); color: var(--text-muted); flex-shrink: 0; }
</style>
