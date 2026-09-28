<script lang="ts">
	interface Props {
		margin?: string;
	}

	let { margin = '16px 0' }: Props = $props();
</script>

<div class="ui-divider" style="margin:{margin}"></div>

<style>
	.ui-divider {
		height: 1px;
		background: var(--border);
		flex-shrink: 0;
	}
</style>
