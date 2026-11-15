<script lang="ts">
	import SlidePanel from '$lib/components/SlidePanel.svelte';
	import { Button, InlineAlert, Spinner } from '$lib/components/ui';
	import { FolderOpen, ArrowRight, AlertTriangle, Layers, HardDrive } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import type { ServiceTransferItem, Project } from '$lib/api/types';

	interface Props {
		open: boolean;
		transfer: ServiceTransferItem | null;
		orgId: string;
		zIndex?: number;
		onClaim: (transferId: string, targetProjectId: string) => Promise<boolean | void>;
		onClose: () => void;
	}

	let {
		open = $bindable(false),
		transfer,
		orgId,
		zIndex = 80,
		onClaim,
		onClose
	}: Props = $props();

	let projects = $state<Project[]>([]);
	let loadingProjects = $state(false);
	let selectedProjectId = $state('');
	let submitting = $state(false);
	let errorMessage = $state('');

	$effect(() => {
		if (open && orgId) {
			selectedProjectId = '';
			errorMessage = '';
			loadingProjects = true;
			api.getProjects(orgId).then((res) => {
				if (res.data) {
					projects = res.data;
					if (projects.length > 0) {
						selectedProjectId = projects[0].id;
					}
				} else if (res.error) {
					errorMessage = res.error.message;
				}
				loadingProjects = false;
			});
		}
	});

	async function handleClaim() {
		if (!transfer || !selectedProjectId) return;
		submitting = true;
		errorMessage = '';
		try {
			const ok = await onClaim(transfer.id, selectedProjectId);
			if (ok !== false) {
				open = false;
				onClose();
			}
		} catch (err: any) {
			errorMessage = err.message || 'Failed to claim service transfer';
		} finally {
			submitting = false;
		}
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
		aria-label="Close claim panel"
	></div>

	<SlidePanel
		title="Claim Incoming Service"
		onClose={handleCloseModal}
		{zIndex}
	>
		<div class="panel-body-content">
			{#if errorMessage}
				<InlineAlert tone="error">
					<span class="error-text"><AlertTriangle size={14} /> {errorMessage}</span>
				</InlineAlert>
			{/if}

			{#if transfer}
				<div class="transfer-info">
					<div class="info-row">
						<span class="info-label">Service</span>
						<span class="info-val">{transfer.service_name} <span class="dim">({transfer.service_slug})</span></span>
					</div>
					<div class="info-row">
						<span class="info-label">From</span>
						<span class="info-val">{transfer.source_org_name} / {transfer.source_project_name}</span>
					</div>
				</div>

				<div class="note-box">
					<HardDrive size={15} class="note-icon" />
					<div class="note-text">
						Persistent volumes will be preserved and moved into the target project. The container image will be re-built/re-deployed in your project network.
					</div>
				</div>

				<div class="field-wrap">
					<label for="project-select" class="field-label">Select Target Project in Your Organization</label>
					{#if loadingProjects}
						<div class="loading-state">
							<Spinner size={16} />
							<span>Loading projects…</span>
						</div>
					{:else if projects.length === 0}
						<InlineAlert tone="warning">
							<span>No projects found in this organization. Please create a project first before claiming this service.</span>
						</InlineAlert>
					{:else}
						<div class="project-options">
							{#each projects as proj}
								<button
									type="button"
									class="project-card"
									class:selected={selectedProjectId === proj.id}
									onclick={() => (selectedProjectId = proj.id)}
								>
									<div class="proj-icon">
										<FolderOpen size={16} />
									</div>
									<div class="proj-meta">
										<span class="proj-name">{proj.name}</span>
										<span class="proj-slug">{proj.slug}</span>
									</div>
									<div class="radio-indicator"></div>
								</button>
							{/each}
						</div>
					{/if}
				</div>

				<div class="panel-actions">
					<Button variant="ghost" onclick={handleCloseModal} disabled={submitting}>Cancel</Button>
					<Button
						variant="primary"
						onclick={handleClaim}
						disabled={submitting || !selectedProjectId || projects.length === 0}
					>
						{#if submitting}
							<Spinner size={14} />
							<span>Claiming…</span>
						{:else}
							<span>Claim Service into Project</span>
						{/if}
					</Button>
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

	.transfer-info {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 14px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
	}

	.info-row {
		display: flex;
		justify-content: space-between;
		align-items: center;
		font-size: 13px;
		gap: 8px;
	}

	.info-label {
		color: var(--text-muted);
		font-weight: 500;
		flex-shrink: 0;
	}

	.info-val {
		color: var(--text-primary);
		font-weight: 600;
		text-align: right;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.dim {
		font-weight: 400;
		color: var(--text-muted);
		font-family: var(--font-mono);
		font-size: 11px;
	}

	.note-box {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 12px;
		background: var(--bg-elevated);
		border-radius: var(--radius-sm);
		font-size: 12px;
		color: var(--text-secondary);
		line-height: 1.45;
	}

	:global(.note-icon) {
		color: var(--accent);
		flex-shrink: 0;
		margin-top: 1px;
	}

	.note-text {
		flex: 1;
	}

	.field-wrap {
		display: flex;
		flex-direction: column;
		gap: 10px;
		flex: 1;
	}

	.field-label {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-primary);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.loading-state {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-muted);
		font-size: 13px;
		padding: 20px 0;
	}

	.project-options {
		display: flex;
		flex-direction: column;
		gap: 8px;
		max-height: 300px;
		overflow-y: auto;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 6px;
		background: var(--bg-surface);
	}

	.project-card {
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

	.project-card:hover {
		background: var(--bg-elevated);
		border-color: var(--border);
	}

	.project-card.selected {
		background: var(--accent-subtle, rgba(59, 130, 246, 0.08));
		border-color: var(--accent);
	}

	.proj-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border-radius: var(--radius-sm);
		background: var(--bg-base);
		color: var(--text-primary);
		flex-shrink: 0;
	}

	.proj-meta {
		display: flex;
		flex-direction: column;
		gap: 2px;
		flex: 1;
		min-width: 0;
	}

	.proj-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.proj-slug {
		font-size: 11px;
		font-family: var(--font-mono);
		color: var(--text-muted);
	}

	.radio-indicator {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		border: 2px solid var(--border);
		background: transparent;
		transition: all var(--transition-fast);
		flex-shrink: 0;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	.project-card.selected .radio-indicator {
		border-color: var(--accent);
		background: var(--accent);
		box-shadow: inset 0 0 0 3px var(--bg-surface);
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
	}
</style>
