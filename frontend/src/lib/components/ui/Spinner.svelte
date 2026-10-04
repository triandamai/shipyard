<script lang="ts">
	interface Props {
		size?: number;
		tone?: 'accent' | 'current';
	}

	let { size = 16, tone = 'accent' }: Props = $props();
</script>

<span
	class="ui-spinner ui-spinner--{tone}"
	style="width:{size}px;height:{size}px;border-width:{Math.max(2, Math.round(size / 8))}px"
	role="status"
	aria-label="Loading"
></span>

<style>
	.ui-spinner {
		display: inline-block;
		border-style: solid;
		border-radius: 50%;
		animation: ui-spin 0.7s linear infinite;
		flex-shrink: 0;
	}
	.ui-spinner--accent {
		border-color: var(--border);
		border-top-color: var(--accent);
	}
	.ui-spinner--current {
		border-color: color-mix(in srgb, currentColor 30%, transparent);
		border-top-color: currentColor;
	}
	@keyframes ui-spin {
		to { transform: rotate(360deg); }
	}
</style>
