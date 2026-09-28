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

	const WIDTH = 300;
	let max = $derived(Math.max(1, ...data.map((d) => d.value)));

	let points = $derived(
		data.map((d, i) => {
			const x = data.length > 1 ? (i / (data.length - 1)) * WIDTH : 0;
			const y = height - (d.value / max) * height;
			return `${x.toFixed(1)},${y.toFixed(1)}`;
		})
	);

	let linePath = $derived(`M ${points.join(' L ')}`);
	let areaPath = $derived(`${linePath} L ${WIDTH},${height} L 0,${height} Z`);
</script>

<svg class="ui-area-chart" viewBox="0 0 {WIDTH} {height}" preserveAspectRatio="none">
	<path d={areaPath} fill={color} opacity="0.12" />
	<path d={linePath} fill="none" stroke={color} stroke-width="2" stroke-linejoin="round" stroke-linecap="round" />
</svg>

<style>
	.ui-area-chart {
		width: 100%;
		display: block;
	}
</style>
