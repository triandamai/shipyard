<script lang="ts">
	interface Props {
		value: number;
		tone?: 'blue' | 'green' | 'red' | 'yellow';
	}

	let { value, tone = 'blue' }: Props = $props();

	let clamped = $derived(Math.max(0, Math.min(100, value)));
</script>

<div class="ui-progress" role="progressbar" aria-valuenow={clamped} aria-valuemin={0} aria-valuemax={100}>
	<div class="ui-progress-fill ui-progress-fill--{tone}" style="width:{clamped}%"></div>
</div>

<style>
	.ui-progress {
		width: 100%;
		height: 6px;
		border-radius: 2px;
		background: var(--border);
		overflow: hidden;
	}
	.ui-progress-fill {
		height: 100%;
		border-radius: 2px;
		transition: width var(--transition-normal);
	}
	.ui-progress-fill--blue   { background: var(--accent); }
	.ui-progress-fill--green  { background: var(--accent-green); }
	.ui-progress-fill--red    { background: var(--accent-red); }
	.ui-progress-fill--yellow { background: var(--accent-yellow); }
</style>
