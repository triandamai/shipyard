<script lang="ts">
	import { Plug, Tag, GitCommitHorizontal } from '@lucide/svelte';

	interface Props {
		labels?: Record<string, string> | null;
		/** Integration name; falls back to the `shipyard.io/integration` label. */
		integration?: string | null;
		/** Only the integration badge and version — for cards and list rows. */
		compact?: boolean;
	}

	let { labels = {}, integration = null, compact = false }: Props = $props();

	const SYSTEM = 'shipyard.io/';

	let all = $derived(labels ?? {});
	let source = $derived(integration ?? all['shipyard.io/integration'] ?? null);
	let environment = $derived(all['shipyard.io/environment'] ?? null);
	let version = $derived(all['shipyard.io/version'] ?? null);
	let custom = $derived(
		compact ? [] : Object.entries(all).filter(([k]) => !k.startsWith(SYSTEM))
	);
</script>

{#if source || version || (!compact && (environment || custom.length))}
	<div class="label-chips">
		{#if source}
			<span class="chip chip-integration" title="Managed by the {source} integration">
				<Plug size={11} />
				{source}
			</span>
		{/if}
		{#if environment && !compact}
			<span class="chip" title="Environment">{environment}</span>
		{/if}
		{#if version}
			<span class="chip chip-mono" title="Version (image tag)">
				<GitCommitHorizontal size={11} />
				{version}
			</span>
		{/if}
		{#each custom as [key, value] (key)}
			<span class="chip chip-mono" title="{key}: {value}">
				<Tag size={11} />
				<span class="chip-key">{key}</span>{value}
			</span>
		{/each}
	</div>
{/if}

<style>
	.label-chips {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		min-width: 0;
	}
	.chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		max-width: 220px;
		padding: 1px 6px;
		border-radius: var(--radius-sm);
		background: var(--bg-hover);
		color: var(--text-secondary);
		font-size: 11px;
		font-weight: 500;
		line-height: 18px;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.chip-integration {
		background: var(--accent-blue-muted);
		color: var(--accent-blue-text);
		text-transform: capitalize;
	}
	.chip-mono {
		font-family: var(--font-mono);
	}
	.chip-key {
		color: var(--text-muted);
	}
	.chip-key::after {
		content: '=';
	}
</style>
