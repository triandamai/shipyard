<script lang="ts">
	import { Modal, Button, SearchInput, InlineAlert, Spinner, Badge } from '$lib/components/ui';
	import { Building2, ArrowRight, AlertTriangle, Check, ShieldAlert } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import type { TransferOrgSearchItem } from '$lib/api/types';

	interface Props {
		open: boolean;
		mode: 'project' | 'service';
		resourceName: string;
		currentOrgId?: string;
		onConfirm: (targetOrg: TransferOrgSearchItem) => Promise<boolean | void>;
		onClose: () => void;
	}

	let { open = $bindable(false), mode, resourceName, currentOrgId, onConfirm, onClose }: Props = $props();

	let searchQuery = $state('');
	let searching = $state(false);
	let orgs = $state<TransferOrgSearchItem[]>([]);
	let selectedOrg = $state<TransferOrgSearchItem | null>(null);
	let step = $state<'select' | 'confirm'>('select');
	let submitting = $state(false);
	let errorMessage = $state('');

	let searchTimeout: ReturnType<typeof setTimeout> | null = null;

	async function performSearch(q: string) {
		searching = true;
		errorMessage = '';
		try {
			const res = await api.searchOrganizations(q);
			if (res.data) {
				orgs = res.data.filter(o => o.id !== currentOrgId);
			} else if (res.error) {
				errorMessage = res.error.message;
			}
		} catch (err: any) {
			errorMessage = err.message || 'Failed to search organizations';
		} finally {
			searching = false;
		}
	}

	$effect(() => {
		if (open) {
			step = 'select';
			selectedOrg = null;
			searchQuery = '';
			errorMessage = '';
			performSearch('');
		}
	});

	function onSearchInput(val: string) {
		searchQuery = val;
		if (searchTimeout) clearTimeout(searchTimeout);
		searchTimeout = setTimeout(() => {
			performSearch(val.trim());
		}, 300);
	}

	function handleSelect(org: TransferOrgSearchItem) {
		selectedOrg = org;
		errorMessage = '';
		step = 'confirm';
	}

	async function handleFinalConfirm() {
		if (!selectedOrg) return;
		submitting = true;
		errorMessage = '';
		try {
			const ok = await onConfirm(selectedOrg);
			if (ok !== false) {
				open = false;
				onClose();
			}
		} catch (err: any) {
			errorMessage = err.message || 'Transfer failed';
		} finally {
			submitting = false;
		}
	}

	function handleBack() {
		step = 'select';
		errorMessage = '';
	}

	function handleCloseModal() {
		open = false;
		onClose();
	}
</script>

<Modal
	bind:open
	title={step === 'select'
		? `Transfer ${mode === 'project' ? 'Project' : 'Service'}: Select Target Organization`
		: `Confirm ${mode === 'project' ? 'Project' : 'Service'} Transfer`}
	onclose={handleCloseModal}
