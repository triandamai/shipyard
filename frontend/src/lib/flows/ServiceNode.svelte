<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { Layers } from '@lucide/svelte';
	import BrandLogo from '$lib/components/BrandLogo.svelte';
	import StatusDot from '$lib/components/ui/StatusDot.svelte';
	import { toDotStatus } from '$lib/utils/status';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let name            = $derived((data.name as string)            ?? 'Service');
	let slug            = $derived((data.slug as string)            ?? '');
	let status          = $derived((data.status as string)          ?? 'stopped');
	let replicas        = $derived((data.replicas as number)        ?? 0);
	let runningReplicas = $derived((data.running_replicas as number) ?? 0);
	let svcType         = $derived((data.type as string)            ?? '');
	let ports           = $derived(Array.isArray(data.ports) ? (data.ports as string[]) : []);

	// Replica stack card behind the node
	let showReplicaStack = $derived(replicas > 0);
	let replicaCountLabel = $derived(`${replicas} replica${replicas === 1 ? '' : 's'}`);
	let replicaStackTitle = $derived(`${runningReplicas}/${replicas} replica${replicas === 1 ? '' : 's'} running`);

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

	function statusTone(s: string): 'green' | 'blue' | 'yellow' | 'red' | 'neutral' {
		switch (s) {
			case 'running': return 'green';
			case 'deploying':
			case 'queued':
			case 'pending':
			case 'preparing': return 'blue';
			case 'stopping':
			case 'need_attention': return 'yellow';
			case 'failed':
			case 'rejected': return 'red';
			default: return 'neutral';
		}
	}

	let tone = $derived(statusTone(status));
</script>

<!-- Input connection handle (left) -->
<Handle
	type="target"
	position={Position.Left}
	class="flow-handle flow-handle-target"
	title="Connect input (e.g. from network or service)"
/>

