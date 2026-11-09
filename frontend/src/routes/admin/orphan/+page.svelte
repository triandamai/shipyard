<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { toastStore } from '$lib/stores/toast.store';
	import {
		PageHeader,
		StatCard,
		Badge,
		Button,
		DataTable,
		EmptyState,
		InlineAlert,
		ConfirmDialog,
		Spinner,
		SearchInput,
		Tabs,
		type TabItem
	} from '$lib/components/ui';
	import { Trash2, RefreshCw, FolderX, HardDrive, Building2, FolderKanban, Box, Layers } from '@lucide/svelte';

	export type OrphanType = 'org' | 'project' | 'service' | 'volume' | 'directory';

	export interface OrphanResource {
		id: string;
		orphan_type: OrphanType;
		identifier: string;
		path: string;
		size_bytes: number;
		details: string | null;
		detected_at: string;
		updated_at: string;
	}

	interface OrphanListResponse {
		items: OrphanResource[];
		total_count: number;
		total_size_bytes: number;
	}

	let orphans = $state<OrphanResource[]>([]);
	let loading = $state(false);
	let scanning = $state(false);
	let error = $state('');
	let filterType = $state<string>('all');
	let searchQuery = $state('');

	// Delete confirmation modal state
	let deleteTarget = $state<OrphanResource | null>(null);
	let showDeleteConfirm = $state(false);
	let deleting = $state(false);

	// Batch delete confirmation state
	let showBatchDeleteConfirm = $state(false);
	let batchDeleting = $state(false);

	function fmtBytes(bytes: number): string {
		if (bytes === 0) return '0 B';
		const k = 1024;
		const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
	}

	function timeAgo(dateStr: string): string {
		const s = Math.floor((Date.now() - new Date(dateStr).getTime()) / 1000);
		if (s < 60) return 'just now';
		if (s < 3600) return `${Math.floor(s / 60)}m ago`;
		if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
		return `${Math.floor(s / 86400)}d ago`;
	}

	function typeTone(t: OrphanType): 'purple' | 'blue' | 'green' | 'yellow' | 'neutral' {
		switch (t) {
			case 'org': return 'purple';
			case 'project': return 'blue';
			case 'service': return 'green';
			case 'volume': return 'yellow';
			case 'directory': return 'neutral';
			default: return 'neutral';
		}
	}

	function typeLabel(t: OrphanType): string {
		switch (t) {
			case 'org': return 'Organization';
			case 'project': return 'Project';
			case 'service': return 'Service';
			case 'volume': return 'Volume';
			case 'directory': return 'Directory';
			default: return t;
		}
	}

	async function loadOrphans() {
		loading = true;
		error = '';
		try {
			const res = await api.get<OrphanListResponse>('/admin/orphans');
			if (res.data) {
				orphans = res.data.items ?? [];
			} else {
				error = res.error?.message ?? 'Failed to load orphan resources';
			}
		} catch (e: any) {
			error = e.message ?? 'Failed to load orphan resources';
		} finally {
			loading = false;
		}
	}

	async function triggerScan() {
		scanning = true;
		error = '';
		try {
			const res = await api.post<OrphanListResponse>('/admin/orphans/scan');
			if (res.data) {
				orphans = res.data.items ?? [];
				toastStore.add({
					type: 'success',
					title: 'Scan completed',
					message: `Found ${res.data.total_count} orphan items (${fmtBytes(res.data.total_size_bytes)})`
				});
			} else {
				error = res.error?.message ?? 'Scan failed';
				toastStore.add({ type: 'error', title: 'Scan failed', message: error });
			}
		} catch (e: any) {
			error = e.message ?? 'Scan failed';
			toastStore.add({ type: 'error', title: 'Scan error', message: error });
		} finally {
			scanning = false;
		}
	}

	function askDelete(item: OrphanResource) {
		deleteTarget = item;
		showDeleteConfirm = true;
	}

	async function confirmDelete() {
		if (!deleteTarget) return;
		deleting = true;
		try {
			const res = await api.delete(`/admin/orphans/${deleteTarget.id}`);
			if (res.error) {
				toastStore.add({ type: 'error', title: 'Deletion failed', message: res.error.message });
			} else {
				toastStore.add({
					type: 'success',
					title: 'Orphan deleted',
					message: `Cleaned ${deleteTarget.identifier}`
				});
				orphans = orphans.filter((o) => o.id !== deleteTarget!.id);
			}
		} catch (e: any) {
			toastStore.add({ type: 'error', title: 'Deletion error', message: e.message });
		} finally {
			deleting = false;
			showDeleteConfirm = false;
			deleteTarget = null;
		}
	}

	async function confirmBatchDelete() {
		const targetIds = filteredOrphans.map((o) => o.id);
		if (targetIds.length === 0) return;
		batchDeleting = true;
		try {
			const res = await api.post<{ deleted_count: number }>('/admin/orphans/delete-batch', {
				ids: targetIds
			});
			if (res.error) {
				toastStore.add({ type: 'error', title: 'Batch deletion failed', message: res.error.message });
			} else {
				const count = res.data?.deleted_count ?? targetIds.length;
				toastStore.add({
					type: 'success',
					title: 'Orphans cleaned',
					message: `Successfully deleted ${count} orphan resource${count !== 1 ? 's' : ''}`
				});
				await loadOrphans();
			}
		} catch (e: any) {
			toastStore.add({ type: 'error', title: 'Batch deletion error', message: e.message });
		} finally {
			batchDeleting = false;
			showBatchDeleteConfirm = false;
		}
	}

	// Filtered list derived from search and type filter
	let filteredOrphans = $derived(
		orphans.filter((item) => {
			if (filterType !== 'all' && item.orphan_type !== filterType) {
				return false;
			}
			if (searchQuery.trim()) {
				const q = searchQuery.toLowerCase();
				return (
					item.identifier.toLowerCase().includes(q) ||
					item.path.toLowerCase().includes(q) ||
					(item.details && item.details.toLowerCase().includes(q))
				);
			}
			return true;
		})
	);

	// Summary statistics
	let totalBytes = $derived(orphans.reduce((acc, curr) => acc + curr.size_bytes, 0));
	let orgCount = $derived(orphans.filter((o) => o.orphan_type === 'org').length);
	let projectCount = $derived(orphans.filter((o) => o.orphan_type === 'project').length);
	let serviceCount = $derived(orphans.filter((o) => o.orphan_type === 'service').length);
	let volumeCount = $derived(orphans.filter((o) => o.orphan_type === 'volume').length);
	let directoryCount = $derived(orphans.filter((o) => o.orphan_type === 'directory').length);

	let tabs: TabItem[] = $derived([
		{ id: 'all', label: `All (${orphans.length})` },
		{ id: 'org', label: `Organizations (${orgCount})` },
		{ id: 'project', label: `Projects (${projectCount})` },
		{ id: 'service', label: `Services (${serviceCount})` },
		{ id: 'volume', label: `Volumes (${volumeCount})` },
		{ id: 'directory', label: `Directories (${directoryCount})` }
	]);

	onMount(loadOrphans);
