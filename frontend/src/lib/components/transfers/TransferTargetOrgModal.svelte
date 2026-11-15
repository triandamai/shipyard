<script lang="ts">
	import SlidePanel from '$lib/components/SlidePanel.svelte';
	import { Button, SearchInput, InlineAlert, Spinner, Badge } from '$lib/components/ui';
	import {
		Building2, ArrowRight, AlertTriangle, Check, ShieldAlert,
		HelpCircle, Info, Database, Layers, Globe, HardDrive, Network
	} from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import type { TransferOrgSearchItem } from '$lib/api/types';

	interface Props {
		open: boolean;
		mode: 'project' | 'service';
		resourceName: string;
		currentOrgId?: string;
		zIndex?: number;
		onConfirm: (targetOrg: TransferOrgSearchItem) => Promise<boolean | void>;
		onClose: () => void;
	}

	let {
		open = $bindable(false),
		mode,
		resourceName,
		currentOrgId,
		zIndex = 80,
		onConfirm,
		onClose
	}: Props = $props();

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

	function handleSearchChange(val: string) {
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

<svelte:window onkeydown={(e) => { if (open && e.key === 'Escape') handleCloseModal(); }} />

{#if open}
	<div
		class="panel-scrim"
		style="z-index: {zIndex - 1};"
		onclick={handleCloseModal}
		onkeydown={(e) => { if (e.key === 'Enter' || e.key === ' ') handleCloseModal(); }}
		role="button"
		tabindex="0"
		aria-label="Close transfer panel"
	></div>

	<SlidePanel
		title={step === 'select'
			? `Transfer ${mode === 'project' ? 'Project' : 'Service'}`
			: `Confirm ${mode === 'project' ? 'Project' : 'Service'} Transfer`}
		onClose={handleCloseModal}
		{zIndex}
	>
		<div class="panel-body-content">
			{#if errorMessage}
				<InlineAlert tone="error">
					<span class="error-text"><AlertTriangle size={14} /> {errorMessage}</span>
				</InlineAlert>
			{/if}

			{#if step === 'select'}
				<div class="select-step">
					<p class="desc">
						Select the target organization to transfer <strong>{resourceName}</strong> to.
					</p>

					<div class="search-box">
						<SearchInput
							value={searchQuery}
							placeholder="Search organization by name or slug…"
							oninput={handleSearchChange}
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
				</div>

				<div class="panel-actions">
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

					<!-- What will happen & What changes -->
					<div class="transfer-impact">
						<div class="impact-section">
							<span class="impact-title">
								<Info size={14} class="impact-icon-info" /> What will happen:
							</span>
							<ul class="impact-list">
								{#if mode === 'project'}
									<li>Ownership of this project and all services will be transferred to <strong>{selectedOrg.name}</strong>.</li>
									<li>The transfer runs asynchronously in the background with <strong>zero downtime</strong>.</li>
									<li>All existing volumes, networks, environment variables, domains, and deployments stay intact.</li>
								{:else}
									<li>This service will be detached from this project and put in <strong>Pending Claim</strong> status.</li>
									<li>An owner of <strong>{selectedOrg.name}</strong> must claim it from their <em>Transfers</em> page into one of their projects.</li>
									<li>Persistent volume data and configuration are preserved during claim.</li>
								{/if}
							</ul>
						</div>

						<div class="impact-section">
							<span class="impact-title">
								<ShieldAlert size={14} class="impact-icon-warn" /> What will change:
							</span>
							<ul class="impact-list">
								{#if mode === 'project'}
									<li>Members of this current organization will <strong>lose access</strong> to this project (unless they are also members of the target org).</li>
									<li>Billing, compute limits, and quota attribution will move to <strong>{selectedOrg.name}</strong>.</li>
								{:else}
									<li>Members of this organization will <strong>lose access</strong> once the service transfer is completed.</li>
									<li>The service will not receive traffic until claimed by the target organization.</li>
								{/if}
							</ul>
						</div>
					</div>

					<div class="panel-actions">
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
								<span>Confirm & Transfer</span>
							{/if}
						</Button>
					</div>
				</div>
			{/if}
		</div>
	</SlidePanel>
{/if}

<style>
	.panel-scrim {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		cursor: pointer;
	}

	.panel-body-content {
		display: flex;
		flex-direction: column;
		gap: 16px;
		padding: 20px;
		min-height: 100%;
		box-sizing: border-box;
	}

	.select-step {
		display: flex;
		flex-direction: column;
		gap: 16px;
		flex: 1;
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
		max-height: 380px;
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
		box-sizing: border-box;
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
		flex: 1;
	}

	.summary-card {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 16px;
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
		min-width: 0;
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
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
		flex-shrink: 0;
	}

	/* Impact section */
	.transfer-impact {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.impact-section {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 12px 14px;
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		border: 1px solid var(--border);
	}

	.impact-title {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-primary);
		display: flex;
		align-items: center;
		gap: 6px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	:global(.impact-icon-info) {
		color: var(--accent);
	}

	:global(.impact-icon-warn) {
		color: var(--accent-yellow);
	}

	.impact-list {
		margin: 0;
		padding-left: 18px;
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 12.5px;
		color: var(--text-secondary);
		line-height: 1.45;
	}

	.panel-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		margin-top: auto;
		padding-top: 12px;
	}

	.error-text {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}

	@media (max-width: 639px) {
		.panel-body-content {
			padding: 16px;
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
