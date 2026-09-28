<script lang="ts">
	interface Point {
		label: string;
		value: number;
	}

	interface Props {
		data: Point[];
		height?: number;
		color?: string;
	}

	let { data, height = 120, color = 'var(--accent)' }: Props = $props();

	let max = $derived(Math.max(1, ...data.map((d) => d.value)));
</script>

<div class="ui-bar-chart" style="height:{height}px">
	{#each data as d}
		<div class="ui-bar-col" title="{d.label}: {d.value}">
			<div class="ui-bar" style="height:{(d.value / max) * 100}%;background:{color}"></div>
		</div>
	{/each}
</div>

<style>
	.ui-bar-chart {
		display: flex;
		align-items: flex-end;
		gap: 3px;
	}
	.ui-bar-col {
		flex: 1;
		height: 100%;
		display: flex;
		align-items: flex-end;
		min-width: 0;
	}
	.ui-bar {
		width: 100%;
		border-radius: 2px 2px 0 0;
		min-height: 2px;
		transition: height var(--transition-normal);
	}
</style>