</script>

<div class="orphan-page">
	<PageHeader
		title="Orphan Resources"
		description="Unused directories, orgs, projects, services, and volumes under /shipyard. Review and decide whether to delete them."
	>
		{#snippet actions()}
			<Button
				variant="secondary"
				size="sm"
				disabled={scanning || loading}
				onclick={triggerScan}
			>
				{#if scanning}
					<Spinner size={13} />
					<span>Scanning…</span>
				{:else}
					<RefreshCw size={13} />
					<span>Scan /shipyard</span>
				{/if}
			</Button>

			{#if filteredOrphans.length > 0}
				<Button
					variant="danger"
					size="sm"
					disabled={batchDeleting || scanning}
					onclick={() => (showBatchDeleteConfirm = true)}
				>
					<Trash2 size={13} />
					<span>Delete Filtered ({filteredOrphans.length})</span>
				</Button>
			{/if}
		{/snippet}
	</PageHeader>

	{#if error}
		<div class="alert-wrap">
			<InlineAlert tone="error" message={error} />
		</div>
	{/if}

	<!-- ── Summary Stats ── -->
	<div class="stats-grid">
		<StatCard
			value={orphans.length}
			label="Total Orphans"
			tone={orphans.length > 0 ? 'yellow' : 'neutral'}
		>
			{#snippet icon()}<FolderX size={18} />{/snippet}
		</StatCard>
		<StatCard
			value={fmtBytes(totalBytes)}
			label="Reclaimable Space"
			tone={totalBytes > 0 ? 'yellow' : 'neutral'}
		>
			{#snippet icon()}<HardDrive size={18} />{/snippet}
		</StatCard>
		<StatCard
			value={`${serviceCount} svc / ${volumeCount} vol`}
			label="Services & Volumes"
			tone="neutral"
		>
			{#snippet icon()}<Box size={18} />{/snippet}
		</StatCard>
		<StatCard
			value={`${orgCount} org / ${projectCount} proj`}
			label="Orgs & Projects"
			tone="neutral"
		>
			{#snippet icon()}<Building2 size={18} />{/snippet}
		</StatCard>
	</div>

	<!-- ── Filters & Search ── -->
	<div class="filter-bar">
		<div class="tabs-wrap">
			<Tabs
				{tabs}
				value={filterType}
				onChange={(val) => (filterType = val)}
				ariaLabel="Filter by orphan type"
			/>
		</div>
		<div class="search-wrap">
			<SearchInput bind:value={searchQuery} placeholder="Search path, identifier, reason…" />
		</div>
	</div>

	<!-- ── Orphan List Table ── -->
	{#if loading && orphans.length === 0}
		<div class="loading-state">
			<Spinner size={24} />
			<p>Scanning /shipyard directory for orphans…</p>
		</div>
	{:else if filteredOrphans.length === 0}
		<EmptyState
			message={searchQuery || filterType !== 'all'
				? 'No orphan resources match your filter.'
				: 'No orphan resources detected. Your Shipyard directories and volumes are clean!'}
		>
			{#snippet icon()}<FolderX size={32} />{/snippet}
		</EmptyState>
	{:else}
		<DataTable
			items={filteredOrphans}
			rowKey={(item: OrphanResource) => item.id}
			searchable={false}
			columns={[
				{ key: 'orphan_type', label: 'Type', width: '130px' },
				{ key: 'identifier', label: 'Resource / Identifier', width: '220px' },
				{ key: 'path', label: 'Host Path' },
				{ key: 'size_bytes', label: 'Size', width: '100px' },
				{ key: 'details', label: 'Details / Reason' },
				{ key: 'detected_at', label: 'Detected', width: '110px' },
				{ key: 'actions', label: '', width: '80px' }
			]}
		>
			{#snippet row(item: OrphanResource)}
				<tr class="orphan-row">
					<td>
						<Badge tone={typeTone(item.orphan_type)}>{typeLabel(item.orphan_type)}</Badge>
					</td>
					<td>
						<div class="ident-cell">
							<span class="ident-text font-mono" title={item.identifier}>{item.identifier}</span>
						</div>
					</td>
					<td>
						<div class="path-cell">
							<code class="path-text font-mono" title={item.path}>{item.path}</code>
						</div>
					</td>
					<td>
						<span class="size-text font-mono">{fmtBytes(item.size_bytes)}</span>
					</td>
					<td>
						<span class="details-text" title={item.details ?? ''}>{item.details || '—'}</span>
					</td>
					<td>
						<span class="muted-text">{timeAgo(item.detected_at)}</span>
					</td>
					<td class="action-cell">
						<Button
							variant="danger-outline"
							size="icon"
							title={`Delete ${item.identifier}`}
							aria-label={`Delete ${item.identifier}`}
							onclick={() => askDelete(item)}
						>
							<Trash2 size={13} />
						</Button>
					</td>
				</tr>
			{/snippet}
		</DataTable>
	{/if}
</div>

<!-- ── Single Deletion Confirmation Dialog ── -->
<ConfirmDialog
	open={showDeleteConfirm}
	title="Delete Orphan Resource"
	message={`Permanently remove "${deleteTarget?.identifier}" from disk?\nPath: ${deleteTarget?.path}\nReclaimed size: ${fmtBytes(deleteTarget?.size_bytes ?? 0)}.\n\nThis cannot be undone.`}
	confirmLabel="Delete Permanently"
	tone="danger"
	loading={deleting}
	onConfirm={confirmDelete}
	onCancel={() => {
		showDeleteConfirm = false;
		deleteTarget = null;
	}}
/>

<!-- ── Batch Deletion Confirmation Dialog ── -->
<ConfirmDialog
	open={showBatchDeleteConfirm}
	title="Batch Delete Orphans"
	message={`Are you sure you want to permanently delete all ${filteredOrphans.length} filtered orphan resources from disk? This will reclaim ${fmtBytes(filteredOrphans.reduce((a, b) => a + b.size_bytes, 0))}.\n\nThis cannot be undone.`}
	confirmLabel={`Delete ${filteredOrphans.length} Resources`}
	tone="danger"
	loading={batchDeleting}
	onConfirm={confirmBatchDelete}
	onCancel={() => (showBatchDeleteConfirm = false)}
/>

<style>
	.orphan-page {
		display: flex;
		flex-direction: column;
		gap: 20px;
		padding: 24px 32px;
		max-width: 1400px;
		margin: 0 auto;
		width: 100%;
		box-sizing: border-box;
	}

	.alert-wrap {
		width: 100%;
	}

	.stats-grid {
		display: grid;
		grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
		gap: 12px;
	}

	.filter-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		flex-wrap: wrap;
	}

	.tabs-wrap {
		flex: 1;
		min-width: 300px;
	}

	.search-wrap {
		width: 320px;
		max-width: 100%;
	}

	.loading-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		padding: 64px 20px;
		color: var(--text-muted);
		font-size: 13px;
	}

	.orphan-row td {
		vertical-align: middle;
	}

	.ident-cell {
		max-width: 220px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ident-text {
		font-size: 12px;
		color: var(--text-primary);
		font-weight: 500;
	}

	.path-cell {
		max-width: 340px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.path-text {
		font-size: 11.5px;
		background: var(--bg-elevated);
		padding: 2px 6px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		color: var(--text-secondary);
	}

	.size-text {
		font-size: 12px;
		color: var(--text-secondary);
	}

	.details-text {
		font-size: 12px;
		color: var(--text-dim);
		display: -webkit-box;
		-webkit-line-clamp: 2;
		-webkit-box-orient: vertical;
		overflow: hidden;
		line-height: 1.4;
	}

	.muted-text {
		font-size: 12px;
		color: var(--text-muted);
		white-space: nowrap;
	}

	.action-cell {
		text-align: right;
	}

	.font-mono {
		font-family: var(--font-mono);
	}

	@media (max-width: 768px) {
		.orphan-page {
			padding: 16px;
		}
		.filter-bar {
			flex-direction: column;
			align-items: stretch;
		}
		.search-wrap {
			width: 100%;
		}
	}
</style>
