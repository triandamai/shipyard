<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { toastStore } from '$lib/stores/toast.store';
	import {
		PageHeader, Card, Button, Badge, InlineAlert, Spinner, EmptyState, ConfirmDialog
	} from '$lib/components/ui';
	import {
		ArrowLeftRight, Download, Upload, FolderSync, RefreshCw, CheckCircle2,
		XCircle, Clock, AlertTriangle, ShieldAlert, History, Layers
	} from '@lucide/svelte';
	import type { OrgTransfersResponse, ServiceTransferItem, ProjectTransferItem } from '$lib/api/types';
	import ClaimServiceModal from '$lib/components/transfers/ClaimServiceModal.svelte';

	let orgSlug = $derived(page.params.orgSlug ?? '');
	let currentOrg = $derived($orgStore.activeOrg);
	let myRole = $derived($orgStore.myMembership?.role ?? '');
	let isOwner = $derived(myRole === 'owner');

	let loading = $state(true);
	let error = $state('');
	let transfersData = $state<OrgTransfersResponse>({
		incoming_service_transfers: [],
		outgoing_service_transfers: [],
		project_transfers: []
	});

	// Claim panel state
	let claimModalOpen = $state(false);
	let activeClaimTransfer = $state<ServiceTransferItem | null>(null);

	// Confirm dialog state for cancel / reject
	let confirmOpen = $state(false);
	let confirmTitle = $state('');
	let confirmMessage = $state('');
	let confirmAction = $state<(() => Promise<void>) | null>(null);

	// History filter tabs: 'all' | 'services' | 'projects'
	let historyFilter = $state<'all' | 'services' | 'projects'>('all');

	interface HistoryItem {
		id: string;
		kind: 'service' | 'project';
		name: string;
		slug: string;
		direction: 'incoming' | 'outgoing';
		sourceOrgName: string;
		sourceOrgSlug: string;
		sourceProjectName?: string;
		targetOrgName: string;
		targetOrgSlug: string;
		targetProjectName?: string;
		status: string;
		errorMessage?: string | null;
		date: string;
	}

	// Active incoming service transfers pending claim
	let pendingIncomingServices = $derived(
		transfersData.incoming_service_transfers.filter((t) => t.status === 'pending_claim')
	);

	// Active outgoing service transfers awaiting claim or in-progress
	let pendingOutgoingServices = $derived(
		transfersData.outgoing_service_transfers.filter(
			(t) => t.status === 'pending_claim' || t.status === 'in_progress'
		)
	);

	// Service transfer history: completed, cancelled, rejected, failed
	let serviceHistory = $derived.by(() => {
		const items: HistoryItem[] = [];

		for (const t of transfersData.incoming_service_transfers) {
			if (t.status !== 'pending_claim') {
				items.push({
					id: t.id,
					kind: 'service',
					name: t.service_name,
					slug: t.service_slug,
					direction: 'incoming',
					sourceOrgName: t.source_org_name,
					sourceOrgSlug: t.source_org_slug,
					sourceProjectName: t.source_project_name,
					targetOrgName: t.target_org_name,
					targetOrgSlug: t.target_org_slug,
					targetProjectName: t.target_project_name,
					status: t.status,
					errorMessage: t.error_message,
					date: t.updated_at || t.created_at
				});
			}
		}

		for (const t of transfersData.outgoing_service_transfers) {
			if (t.status !== 'pending_claim' && t.status !== 'in_progress') {
				items.push({
					id: t.id,
					kind: 'service',
					name: t.service_name,
					slug: t.service_slug,
					direction: 'outgoing',
					sourceOrgName: t.source_org_name,
					sourceOrgSlug: t.source_org_slug,
					sourceProjectName: t.source_project_name,
					targetOrgName: t.target_org_name,
					targetOrgSlug: t.target_org_slug,
					targetProjectName: t.target_project_name,
					status: t.status,
					errorMessage: t.error_message,
					date: t.updated_at || t.created_at
				});
			}
		}

		return items;
	});

	// Project transfer history
	let projectHistory = $derived.by(() => {
		const items: HistoryItem[] = [];
		for (const pt of transfersData.project_transfers) {
			const isIncoming = pt.target_org_id === currentOrg?.id;
			items.push({
				id: pt.id,
				kind: 'project',
				name: pt.project_name,
				slug: pt.project_slug,
				direction: isIncoming ? 'incoming' : 'outgoing',
				sourceOrgName: pt.source_org_name,
				sourceOrgSlug: pt.source_org_slug,
				targetOrgName: pt.target_org_name,
				targetOrgSlug: pt.target_org_slug,
				status: pt.status,
				errorMessage: pt.error_message,
				date: pt.updated_at || pt.created_at
			});
		}
		return items;
	});

	// Consolidated history sorted newest first
	let allHistory = $derived(
		[...serviceHistory, ...projectHistory].sort(
			(a, b) => new Date(b.date).getTime() - new Date(a.date).getTime()
		)
	);

	let filteredHistory = $derived.by(() => {
		if (historyFilter === 'services') return serviceHistory;
		if (historyFilter === 'projects') return projectHistory;
		return allHistory;
	});

	async function loadTransfers() {
		if (!currentOrg?.id) return;
		loading = true;
		error = '';
		try {
			const res = await api.getOrgTransfers(currentOrg.id);
			if (res.data) {
				transfersData = res.data;
			} else if (res.error) {
				error = res.error.message;
			}
		} catch (err: any) {
			error = err.message || 'Failed to load transfers';
		} finally {
			loading = false;
		}
	}

	onMount(() => {
		loadTransfers();
	});

	$effect(() => {
		if (currentOrg?.id) {
			loadTransfers();
		}
	});

	function openClaimModal(transfer: ServiceTransferItem) {
		activeClaimTransfer = transfer;
		claimModalOpen = true;
	}

	async function handleClaim(transferId: string, targetProjectId: string): Promise<boolean | void> {
		const res = await api.claimServiceTransfer(transferId, targetProjectId);
		if (res.error) {
			toastStore.add({ type: 'error', title: 'Claim failed', message: res.error.message });
			return false;
		}
		toastStore.add({
			type: 'success',
			title: 'Service claimed',
			message: 'Service migration started in background.'
		});
		await loadTransfers();
	}

	function confirmCancel(transfer: ServiceTransferItem) {
		confirmTitle = `Cancel Transfer for ${transfer.service_name}`;
		confirmMessage = `Are you sure you want to cancel the transfer to ${transfer.target_org_name}? The service will remain in your project.`;
		confirmAction = async () => {
			const res = await api.cancelServiceTransfer(transfer.id);
			if (res.error) {
				toastStore.add({ type: 'error', title: 'Cancellation failed', message: res.error.message });
			} else {
				toastStore.add({ type: 'info', title: 'Transfer cancelled', message: 'Service transfer was cancelled.' });
				await loadTransfers();
			}
		};
		confirmOpen = true;
	}

	function confirmReject(transfer: ServiceTransferItem) {
		confirmTitle = `Reject Incoming Transfer for ${transfer.service_name}`;
		confirmMessage = `Are you sure you want to decline this incoming transfer from ${transfer.source_org_name}?`;
		confirmAction = async () => {
			const res = await api.rejectServiceTransfer(transfer.id);
			if (res.error) {
				toastStore.add({ type: 'error', title: 'Rejection failed', message: res.error.message });
			} else {
				toastStore.add({ type: 'info', title: 'Transfer rejected', message: 'Incoming service transfer was rejected.' });
				await loadTransfers();
			}
		};
		confirmOpen = true;
	}

	function formatDate(iso: string) {
		return new Date(iso).toLocaleString('en-US', {
			month: 'short',
			day: 'numeric',
			year: 'numeric',
			hour: '2-digit',
			minute: '2-digit'
		});
	}

	function statusTone(status: string): 'green' | 'red' | 'yellow' | 'blue' | 'neutral' {
		switch (status) {
			case 'completed': return 'green';
			case 'in_progress': return 'blue';
			case 'pending_claim': return 'yellow';
			case 'failed':
			case 'cancelled':
			case 'rejected': return 'red';
			default: return 'neutral';
		}
	}
