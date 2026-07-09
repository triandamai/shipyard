<script lang="ts">
	import { Plus } from '@lucide/svelte';

	interface Props {
		data?: Record<string, unknown>;
		selected?: boolean;
	}

	let { selected = false }: Props = $props();
</script>

<div class="add-resource-node" class:selected>
	<div class="node-icon">
		<Plus size={16} />
	</div>
	<span class="node-name">Add Resource</span>
</div>

<style>
	.add-resource-node {
		background: transparent;
		border: 2px dashed var(--border);
		border-radius: var(--radius-lg);
		padding: 16px 20px;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		min-width: 140px;
		min-height: 80px;
		cursor: pointer;
		transition: all var(--transition-fast);
		font-family: var(--font-sans);
		color: var(--text-secondary);
	}

	.add-resource-node:hover {
		border-color: var(--accent);
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 4%, var(--bg-surface));
	}

	.add-resource-node.selected {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 20%, transparent);
	}

	.node-icon {
		width: 32px;
		height: 32px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--accent) 10%, var(--bg-elevated));
		color: var(--accent);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition: all var(--transition-fast);
	}
	
	.add-resource-node:hover .node-icon {
		background: var(--accent);
		color: white;
	}

	.node-name {
		font-size: 13px;
		font-weight: 600;
	}
</style>
