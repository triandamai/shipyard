<script lang="ts">
	import type { Anchor } from '@lucide/svelte';

	interface Item {
		href: string;
		icon: typeof Anchor;
		label: string;
		active: boolean;
	}

	interface Props {
		open: boolean;
		title: string;
		items: Item[];
		onNavigate: () => void;
		/** Persistent: docked beside the content, pushes it, stays open. Modal (default): overlays the content and closes on outside click, Escape, or item click. */
		persistent?: boolean;
		onClose?: () => void;
	}

	let { open, title, items, onNavigate, persistent = false, onClose }: Props = $props();

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
>
	<div class="ui-nav-drawer-inner">
		<div class="ui-nav-drawer-title">{title}</div>
		{#each items as item (item.href)}
			{@const Icon = item.icon}
			<a
				href={item.href}
				class="ui-nav-drawer-item"
				class:ui-nav-drawer-item--active={item.active}
				onclick={onNavigate}
			>
				<span class="ui-nav-drawer-icon"><Icon size={16} /></span>
				{item.label}
			</a>
		{/each}
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
		padding: 18px 12px;
	}
	.ui-nav-drawer-title {
		font-size: 11px;
		font-weight: 700;
		color: var(--text-primary);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		padding: 4px 10px 10px;
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
	}
	.ui-nav-drawer-item:hover { background: var(--bg-hover); color: var(--text-primary); }
	.ui-nav-drawer-item--active { background: var(--accent-muted); color: var(--accent); font-weight: 600; }
	.ui-nav-drawer-icon {
		width: 16px;
		display: flex;
		justify-content: center;
		opacity: 0.85;
	}
</style>
