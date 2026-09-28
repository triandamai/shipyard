<script lang="ts">
	interface Props {
		size?: number;
	}

	let { size = 16 }: Props = $props();
</script>

<span
	class="ui-spinner"
	style="width:{size}px;height:{size}px;border-width:{Math.max(2, Math.round(size / 8))}px"
	role="status"
	aria-label="Loading"
></span>

<style>
	.ui-spinner {
		display: inline-block;
		border-style: solid;
		border-color: var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: ui-spin 0.7s linear infinite;
		flex-shrink: 0;
	}
	@keyframes ui-spin {
		to { transform: rotate(360deg); }
	}
</style>
