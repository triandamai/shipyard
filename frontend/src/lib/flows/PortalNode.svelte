<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { Container, Network, Database, Globe } from '@lucide/svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let resourceType  = $derived((data.resource_type  as string) ?? 'service');
	let resourceName  = $derived((data.resource_name  as string) ?? 'Unknown');
	let projectName   = $derived((data.source_project_name as string) ?? '');
	let envKey        = $derived((data.env_key        as string) ?? '');
	let fullMatch     = $derived((data.full_match     as string) ?? '');

	const iconMap: Record<string, typeof Container> = {
		service: Container,
		network: Network,
		volume:  Database,
	};

	let Icon = $derived(iconMap[resourceType] ?? Globe);
</script>

<Handle type="target" position={Position.Left} />

<div class="portal-node" class:selected>
	<div class="cross-chip">
		<Globe size={9} />
		Cross-project
	</div>

	<div class="node-header">
		<div class="node-icon">
			<Icon size={13} />
		</div>
		<div class="node-title">
			<span class="node-name" title={resourceName}>{resourceName}</span>
			<span class="node-type">{resourceType}</span>
		</div>
	</div>

	<div class="node-body">
		<div class="from-row">
			<span class="from-label">from</span>
			<span class="from-value">{projectName}</span>
		</div>
		{#if envKey}
			<div class="env-row">
				<span class="env-key">{envKey}</span>
				<span class="env-sep">←</span>
				<span class="env-match">{fullMatch}</span>
			</div>
		{/if}
	</div>
</div>

<style>
	.portal-node {
		background: color-mix(in srgb, #7c3aed 6%, var(--bg-surface));
		border: 1.5px dashed #7c3aed;
		border-radius: var(--radius-md);
		padding: 10px 14px;
		min-width: 190px;
		max-width: 250px;
		box-shadow: 0 0 12px rgba(124, 58, 237, 0.15), var(--shadow-sm);
		cursor: default;
		transition: all var(--transition-fast);
		font-family: var(--font-sans);
		position: relative;
	}

	.portal-node:hover {
		box-shadow: 0 0 20px rgba(124, 58, 237, 0.25), var(--shadow-md);
		border-color: #a78bfa;
	}

	.portal-node.selected {
		border-color: #a78bfa;
		box-shadow: 0 0 0 2px rgba(124, 58, 237, 0.3), 0 0 20px rgba(124, 58, 237, 0.3);
	}

	.cross-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.06em;
		text-transform: uppercase;
		padding: 2px 6px;
		border-radius: 100px;
		background: rgba(124, 58, 237, 0.18);
		color: #a78bfa;
		border: 1px solid rgba(124, 58, 237, 0.35);
		margin-bottom: 8px;
	}

	.node-header {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		margin-bottom: 8px;
	}

	.node-icon {
		width: 24px;
		height: 24px;
		border-radius: var(--radius-sm);
		background: rgba(124, 58, 237, 0.18);
		color: #a78bfa;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		margin-top: 1px;
	}

	.node-title {
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}

	.node-name {
		font-size: 13px;
		font-weight: 600;
		color: #c4b5fd;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		line-height: 1.3;
	}

	.node-type {
		font-size: 10px;
		color: #7c3aed;
		font-family: var(--font-mono);
		text-transform: capitalize;
	}

	.node-body {
		display: flex;
		flex-direction: column;
		gap: 5px;
	}

	.from-row {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 11px;
	}

	.from-label {
		color: var(--text-dim);
		flex-shrink: 0;
	}

	.from-value {
		color: #a78bfa;
		font-weight: 500;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.env-row {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 10px;
		font-family: var(--font-mono);
		background: rgba(124, 58, 237, 0.1);
		border: 1px solid rgba(124, 58, 237, 0.2);
		border-radius: 4px;
		padding: 2px 6px;
	}

	.env-key   { color: #c4b5fd; }
	.env-sep   { color: var(--text-dim); }
	.env-match { color: #a78bfa; }
</style>
