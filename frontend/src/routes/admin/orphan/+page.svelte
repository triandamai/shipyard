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
		Modal,
		Toggle,
		FormField,
		TextField,
		type TabItem
	} from '$lib/components/ui';
	import {
		Trash2,
		RefreshCw,
		FolderX,
		HardDrive,
		Building2,
		FolderKanban,
		Box,
		Layers,
		GitBranch,
		FileText,
		Clock,
		Settings,
		Archive
	} from '@lucide/svelte';

	export type OrphanType = 'org' | 'project' | 'service' | 'volume' | 'directory' | 'git' | 'file' | 'static_file' | 'registry';

	export interface CleanupSettings {
		orphan_retention_days: number;
		orphan_auto_destruct_enabled: boolean;
		registry_unused_retention_days: number;
		registry_auto_removal_enabled: boolean;
	}

	export interface OrphanResource {
		id: string;
		orphan_type: OrphanType;
		identifier: string;
		path: string;
		size_bytes: number;
		details: string | null;
		detected_at: string;
		auto_destruct_at: string;
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

	function autoDestructCountdown(destructDateStr: string): { label: string; tone: 'danger' | 'yellow' | 'neutral'; expired: boolean } {
		const ms = new Date(destructDateStr).getTime() - Date.now();
		if (ms <= 0) {
			return { label: 'Expired', tone: 'danger', expired: true };
		}
		const hours = Math.floor(ms / (1000 * 60 * 60));
		const days = Math.floor(hours / 24);
		if (days > 1) {
			return { label: `${days}d left`, tone: days <= 3 ? 'danger' : 'neutral', expired: false };
		}
		if (days === 1) {
			return { label: '1d left', tone: 'yellow', expired: false };
		}
		if (hours > 0) {
			return { label: `${hours}h left`, tone: 'danger', expired: false };
		}
		const mins = Math.floor(ms / (1000 * 60));
		return { label: `${mins}m left`, tone: 'danger', expired: false };
	}

	function typeTone(t: OrphanType): 'purple' | 'blue' | 'green' | 'yellow' | 'neutral' | 'red' {
		switch (t) {
			case 'org': return 'purple';
			case 'project': return 'blue';
			case 'service': return 'green';
			case 'volume': return 'yellow';
			case 'git': return 'purple';
			case 'static_file': return 'blue';
			case 'file': return 'yellow';
			case 'directory': return 'neutral';
			case 'registry': return 'red';
			default: return 'neutral';
		}
	}

	function typeLabel(t: OrphanType): string {
		switch (t) {
			case 'org': return 'Organization';
			case 'project': return 'Project';
			case 'service': return 'Service';
			case 'volume': return 'Volume';
			case 'git': return 'Git Repo';
			case 'static_file': return 'Static File';
			case 'file': return 'File';
			case 'directory': return 'Directory';
			case 'registry': return 'Registry Image';
			default: return t;
		}
	}

	// Retention & cleanup settings state
	let settings = $state<CleanupSettings>({
		orphan_retention_days: 30,
		orphan_auto_destruct_enabled: true,
		registry_unused_retention_days: 90,
		registry_auto_removal_enabled: true
	});
	let showSettingsModal = $state(false);
	let savingSettings = $state(false);
	let cleaningRegistry = $state(false);

	let editOrphanRetention = $state('30');
	let editOrphanDestructEnabled = $state(true);
	let editRegistryRetention = $state('90');
	let editRegistryRemovalEnabled = $state(true);

	function openSettingsModal() {
		editOrphanRetention = String(settings.orphan_retention_days);
		editOrphanDestructEnabled = settings.orphan_auto_destruct_enabled;
		editRegistryRetention = String(settings.registry_unused_retention_days);
		editRegistryRemovalEnabled = settings.registry_auto_removal_enabled;
		showSettingsModal = true;
	}

	async function loadSettings() {
		try {
			const res = await api.get<CleanupSettings>('/admin/orphans/settings');
			if (res.data) {
				settings = res.data;
			}
		} catch (e: any) {
			console.error('Failed to load cleanup settings', e);
		}
	}

	async function saveSettings() {
		const orphanDays = parseInt(editOrphanRetention, 10);
		const regDays = parseInt(editRegistryRetention, 10);

		if (isNaN(orphanDays) || orphanDays < 1) {
			toastStore.add({ type: 'error', title: 'Invalid input', message: 'Orphan retention days must be at least 1 day' });
			return;
		}
		if (isNaN(regDays) || regDays < 1) {
			toastStore.add({ type: 'error', title: 'Invalid input', message: 'Registry retention days must be at least 1 day' });
			return;
		}

		savingSettings = true;
		try {
			const payload: CleanupSettings = {
				orphan_retention_days: orphanDays,
				orphan_auto_destruct_enabled: editOrphanDestructEnabled,
				registry_unused_retention_days: regDays,
				registry_auto_removal_enabled: editRegistryRemovalEnabled
			};
			const res = await api.put<CleanupSettings>('/admin/orphans/settings', payload);
			if (res.error) {
				toastStore.add({ type: 'error', title: 'Settings save failed', message: res.error.message });
			} else {
				if (res.data) settings = res.data;
				toastStore.add({
					type: 'success',
					title: 'Settings saved',
					message: 'Cleanup & retention settings updated successfully'
				});
				showSettingsModal = false;
				await loadOrphans();
			}
		} catch (e: any) {
			toastStore.add({ type: 'error', title: 'Settings error', message: e.message });
		} finally {
			savingSettings = false;
		}
	}

	async function triggerRegistryCleanup() {
		cleaningRegistry = true;
		try {
			const res = await api.post<{
				unused_artifacts_deleted: number;
				manifests_deleted: number;
				blobs_deleted: number;
				freed_blob_bytes: number;
			}>('/admin/orphans/cleanup-registry');
			if (res.error) {
				toastStore.add({ type: 'error', title: 'Registry cleanup failed', message: res.error.message });
			} else {
				const d = res.data;
				toastStore.add({
					type: 'success',
					title: 'Registry cleaned',
					message: `Deleted ${d?.unused_artifacts_deleted ?? 0} unused artifacts, freed ${fmtBytes(d?.freed_blob_bytes ?? 0)}`
				});
				await loadOrphans();
			}
		} catch (e: any) {
			toastStore.add({ type: 'error', title: 'Registry cleanup error', message: e.message });
		} finally {
			cleaningRegistry = false;
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
	let gitCount = $derived(orphans.filter((o) => o.orphan_type === 'git').length);
	let staticFileCount = $derived(orphans.filter((o) => o.orphan_type === 'static_file').length);
	let fileCount = $derived(orphans.filter((o) => o.orphan_type === 'file').length);
	let directoryCount = $derived(orphans.filter((o) => o.orphan_type === 'directory').length);
	let registryCount = $derived(orphans.filter((o) => o.orphan_type === 'registry').length);
	let expiringSoonCount = $derived(
		orphans.filter((o) => {
			const ms = new Date(o.auto_destruct_at).getTime() - Date.now();
			return ms <= 3 * 24 * 60 * 60 * 1000;
		}).length
	);

	let tabs: TabItem[] = $derived([
		{ id: 'all', label: `All (${orphans.length})` },
		{ id: 'directory', label: `Directories (${directoryCount})` },
		{ id: 'git', label: `Git (${gitCount})` },
		{ id: 'file', label: `Files (${fileCount})` },
		{ id: 'static_file', label: `Static Files (${staticFileCount})` },
		{ id: 'registry', label: `Registry (${registryCount})` },
		{ id: 'service', label: `Services (${serviceCount})` },
		{ id: 'volume', label: `Volumes (${volumeCount})` },
		{ id: 'org', label: `Orgs (${orgCount})` },
		{ id: 'project', label: `Projects (${projectCount})` }
	]);

	onMount(async () => {
		loadSettings();
		await triggerScan();
	});
</script>

<div class="orphan-page">
	<PageHeader
		title="Orphan Resources"
		description="Unused directories, git repos, static files, volumes, and registry images. Review to delete manually or configure auto-destruct and auto-removal retention."
	>
		{#snippet actions()}
			<Button
				variant="secondary"
				size="sm"
				onclick={openSettingsModal}
			>
				<Settings size={13} />
				<span>Retention Settings</span>
			</Button>

			<Button
				variant="secondary"
				size="sm"
				disabled={cleaningRegistry}
				onclick={triggerRegistryCleanup}
			>
				{#if cleaningRegistry}
					<Spinner size={13} />
					<span>Cleaning Registry…</span>
				{:else}
					<Archive size={13} />
					<span>Clean Unused Registry</span>
				{/if}
			</Button>

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

	<!-- ── Active Retention Policy Banner ── -->
	<div class="retention-banner">
		<div class="retention-banner-info">
			<Clock size={15} />
			<span class="retention-banner-text">
				<strong>Retention Policy:</strong>
				{#if settings.orphan_auto_destruct_enabled}
					Directories & files auto-destruct after <strong>{settings.orphan_retention_days} days</strong>.
				{:else}
					Orphan auto-destruct is disabled.
				{/if}
				{#if settings.registry_auto_removal_enabled}
					Unused registry images auto-pruned after <strong>{settings.registry_unused_retention_days} days (3 months)</strong>.
				{:else}
					Registry auto-removal is disabled.
				{/if}
			</span>
		</div>
		<Button variant="secondary" size="sm" onclick={openSettingsModal}>
			<Settings size={13} />
			<span>Configure</span>
		</Button>
	</div>

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
			value={`${directoryCount} dirs / ${gitCount} git / ${fileCount} files`}
			label="Directories & Files"
			tone="neutral"
		>
			{#snippet icon()}<Layers size={18} />{/snippet}
		</StatCard>
		<StatCard
			value={settings.orphan_auto_destruct_enabled ? `${expiringSoonCount} soon / ${settings.orphan_retention_days}d window` : 'Auto-Destruct Disabled'}
			label="Auto-Destruct Schedule"
			tone={expiringSoonCount > 0 ? 'yellow' : 'neutral'}
		>
			{#snippet icon()}<Clock size={18} />{/snippet}
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
				{ key: 'size_bytes', label: 'Size', width: '90px' },
				{ key: 'details', label: 'Details / Reason' },
				{ key: 'auto_destruct_at', label: 'Auto-Destruct', width: '130px' },
				{ key: 'detected_at', label: 'Detected', width: '100px' },
				{ key: 'actions', label: '', width: '70px' }
			]}
		>
			{#snippet row(item: OrphanResource)}
				{@const countdown = autoDestructCountdown(item.auto_destruct_at)}
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
						<div class="destruct-cell" title={`Permanently deleted on ${new Date(item.auto_destruct_at).toLocaleString()}`}>
							<Badge tone={countdown.tone}>{countdown.label}</Badge>
						</div>
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

<!-- ── Cleanup & Retention Settings Modal ── -->
<Modal
	bind:open={showSettingsModal}
	title="Cleanup & Retention Settings"
>
	<div class="settings-modal-content">
		<p class="settings-desc">
			Configure automatic cleanup and retention schedules for orphan resources and unused Shipyard Registry artifacts. All settings are stored in the database.
		</p>

		<!-- Orphan Auto-Destruct Policy -->
		<div class="settings-card">
			<div class="settings-card-header">
				<div class="settings-card-title">
					<Clock size={16} />
					<span>Orphan Files & Resources Auto-Destruct</span>
				</div>
				<Toggle
					bind:checked={editOrphanDestructEnabled}
					label="Enable orphan auto-destruct"
				/>
			</div>
			<p class="settings-card-desc">
				Permanently delete unattached directories, git repositories, static bundles, and volumes after retention period expires.
			</p>
			{#if editOrphanDestructEnabled}
				<div class="settings-input-group">
					<FormField
						label="Auto-Destruct Retention Period (days)"
						hint="Default: 30 days. Orphan resources older than this threshold will be permanently deleted."
					>
						<TextField
							type="number"
							min="1"
							max="365"
							bind:value={editOrphanRetention}
							placeholder="30"
						/>
					</FormField>
				</div>
			{/if}
		</div>

		<!-- Shipyard Registry Auto-Removal Policy -->
		<div class="settings-card">
			<div class="settings-card-header">
				<div class="settings-card-title">
					<Archive size={16} />
					<span>Shipyard Registry Unused Images Auto-Removal</span>
				</div>
				<Toggle
					bind:checked={editRegistryRemovalEnabled}
					label="Enable unused registry images auto-removal"
				/>
			</div>
			<p class="settings-card-desc">
				Automatically detect and remove container images, edge functions, and build artifacts from the registry when not referenced by any service or container.
			</p>
			{#if editRegistryRemovalEnabled}
				<div class="settings-input-group">
					<FormField
						label="Unused Image Retention Threshold (days)"
						hint="Default: 90 days (3 months). Images not referenced or deployed for this duration will be safely pruned."
					>
						<TextField
							type="number"
							min="1"
							max="730"
							bind:value={editRegistryRetention}
							placeholder="90"
						/>
					</FormField>
				</div>
			{/if}
		</div>
	</div>

	{#snippet footer()}
		<div class="modal-footer">
			<Button variant="secondary" onclick={() => (showSettingsModal = false)}>
				Cancel
			</Button>
			<Button variant="primary" disabled={savingSettings} onclick={saveSettings}>
				{#if savingSettings}
					<Spinner size={13} />
					<span>Saving…</span>
				{:else}
					<span>Save Settings</span>
				{/if}
			</Button>
		</div>
	{/snippet}
</Modal>

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

	.retention-banner {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 12px 16px;
		flex-wrap: wrap;
	}

	.retention-banner-info {
		display: flex;
		align-items: center;
		gap: 10px;
		color: var(--text-secondary);
		font-size: 13px;
	}

	.retention-banner-text {
		color: var(--text-secondary);
		line-height: 1.4;
	}

	.retention-banner-text strong {
		color: var(--text-primary);
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

	.destruct-cell {
		display: inline-flex;
		align-items: center;
	}

	.action-cell {
		text-align: right;
	}

	.font-mono {
		font-family: var(--font-mono);
	}

	.settings-modal-content {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.settings-desc {
		font-size: 13px;
		color: var(--text-secondary);
		line-height: 1.5;
		margin: 0;
	}

	.settings-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.settings-card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.settings-card-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13.5px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.settings-card-desc {
		font-size: 12px;
		color: var(--text-muted);
		line-height: 1.4;
		margin: 0;
	}

	.settings-input-group {
		margin-top: 4px;
	}

	.modal-footer {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 10px;
		width: 100%;
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
