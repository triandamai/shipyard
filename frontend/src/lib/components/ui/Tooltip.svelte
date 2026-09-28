<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		text: string;
		children: Snippet;
	}

	let { text, children }: Props = $props();
</script>

<span class="ui-tooltip-wrap">
	{@render children()}
	<span class="ui-tooltip" role="tooltip">{text}</span>
</span>

<style>
	.ui-tooltip-wrap {
		position: relative;
		display: inline-flex;
	}
	.ui-tooltip {
		position: absolute;
		bottom: calc(100% + 6px);
		left: 50%;
		transform: translateX(-50%);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text-primary);
		font-size: 11px;
		font-weight: 500;
		padding: 4px 9px;
		border-radius: var(--radius-sm);
		white-space: nowrap;
		pointer-events: none;
		opacity: 0;
		transition: opacity var(--transition-fast);
		box-shadow: var(--shadow-md);
		z-index: 100;
	}
	.ui-tooltip-wrap:hover .ui-tooltip,
	.ui-tooltip-wrap:focus-within .ui-tooltip {
		opacity: 1;
	}
</style>
