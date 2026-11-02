<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import {
		Building2,
		ArrowLeft,
		Plus,
		Search,
		Unlink,
		Copy,
		Check,
		RefreshCw
	} from '@lucide/svelte';
	import {
		Button,
		Badge,
		Spinner,
		TextField,
		FormField,
		PageHeader,
		EmptyState,
		InlineAlert,
		Modal,
		ConfirmDialog,
		Pagination,
		SearchInput
	} from '$lib/components/ui';
	import api from '$lib/api/client';
	import type { Integration, LinkedOrg, AdminOrg } from '$lib/api/types';

	const integrationId = $derived(page.params.id);

	let integration = $state<Integration | null>(null);
	let items = $state<LinkedOrg[]>([]);
	let allOrgs = $state<AdminOrg[]>([]);
	let total = $state(0);
	let currentPage = $state(0); // 0-indexed for Pagination component
	let pageSize = $state(20);
	let searchQuery = $state('');
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Link Org Modal
	let showLinkModal = $state(false);
	let selectedOrgId = $state('');
	let externalId = $state('');
	let linking = $state(false);
	let linkError = $state('');

	// Unlink Dialog
	let unlinkDialogOpen = $state(false);
	let unlinkTarget = $state<LinkedOrg | null>(null);
	let unlinking = $state(false);

	let copiedId = $state<string | null>(null);

	onMount(() => {
		void loadIntegrationInfo();
		void loadOrgs();
	});

	async function loadIntegrationInfo() {
		const res = await api.getAdminIntegrations();
		if (res.data) {
			const found = res.data.find((i) => i.id === integrationId);
			if (found) {
				integration = found;
			}
		}
		const orgsRes = await api.getAdminOrgs();
		if (orgsRes.data) {
			allOrgs = orgsRes.data;
		}
	}

	async function loadOrgs() {
		loading = true;
		error = null;
		try {
			const res = await api.getAdminIntegrationOrgs(integrationId, {
				page: currentPage + 1, // backend is 1-indexed
				limit: pageSize,
				search: searchQuery.trim() || undefined
			});

			if (res.data) {
				items = res.data.items;
				total = res.data.total;
			} else {
				error = res.error?.message ?? 'Failed to load linked organizations';
			}
		} catch (e: any) {
			error = e?.message ?? 'Failed to load linked organizations';
		} finally {
			loading = false;
		}
	}

	function handleSearch(q: string) {
		searchQuery = q;
		currentPage = 0;
		void loadOrgs();
	}

	function handlePageChange(newPage: number) {
		currentPage = newPage;
		void loadOrgs();
	}

	function handlePageSizeChange(newSize: number) {
		pageSize = newSize;
		currentPage = 0;
		void loadOrgs();
	}

	function openLinkModal() {
		selectedOrgId = '';
		externalId = '';
		linkError = '';
		showLinkModal = true;
	}

	async function handleLinkOrg() {
		if (!selectedOrgId) {
			linkError = 'Please select an organization';
			return;
		}
		linking = true;
		linkError = '';
		try {
			const res = await api.linkAdminIntegrationOrg(
				integrationId,
				selectedOrgId,
				externalId.trim() ? { external_id: externalId.trim() } : {}
			);
			if (res.data) {
				showLinkModal = false;
				await loadOrgs();
				await loadIntegrationInfo();
			} else {
				linkError = res.error?.message ?? 'Failed to link organization';
			}
		} catch (e: any) {
			linkError = e?.message ?? 'Failed to link organization';
		} finally {
			linking = false;
		}
	}

	function openUnlinkDialog(org: LinkedOrg) {
		unlinkTarget = org;
		unlinkDialogOpen = true;
	}

	async function confirmUnlink() {
		if (!unlinkTarget) return;
		unlinking = true;
		try {
			const res = await api.unlinkAdminIntegrationOrg(integrationId, unlinkTarget.id);
			if (res.data) {
				unlinkDialogOpen = false;
				unlinkTarget = null;
				await loadOrgs();
				await loadIntegrationInfo();
			} else {
				error = res.error?.message ?? 'Failed to unlink organization';
			}
		} catch (e: any) {
			error = e?.message ?? 'Failed to unlink organization';
		} finally {
			unlinking = false;
		}
	}

	async function copyToClipboard(text: string, id: string) {
		try {
			await navigator.clipboard.writeText(text);
			copiedId = id;
			setTimeout(() => {
				if (copiedId === id) copiedId = null;
			}, 1800);
		} catch {
			/* clipboard unavailable */
		}
	}

	// Unlinked orgs available for linking
	let linkableOrgs = $derived(
		allOrgs.filter((org) => org.integration_id !== integrationId)
	);