>
	<div class="modal-body">
		{#if errorMessage}
			<InlineAlert tone="error">
				<span class="error-text"><AlertTriangle size={14} /> {errorMessage}</span>
			</InlineAlert>
		{/if}

		{#if step === 'select'}
			<p class="desc">
				Select the target organization to transfer <strong>{resourceName}</strong> to.
			</p>

			<div class="search-box">
				<SearchInput
					value={searchQuery}
					placeholder="Search organization by name or slug…"
					oninput={onSearchInput}
				/>
			</div>

			<div class="org-list">
				{#if searching}
					<div class="state-center">
						<Spinner size={18} />
						<span>Searching organizations…</span>
					</div>
				{:else if orgs.length === 0}
					<div class="state-center">
						<span>No matching organizations found.</span>
					</div>
				{:else}
					{#each orgs as org}
						<button class="org-item" type="button" onclick={() => handleSelect(org)}>
							<div class="org-icon">
								<Building2 size={16} />
							</div>
							<div class="org-info">
								<span class="org-name">{org.name}</span>
								<span class="org-slug">{org.slug}</span>
							</div>
							<div class="org-action">
								<ArrowRight size={14} />
							</div>
						</button>
					{/each}
				{/if}
			</div>

			<div class="modal-actions">
				<Button variant="ghost" onclick={handleCloseModal}>Cancel</Button>
			</div>
		{:else if selectedOrg}
			<div class="confirm-view">
				<div class="summary-card">
					<div class="summary-col">
						<span class="summary-label">{mode === 'project' ? 'Project' : 'Service'}</span>
						<span class="summary-val">{resourceName}</span>
					</div>
					<div class="summary-arrow">
						<ArrowRight size={18} />
					</div>
					<div class="summary-col">
						<span class="summary-label">Target Organization</span>
						<span class="summary-val">{selectedOrg.name}</span>
						<span class="summary-sub">({selectedOrg.slug})</span>
					</div>
				</div>

				<div class="warning-box">
					<span class="warn-icon"><AlertTriangle size={16} /></span>
					<div class="warn-content">
						{#if mode === 'project'}
							<strong>This will transfer the entire project and all services</strong>
							<p>All services, volumes, networks, and domains in this project will move to <strong>{selectedOrg.name}</strong>. This process runs in the background.</p>
						{:else}
							<strong>Service transfer requires target organization claim</strong>
							<p>This service will be put in <em>Pending Claim</em> status. An owner of <strong>{selectedOrg.name}</strong> must claim it from their Transfers page and assign it to a target project. Volumes and data will persist.</p>
						{/if}
					</div>
				</div>

				<div class="modal-actions">
					<Button variant="ghost" onclick={handleBack} disabled={submitting}>Back</Button>
					<Button
						variant="danger"
						onclick={handleFinalConfirm}
						disabled={submitting}
					>
						{#if submitting}
							<Spinner size={14} />
							<span>Transferring…</span>
						{:else}
							<span>Confirm Transfer</span>
						{/if}
					</Button>
				</div>
			</div>
		{/if}
	</div>
</Modal>

<style>
	.modal-body {
		display: flex;
		flex-direction: column;
		gap: 16px;
		min-width: 440px;
		max-width: 520px;
	}

	.desc {
		margin: 0;
		font-size: 13px;
		color: var(--text-secondary);
		line-height: 1.5;
	}

	.search-box {
		width: 100%;
	}

	.org-list {
		display: flex;
		flex-direction: column;
		gap: 6px;
		max-height: 260px;
		overflow-y: auto;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 6px;
		background: var(--bg-surface);
	}

	.state-center {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 32px 16px;
		color: var(--text-muted);
		font-size: 13px;
	}

	.org-item {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 12px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
		cursor: pointer;
		text-align: left;
		transition: all var(--transition-fast);
		color: inherit;
		width: 100%;
		font-family: inherit;
	}

	.org-item:hover {
		background: var(--bg-elevated);
		border-color: var(--border);
	}

	.org-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border-radius: var(--radius-sm);
		background: var(--bg-base);
		color: var(--text-primary);
		flex-shrink: 0;
	}

	.org-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.org-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.org-slug {
		font-size: 11px;
		font-family: var(--font-mono);
		color: var(--text-muted);
	}

	.org-action {
		color: var(--text-dim);
	}

	.org-item:hover .org-action {
		color: var(--text-primary);
	}

	/* Confirm view */
	.confirm-view {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.summary-card {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		gap: 12px;
	}

	.summary-col {
		display: flex;
		flex-direction: column;
		gap: 3px;
		flex: 1;
	}

	.summary-label {
		font-size: 11px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-dim);
		font-weight: 600;
	}

	.summary-val {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.summary-sub {
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}

	.summary-arrow {
		color: var(--text-muted);
		display: flex;
		align-items: center;
	}

	.warning-box {
		display: flex;
		gap: 12px;
		padding: 14px;
		background: rgba(234, 179, 8, 0.08);
		border: 1px solid rgba(234, 179, 8, 0.25);
		border-radius: var(--radius-md);
		color: var(--text-primary);
	}

	:global([data-theme='dark']) .warning-box {
		background: rgba(234, 179, 8, 0.12);
		border-color: rgba(234, 179, 8, 0.3);
	}

	.warn-icon {
		color: var(--accent-yellow);
		flex-shrink: 0;
		margin-top: 2px;
	}

	.warn-content {
		font-size: 12.5px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		line-height: 1.45;
	}

	.warn-content strong {
		font-size: 13px;
	}

	.warn-content p {
		margin: 0;
		color: var(--text-secondary);
	}

	.modal-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: 4px;
	}

	.error-text {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}

	@media (max-width: 639px) {
		.modal-body {
			min-width: 100%;
			max-width: 100%;
		}
		.summary-card {
			flex-direction: column;
			align-items: flex-start;
		}
		.summary-arrow {
			transform: rotate(90deg);
			margin: 4px 0;
		}
	}
</style>
