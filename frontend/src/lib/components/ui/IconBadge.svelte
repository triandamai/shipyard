<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone?: 'blue' | 'green' | 'red' | 'yellow';
		size?: number;
		children: Snippet;
	}

	let { tone = 'blue', size = 32, children }: Props = $props();
</script>

<span class="ui-icon-badge ui-icon-badge--{tone}" style="width:{size}px;height:{size}px">
	{@render children()}
</span>

<style>
	.ui-icon-badge {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		flex-shrink: 0;
	}
	.ui-icon-badge--blue   { background: var(--accent-muted);        color: var(--accent); }
	.ui-icon-badge--green  { background: var(--accent-green-muted);  color: var(--accent-green); }
	.ui-icon-badge--red    { background: var(--accent-red-muted);    color: var(--accent-red); }
	.ui-icon-badge--yellow { background: var(--accent-yellow-muted); color: var(--accent-yellow); }
</style>