</script>

<div class="page">
	<PageHeader
		eyebrow="Platform / Integrations"
		title={integration ? `Linked Organizations: ${integration.name}` : 'Linked Organizations'}
		subtitle="Organizations linked to this platform integration. Integration keys can orchestrate these orgs."
	>
		{#snippet actions()}
			<div class="header-actions">
				<Button
					variant="secondary"
					size="sm"
					onclick={() => goto('/admin/integrations')}
				>
					<ArrowLeft size={14} />
					Back to Integrations
				</Button>
				<Button
					variant="secondary"
					size="sm"
					onclick={loadOrgs}
					disabled={loading}
					aria-label="Refresh list"
				>
					{#if loading}
						<Spinner size={14} />
					{:else}
						<RefreshCw size={14} />
					{/if}
				</Button>
				<Button variant="primary" size="sm" onclick={openLinkModal}>
					<Plus size={14} />
					Link Organization
				</Button>
			</div>
		{/snippet}
	</PageHeader>

	{#if error}
		<InlineAlert tone="error" title="Error">
			{error}
		</InlineAlert>
	{/if}

	<!-- Controls & Search -->
	<div class="table-controls">
		<div class="search-box">
			<SearchInput
				placeholder="Filter organizations by name, slug, or external ID…"
				value={searchQuery}
				oninput={(e) => handleSearch((e.target as HTMLInputElement).value)}
			/>
		</div>
		<div class="count-badge">
			<Badge tone="neutral">{total} {total === 1 ? 'organization' : 'organizations'}</Badge>
		</div>
	</div>

	<!-- Table Area -->
	<div class="table-container">
		{#if loading && items.length === 0}
			<div class="loading-state">
				<Spinner size={28} />
			</div>
		{:else if items.length === 0}
			<div class="empty-state-wrap">
				<EmptyState
					message={searchQuery ? 'No matching organizations' : 'No organizations linked'}
					sub={searchQuery ? 'Try adjusting your search filter.' : 'Link an existing organization so this integration’s keys can orchestrate its apps, databases, and environments.'}
				>
					{#snippet icon()}
						<Building2 size={32} />
					{/snippet}
				</EmptyState>
				{#if !searchQuery}
					<div class="empty-cta">
						<Button variant="primary" onclick={openLinkModal}>
							<Plus size={14} />
							Link Organization
						</Button>
					</div>
				{/if}
			</div>
		{:else}
			<div class="drydock-table-wrap">
				<table class="drydock-table">
					<thead>
						<tr>
							<th>Organization</th>
							<th>Slug</th>
							<th>External ID (Nomi)</th>
							<th class="actions-col">Actions</th>
						</tr>
					</thead>
					<tbody>
						{#each items as org (org.id)}
							<tr>
								<td>
									<div class="org-cell">
										<div class="org-icon">
											<Building2 size={15} />
										</div>
										<span class="org-name" title={org.name}>{org.name}</span>
									</div>
								</td>
								<td>
									<span class="code-val">{org.slug}</span>
								</td>
								<td>
									{#if org.external_id}
										<button
											type="button"
											class="id-chip"
											onclick={() => copyToClipboard(org.external_id!, org.id)}
											title="Click to copy External ID"
										>
											<span>{org.external_id}</span>
											{#if copiedId === org.id}
												<Check size={11} class="id-copied" />
											{:else}
												<Copy size={11} />
											{/if}
										</button>
									{:else}
										<span class="muted-dash">—</span>
									{/if}
								</td>
								<td class="actions-col">
									<button
										type="button"
										class="row-action-btn row-action-btn--danger"
										onclick={() => openUnlinkDialog(org)}
										title="Unlink organization"
									>
										<Unlink size={13} />
										<span>Unlink</span>
									</button>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>

			<!-- Pagination Footer -->
			{#if total > 0}
				<div class="pagination-footer">
					<Pagination
						page={currentPage}
						{pageSize}
						{total}
						onPageChange={handlePageChange}
						onPageSizeChange={handlePageSizeChange}
					/>
				</div>
			{/if}
		{/if}
	</div>
</div>

<!-- Link Organization Modal -->
<Modal bind:open={showLinkModal} title="Link Organization">
	<form onsubmit={(e) => { e.preventDefault(); void handleLinkOrg(); }} class="dialog-form">
		{#if linkError}
			<InlineAlert tone="error">{linkError}</InlineAlert>
		{/if}

		<div class="info-callout">
			Linking an organization allows this integration's scoped keys (<code>ship_int_...</code>) to orchestrate its dev databases, apps, and publish lifecycles.
		</div>

		<FormField label="Organization" required>
			{#if linkableOrgs.length === 0}
				<p class="form-hint-empty">No unlinked organizations found in Shipyard.</p>
			{:else}
				<select
					class="drydock-select"
					bind:value={selectedOrgId}
					required
				>
					<option value="" disabled>Select an organization…</option>
					{#each linkableOrgs as org}
						<option value={org.id}>{org.name} ({org.slug})</option>
					{/each}
				</select>
			{/if}
		</FormField>

		<FormField label="External ID (optional)">
			<TextField
				bind:value={externalId}
				placeholder="e.g. org_2Nf8sK (defaults to org slug)"
			/>
		</FormField>

		<div class="modal-buttons">
			<Button variant="secondary" onclick={() => (showLinkModal = false)}>Cancel</Button>
			<Button
				variant="primary"
				type="submit"
				disabled={linking || !selectedOrgId}
			>
				{#if linking}
					<Spinner size={14} /> Linking…
				{:else}
					Link Organization
				{/if}
			</Button>
		</div>
	</form>
</Modal>

<!-- Unlink Confirmation Dialog -->
<ConfirmDialog
	bind:open={unlinkDialogOpen}
	title="Unlink Organization"
	message="Are you sure you want to unlink '{unlinkTarget?.name}'? The organization and its projects will remain in Shipyard, but external platform keys will no longer be able to manage it."
	confirmLabel={unlinking ? 'Unlinking…' : 'Unlink Organization'}
	danger={true}
	onConfirm={confirmUnlink}
/>

<style>
	.page {
		display: flex;
		flex-direction: column;
		gap: 20px;
		max-width: 1200px;
		margin: 0 auto;
		padding: 24px 24px 64px;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.table-controls {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		flex-wrap: wrap;
	}

	.search-box {
		flex: 1;
		max-width: 480px;
	}

	.count-badge {
		flex-shrink: 0;
	}

	.table-container {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
		box-shadow: var(--shadow-sm);
	}

	.loading-state {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 64px;
	}

	.empty-state-wrap {
		padding: 48px 16px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
	}

	.empty-cta {
		margin-top: -8px;
	}

	.drydock-table-wrap {
		width: 100%;
		overflow-x: auto;
	}

	.drydock-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
		text-align: left;
	}

	.drydock-table thead th {
		background: var(--bg-elevated);
		color: var(--text-muted);
		font-size: 11.5px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
	}

	.drydock-table tbody td {
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
		color: var(--text-primary);
		vertical-align: middle;
	}

	.drydock-table tbody tr:last-child td {
		border-bottom: none;
	}

	.drydock-table tbody tr:hover td {
		background: var(--bg-hover);
	}

	.actions-col {
		text-align: right;
		width: 120px;
	}

	.org-cell {
		display: flex;
		align-items: center;
		gap: 10px;
	}

	.org-icon {
		width: 28px;
		height: 28px;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text-secondary);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.org-name {
		font-weight: 600;
		color: var(--text-primary);
	}

	.code-val {
		font-family: var(--font-mono);
		font-size: 11.5px;
		background: var(--bg-elevated);
		padding: 2px 6px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		color: var(--text-secondary);
	}

	.id-chip {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-elevated);
		color: var(--text-secondary);
		border: 1px solid var(--border-control);
		padding: 2px 8px;
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
	}
	.id-chip:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
		border-color: var(--border-hover);
	}
	:global(.id-copied) {
		color: var(--accent-green) !important;
	}

	.muted-dash {
		color: var(--text-dim);
	}

	.row-action-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 500;
		padding: 4px 10px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		background: var(--bg-surface);
		color: var(--text-secondary);
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
	}
	.row-action-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.row-action-btn--danger {
		color: var(--accent-red);
		border-color: transparent;
		background: transparent;
	}
	.row-action-btn--danger:hover {
		background: var(--accent-red-muted);
		border-color: color-mix(in srgb, var(--accent-red) 30%, transparent);
	}

	.pagination-footer {
		padding: 8px 16px;
		border-top: 1px solid var(--border);
		background: var(--bg-surface);
	}

	/* ── Modals & forms ── */
	.dialog-form {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.info-callout {
		padding: 10px 12px;
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		font-size: 12px;
		color: var(--text-muted);
		line-height: 1.5;
	}

	.drydock-select {
		width: 100%;
		padding: 8px 10px;
		font-family: var(--font-sans);
		font-size: 13px;
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		border: 1px solid var(--border-control);
		color: var(--text-primary);
	}
	.drydock-select:focus {
		outline: none;
		box-shadow: var(--focus-ring);
	}

	.form-hint-empty {
		font-size: 12px;
		color: var(--text-muted);
		margin: 0;
	}

	.modal-buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding-top: 8px;
	}
</style>
