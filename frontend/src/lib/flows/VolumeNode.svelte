<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { HardDrive } from '@lucide/svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let name      = $derived((data.name       as string) ?? 'Volume');
	let mountPath = $derived((data.mount_path as string) ?? '');
	let sizeMb    = $derived(data.size_mb as number | undefined);

	function fmtSize(mb: number): string {
		return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
	}
</script>

<Handle type="target" position={Position.Left} />

<div class="volume-node" class:selected>
	<div class="node-header">
		<div class="node-icon">
			<HardDrive size={13} />
		</div>
		<div class="node-title">
			<span class="node-name" title={name}>{name}</span>
			{#if sizeMb != null}
				<span class="node-sub">{fmtSize(sizeMb)}</span>
			{/if}
		</div>
	</div>
	{#if mountPath}
		<div class="node-meta">
			<span class="meta-mono">{mountPath}</span>
		</div>
	{/if}
</div>

<Handle type="source" position={Position.Right} />

<style>
	.volume-node {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 10px 14px;
		min-width: 160px;
		max-width: 220px;
		box-shadow: var(--shadow-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
		font-family: var(--font-sans);
	}

	.volume-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.volume-node.selected {
		border-color: var(--accent-yellow);
		box-shadow: 0 0 0 2px var(--accent-yellow-muted), var(--shadow-md);
	}

	.node-header {
		display: flex;
		align-items: flex-start;
		gap: 8px;
	}

	.node-icon {
		width: 24px;
		height: 24px;
		border-radius: var(--radius-sm);
		background: var(--accent-yellow-muted);
		color: var(--accent-yellow);
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
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.node-sub {
		font-size: 10px;
		color: var(--text-dim);
	}

	.node-meta {
		margin-top: 6px;
		padding-left: 32px;
	}

	.meta-mono {
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}
</style>
