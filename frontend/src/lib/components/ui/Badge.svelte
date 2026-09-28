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
		padding: 2px 9px;
		border-radius: 999px;
		font-size: 11px;
		font-weight: 600;
		font-family: var(--font-sans);
		letter-spacing: 0.01em;
		white-space: nowrap;
	}
	.ui-badge--green   { background: var(--accent-green-muted);  color: var(--accent-green); }
	.ui-badge--red     { background: var(--accent-red-muted);    color: var(--accent-red); }
	.ui-badge--yellow  { background: var(--accent-yellow-muted); color: var(--accent-yellow); }
	.ui-badge--blue    { background: var(--accent-muted);        color: var(--accent); }
	.ui-badge--neutral { background: var(--bg-hover);            color: var(--text-muted); }
</style>
