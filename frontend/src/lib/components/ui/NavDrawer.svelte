<script lang="ts">
	import type { Snippet } from 'svelte';
	import type { Anchor } from '@lucide/svelte';
	import Spinner from './Spinner.svelte';

	interface Item {
		href: string;
		icon?: typeof Anchor;
		label: string;
		active: boolean;
	}

	interface Props {
		open: boolean;
		title?: string;
		items: Item[];
		onNavigate: () => void;
		/** Persistent: docked beside the content, pushes it, stays open. Modal (default): overlays the content and closes on outside click, Escape, or item click. */
		persistent?: boolean;
		onClose?: () => void;
		header?: Snippet;
		footer?: Snippet;
		loading?: boolean;
		emptyText?: string;
		hideOnPhone?: boolean;
	}

	let {
		open,
		title,
		items,
		onNavigate,
		persistent = false,
		onClose,
		header,
		footer,
		loading = false,
		emptyText,
		hideOnPhone = false
	}: Props = $props();

	function handleKeydown(e: KeyboardEvent) {
		if (open && !persistent && e.key === 'Escape') onClose?.();
	}
</script>

<svelte:window onkeydown={handleKeydown} />

{#if open && !persistent}
	<div class="ui-nav-drawer-scrim" role="presentation" onclick={() => onClose?.()}></div>
{/if}

<div
	class="ui-nav-drawer"
	class:ui-nav-drawer--open={open}
	class:ui-nav-drawer--persistent={persistent}
	class:ui-nav-drawer--hide-phone={hideOnPhone}
>
	<div class="ui-nav-drawer-inner">
		{#if header}<div class="ui-nav-drawer-header">{@render header()}</div>{/if}
		{#if title}<div class="ui-nav-drawer-title">{title}</div>{/if}
		<div class="ui-nav-drawer-items">
			{#if loading}
				<div class="ui-nav-drawer-state"><Spinner size={13} /> Loading…</div>
			{:else if items.length === 0 && emptyText}
				<div class="ui-nav-drawer-state">{emptyText}</div>
			{:else}
				{#each items as item (item.href)}
					{@const Icon = item.icon}
					<a
						href={item.href}
						class="ui-nav-drawer-item"
						class:ui-nav-drawer-item--active={item.active}
						aria-current={item.active ? 'page' : undefined}
						onclick={onNavigate}
					>
						<span class="ui-nav-drawer-icon">
							{#if Icon}<Icon size={16} />{:else}<span class="ui-nav-drawer-dot"></span>{/if}
						</span>
						<span class="ui-nav-drawer-label">{item.label}</span>
					</a>
				{/each}
			{/if}
		</div>
		{#if footer}<div class="ui-nav-drawer-footer">{@render footer()}</div>{/if}
	</div>
</div>

<style>
	.ui-nav-drawer {
		position: absolute;
		top: 0;
		left: 60px;
		bottom: 0;
		width: 0;
		background: var(--bg-surface);
		border-right: 1px solid var(--border);
		overflow: hidden;
		transition: width var(--transition-normal);
		z-index: 4;
		box-shadow: var(--shadow-lg);
	}
	.ui-nav-drawer--open { width: 230px; }
	.ui-nav-drawer:not(.ui-nav-drawer--open) { border-right-width: 0; box-shadow: none; }
	.ui-nav-drawer--persistent {
		position: relative;
		left: auto;
		height: 100vh;
		flex-shrink: 0;
		box-shadow: none;
		z-index: auto;
	}
	.ui-nav-drawer-scrim {
		position: fixed;
		top: 0;
		bottom: 0;
		left: 60px;
		right: 0;
		z-index: 3;
		background: rgba(0, 0, 0, 0.32);
	}
	.ui-nav-drawer-inner {
		width: 230px;
		height: 100%;
		padding: 18px 12px;
		display: flex;
		flex-direction: column;
	}
	.ui-nav-drawer-header { padding: 0 4px 12px; margin-bottom: 8px; border-bottom: 1px solid var(--border); }
	.ui-nav-drawer-title {
		font-size: 11px;
		font-weight: 700;
		color: var(--text-primary);
		padding: 4px 10px 10px;
	}
	.ui-nav-drawer-items { flex: 1; min-height: 0; overflow-y: auto; }
	.ui-nav-drawer-state {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		font-size: 12px;
		color: var(--text-dim);
	}
	.ui-nav-drawer-item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 10px;
		border-radius: var(--radius-md);
		font-size: 13px;
		font-weight: 500;
		color: var(--text-secondary);
		text-decoration: none;
		cursor: pointer;
		min-width: 0;
	}
	.ui-nav-drawer-item:hover { background: var(--bg-hover); color: var(--text-primary); }
	.ui-nav-drawer-item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
	.ui-nav-drawer-item--active { background: var(--accent-muted); color: var(--accent); font-weight: 600; }
	.ui-nav-drawer-icon {
		width: 16px;
		display: flex;
		justify-content: center;
		opacity: 0.85;
		flex-shrink: 0;
	}
	.ui-nav-drawer-dot { width: 6px; height: 6px; border-radius: 50%; background: currentColor; opacity: 0.6; }
	.ui-nav-drawer-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
	.ui-nav-drawer-footer { padding-top: 10px; border-top: 1px solid var(--border); }

	@media (max-width: 639px) {
		.ui-nav-drawer--hide-phone { display: none; }
	}
</style>
