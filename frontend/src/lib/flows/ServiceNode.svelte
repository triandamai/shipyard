<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import BrandLogo from '$lib/components/BrandLogo.svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let name     = $derived((data.name as string)     ?? 'Service');
	let slug     = $derived((data.slug as string)     ?? '');
	let status   = $derived((data.status as string)   ?? 'stopped');
	let replicas = $derived((data.replicas as number) ?? 0);
	let svcType  = $derived((data.type as string)     ?? '');
	let ports    = $derived(Array.isArray(data.ports) ? (data.ports as string[]) : []);

	type StatusKey = 'running' | 'deploying' | 'stopping' | 'pending' | 'failed' | 'need_attention' | 'stopped';

	function statusClass(s: string): StatusKey {
		if (s === 'running')                              return 'running';
		if (s === 'deploying')                            return 'deploying';
		if (s === 'stopping')                             return 'stopping';
		if (s === 'need_attention')                       return 'need_attention';
		if (s === 'pending' || s === 'preparing' || s === 'queued') return 'pending';
		if (s === 'failed'  || s === 'rejected')          return 'failed';
		return 'stopped';
	}

	function statusLabel(s: string): string {
		return ({
			running:        'Running',
			deploying:      'Deploying',
			stopping:       'Stopping',
			queued:         'Queued',
			need_attention: 'Need attention',
			pending:        'Pending',
			preparing:      'Preparing',
			failed:         'Failed',
			rejected:       'Rejected',
			stopped:        'Stopped'
		} as Record<string, string>)[s] ?? s;
	}
</script>

<Handle type="target" position={Position.Left} />

<div class="service-node"
	class:selected
	class:deploying={status === 'deploying'}
	class:stopping={status === 'stopping'}
	class:need-attention={status === 'need_attention'}
>
	<div class="node-header">
		<BrandLogo icon={data.icon as string | null} type={svcType} size={24} iconSize={13} class="node-icon" />
		<div class="node-title">
			<span class="node-name" title={name}>{name}</span>
			{#if slug}
				<span class="node-slug">{slug}</span>
			{/if}
		</div>
	</div>

	<div class="node-body">
		<div class="node-status">
			<span class="status-dot {statusClass(status)}"></span>
			<span class="status-text">{statusLabel(status)}</span>
		</div>

		<div class="node-meta">
			{#if svcType}
				<span class="meta-chip">{svcType}</span>
			{/if}
			<span class="meta-chip replicas-chip">
				{replicas} replica{replicas === 1 ? '' : 's'}
			</span>
		</div>

		{#if ports.length > 0}
			<div class="port-row">
				{#each ports as port (port)}
					<span class="port-chip">{port}</span>
				{/each}
			</div>
		{/if}
	</div>
</div>

<Handle type="source" position={Position.Right} />

<style>
	.service-node {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 10px 14px;
		min-width: 190px;
		max-width: 250px;
		box-shadow: var(--shadow-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
		font-family: var(--font-sans);
	}

	.service-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.service-node.selected {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px var(--accent-muted), var(--shadow-md);
	}

	.service-node.deploying {
		border-color: #3b82f6;
		animation: node-deploy-pulse 1.4s ease-in-out infinite;
	}

	.service-node.stopping {
		border-color: #f59e0b;
		animation: node-deploy-pulse 1.8s ease-in-out infinite;
	}

	.service-node.need-attention {
		border-color: #f97316;
	}

	@keyframes node-deploy-pulse {
		0%, 100% { box-shadow: 0 0 0 0 transparent; }
		50%       { box-shadow: 0 0 0 3px color-mix(in srgb, #3b82f6 30%, transparent); }
	}

	.service-node.stopping {
		animation-name: node-stop-pulse;
	}
	@keyframes node-stop-pulse {
		0%, 100% { box-shadow: 0 0 0 0 transparent; }
		50%       { box-shadow: 0 0 0 3px color-mix(in srgb, #f59e0b 30%, transparent); }
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
		background: var(--accent-muted);
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
		line-height: 1.3;
	}

	.node-slug {
		font-size: 10px;
		color: var(--text-dim);
		font-family: var(--font-mono);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.node-body {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.node-status {
		display: flex;
		align-items: center;
		gap: 5px;
	}

	.status-text {
		font-size: 11px;
		color: var(--text-muted);
	}

	.node-meta {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-wrap: wrap;
	}

	.meta-chip {
		font-size: 10px;
		font-weight: 500;
		padding: 1px 6px;
		border-radius: 100px;
		background: var(--bg-elevated);
		color: var(--text-muted);
		border: 1px solid var(--border);
		text-transform: capitalize;
	}

	.replicas-chip {
		background: var(--accent-blue-muted);
		color: var(--accent-blue);
		border-color: transparent;
	}

	.port-row {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-wrap: wrap;
		margin-top: 2px;
	}

	.port-chip {
		font-size: 10px;
		font-weight: 600;
		font-family: var(--font-mono);
		padding: 1px 6px;
		border-radius: 4px;
		background: color-mix(in srgb, var(--accent) 8%, transparent);
		color: var(--accent);
		border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
	}
</style>
