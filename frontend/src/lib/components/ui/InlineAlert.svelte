<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone: 'error' | 'success' | 'warning' | 'info';
		children: Snippet;
	}

	let { tone, children }: Props = $props();
</script>

<div class="ui-alert ui-alert--{tone}">{@render children()}</div>

<style>
	.ui-alert {
		font-size: 12px;
		padding: 9px 12px;
		border-radius: var(--radius-sm);
		border: 1px solid;
		line-height: 1.5;
	}
	.ui-alert--error {
		background: var(--accent-red-muted);
		border-color: color-mix(in srgb, var(--accent-red) 30%, transparent);
		color: var(--accent-red-text);
	}
	.ui-alert--success {
		background: var(--accent-green-muted);
		border-color: color-mix(in srgb, var(--accent-green) 30%, transparent);
		color: var(--accent-green-text);
	}
	.ui-alert--warning {
		background: var(--accent-yellow-muted);
		border-color: color-mix(in srgb, var(--accent-yellow) 30%, transparent);
		color: var(--accent-yellow-text);
	}
	.ui-alert--info {
		background: var(--accent-blue-muted);
		border-color: color-mix(in srgb, var(--accent-blue) 30%, transparent);
		color: var(--accent-blue-text);
	}
</style>
