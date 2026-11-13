<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { Globe, ExternalLink, ShieldCheck, Plus } from '@lucide/svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import DomainAddPanel from '$lib/panels/resources/DomainAddPanel.svelte';

	interface DomainItem {
		id: string;
		hostname: string;
		tls_enabled: boolean;
		port?: number | null;
	}

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let rawDomains = $derived(
		Array.isArray(data.domains)
			? (data.domains as DomainItem[])
			: (data.hostname ? [{ id: '1', hostname: data.hostname as string, tls_enabled: (data.tls_enabled as boolean) ?? true, port: data.port as number | undefined }] : [])
	);

	let count = $derived(rawDomains.length);
	let primaryTitle = $derived(count === 1 ? rawDomains[0].hostname : `${count} Custom Domains`);
	let allTls = $derived(rawDomains.length > 0 && rawDomains.every((d) => d.tls_enabled));

	function openAddDomain(e: MouseEvent) {
		e.stopPropagation();
		const sId = ((data.service_id as string) ?? '').replace(/^dom_svc_/, '').replace(/^svc_/, '');
		if (!sId) return;
		uiStore.pushPanel({
			component: DomainAddPanel,
			title: 'Add Domain',
			props: {
				serviceId: sId,
				onCreated: () => {
					if (typeof window !== 'undefined') {
						window.dispatchEvent(new CustomEvent('shipyard:sync-topology'));
					}
				}
			}
		});
	}
</script>

<Handle
	type="target"
	position={Position.Left}
	class="flow-handle"
	title="Connect input"
/>

<div class="node-stack-wrapper">
	<!-- Stacked peek layer with shortcut button to add a domain -->
	<div class="stack-layer domain-stack-peek" title="Add domain to this service">
		<button
			type="button"
			class="stack-add-btn"
			onclick={openAddDomain}
			title="Add domain to this service"
		>
			<Plus size={11} />
			<span>Add Domain</span>
		</button>
	</div>

	<div class="domain-node" class:selected>
		<div class="node-header">
			<div class="node-icon">
				<Globe size={14} />
			</div>
			<div class="node-title">
				<span class="node-name" title={primaryTitle}>{primaryTitle}</span>
				{#if count > 1}
					<span class="node-sub">{count} domains attached</span>
				{/if}
			</div>
			{#if allTls}
				<span class="tls-pill" title="TLS / HTTPS enabled">
					<ShieldCheck size={10} />
					<span>TLS</span>
				</span>
			{/if}
		</div>

		{#if count > 0}
			<div class="domain-list">
				{#each rawDomains as dom (dom.id || dom.hostname)}
					{@const url = `${dom.tls_enabled ? 'https' : 'http'}://${dom.hostname}`}
					<div class="domain-item">
						<div class="domain-info">
							<span class="domain-name" title={dom.hostname}>{dom.hostname}</span>
							{#if dom.port}
								<span class="port-chip">:{dom.port}</span>
							{/if}
						</div>

						<a
							class="domain-link-btn"
							href={url}
							target="_blank"
							rel="noopener noreferrer"
							title="Open {dom.hostname} in a new tab"
							aria-label="Open {dom.hostname} in a new tab"
							onclick={(e) => e.stopPropagation()}
						>
							<ExternalLink size={11} />
						</a>
					</div>
				{/each}
			</div>
		{/if}
	</div>
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
		background: var(--accent-green) !important;
		border-color: var(--accent-green) !important;
		box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent-green) 25%, transparent) !important;
	}

	.node-stack-wrapper {
		position: relative;
	}

	/* Stacked peek layer underneath domain node */
	.stack-layer {
		position: absolute;
		inset: 0;
		border-radius: var(--radius-md);
		background: color-mix(in srgb, var(--accent-green-muted) 35%, var(--bg-surface));
		border: 1px solid var(--border);
		box-shadow: var(--shadow-sm);
		transition: border-color var(--transition-fast);
		display: flex;
		align-items: flex-end;
		justify-content: center;
		padding: 0 10px 4px;
	}

	.domain-stack-peek {
		transform: translateY(20px);
		opacity: 1;
		z-index: 0;
	}

	.node-stack-wrapper:hover .domain-stack-peek {
		border-color: var(--accent-green);
	}

	.stack-add-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 10px;
		font-weight: 600;
		font-family: var(--font-sans);
		color: var(--accent-green);
		background: transparent;
		border: none;
		cursor: pointer;
		padding: 2px 8px;
		border-radius: var(--radius-sm);
		transition: all var(--transition-fast);
		line-height: 1;
	}

	.stack-add-btn:hover {
		background: var(--accent-green-muted);
		color: var(--accent-green-text);
		box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent-green) 30%, transparent);
	}

	.domain-node {
		position: relative;
		z-index: 1;
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

	.domain-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.domain-node.selected {
		border-color: var(--accent-green);
		outline: 2px solid var(--accent-green);
		outline-offset: 2px;
		box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent-green) 20%, transparent);
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

	.tls-pill {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 9.5px;
		font-weight: 700;
		padding: 2px 5px;
		border-radius: var(--radius-sm);
		background: var(--accent-green-muted);
		color: var(--accent-green-text);
		border: 1px solid color-mix(in srgb, var(--accent-green) 25%, transparent);
		flex-shrink: 0;
		letter-spacing: 0.02em;
	}

	.domain-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
		margin-top: 8px;
		padding-top: 6px;
		border-top: 1px solid var(--border);
	}

	.domain-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 6px;
		padding: 3px 6px;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
	}

	.domain-info {
		display: flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		flex: 1;
	}

	.domain-name {
		font-size: 10.5px;
		font-family: var(--font-mono);
		color: var(--text-secondary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.port-chip {
		font-size: 9px;
		font-family: var(--font-mono);
		font-weight: 600;
		color: var(--text-dim);
	}

	.domain-link-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		color: var(--text-dim);
		border-radius: var(--radius-sm);
		padding: 2px;
		transition: all var(--transition-fast);
	}

	.domain-link-btn:hover {
		color: var(--accent);
		background: var(--bg-surface);
	}
</style>
