<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { Globe } from '@lucide/svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let hostname   = $derived((data.hostname    as string)       ?? 'Domain');
	let tlsEnabled = $derived((data.tls_enabled as boolean)      ?? false);
	let port       = $derived((data.port        as number | null) ?? null);

	let protocol   = $derived(tlsEnabled ? 'https' : 'http');
	let defaultPort = $derived(tlsEnabled ? 443 : 80);
	let portLabel   = $derived(port !== null && port !== defaultPort ? `:${port}` : '');
</script>

<Handle type="target" position={Position.Left} />

<div class="domain-node" class:selected>
	<div class="node-header">
		<div class="node-icon">
			<Globe size={13} />
		</div>
		<div class="node-title">
			<span class="node-name" title={hostname}>{hostname}</span>
			<span class="node-sub">{tlsEnabled ? 'HTTPS · TLS on' : 'HTTP'}</span>
		</div>
		{#if tlsEnabled}
			<span class="tls-badge">TLS</span>
		{/if}
	</div>
	<div class="node-footer">
		<span class="port-url">
			<span class="protocol">{protocol}://</span>{hostname}{portLabel}
		</span>
		{#if port !== null}
			<span class="port-chip">→ :{port}</span>
		{/if}
	</div>
</div>

<Handle type="source" position={Position.Right} />

<style>
	.domain-node {
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

	.domain-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.domain-node.selected {
		border-color: var(--accent-green);
		box-shadow: 0 0 0 2px var(--accent-green-muted), var(--shadow-md);
	}

	.node-header {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.node-icon {
		width: 24px;
		height: 24px;
		border-radius: var(--radius-sm);
		background: var(--accent-green-muted);
		color: var(--accent-green);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.node-title {
		display: flex;
		flex-direction: column;
		gap: 1px;
		flex: 1;
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

	.tls-badge {
		font-size: 10px;
		font-weight: 700;
		padding: 1px 6px;
		border-radius: 100px;
		background: var(--accent-green-muted);
		color: var(--accent-green);
		flex-shrink: 0;
		letter-spacing: 0.03em;
	}

	.node-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		margin-top: 7px;
		padding-top: 7px;
		border-top: 1px solid var(--border);
	}

	.port-url {
		font-size: 10px;
		font-family: var(--font-mono);
		color: var(--text-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.protocol {
		color: var(--text-dim);
	}

	.port-chip {
		font-size: 10px;
		font-weight: 600;
		font-family: var(--font-mono);
		padding: 1px 6px;
		border-radius: 4px;
		background: color-mix(in srgb, var(--accent-green) 10%, transparent);
		color: var(--accent-green);
		border: 1px solid color-mix(in srgb, var(--accent-green) 25%, transparent);
		flex-shrink: 0;
		white-space: nowrap;
	}
</style>
