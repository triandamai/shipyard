<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Check } from '@lucide/svelte';

	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		trigger: Snippet;
		options?: Option[];
		value?: string;
		menu?: Snippet<[() => void]>;
		placement?: 'bottom-start' | 'bottom-end' | 'top-start' | 'right-end';
		triggerLabel?: string;
	}

	let {
		trigger,
		options = [],
		value = $bindable(''),
		menu,
		placement = 'bottom-start',
		triggerLabel
	}: Props = $props();

	let open = $state(false);
	let rootEl: HTMLDivElement | undefined = $state();
	let triggerEl: HTMLButtonElement | undefined = $state();
	let menuEl: HTMLDivElement | undefined = $state();

	function items(): HTMLElement[] {
		if (!menuEl) return [];
		return [
			...menuEl.querySelectorAll<HTMLElement>('[role="menuitem"]:not([disabled]):not([aria-disabled="true"])')
		];
	}

	function openMenu() {
		open = true;
		queueMicrotask(() => items()[0]?.focus());
	}

	function close(restoreFocus = true) {
		open = false;
		if (restoreFocus) triggerEl?.focus();
	}

	function select(v: string) {
		value = v;
		close();
	}

	function onTriggerKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ') {
			e.preventDefault();
			openMenu();
		}
	}

	function onMenuKeydown(e: KeyboardEvent) {
		const list = items();
		const i = list.indexOf(document.activeElement as HTMLElement);
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			list[(i + 1) % list.length]?.focus();
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			list[(i - 1 + list.length) % list.length]?.focus();
		} else if (e.key === 'Home') {
			e.preventDefault();
			list[0]?.focus();
		} else if (e.key === 'End') {
			e.preventDefault();
			list[list.length - 1]?.focus();
		} else if (e.key === 'Escape') {
			e.preventDefault();
			e.stopPropagation();
			close();
		} else if (e.key === 'Tab') {
			close(false);
		}
	}

	function onWindowClick(e: MouseEvent) {
		if (open && rootEl && !rootEl.contains(e.target as Node)) close(false);
	}
</script>

<svelte:window onclick={onWindowClick} />

<div class="ui-dropdown" bind:this={rootEl}>
	<button
		bind:this={triggerEl}
		type="button"
		class="ui-dropdown-trigger"
		aria-haspopup="menu"
		aria-expanded={open}
		aria-label={triggerLabel}
		onclick={() => (open ? close(false) : openMenu())}
		onkeydown={onTriggerKeydown}
	>
		{@render trigger()}
	</button>
	{#if open}
		<div
			bind:this={menuEl}
			class="ui-dropdown-menu ui-dropdown-menu--{placement}"
			role="menu"
			tabindex="-1"
			onkeydown={onMenuKeydown}
		>
			{#if menu}
				{@render menu(() => close(false))}
			{:else}
				{#each options as opt (opt.value)}
					<button type="button" class="ui-dropdown-item" role="menuitem" onclick={() => select(opt.value)}>
						<span class="ui-dropdown-check">{#if opt.value === value}<Check size={13} />{/if}</span>
						{opt.label}
					</button>
				{/each}
			{/if}
		</div>
	{/if}
</div>

<style>
	.ui-dropdown { position: relative; display: inline-block; }
	.ui-dropdown-trigger { background: none; border: none; padding: 0; cursor: pointer; border-radius: var(--radius-md); }
	.ui-dropdown-trigger:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
	.ui-dropdown-menu {
		position: absolute;
		min-width: 180px;
		max-height: 320px;
		overflow-y: auto;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-md);
		z-index: 70;
		padding: 4px;
		outline: none;
	}
	.ui-dropdown-menu--bottom-start { top: calc(100% + 6px); left: 0; }
	.ui-dropdown-menu--bottom-end   { top: calc(100% + 6px); right: 0; }
	.ui-dropdown-menu--top-start    { bottom: calc(100% + 6px); left: 0; }
	.ui-dropdown-menu--right-end    { left: calc(100% + 8px); bottom: 0; }

	.ui-dropdown-menu :global(.ui-dropdown-item) {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 9px;
		background: none;
		border: none;
		border-radius: var(--radius-sm);
		font-size: 12.5px;
		font-family: var(--font-sans);
		color: var(--text-secondary);
		text-align: left;
		text-decoration: none;
		cursor: pointer;
	}
	.ui-dropdown-menu :global(.ui-dropdown-item:hover),
	.ui-dropdown-menu :global(.ui-dropdown-item:focus-visible) {
		background: var(--bg-hover);
		color: var(--text-primary);
		outline: none;
	}
	.ui-dropdown-menu :global(.ui-dropdown-item:disabled) { opacity: 0.5; cursor: not-allowed; }
	.ui-dropdown-menu :global(.ui-dropdown-item--danger) { color: var(--accent-red); }
	.ui-dropdown-menu :global(.ui-dropdown-item--danger:hover),
	.ui-dropdown-menu :global(.ui-dropdown-item--danger:focus-visible) { background: var(--accent-red-muted); color: var(--accent-red); }
	.ui-dropdown-menu :global(.ui-dropdown-sep) { height: 1px; background: var(--border); margin: 4px 0; }
	.ui-dropdown-menu :global(.ui-dropdown-header) { padding: 8px 9px; }
	.ui-dropdown-check { width: 13px; display: flex; color: var(--accent); flex-shrink: 0; }
</style>
