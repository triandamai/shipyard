<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Anchor } from '@lucide/svelte';

	interface Group {
		key: string;
		icon: Snippet;
		label: string;
	}

	interface Props {
		groups: Group[];
		activeGroup: string | null;
		onSelectGroup: (key: string | null) => void;
	}

	let { groups, activeGroup = $bindable(), onSelectGroup }: Props = $props();

	function handleClick(key: string) {
		onSelectGroup(activeGroup === key ? null : key);
	}
</script>

<aside class="ui-nav-rail">
	<div class="ui-nav-rail-logo">
		<Anchor size={16} strokeWidth={2.5} />
	</div>
	<nav class="ui-nav-rail-items">
		{#each groups as g (g.key)}
			<div class="ui-nav-rail-group">
				<button
					type="button"
					class="ui-nav-rail-btn"
					class:ui-nav-rail-btn--active={activeGroup === g.key}
					onclick={() => handleClick(g.key)}
					aria-label={g.label}
					aria-expanded={activeGroup === g.key}
				>
					{@render g.icon()}
				</button>
				<span class="ui-nav-rail-label">{g.label}</span>
			</div>
		{/each}
	</nav>
</aside>

<style>
	.ui-nav-rail {
		width: 60px;
		flex-shrink: 0;
		background: var(--bg-surface);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 14px 0;
		gap: 4px;
		height: 100vh;
		position: relative;
		z-index: 5;
	}
	.ui-nav-rail-logo {
		width: 30px;
		height: 30px;
		border-radius: var(--radius-sm);
		background: var(--accent);
		color: #fff;
		margin-bottom: 14px;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.ui-nav-rail-items {
		display: flex;
		flex-direction: column;
		gap: 6px;
		width: 100%;
		align-items: center;
	}
	.ui-nav-rail-group {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 2px;
	}
	.ui-nav-rail-btn {
		width: 40px;
		height: 40px;
		border-radius: var(--radius-md);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-muted);
		background: none;
		border: none;
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast);
	}
	.ui-nav-rail-btn:hover { background: var(--bg-hover); color: var(--accent); }
	.ui-nav-rail-btn--active { background: var(--accent-muted); color: var(--accent); }
	.ui-nav-rail-label {
		font-size: 8.5px;
		font-weight: 600;
		color: var(--text-dim);
	}
</style>
