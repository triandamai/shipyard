<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Check } from '@lucide/svelte';

	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		options: Option[];
		value: string;
		trigger: Snippet;
	}

	let { options, value = $bindable(), trigger }: Props = $props();

	let open = $state(false);

	function select(v: string) {
		value = v;
		open = false;
	}

	function onWindowClick() {
		open = false;
	}
</script>

<svelte:window onclick={onWindowClick} />

<div class="ui-dropdown">
	<button type="button" class="ui-dropdown-trigger" onclick={(e) => { e.stopPropagation(); open = !open; }}>
		{@render trigger()}
	</button>
	{#if open}
		<div class="ui-dropdown-menu" onclick={(e) => e.stopPropagation()} role="menu">
			{#each options as opt (opt.value)}
				<button type="button" class="ui-dropdown-item" onclick={() => select(opt.value)} role="menuitem">
					<span class="ui-dropdown-check">{#if opt.value === value}<Check size={13} />{/if}</span>
					{opt.label}
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.ui-dropdown { position: relative; display: inline-block; }
	.ui-dropdown-trigger { background: none; border: none; padding: 0; cursor: pointer; }
	.ui-dropdown-menu {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		min-width: 180px;
		max-height: 280px;
		overflow-y: auto;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-md);
		z-index: 50;
		padding: 4px;
	}
	.ui-dropdown-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 7px 9px;
		background: none;
		border: none;
		border-radius: var(--radius-sm);
		font-size: 12.5px;
		color: var(--text-secondary);
		text-align: left;
		cursor: pointer;
	}
	.ui-dropdown-item:hover { background: var(--bg-hover); color: var(--text-primary); }
	.ui-dropdown-check { width: 13px; display: flex; color: var(--accent); flex-shrink: 0; }
</style>
