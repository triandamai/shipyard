<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { projectStore } from '$lib/stores/project.store';
	import { toastStore } from '$lib/stores/toast.store';
	import { FolderGit2, ArrowUpRight } from '@lucide/svelte';
	import { ConfirmDialog } from '$lib/components/ui';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	// Confirmation modal state
	let showConfirm = $state(false);

	let resourceType = $derived((data.resource_type as string) ?? 'project');
	let resourceName = $derived((data.resource_name as string) ?? '');
	let sourceProjectId = $derived((data.source_project_id as string) ?? '');

	let destProject = $derived(
		$projectStore.projects.find((p) => p.id === sourceProjectId || p.slug === data.source_project_slug)
	);

	let projectName = $derived(
		destProject?.name ?? (data.source_project_name as string) ?? 'Project'
	);
	let projectSlug = $derived(
		destProject?.slug ?? (data.source_project_slug as string) ?? ''
	);
	let orgSlug = $derived(page.params.orgSlug ?? '');
	let destSlug = $derived(projectSlug || projectName);

	// Action to portal modal to body so xyflow zoom/transform doesn't clip or offset fixed positioning
	function portalToBody(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() {
				if (node.parentNode) node.parentNode.removeChild(node);
			}
		};
	}

	function handleNodeClick(e: MouseEvent) {
		e.stopPropagation();
		showConfirm = true;
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			e.stopPropagation();
			showConfirm = true;
		}
	}

	function confirmOpenProject() {
		if (destSlug && orgSlug) {
			toastStore.add({
				type: 'info',
				title: 'Navigating',
				message: `Opening project ${projectName}…`
			});
			goto(`/orgs/${orgSlug}/projects/${destSlug}`);
		}
	}
</script>

<!-- Input connection handle (left) -->
<Handle
	type="target"
	position={Position.Left}
	class="flow-handle"
	title="Connect input to project"
/>

<div
	class="project-link-node"
	class:selected
	onclick={handleNodeClick}
	role="button"
	tabindex="0"
	onkeydown={handleKeydown}
	title="Click to open project {projectName}"
>
	<!-- Card Header -->
	<div class="node-header">
		<div class="node-icon-box">
			<FolderGit2 size={15} />
		</div>
		<div class="node-title-group">
			<div class="node-title-row">
				<span class="node-name" title={projectName}>{projectName}</span>
				<ArrowUpRight size={12} class="open-arrow" />
			</div>
			{#if projectSlug}
				<span class="node-slug" title={projectSlug}>{projectSlug}</span>
			{/if}
		</div>
		<span class="badge">Linked Project</span>
	</div>

	<!-- Project & Target Information -->
	<div class="node-body">
		<div class="info-row">
			<span class="info-label">Target</span>
			<span class="info-value" title={resourceName || 'Entire Project'}>
				{#if resourceType === 'service' && resourceName}
					{resourceName} <span class="sub-type">({resourceType})</span>
				{:else}
					Entire Project
				{/if}
			</span>
		</div>
		{#if orgSlug}
			<div class="info-row">
				<span class="info-label">Organization</span>
				<span class="info-value">{orgSlug}</span>
			</div>
		{/if}
	</div>
</div>

<!-- Output connection handle (right) -->
<Handle
	type="source"
	position={Position.Right}
	class="flow-handle"
	title="Connect output from project"
/>

<!-- Confirmation Modal attached to body -->
<div use:portalToBody>
	<ConfirmDialog
		bind:open={showConfirm}
		title="Open Project"
		message={`Are you sure you want to navigate to project "${projectName}"${projectSlug ? ` (${projectSlug})` : ''}? You will leave the current canvas.`}
		confirmLabel="Open Project"
		danger={false}
		onConfirm={confirmOpenProject}
	/>
</div>

<style>
	:global(.svelte-flow__handle.flow-handle) {
		width: 10px !important;
		height: 10px !important;
		background: var(--bg-surface) !important;
		border: 2px solid var(--border-control) !important;
		border-radius: 50% !important;
		transition: all var(--transition-fast) !important;
		z-index: 10 !important;
	}

	:global(.svelte-flow__handle.flow-handle:hover) {
		width: 13px !important;
		height: 13px !important;
		background: var(--accent) !important;
		border-color: var(--accent) !important;
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 25%, transparent) !important;
	}

	.project-link-node {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 10px 12px;
		min-width: 220px;
		max-width: 280px;
		box-shadow: var(--shadow-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
		font-family: var(--font-sans);
		user-select: none;
	}

	.project-link-node:hover {
		border-color: var(--border-hover);
		background: var(--bg-elevated);
		box-shadow: var(--shadow-md);
		transform: translateY(-1px);
	}

	.project-link-node.selected {
		border-color: var(--accent);
		outline: 2px solid var(--accent);
		outline-offset: 2px;
		box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 15%, transparent);
	}

	.node-header {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 8px;
	}

	.node-icon-box {
		width: 30px;
		height: 30px;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text-secondary);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.project-link-node:hover .node-icon-box {
		color: var(--text-primary);
		border-color: var(--border-hover);
	}

	.node-title-group {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}

	.node-title-row {
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.node-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		line-height: 1.25;
	}

	.open-arrow {
		color: var(--text-dim);
		flex-shrink: 0;
		transition: color var(--transition-fast);
	}

	.project-link-node:hover .open-arrow {
		color: var(--text-primary);
	}

	.node-slug {
		font-size: 10px;
		font-family: var(--font-mono);
		color: var(--text-dim);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		line-height: 1.2;
		margin-top: 1px;
	}

	.badge {
		font-size: 9.5px;
		font-weight: 500;
		color: var(--text-muted);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		padding: 2px 6px;
		border-radius: var(--radius-sm);
		white-space: nowrap;
		flex-shrink: 0;
	}

	.node-body {
		display: flex;
		flex-direction: column;
		gap: 5px;
		padding-top: 8px;
		border-top: 1px solid var(--border);
	}

	.info-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		font-size: 11px;
		min-width: 0;
	}

	.info-label {
		color: var(--text-dim);
		font-size: 10.5px;
		flex-shrink: 0;
	}

	.info-value {
		color: var(--text-secondary);
		font-weight: 500;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		text-align: right;
	}

	.sub-type {
		color: var(--text-dim);
		font-size: 9.5px;
		font-family: var(--font-mono);
	}
</style>
