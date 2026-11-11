<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { HardDrive, Folder } from '@lucide/svelte';

	interface VolumeItem {
		id: string;
		name: string;
		mount_path: string;
		size_mb?: number;
	}

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let rawVolumes = $derived(
		Array.isArray(data.volumes)
			? (data.volumes as VolumeItem[])
			: (data.name ? [{ id: '1', name: data.name as string, mount_path: (data.mount_path as string) ?? '', size_mb: data.size_mb as number | undefined }] : [])
	);

	let count = $derived(rawVolumes.length);
	let primaryTitle = $derived(count === 1 ? rawVolumes[0].name : `${count} Attached Volumes`);

	function fmtSize(mb?: number): string | null {
		if (mb == null || mb === 0) return null;
		return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
	}

	let totalSizeMb = $derived(
		rawVolumes.reduce((acc, v) => acc + (v.size_mb ?? 0), 0)
	);
</script>

<Handle
	type="target"
	position={Position.Left}
	class="flow-handle"
	title="Connect input"
/>

<div class="volume-node" class:selected>
	<div class="node-header">
		<div class="node-icon">
			<HardDrive size={14} />
		</div>
		<div class="node-title">
			<span class="node-name" title={primaryTitle}>{primaryTitle}</span>
			{#if count > 1}
				<span class="node-sub">{count} volumes grouped</span>
			{/if}
		</div>
		{#if totalSizeMb > 0}
			<span class="size-pill">
				{fmtSize(totalSizeMb)}
			</span>
		{/if}
	</div>

	{#if count > 0}
		<div class="volume-list">
			{#each rawVolumes as vol (vol.id || vol.name)}
				<div class="volume-item">
					<div class="volume-info">
						<Folder size={11} class="folder-icon" />
						<span class="mount-path" title={vol.mount_path}>{vol.mount_path || vol.name}</span>
					</div>
					{#if vol.size_mb}
						<span class="vol-size">{fmtSize(vol.size_mb)}</span>
					{/if}
				</div>
			{/each}
		</div>
	{/if}
</div>

<Handle
	type="source"
	position={Position.Right}
	class="flow-handle"
	title="Connect output"
/>

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
		background: var(--accent-yellow) !important;
		border-color: var(--accent-yellow) !important;
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-yellow) 25%, transparent) !important;
	}

	.volume-node {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 10px 13px;
		min-width: 190px;
		max-width: 260px;
		box-shadow: var(--shadow-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
		font-family: var(--font-sans);
		user-select: none;
	}

	.volume-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.volume-node.selected {
		border-color: var(--accent-yellow);
		outline: 2px solid var(--accent-yellow);
		outline-offset: 2px;
		box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent-yellow) 20%, transparent);
	}

	.node-header {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.node-icon {
		width: 28px;
		height: 28px;
		border-radius: var(--radius-sm);
		background: var(--accent-yellow-muted);
		color: var(--accent-yellow);
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

	.node-sub {
		font-size: 10px;
		color: var(--text-dim);
		line-height: 1.2;
		margin-top: 1px;
	}

	.size-pill {
		font-size: 9.5px;
		font-family: var(--font-mono);
		font-weight: 600;
		padding: 2px 5px;
		border-radius: var(--radius-sm);
		background: var(--accent-yellow-muted);
		color: var(--accent-yellow-text);
		border: 1px solid color-mix(in srgb, var(--accent-yellow) 25%, transparent);
		flex-shrink: 0;
	}

	.volume-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-top: 8px;
		padding-top: 6px;
		border-top: 1px solid var(--border);
	}

	.volume-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		padding: 3px 6px;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
	}

	.volume-info {
		display: flex;
		align-items: center;
		gap: 5px;
		min-width: 0;
		flex: 1;
	}

	:global(.volume-info svg) {
		flex-shrink: 0;
		color: var(--text-dim);
	}

	.mount-path {
		font-size: 10.5px;
		font-family: var(--font-mono);
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.vol-size {
		font-size: 9px;
		font-family: var(--font-mono);
		color: var(--text-dim);
		flex-shrink: 0;
	}
</style>
