<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconBadge from './IconBadge.svelte';

	interface Props {
		icon: Snippet;
		iconTone?: 'blue' | 'green' | 'red' | 'yellow';
		title: string;
		meta?: string;
		trailing?: Snippet;
	}

	let { icon, iconTone = 'blue', title, meta, trailing }: Props = $props();
</script>

<div class="ui-list-row">
	<IconBadge tone={iconTone} size={30}>{@render icon()}</IconBadge>
	<div class="ui-list-row-text">
		<span class="ui-list-row-title">{title}</span>
		{#if meta}<span class="ui-list-row-meta">{meta}</span>{/if}
	</div>
	{#if trailing}
		<div class="ui-list-row-trailing">{@render trailing()}</div>
	{/if}
</div>

<style>
	.ui-list-row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 0;
		border-bottom: 1px solid var(--border);
	}
	.ui-list-row:last-child { border-bottom: none; }
	.ui-list-row-text {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.ui-list-row-title {
		font-size: 12.5px;
		font-weight: 600;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ui-list-row-meta {
		font-size: 10.5px;
		color: var(--text-dim);
		margin-top: 1px;
	}
	.ui-list-row-trailing { margin-left: auto; flex-shrink: 0; }
</style>
