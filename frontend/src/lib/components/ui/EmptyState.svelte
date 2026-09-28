<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		icon?: Snippet;
		message: string;
		sub?: string;
	}

	let { icon, message, sub }: Props = $props();
</script>

<div class="ui-empty-state">
	{#if icon}<span class="ui-empty-icon">{@render icon()}</span>{/if}
	<span>{message}</span>
	{#if sub}<span class="ui-empty-sub">{sub}</span>{/if}
</div>

<style>
	.ui-empty-state {
		font-size: 13px;
		color: var(--text-dim);
		text-align: center;
		padding: 40px 24px;
		line-height: 1.6;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 6px;
	}
	:global(.ui-empty-icon) { color: var(--text-dim); }
	.ui-empty-sub { font-size: 11px; }
</style>
