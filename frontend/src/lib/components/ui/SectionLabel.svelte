<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		children: Snippet;
	}

	let { children }: Props = $props();
</script>

<div class="ui-section-label">{@render children()}</div>

<style>
	.ui-section-label {
		font-size: 11px;
		font-weight: 700;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.07em;
		margin-bottom: 12px;
	}
</style>
