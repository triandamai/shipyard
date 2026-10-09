<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		eyebrow?: string;
		title: string;
		subtitle?: string;
		actions?: Snippet;
	}

	let { eyebrow, title, subtitle, actions }: Props = $props();
</script>

<header class="ui-page-header">
	<div class="ui-page-header-text">
		{#if eyebrow}<div class="ui-page-header-eyebrow">{eyebrow}</div>{/if}
		<h1 class="ui-page-header-title">{title}</h1>
		{#if subtitle}<p class="ui-page-header-subtitle">{subtitle}</p>{/if}
	</div>
	{#if actions}
		<div class="ui-page-header-actions">{@render actions()}</div>
	{/if}
</header>

<style>
	.ui-page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
		margin-bottom: 24px;
	}
	.ui-page-header-eyebrow {
		font-size: 11px;
		font-weight: 700;
		color: var(--accent);
		margin-bottom: 6px;
	}
	.ui-page-header-title {
		font-size: 20px;
		font-weight: 700;
		color: var(--text-primary);
		letter-spacing: -0.02em;
		margin: 0;
		line-height: 1.25;
	}
	.ui-page-header-subtitle {
		font-size: 12.5px;
		color: var(--text-muted);
		margin: 4px 0 0;
	}
	.ui-page-header-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
	}
</style>
