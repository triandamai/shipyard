<script lang="ts" module>
	import type { Anchor } from '@lucide/svelte';

	export interface TabItem {
		id: string;
		label: string;
		href?: string;
		icon?: typeof Anchor;
		badge?: string;
		danger?: boolean;
		disabled?: boolean;
	}
</script>

<script lang="ts">
	import Badge from './Badge.svelte';

	interface Props {
		tabs: TabItem[];
		value: string;
		onChange?: (id: string) => void;
		ariaLabel?: string;
	}

	let { tabs, value = $bindable(), onChange, ariaLabel }: Props = $props();

	let isLinkMode = $derived(tabs.length > 0 && tabs.every((t) => !!t.href));
	let listEl: HTMLDivElement | undefined = $state();

	function activate(id: string) {
		if (id === value) return;
		value = id;
		onChange?.(id);
	}

	function onKeydown(e: KeyboardEvent) {
		const enabled = tabs.filter((t) => !t.disabled);
		if (enabled.length === 0) return;
		const i = enabled.findIndex((t) => t.id === value);
		let next = -1;
		if (e.key === 'ArrowRight') next = (i + 1) % enabled.length;
		else if (e.key === 'ArrowLeft') next = (i - 1 + enabled.length) % enabled.length;
		else if (e.key === 'Home') next = 0;
		else if (e.key === 'End') next = enabled.length - 1;
		if (next < 0) return;
		e.preventDefault();
		activate(enabled[next].id);
		listEl?.querySelector<HTMLElement>(`[data-tab-id="${enabled[next].id}"]`)?.focus();
	}
</script>

{#if isLinkMode}
	<nav class="ui-tabs" aria-label={ariaLabel}>
		{#each tabs as tab (tab.id)}
			{@const Icon = tab.icon}
			<a
				href={tab.href}
				class="ui-tab"
				class:ui-tab--active={tab.id === value}
				class:ui-tab--danger={tab.danger}
				aria-current={tab.id === value ? 'page' : undefined}
			>
				{#if Icon}<Icon size={14} />{/if}
				<span>{tab.label}</span>
				{#if tab.badge}<Badge tone="neutral">{tab.badge}</Badge>{/if}
			</a>
		{/each}
	</nav>
{:else}
	<div class="ui-tabs" role="tablist" aria-label={ariaLabel} tabindex="-1" bind:this={listEl} onkeydown={onKeydown}>
		{#each tabs as tab (tab.id)}
			{@const Icon = tab.icon}
			<button
				type="button"
				role="tab"
				data-tab-id={tab.id}
				aria-selected={tab.id === value}
				tabindex={tab.id === value ? 0 : -1}
				disabled={tab.disabled}
				class="ui-tab"
				class:ui-tab--active={tab.id === value}
				class:ui-tab--danger={tab.danger}
				onclick={() => activate(tab.id)}
			>
				{#if Icon}<Icon size={14} />{/if}
				<span>{tab.label}</span>
				{#if tab.badge}<Badge tone="neutral">{tab.badge}</Badge>{/if}
			</button>
		{/each}
	</div>
{/if}

<style>
	.ui-tabs {
		display: flex;
		gap: 2px;
		border-bottom: 1px solid var(--border);
		overflow-x: auto;
		scrollbar-width: none;
		outline: none;
	}
	.ui-tabs::-webkit-scrollbar { display: none; }
	.ui-tab {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 8px 12px;
		font-size: 12.5px;
		font-weight: 500;
		font-family: var(--font-sans);
		color: var(--text-muted);
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		margin-bottom: -1px;
		cursor: pointer;
		white-space: nowrap;
		text-decoration: none;
		transition: color var(--transition-fast), border-color var(--transition-fast);
	}
	.ui-tab:hover { color: var(--text-primary); }
	.ui-tab:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; border-radius: var(--radius-sm); }
	.ui-tab:disabled { opacity: 0.5; cursor: not-allowed; }
	.ui-tab--active { color: var(--accent); border-bottom-color: var(--accent); font-weight: 600; }
	.ui-tab--danger:hover,
	.ui-tab--danger.ui-tab--active { color: var(--accent-red); }
	.ui-tab--danger.ui-tab--active { border-bottom-color: var(--accent-red); }
</style>