<div class="node-stack-wrapper">
	{#if showReplicaStack}
		<div class="stack-layer replica-stack-peek" title={replicaStackTitle}>
			<Layers size={11} class="stack-icon" />
			<span class="stack-count-label">{replicaCountLabel}</span>
		</div>
	{/if}

	<!-- Main Service Card -->
	<div
		class="service-node"
		class:selected
		class:deploying={status === 'deploying'}
		class:stopping={status === 'stopping'}
		class:need-attention={status === 'need_attention'}
		class:failed={status === 'failed'}
		data-tone={tone}
	>
		<!-- Card Top / Header -->
		<div class="node-header">
			<div class="node-icon-box">
				<BrandLogo icon={data.icon as string | null} type={svcType} size={26} iconSize={15} class="node-icon" />
			</div>

			<div class="node-title-group">
				<div class="node-title-row">
					<span class="node-name" title={name}>{name}</span>
				</div>
				{#if slug}
					<span class="node-slug" title={slug}>{slug}</span>
				{/if}
			</div>

			<!-- Status pill badge -->
			<div class="status-pill status-pill-{tone}">
				<StatusDot status={toDotStatus(status)} />
				<span class="status-pill-text">{statusLabel(status)}</span>
			</div>
		</div>

		<!-- Card Body: Meta tags & Ports -->
		<div class="node-body">
			<div class="node-meta-row">
				{#if svcType}
					<span class="meta-tag type-tag">{svcType}</span>
				{/if}

				<span class="meta-tag replicas-tag" title={replicaStackTitle}>
					<span class="replica-count-indicator">{runningReplicas}/{replicas}</span> running
				</span>
			</div>

			{#if ports.length > 0}
				<div class="port-row">
					{#each ports as port (port)}
						<span class="port-tag">{port}</span>
					{/each}
				</div>
			{/if}
		</div>
	</div>
</div>

<!-- Output connection handle (right) -->
<Handle
	type="source"
	position={Position.Right}
	class="flow-handle flow-handle-source"
	title="Connect output (e.g. to network, volume, or service)"
/>

<style>
	/* Handle custom styles */
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

	.node-stack-wrapper {
		position: relative;
	}

	/* Replica stack peek underneath - static and visible */
	.stack-layer {
		position: absolute;
		inset: 0;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--bg-elevated) 85%, var(--bg-surface));
		border: 1px solid var(--border);
		box-shadow: var(--shadow-sm);
		cursor: pointer;
		transition: border-color var(--transition-fast);
		display: flex;
		align-items: flex-end;
		justify-content: center;
		gap: 5px;
		padding: 0 10px 5px;
		color: var(--text-secondary);
	}

	.replica-stack-peek {
		transform: translateY(20px);
		opacity: 1;
		z-index: 0;
	}

	.node-stack-wrapper:hover .replica-stack-peek {
		border-color: var(--border-hover);
	}

	.stack-icon {
		flex-shrink: 0;
		color: var(--text-muted);
	}

	.stack-count-label {
		font-size: 10px;
		font-weight: 600;
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		min-width: 0;
		line-height: 1;
	}

	/* Main service card */
	.service-node {
		position: relative;
		z-index: 1;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 11px 13px;
		min-width: 220px;
		max-width: 280px;
		cursor: pointer;
		box-shadow: var(--shadow-sm);
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast), transform var(--transition-fast);
		font-family: var(--font-sans);
		user-select: none;
	}

	.service-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.service-node.selected {
		border-color: var(--accent);
		outline: 2px solid var(--accent);
		outline-offset: 2px;
		box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 15%, transparent);
	}

	.service-node.deploying {
		border-color: var(--accent-blue);
		animation: node-deploy-pulse 1.4s ease-in-out infinite;
	}

	.service-node.stopping {
		border-color: var(--accent-yellow);
		animation: node-stop-pulse 1.8s ease-in-out infinite;
	}

	.service-node.need-attention {
		border-color: var(--accent-yellow);
	}

	.service-node.failed {
		border-color: var(--accent-red);
	}

	@keyframes node-deploy-pulse {
		0%, 100% { box-shadow: 0 0 0 0 transparent; }
		50%       { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-blue) 30%, transparent); }
	}

	@keyframes node-stop-pulse {
		0%, 100% { box-shadow: 0 0 0 0 transparent; }
		50%       { box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-yellow) 30%, transparent); }
	}

	/* Header layout */
	.node-header {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 8px;
	}

	.node-icon-box {
		width: 32px;
		height: 32px;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
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

	/* Status pill badge */
	.status-pill {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 2px 6px;
		border-radius: var(--radius-sm);
		font-size: 10px;
		font-weight: 500;
		white-space: nowrap;
		flex-shrink: 0;
		border: 1px solid transparent;
	}

	.status-pill-green {
		background: var(--accent-green-muted);
		color: var(--accent-green-text);
		border-color: color-mix(in srgb, var(--accent-green) 20%, transparent);
	}

	.status-pill-blue {
		background: var(--accent-blue-muted);
		color: var(--accent-blue-text);
		border-color: color-mix(in srgb, var(--accent-blue) 20%, transparent);
	}

	.status-pill-yellow {
		background: var(--accent-yellow-muted);
		color: var(--accent-yellow-text);
		border-color: color-mix(in srgb, var(--accent-yellow) 20%, transparent);
	}

	.status-pill-red {
		background: var(--accent-red-muted);
		color: var(--accent-red-text);
		border-color: color-mix(in srgb, var(--accent-red) 20%, transparent);
	}

	.status-pill-neutral {
		background: var(--bg-elevated);
		color: var(--text-muted);
		border-color: var(--border);
	}

	.status-pill-text {
		font-size: 10px;
		line-height: 1;
	}

	/* Body Layout */
	.node-body {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding-top: 4px;
		border-top: 1px solid var(--border);
	}

	.node-meta-row {
		display: flex;
		align-items: center;
		gap: 5px;
		flex-wrap: wrap;
	}

	.meta-tag {
		font-size: 10px;
		font-weight: 500;
		padding: 2px 6px;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		line-height: 1.2;
	}

	.type-tag {
		text-transform: capitalize;
		font-weight: 600;
	}

	.replicas-tag {
		color: var(--text-muted);
	}

	.replica-count-indicator {
		font-family: var(--font-mono);
		font-weight: 600;
		color: var(--text-primary);
	}

	.port-row {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-wrap: wrap;
	}

	.port-tag {
		font-size: 9.5px;
		font-family: var(--font-mono);
		font-weight: 500;
		padding: 1px 5px;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		color: var(--text-secondary);
		border: 1px solid var(--border);
	}
</style>
