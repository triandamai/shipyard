<script lang="ts">
	interface Segment {
		label: string;
		value: number;
		color: string;
	}

	interface Props {
		segments: Segment[];
		size?: number;
	}

	let { segments, size = 120 }: Props = $props();

	const RADIUS = 15.9; // circumference works out to ~100 for easy percentage math
	let total = $derived(segments.reduce((sum, s) => sum + s.value, 0));

	let arcs = $derived.by(() => {
		let offset = 0;
		return segments.map((s) => {
			const pct = total > 0 ? (s.value / total) * 100 : 0;
			const arc = { ...s, pct, dasharray: `${pct} ${100 - pct}`, dashoffset: 25 - offset };
			offset += pct;
			return arc;
		});
	});
</script>

<div class="ui-donut-wrap" style="width:{size}px;height:{size}px">
	<svg viewBox="0 0 42 42" width={size} height={size}>
		<circle cx="21" cy="21" r={RADIUS} fill="transparent" stroke="var(--border)" stroke-width="6" />
		{#each arcs as arc}
			<circle
				cx="21" cy="21" r={RADIUS}
				fill="transparent"
				stroke={arc.color}
				stroke-width="6"
				stroke-dasharray={arc.dasharray}
				stroke-dashoffset={arc.dashoffset}
			/>
		{/each}
	</svg>
</div>

<style>
	.ui-donut-wrap { display: inline-flex; }
</style>
