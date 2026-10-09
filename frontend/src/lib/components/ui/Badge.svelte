<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone?: 'green' | 'red' | 'yellow' | 'blue' | 'neutral';
		children: Snippet;
	}

	let { tone = 'neutral', children }: Props = $props();
</script>

<span class="ui-badge ui-badge--{tone}">{@render children()}</span>

<style>
	.ui-badge {
		display: inline-flex;
		align-items: center;
		padding: 2px 8px;
		border-radius: var(--radius-sm);
		font-size: 12px;
		font-weight: 500;
		font-family: var(--font-sans);
		white-space: nowrap;
	}
	.ui-badge--green   { background: var(--accent-green-muted);  color: var(--accent-green-text); }
	.ui-badge--red     { background: var(--accent-red-muted);    color: var(--accent-red-text); }
	.ui-badge--yellow  { background: var(--accent-yellow-muted); color: var(--accent-yellow-text); }
	.ui-badge--blue    { background: var(--accent-blue-muted);   color: var(--accent-blue-text); }
	.ui-badge--neutral { background: var(--bg-hover);            color: var(--text-secondary); }
</style>
