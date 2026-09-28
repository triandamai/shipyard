<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		padding?: string;
		children: Snippet;
	}

	let { padding = '16px', children }: Props = $props();
</script>

<div class="ui-card" style="padding:{padding}">
	{@render children()}
</div>

<style>
	.ui-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
	}
</style>
