<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconBadge from './IconBadge.svelte';

	interface Props {
		icon: Snippet;
		tone?: 'blue' | 'green' | 'red' | 'yellow';
		value: string | number;
		label: string;
	}

	let { icon, tone = 'blue', value, label }: Props = $props();
</script>

<div class="ui-stat-card">
	<IconBadge {tone} size={32}>{@render icon()}</IconBadge>
	<span class="ui-stat-value">{value}</span>
	<span class="ui-stat-label">{label}</span>
</div>

<style>
	.ui-stat-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.ui-stat-value {
		font-size: 19px;
		font-weight: 700;
		color: var(--text-primary);
		letter-spacing: -0.01em;
		font-variant-numeric: tabular-nums;
	}
	.ui-stat-label {
		font-size: 11px;
		color: var(--text-muted);
		margin-top: -4px;
	}
</style>
