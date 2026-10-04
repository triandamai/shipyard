<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconBadge from './IconBadge.svelte';

	interface Props {
		icon: Snippet;
		iconTone?: 'blue' | 'green' | 'red' | 'yellow';
		title: string;
		meta?: string;
		trailing?: Snippet;
		onclick?: (e: MouseEvent) => void;
		ariaLabel?: string;
		ariaExpanded?: boolean;
	}

	let { icon, iconTone = 'blue', title, meta, trailing, onclick, ariaLabel, ariaExpanded }: Props = $props();

	// Clickable rows are a <button>, so everything inside must be phrasing content (spans).
	// Non-clickable rows keep block containers so trailing content may hold divs.
	let clickable = $derived(!!onclick);
</script>

{#if clickable}
	<button
		type="button"
		class="ui-list-row ui-list-row--clickable"
		{onclick}
		aria-label={ariaLabel}
		aria-expanded={ariaExpanded}
	>
		<IconBadge tone={iconTone} size={30}>{@render icon()}</IconBadge>
		<span class="ui-list-row-text">
			<span class="ui-list-row-title">{title}</span>
			{#if meta}<span class="ui-list-row-meta">{meta}</span>{/if}
		</span>
		{#if trailing}
			<span class="ui-list-row-trailing">{@render trailing()}</span>
		{/if}
	</button>
{:else}
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
{/if}

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

	/* Clickable variant: native button chrome removed, same geometry as the static row */
	.ui-list-row--clickable {
		box-sizing: border-box;
		width: calc(100% + 16px);
		margin: 0 -8px;
		padding: 10px 8px;
		background: transparent;
		border: none;
		border-bottom: 1px solid var(--border);
		color: inherit;
		font: inherit;
		text-align: left;
		cursor: pointer;
		transition: background var(--transition-fast);
	}
	.ui-list-row--clickable:last-child { border-bottom: none; }
	.ui-list-row--clickable:hover { background: var(--bg-hover); }
	.ui-list-row--clickable:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
	.ui-list-row--clickable .ui-list-row-text { flex: 1; align-items: flex-start; }
</style>
