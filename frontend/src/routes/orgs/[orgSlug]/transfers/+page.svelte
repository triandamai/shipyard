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
		XCircle, Clock, AlertTriangle, ShieldAlert
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

	// Claim modal state
	let claimModalOpen = $state(false);
	let activeClaimTransfer = $state<ServiceTransferItem | null>(null);

	// Confirm dialog state for cancel / reject
	let confirmOpen = $state(false);
	let confirmTitle = $state('');
	let confirmMessage = $state('');
	let confirmAction = $state<(() => Promise<void>) | null>(null);

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
		<!-- 1. Incoming Service Transfers -->
		<Card padding="20px">
			<div class="section-header">
				<div class="section-title-wrap">
					<span class="section-icon"><Download size={16} /></span>
					<h2 class="section-title">Incoming Service Transfers</h2>
					{#if transfersData.incoming_service_transfers.filter(t => t.status === 'pending_claim').length > 0}
						<Badge tone="yellow">
							{transfersData.incoming_service_transfers.filter(t => t.status === 'pending_claim').length} pending
						</Badge>
					{/if}
				</div>
				<span class="section-desc">Services transferred to this organization that require claiming into a project.</span>
			</div>

			{#if transfersData.incoming_service_transfers.length === 0}
				<div class="empty-state-wrap">
					<span class="empty-text">No incoming service transfers.</span>
				</div>
			{:else}
				<div class="transfer-list">
					{#each transfersData.incoming_service_transfers as item}
						<div class="transfer-card">
							<div class="transfer-main">
								<div class="transfer-title-row">
									<span class="resource-name">{item.service_name}</span>
									<span class="resource-slug">{item.service_slug}</span>
									<Badge tone={statusTone(item.status)}>
										{item.status.replace('_', ' ')}
									</Badge>
								</div>
								<div class="transfer-meta-row">
									<span>From: <strong>{item.source_org_name}</strong> ({item.source_org_slug}) &middot; Project: {item.source_project_name}</span>
									<span>Initiated: {formatDate(item.created_at)}</span>
									{#if item.target_project_name}
										<span>Target Project: <strong>{item.target_project_name}</strong></span>
									{/if}
								</div>
								{#if item.error_message}
									<div class="transfer-error">{item.error_message}</div>
								{/if}
							</div>
							<div class="transfer-actions">
								{#if item.status === 'pending_claim'}
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
								{/if}
							</div>
						</div>
					{/each}
				</div>
			{/if}
		</Card>

		<!-- 2. Outgoing Service Transfers -->
		<Card padding="20px">
			<div class="section-header">
				<div class="section-title-wrap">
					<span class="section-icon"><Upload size={16} /></span>
					<h2 class="section-title">Outgoing Service Transfers</h2>
				</div>
				<span class="section-desc">Services from this organization being transferred to other organizations.</span>
			</div>

			{#if transfersData.outgoing_service_transfers.length === 0}
				<div class="empty-state-wrap">
					<span class="empty-text">No outgoing service transfers.</span>
				</div>
			{:else}
				<div class="transfer-list">
					{#each transfersData.outgoing_service_transfers as item}
						<div class="transfer-card">
							<div class="transfer-main">
								<div class="transfer-title-row">
									<span class="resource-name">{item.service_name}</span>
									<span class="resource-slug">{item.service_slug}</span>
									{#if item.status === 'pending_claim'}
										<Badge tone="yellow">Waiting for claim by {item.target_org_name}</Badge>
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

		<!-- 3. Project Transfers -->
		<Card padding="20px">
			<div class="section-header">
				<div class="section-title-wrap">
					<span class="section-icon"><FolderSync size={16} /></span>
					<h2 class="section-title">Project Transfers History</h2>
				</div>
				<span class="section-desc">Projects transferred into or out of this organization.</span>
			</div>

			{#if transfersData.project_transfers.length === 0}
				<div class="empty-state-wrap">
					<span class="empty-text">No project transfers found.</span>
				</div>
			{:else}
				<div class="transfer-list">
					{#each transfersData.project_transfers as item}
						<div class="transfer-card">
							<div class="transfer-main">
								<div class="transfer-title-row">
									<span class="resource-name">{item.project_name}</span>
									<span class="resource-slug">{item.project_slug}</span>
									<Badge tone={statusTone(item.status)}>
										{item.status.replace('_', ' ')}
									</Badge>
								</div>
								<div class="transfer-meta-row">
									<span>From: <strong>{item.source_org_name}</strong></span>
									<span>To: <strong>{item.target_org_name}</strong></span>
									<span>Initiated: {formatDate(item.created_at)}</span>
								</div>
								{#if item.error_message}
									<div class="transfer-error">{item.error_message}</div>
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

<!-- Claim modal -->
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
