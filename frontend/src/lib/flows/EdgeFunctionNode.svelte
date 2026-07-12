<script lang="ts">
	import { Handle, Position } from '@xyflow/svelte';
	import { Zap } from '@lucide/svelte';

	interface Props {
		data: Record<string, unknown>;
		selected?: boolean;
	}

	let { data, selected = false }: Props = $props();

	let repoName      = $derived((data.repo_name as string)      ?? (data.repo_url as string) ?? 'Edge Functions');
	let branch        = $derived((data.branch as string)         ?? 'main');
	let provider      = $derived((data.provider as string)       ?? '');
	let fnCount       = $derived((data.function_count as number) ?? 0);
	let activeCount   = $derived((data.active_count as number)   ?? 0);
	let lastSha       = $derived((data.last_deployed_sha as string | null) ?? null);

	let shortSha = $derived(lastSha ? lastSha.slice(0, 7) : null);

	const PROVIDER_COLOR: Record<string, string> = {
		github:    '#24292f',
		gitlab:    '#FC6D26',
		bitbucket: '#0052CC',
	};

	let providerColor = $derived(PROVIDER_COLOR[provider] ?? '#6b7280');
	let isLive = $derived(activeCount > 0);
</script>

<Handle type="target" position={Position.Left} />

<div class="efn-node" class:selected class:live={isLive}>
	<div class="node-header">
		<div class="node-icon" class:icon-live={isLive}>
			<Zap size={13} />
		</div>
		<div class="node-title">
			<span class="node-name" title={repoName}>{repoName}</span>
			<span class="node-branch">{branch}</span>
		</div>
		{#if isLive}
			<span class="live-badge">live</span>
		{/if}
	</div>

	<div class="node-body">
		<div class="fn-stats">
			<span class="stat">
				<span class="stat-val" class:val-live={isLive}>{activeCount}</span>
				<span class="stat-label">active</span>
			</span>
			<span class="stat-sep">/</span>
			<span class="stat">
				<span class="stat-val">{fnCount}</span>
				<span class="stat-label">total</span>
			</span>
		</div>

		<div class="node-meta">
			{#if provider}
				<span class="meta-chip provider-chip" style="background: color-mix(in srgb, {providerColor} 12%, transparent); color: {providerColor}; border-color: color-mix(in srgb, {providerColor} 30%, transparent);">
					{provider}
				</span>
			{/if}
			{#if shortSha}
				<span class="meta-chip sha-chip">{shortSha}</span>
			{:else}
				<span class="meta-chip sha-chip">not deployed</span>
			{/if}
		</div>
	</div>
</div>

<Handle type="source" position={Position.Right} />

<style>
	.efn-node {
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

	.efn-node:hover {
		border-color: var(--border-hover);
		box-shadow: var(--shadow-md);
	}

	.efn-node.selected {
		border-color: var(--accent);
		box-shadow: 0 0 0 2px var(--accent-muted), var(--shadow-md);
	}

	.efn-node.live {
		border-color: color-mix(in srgb, #22c55e 40%, transparent);
	}
	.efn-node.live:hover {
		border-color: #22c55e;
	}

	.live-badge {
		font-size: 9px; font-weight: 700; letter-spacing: 0.06em;
		text-transform: uppercase; padding: 1px 6px; border-radius: 99px;
		background: color-mix(in srgb, #22c55e 15%, transparent);
		color: #22c55e; border: 1px solid color-mix(in srgb, #22c55e 30%, transparent);
		flex-shrink: 0;
	}

	.icon-live { background: color-mix(in srgb, #22c55e 15%, transparent); color: #22c55e; }
	.val-live { color: #22c55e; }

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
		background: color-mix(in srgb, var(--accent) 12%, transparent);
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

	.node-branch {
		font-size: 10px;
		color: var(--text-dim);
		font-family: var(--font-mono);
	}

	.node-body {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.fn-stats {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
	}

	.stat { display: flex; align-items: baseline; gap: 2px; }
	.stat-val { font-weight: 700; color: var(--text-primary); }
	.stat-label { color: var(--text-dim); }
	.stat-sep { color: var(--text-dim); }

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

	.sha-chip {
		font-family: var(--font-mono);
		font-size: 10px;
	}
</style>
