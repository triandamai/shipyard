<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { Code2 } from '@lucide/svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let name   = $derived((data.name as string) ?? 'App');
	let slug   = $derived((data.slug as string) ?? '');
	let status = $derived((data.status as string) ?? 'stopped');
</script>

<Handle type="target" position={Position.Left} />

<div class="sandbox-app-node" class:selected>
	<div class="node-header">
		<div class="node-icon"><Code2 size={13} /></div>
		<div class="node-title">
			<span class="node-name" title={name}>{name}</span>
			{#if slug}
				<span class="node-slug">{slug}</span>
			{/if}
		</div>
	</div>
	<span class="status-dot" class:running={status === 'running'}></span>
</div>

<Handle type="source" position={Position.Right} />

<style>
	.sandbox-app-node {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 10px 14px;
		min-width: 190px;
		max-width: 250px;
		box-shadow: var(--shadow-sm);
		cursor: pointer;
		display: flex;
		align-items: center;
		gap: 8px;
		font-family: var(--font-sans);
	}
	.sandbox-app-node.selected {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px var(--accent-muted), var(--shadow-md);
	}
	.node-icon {
		width: 24px;
		height: 24px;
		border-radius: var(--radius-sm);
		background: var(--accent-muted);
		color: var(--accent);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
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
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.node-slug {
		font-size: 10px;
		color: var(--text-dim);
		font-family: var(--font-mono);
	}
	.status-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--text-dim);
		flex-shrink: 0;
		margin-left: auto;
	}
	.status-dot.running {
		background: var(--status-running, #22c55e);
	}
</style>