</script>

<div class="transfers-scroll">
<div class="transfers-page">
	<div class="page-header-wrap">
		<PageHeader
			title="Ownership Transfers"
			subtitle="Manage incoming and outgoing service and project ownership transfers."
		/>
		<div class="header-actions">
			<Button variant="ghost" size="sm" onclick={loadTransfers} disabled={loading}>
				<RefreshCw size={14} class={loading ? 'spin' : ''} />
				<span>Refresh</span>
			</Button>
		</div>
	</div>

	{#if !isOwner}
		<InlineAlert tone="info">
			<span class="role-notice">
				<ShieldAlert size={14} />
				<span>Only organization owners can initiate, claim, or manage ownership transfers.</span>
			</span>
		</InlineAlert>
	{/if}

	{#if error}
		<InlineAlert tone="error">
			<span>{error}</span>
		</InlineAlert>
	{/if}

	{#if loading && transfersData.incoming_service_transfers.length === 0 && transfersData.outgoing_service_transfers.length === 0 && transfersData.project_transfers.length === 0}
		<div class="state-center">
			<Spinner size={24} />
			<span>Loading transfers…</span>
		</div>
	{:else}
		<!-- 1. Incoming Service Transfers (Active pending claim only) -->
		<Card padding="20px">
			<div class="section-header">
				<div class="section-title-wrap">
					<span class="section-icon"><Download size={16} /></span>
					<h2 class="section-title">Incoming Service Transfers</h2>
					{#if pendingIncomingServices.length > 0}
						<Badge tone="yellow">
							{pendingIncomingServices.length} pending
						</Badge>
					{/if}
				</div>
				<span class="section-desc">Services transferred to this organization that require claiming into a project.</span>
			</div>

			{#if pendingIncomingServices.length === 0}
				<div class="empty-state-wrap">
					<span class="empty-text">No incoming service transfers pending claim.</span>
				</div>
			{:else}
				<div class="transfer-list">
					{#each pendingIncomingServices as item}
						<div class="transfer-card">
							<div class="transfer-main">
								<div class="transfer-title-row">
									<span class="resource-name">{item.service_name}</span>
									<span class="resource-slug">{item.service_slug}</span>
									<Badge tone="yellow">Pending claim</Badge>
								</div>
								<div class="transfer-meta-row">
									<span>From: <strong>{item.source_org_name}</strong> ({item.source_org_slug}) &middot; Project: {item.source_project_name}</span>
									<span>Initiated: {formatDate(item.created_at)}</span>
								</div>
								{#if item.error_message}
									<div class="transfer-error">{item.error_message}</div>
								{/if}
							</div>
							<div class="transfer-actions">
								<Button
									variant="primary"
									size="sm"
									disabled={!isOwner}
									onclick={() => openClaimModal(item)}
								>
									Claim Service
								</Button>
								<Button
									variant="ghost"
									size="sm"
									disabled={!isOwner}
									onclick={() => confirmReject(item)}
								>
									Decline
								</Button>
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</Card>

		<!-- 2. Outgoing Service Transfers (Active pending or migrating only) -->
		<Card padding="20px">
			<div class="section-header">
				<div class="section-title-wrap">
					<span class="section-icon"><Upload size={16} /></span>
					<h2 class="section-title">Outgoing Service Transfers</h2>
					{#if pendingOutgoingServices.length > 0}
						<Badge tone="yellow">
							{pendingOutgoingServices.length} active
						</Badge>
					{/if}
				</div>
				<span class="section-desc">Services from this organization awaiting claim by target organizations.</span>
			</div>

			{#if pendingOutgoingServices.length === 0}
				<div class="empty-state-wrap">
					<span class="empty-text">No active outgoing service transfers.</span>
				</div>
			{:else}
				<div class="transfer-list">
					{#each pendingOutgoingServices as item}
						<div class="transfer-card">
							<div class="transfer-main">
								<div class="transfer-title-row">
									<span class="resource-name">{item.service_name}</span>
									<span class="resource-slug">{item.service_slug}</span>
									{#if item.status === 'pending_claim'}
										<Badge tone="yellow">Waiting for claim by {item.target_org_name}</Badge>
									{:else if item.status === 'in_progress'}
										<Badge tone="blue">Migrating in background</Badge>
									{:else}
										<Badge tone={statusTone(item.status)}>
											{item.status.replace('_', ' ')}
										</Badge>
									{/if}
								</div>
								<div class="transfer-meta-row">
									<span>To: <strong>{item.target_org_name}</strong> ({item.target_org_slug})</span>
									<span>From Project: {item.source_project_name}</span>
									<span>Initiated: {formatDate(item.created_at)}</span>
								</div>
								{#if item.error_message}
									<div class="transfer-error">{item.error_message}</div>
								{/if}
							</div>
							<div class="transfer-actions">
								{#if item.status === 'pending_claim'}
									<Button
										variant="danger-outline"
										size="sm"
										disabled={!isOwner}
										onclick={() => confirmCancel(item)}
									>
										Cancel Transfer
									</Button>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</Card>

		<!-- 3. Transfer History (Consolidated service + project transfers) -->
		<Card padding="20px">
			<div class="section-header-row">
				<div class="section-header">
					<div class="section-title-wrap">
						<span class="section-icon"><History size={16} /></span>
						<h2 class="section-title">Transfer History</h2>
						{#if allHistory.length > 0}
							<Badge tone="neutral">{allHistory.length} total</Badge>
						{/if}
					</div>
					<span class="section-desc">Completed, rejected, and cancelled service and project transfers.</span>
				</div>

				<div class="filter-tabs">
					<button
						type="button"
						class="filter-tab"
						class:active={historyFilter === 'all'}
						onclick={() => (historyFilter = 'all')}
					>
						All ({allHistory.length})
					</button>
					<button
						type="button"
						class="filter-tab"
						class:active={historyFilter === 'services'}
						onclick={() => (historyFilter = 'services')}
					>
						Services ({serviceHistory.length})
					</button>
					<button
						type="button"
						class="filter-tab"
						class:active={historyFilter === 'projects'}
						onclick={() => (historyFilter = 'projects')}
					>
						Projects ({projectHistory.length})
					</button>
				</div>
			</div>

			{#if filteredHistory.length === 0}
				<div class="empty-state-wrap">
					<span class="empty-text">
						{historyFilter === 'all'
							? 'No transfer history found.'
							: `No ${historyFilter} transfer history found.`}
					</span>
				</div>
			{:else}
				<div class="transfer-list">
					{#each filteredHistory as item (item.id + item.direction)}
						<div class="transfer-card">
							<div class="transfer-main">
								<div class="transfer-title-row">
									<span class="history-kind-badge">
										{#if item.kind === 'service'}
											<Layers size={13} />
											<span>Service</span>
										{:else}
											<FolderSync size={13} />
											<span>Project</span>
										{/if}
									</span>

									<span class="resource-name">{item.name}</span>
									<span class="resource-slug">{item.slug}</span>

									{#if item.direction === 'incoming'}
										<Badge tone="blue">
											<Download size={11} /> Incoming
										</Badge>
									{:else}
										<Badge tone="neutral">
											<Upload size={11} /> Outgoing
										</Badge>
									{/if}

									<Badge tone={statusTone(item.status)}>
										{item.status.replace('_', ' ')}
									</Badge>
								</div>
								<div class="transfer-meta-row">
									{#if item.kind === 'service'}
										<span>From: <strong>{item.sourceOrgName}</strong> ({item.sourceProjectName || 'Project'}) &rarr; To: <strong>{item.targetOrgName}</strong> {#if item.targetProjectName}({item.targetProjectName}){/if}</span>
									{:else}
										<span>From: <strong>{item.sourceOrgName}</strong> &rarr; To: <strong>{item.targetOrgName}</strong></span>
									{/if}
									<span>{formatDate(item.date)}</span>
								</div>
								{#if item.errorMessage}
									<div class="transfer-error">{item.errorMessage}</div>
								{/if}
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</Card>
	{/if}
</div>
</div>

<!-- Claim slide panel -->
{#if currentOrg}
	<ClaimServiceModal
		bind:open={claimModalOpen}
		transfer={activeClaimTransfer}
		orgId={currentOrg.id}
		onClaim={handleClaim}
		onClose={() => { activeClaimTransfer = null; }}
	/>
{/if}

<!-- Confirm cancel/reject dialog -->
<ConfirmDialog
	bind:open={confirmOpen}
	title={confirmTitle}
	message={confirmMessage}
	confirmLabel="Confirm"
	onConfirm={async () => {
		if (confirmAction) await confirmAction();
		confirmOpen = false;
	}}
/>

<style>
	.transfers-scroll {
		height: 100%;
		overflow-y: auto;
	}

	.transfers-page {
		max-width: 860px;
		margin: 0 auto;
		padding: 32px;
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	.page-header-wrap {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
	}

	.page-header-wrap :global(.ui-page-header) {
		margin-bottom: 0;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-top: 4px;
	}

	.role-notice {
		display: inline-flex;
		align-items: center;
		gap: 8px;
	}

	.state-center {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		padding: 64px 0;
		color: var(--text-muted);
		font-size: 13px;
	}

	.section-header {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-bottom: 16px;
	}

	.section-header-row {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
		margin-bottom: 16px;
		flex-wrap: wrap;
	}

	.section-header-row .section-header {
		margin-bottom: 0;
	}

	.section-title-wrap {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.section-icon {
		color: var(--accent-primary, #3b82f6);
	}

	.section-title {
		font-size: 15px;
		font-weight: 600;
		color: var(--text-primary);
		margin: 0;
	}

	.section-desc {
		font-size: 12.5px;
		color: var(--text-muted);
	}

	.filter-tabs {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		background: var(--bg-base);
		padding: 3px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
	}

	.filter-tab {
		background: transparent;
		border: none;
		border-radius: calc(var(--radius-sm) - 2px);
		padding: 4px 10px;
		font-size: 12px;
		font-weight: 500;
		color: var(--text-muted);
		cursor: pointer;
		transition: all var(--transition-fast);
		font-family: inherit;
	}

	.filter-tab:hover {
		color: var(--text-primary);
	}

	.filter-tab.active {
		background: var(--bg-surface);
		color: var(--text-primary);
		font-weight: 600;
		box-shadow: 0 1px 2px rgba(0, 0, 0, 0.08);
	}

	.empty-state-wrap {
		padding: 24px;
		text-align: center;
		border: 1px dashed var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-surface);
	}

	.empty-text {
		font-size: 13px;
		color: var(--text-muted);
	}

	.transfer-list {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.transfer-card {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 16px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		gap: 16px;
		transition: border-color var(--transition-fast);
	}

	.transfer-card:hover {
		border-color: var(--border-hover, var(--border));
	}

	.transfer-main {
		display: flex;
		flex-direction: column;
		gap: 4px;
		flex: 1;
		min-width: 0;
	}

	.transfer-title-row {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.history-kind-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-dim);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		padding: 2px 6px;
		border-radius: var(--radius-xs, 4px);
	}

	.resource-name {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.resource-slug {
		font-size: 11px;
		font-family: var(--font-mono);
		color: var(--text-muted);
	}

	.transfer-meta-row {
		display: flex;
		align-items: center;
		gap: 14px;
		font-size: 12px;
		color: var(--text-muted);
		flex-wrap: wrap;
	}

	.transfer-meta-row strong {
		color: var(--text-secondary);
	}

	.transfer-error {
		font-size: 11.5px;
		color: var(--accent-red);
		margin-top: 2px;
	}

	.transfer-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
	}

	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	@media (max-width: 639px) {
		.transfers-page {
			padding: 16px;
		}
		.section-header-row {
			flex-direction: column;
			gap: 12px;
		}
		.filter-tabs {
			width: 100%;
			justify-content: space-between;
		}
		.filter-tab {
			flex: 1;
			text-align: center;
		}
		.transfer-card {
			flex-direction: column;
			align-items: flex-start;
		}
		.transfer-actions {
			width: 100%;
			justify-content: flex-end;
		}
	}
</style>
