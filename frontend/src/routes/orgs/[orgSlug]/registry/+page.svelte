<script lang="ts">
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { ChevronRight, RefreshCw, FolderOpen } from '@lucide/svelte';
	import { Button, DataTable } from '$lib/components/ui';

	let orgId   = $derived($orgStore.activeOrg?.id ?? '');
	let orgSlug = $derived(page.params.orgSlug ?? '');

	type Namespace = {
		id: string;
		slug: string;
		artifact_count: number;
		total_size: number;
		last_pushed: string | null;
	};

	let refreshKey = $state(0);

	async function fetchNamespaces({ page: pageIdx, pageSize }: { page: number; pageSize: number; search: string }) {
		const res = await api.get(`/orgs/${orgId}/registry/namespaces?page=${pageIdx + 1}&per_page=${pageSize}`);
		const rows: Namespace[] = res.data?.items ?? res.data ?? [];
		return { rows, total: res.data?.total ?? rows.length };
	}

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
		<Button variant="secondary" size="icon" title="Refresh" aria-label="Refresh" onclick={() => refreshKey++}>
			<RefreshCw size={13} />
		</Button>
	</div>

	{#if orgId}
		{#key `${orgId}:${refreshKey}`}
			<DataTable
				fetchPage={fetchNamespaces}
				rowKey={(ns: Namespace) => ns.id}
				searchable={false}
				pageSize={20}
				columns={[
					{ key: 'slug', label: 'Namespace' },
					{ key: 'artifact_count', label: 'Artifacts' },
					{ key: 'total_size', label: 'Size' },
					{ key: 'last_pushed', label: 'Last pushed' },
					{ key: 'go', label: '', width: '32px' }
				]}
				emptyMessage="No namespaces yet. Deploy a project or push an image to create your first namespace."
			>
				{#snippet row(ns: Namespace)}
					<tr class="clickable" role="button" tabindex="0"
						onclick={() => (location.href = `/orgs/${orgSlug}/registry/${ns.id}`)}
						onkeydown={(e) => e.key === 'Enter' && (location.href = `/orgs/${orgSlug}/registry/${ns.id}`)}>
						<td>
							<div class="ns-name">
								<div class="ns-icon"><FolderOpen size={13} /></div>
								<span class="mono">{ns.slug}</span>
							</div>
						</td>
						<td class="muted">{ns.artifact_count}</td>
						<td class="muted">{fmtBytes(ns.total_size)}</td>
						<td class="muted">{timeAgo(ns.last_pushed)}</td>
						<td class="action-col"><ChevronRight size={14} class="row-arrow" /></td>
					</tr>
				{/snippet}
			</DataTable>
		{/key}
	{/if}
</div>

<style>
	.browser { padding: 20px 32px 40px; display: flex; flex-direction: column; gap: 16px; max-width: 1000px; }
	.topbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
	.breadcrumb { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
	.bc-item { display: flex; align-items: center; gap: 5px; font-size: 13px; font-weight: 500; color: var(--text-muted); padding: 3px 5px; border-radius: var(--radius-sm); }
	.bc-item.bc-active { color: var(--text-primary); }
	.clickable { cursor: pointer; }
	:global(.row-arrow) { color: var(--text-muted); }
	.action-col { text-align: right; }
	.muted { color: var(--text-muted); font-size: 12px; white-space: nowrap; }
	.mono { font-family: var(--font-mono); font-size: 12px; color: var(--text-primary); }
	.ns-name { display: flex; align-items: center; gap: 8px; }
	.ns-icon { width: 26px; height: 26px; border-radius: var(--radius-sm); display: flex; align-items: center; justify-content: center; background: var(--bg-elevated); color: var(--text-muted); flex-shrink: 0; }
</style>
