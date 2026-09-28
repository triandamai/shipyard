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

<div class="ui-hero">
	<div class="ui-hero-content">
		{#if eyebrow}<div class="ui-hero-eyebrow">{eyebrow}</div>{/if}
		<h2 class="ui-hero-title">{title}</h2>
		{#if subtitle}<p class="ui-hero-subtitle">{subtitle}</p>{/if}
		{#if actions}
			<div class="ui-hero-actions">{@render actions()}</div>
		{/if}
	</div>
</div>

<style>
	.ui-hero {
		position: relative;
		overflow: hidden;
		border-radius: var(--radius-lg);
		padding: 22px 24px;
		background: linear-gradient(
			135deg,
			color-mix(in srgb, var(--accent) 14%, var(--bg-surface)) 0%,
			var(--bg-surface) 65%
		);
		border: 1px solid var(--border);
	}
	.ui-hero::before {
		content: '';
		position: absolute;
		right: -30px;
		top: -40px;
		width: 150px;
		height: 150px;
		border-radius: 50%;
		background: color-mix(in srgb, var(--accent) 18%, transparent);
	}
	.ui-hero-content { position: relative; }
	.ui-hero-eyebrow {
		font-size: 10.5px;
		font-weight: 700;
		color: var(--accent);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}
	.ui-hero-title {
		font-size: 21px;
		font-weight: 700;
		color: var(--text-primary);
		margin: 6px 0 0;
		letter-spacing: -0.02em;
	}
	.ui-hero-subtitle {
		font-size: 12px;
		color: var(--text-muted);
		margin: 4px 0 0;
	}
	.ui-hero-actions {
		display: flex;
		gap: 8px;
		margin-top: 16px;
	}
</style>
