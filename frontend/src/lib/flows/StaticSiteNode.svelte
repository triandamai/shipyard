<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { Globe } from '@lucide/svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let name         = $derived((data.name as string)         ?? 'Static Site');
	let slug         = $derived((data.slug as string)         ?? '');
	let deployStatus = $derived((data.deploy_status as string) ?? 'none');
	let source       = $derived((data.source as string)       ?? 'git');
	let domains      = $derived(Array.isArray(data.domains) ? (data.domains as string[]) : []);

	function deployStatusClass(s: string): string {
		if (s === 'success')                             return 'running';
		if (s === 'running' || s === 'pending')          return 'pending';
		if (s === 'failed')                              return 'failed';
		return 'stopped';
	}

	function deployStatusLabel(s: string): string {
		return ({ success: 'Live', running: 'Deploying', pending: 'Queued',
		          failed: 'Failed', none: 'Not deployed' } as Record<string, string>)[s] ?? s;
	}
	const BRAND_COLORS: Record<string, string> = {
		postgresql: '#336791',
		mysql:      '#00758f',
		redis:      '#dc382d',
		mongodb:    '#47a248',
		nextjs:     '#000000',
		nuxtjs:     '#00dc82',
		sveltekit:  '#ff3e00',
		astro:      '#bc52ee',
		vite:       '#646cff',
		vue:        '#4fc08d',
		react:      '#61dafb',
		angular:    '#dd0031',
		solid:      '#2c4f7c',
		docker:     '#2496ed',
		html5:      '#e34f26',
	};

	let hasIconError = $state(false);
	let iconName = $derived(data.icon as string | null);
	let brandColor = $derived(iconName && !hasIconError ? BRAND_COLORS[iconName] : null);
</script>

<Handle type="target" position={Position.Left} />

<div class="static-node" class:selected>
	<div class="node-header">
		<div
			class="node-icon"
			style={brandColor ? `background: ${brandColor}12; color: ${brandColor}; border: 1px solid ${brandColor}24;` : ''}
		>
			{#if iconName && !hasIconError}
				<img
					src="/brands/{iconName}/logo.svg"
					alt={name}
					style="width: 14px; height: 14px; object-fit: contain;"
					onerror={() => hasIconError = true}
				/>
			{:else}
				<Globe size={13} />
			{/if}
		</div>
		<div class="node-title">
			<span class="node-name" title={name}>{name}</span>
			{#if slug}
				<span class="node-slug">{slug}</span>
			{/if}
		</div>
	</div>

	<div class="node-body">
		<div class="node-status">
			<span class="status-dot {deployStatusClass(deployStatus)}"></span>
			<span class="status-text">{deployStatusLabel(deployStatus)}</span>
		</div>

		<div class="node-meta">
			<span class="meta-chip source-chip">{source}</span>
		</div>

		{#if domains.length > 0}
			<div class="domain-row">
				{#each domains.slice(0, 2) as d (d)}
					<span class="domain-chip" title={d}>{d}</span>
				{/each}
				{#if domains.length > 2}
					<span class="domain-more">+{domains.length - 2}</span>
				{/if}
			</div>
		{/if}
	</div>
</div>

<Handle type="source" position={Position.Right} />

<style>
	.static-node {
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

	.static-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.static-node.selected {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px var(--accent-muted), var(--shadow-md);
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
		background: color-mix(in srgb, #22c55e 12%, transparent);
		color: #22c55e;
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

	.source-chip {
		background: color-mix(in srgb, #22c55e 10%, transparent);
		color: #22c55e;
		border-color: color-mix(in srgb, #22c55e 25%, transparent);
	}

	.domain-row {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-wrap: wrap;
		margin-top: 2px;
	}

	.domain-chip {
		font-size: 10px;
		font-weight: 500;
		font-family: var(--font-mono);
		padding: 1px 6px;
		border-radius: 4px;
		background: color-mix(in srgb, #22c55e 8%, transparent);
		color: #22c55e;
		border: 1px solid color-mix(in srgb, #22c55e 25%, transparent);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		max-width: 120px;
	}

	.domain-more {
		font-size: 10px;
		color: var(--text-dim);
	}

	:global .status-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		flex-shrink: 0;
	}

	:global .status-dot.running  { background: var(--status-running, #22c55e); }
	:global .status-dot.pending  { background: var(--status-pending, #f59e0b); }
	:global .status-dot.failed   { background: var(--status-failed,  #ef4444); }
	:global .status-dot.stopped  { background: var(--status-stopped, #6b7280); }
</style>
