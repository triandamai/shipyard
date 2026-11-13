<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { projectStore } from '$lib/stores/project.store';
	import { toastStore } from '$lib/stores/toast.store';
	import { Container, Network, Database, Globe, Compass, ArrowRight, ExternalLink } from '@lucide/svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let resourceType  = $derived((data.resource_type  as string) ?? 'service');
	let resourceName  = $derived((data.resource_name  as string) ?? 'Project');
	let projectName   = $derived((data.source_project_name as string) ?? 'Destination');
	let projectSlug   = $derived((data.source_project_slug as string) ?? '');

	const iconMap: Record<string, typeof Container> = {
		service: Container,
		network: Network,
		volume:  Database,
	};

	let Icon = $derived(iconMap[resourceType] ?? Globe);

	function handleTeleport(e: MouseEvent) {
		e.stopPropagation();
		const orgSlug = page.params.orgSlug;
		const destSlug = projectSlug
			|| $projectStore.projects.find((p) => p.id === data.source_project_id)?.slug
			|| projectName;

		if (destSlug && orgSlug) {
			toastStore.add({
				type: 'info',
				title: 'Teleporting...',
				message: `Entering portal to ${projectName}…`
			});
			goto(`/orgs/${orgSlug}/projects/${destSlug}`);
		}
	}
</script>

<Handle
	type="target"
	position={Position.Left}
	class="flow-handle portal-handle"
	title="Connect input service to portal"
/>

<div
	class="portal-node"
	class:selected
	onclick={handleTeleport}
	role="button"
	tabindex="0"
	onkeydown={(e) => { if (e.key === 'Enter') handleTeleport(e as any); }}
	title="Click to teleport to project {projectName}"
>
	<div class="portal-badge">
		<Compass size={11} class="portal-spin" />
		<span>Teleport Portal</span>
	</div>

	<div class="node-header">
		<div class="node-icon">
			<Icon size={14} />
		</div>
		<div class="node-title">
			<span class="node-name" title={resourceName}>{resourceName}</span>
			<span class="node-type">{resourceType}</span>
		</div>
	</div>

	<div class="node-body">
		<div class="dest-row">
			<span class="dest-label">Project:</span>
			<span class="dest-name" title={projectName}>{projectName}</span>
		</div>

		<button type="button" class="teleport-btn" onclick={handleTeleport}>
			<span>Enter Portal</span>
			<ArrowRight size={11} />
		</button>
	</div>
</div>

<Handle
	type="source"
	position={Position.Right}
	class="flow-handle portal-handle"
	title="Connect output from portal"
/>

<style>
	:global(.svelte-flow__handle.portal-handle) {
		width: 10px !important;
		height: 10px !important;
		background: var(--bg-surface) !important;
		border: 2px solid var(--accent-purple) !important;
		border-radius: 50% !important;
		transition: all var(--transition-fast) !important;
		z-index: 10 !important;
	}

	:global(.svelte-flow__handle.portal-handle:hover) {
		width: 13px !important;
		height: 13px !important;
		background: var(--accent-purple) !important;
		border-color: var(--accent-purple) !important;
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-purple) 35%, transparent) !important;
	}

	.portal-node {
		background: radial-gradient(circle at 50% 0%, color-mix(in srgb, var(--accent-purple) 18%, var(--bg-surface)), var(--bg-surface));
		border: 1.5px solid var(--accent-purple);
		border-radius: var(--radius-md);
		padding: 10px 13px;
		min-width: 200px;
		max-width: 260px;
		box-shadow: 0 0 16px color-mix(in srgb, var(--accent-purple) 22%, transparent), var(--shadow-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
		font-family: var(--font-sans);
		position: relative;
		user-select: none;
	}

	.portal-node:hover {
		box-shadow: 0 0 24px color-mix(in srgb, var(--accent-purple) 45%, transparent), var(--shadow-md);
		transform: translateY(-1px);
	}

	.portal-node.selected {
		border-color: var(--accent-purple);
		outline: 2px solid var(--accent-purple);
		outline-offset: 2px;
		box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent-purple) 25%, transparent), 0 0 24px color-mix(in srgb, var(--accent-purple) 35%, transparent);
	}

	.portal-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 9px;
		font-weight: 700;
		padding: 2px 7px;
		border-radius: 100px;
		background: color-mix(in srgb, var(--accent-purple) 22%, transparent);
		color: var(--accent-purple);
		border: 1px solid color-mix(in srgb, var(--accent-purple) 40%, transparent);
		margin-bottom: 8px;
		letter-spacing: 0.03em;
		text-transform: uppercase;
	}

	:global(.portal-spin) {
		animation: portal-spin 10s linear infinite;
	}

	@keyframes portal-spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
	}

	.node-header {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 8px;
	}

	.node-icon {
		width: 26px;
		height: 26px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--accent-purple) 20%, transparent);
		color: var(--accent-purple);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.node-title {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
	}

	.node-name {
		font-size: 12.5px;
		font-weight: 600;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		line-height: 1.25;
	}

	.node-type {
		font-size: 9.5px;
		color: var(--accent-purple);
		font-family: var(--font-mono);
		text-transform: capitalize;
		line-height: 1.2;
	}

	.node-body {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding-top: 6px;
		border-top: 1px solid color-mix(in srgb, var(--accent-purple) 25%, var(--border));
	}

	.dest-row {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 11px;
		min-width: 0;
	}

	.dest-label {
		color: var(--text-dim);
		flex-shrink: 0;
		font-size: 10px;
	}

	.dest-name {
		color: var(--accent-purple);
		font-weight: 600;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.teleport-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 5px;
		width: 100%;
		padding: 4px 8px;
		font-size: 10px;
		font-weight: 600;
		font-family: var(--font-sans);
		color: var(--accent-purple);
		background: color-mix(in srgb, var(--accent-purple) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent-purple) 30%, transparent);
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
	}

	.teleport-btn:hover {
		background: color-mix(in srgb, var(--accent-purple) 22%, transparent);
		border-color: var(--accent-purple);
		box-shadow: 0 0 10px color-mix(in srgb, var(--accent-purple) 30%, transparent);
	}
</style>
