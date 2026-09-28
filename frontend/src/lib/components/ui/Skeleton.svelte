<script lang="ts">
	interface Props {
		variant?: 'row' | 'card' | 'text';
		width?: string;
		height?: string;
	}

	let { variant = 'text', width = '100%', height }: Props = $props();

	let resolvedHeight = $derived(
		height ?? (variant === 'row' ? '52px' : variant === 'card' ? '96px' : '14px')
	);
</script>

<div
	class="ui-skeleton ui-skeleton--{variant}"
	style="width:{width};height:{resolvedHeight}"
></div>

<style>
	.ui-skeleton {
		background: var(--border);
		border-radius: var(--radius-sm);
		animation: ui-skeleton-pulse 1.3s ease-in-out infinite;
	}
	.ui-skeleton--card { border-radius: var(--radius-lg); }
	.ui-skeleton--row { border-radius: var(--radius-md); }

	@keyframes ui-skeleton-pulse {
		0%, 100% { opacity: 0.5; }
		50%      { opacity: 1; }
	}
</style>
