<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { Cpu } from '@lucide/svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	const STATUS_COLORS: Record<string, string> = {
		running:   '#22C55E',
		pending:   '#F59E0B',
		preparing: '#60A5FA',
		failed:    '#F87171',
		shutdown:  '#6B7280',
		complete:  '#9CA3AF',
		rejected:  '#F87171',
		orphan:    '#D1D5DB',
	};

	let status        = $derived((data.status as string) ?? 'pending');
	let statusColor   = $derived(STATUS_COLORS[status] ?? '#9CA3AF');
	let replicaIndex  = $derived(data.replica_index as number | null | undefined);
	let replicaLabel  = $derived(replicaIndex != null ? `replica-${replicaIndex}` : 'container');
	let containerId   = $derived((data.container_id as string | null | undefined) ?? '');
	let imageName     = $derived.by(() => {
		const img = data.image as string | null | undefined;
		if (!img) return null;
		const raw = img.split('/').pop() ?? img;
		return raw.length > 24 ? raw.slice(0, 24) + '…' : raw;
	});
</script>

<!-- Container nodes only receive edges (source is the service node) -->
<Handle type="target" position={Position.Left} />

<div class="container-node" class:selected>
	<div class="node-header">
		<div class="node-icon">
			<Cpu size={11} />
		</div>
		<div class="node-title">
			<div class="title-row">
				<span class="replica-label">{replicaLabel}</span>
				<span class="status-dot" style="background: {statusColor}" title={status}></span>
			</div>
			{#if imageName}
				<span class="image-name">{imageName}</span>
			{/if}
		</div>
	</div>
	{#if containerId}
		<div class="container-id">{containerId}</div>
	{/if}
</div>

<style>
	.container-node {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 8px 12px;
		min-width: 155px;
		max-width: 200px;
		box-shadow: var(--shadow-sm);
		font-family: var(--font-sans);
		transition: all var(--transition-fast);
	}

	.container-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.container-node.selected {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px color-mix(in srgb, var(--accent) 20%, transparent), var(--shadow-md);
	}

	.node-header {
		display: flex;
		align-items: flex-start;
		gap: 8px;
	}

	.node-icon {
		width: 20px;
		height: 20px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		color: var(--accent);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		margin-top: 1px;
	}

	.node-title {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.title-row {
		display: flex;
		align-items: center;
		gap: 5px;
	}

	.replica-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.status-dot {
		width: 7px;
		height: 7px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.image-name {
		font-size: 10px;
		color: var(--text-muted);
		font-family: var(--font-mono, monospace);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.container-id {
		margin-top: 5px;
		padding-left: 28px;
		font-size: 10px;
		color: var(--text-dim, var(--text-muted));
		font-family: var(--font-mono, monospace);
		opacity: 0.7;
	}
</style>
