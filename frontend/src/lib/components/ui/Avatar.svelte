<script lang="ts">
	interface Props {
		initials: string;
		tone?: 'blue' | 'green' | 'red' | 'yellow' | 'purple';
		size?: number;
	}

	let { initials, tone = 'blue', size = 30 }: Props = $props();
</script>

<span
	class="ui-avatar ui-avatar--{tone}"
	style="width:{size}px;height:{size}px;font-size:{Math.round(size * 0.38)}px"
>
	{initials.slice(0, 2).toUpperCase()}
</span>

<style>
	.ui-avatar {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		font-weight: 700;
		font-family: var(--font-sans);
		flex-shrink: 0;
	}
	.ui-avatar--blue   { background: var(--accent-muted);        color: var(--accent); }
	.ui-avatar--green  { background: var(--accent-green-muted);  color: var(--accent-green); }
	.ui-avatar--red    { background: var(--accent-red-muted);    color: var(--accent-red); }
	.ui-avatar--yellow { background: var(--accent-yellow-muted); color: var(--accent-yellow); }
	.ui-avatar--purple { background: color-mix(in srgb, var(--accent-purple) 14%, transparent); color: var(--accent-purple); }
</style>
