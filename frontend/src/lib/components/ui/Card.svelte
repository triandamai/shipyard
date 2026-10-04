<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		padding?: string;
		tone?: 'default' | 'danger';
		children: Snippet;
	}

	let { padding = '16px', tone = 'default', children }: Props = $props();
</script>

<div class="ui-card ui-card--{tone}" style="padding:{padding}">
	{@render children()}
</div>

<style>
	.ui-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
	}
	.ui-card--danger {
		border-color: color-mix(in srgb, var(--accent-red) 45%, var(--border));
	}
</style>
