<script lang="ts">
	import { Anchor } from '@lucide/svelte';

	interface Group {
		key: string;
		icon: typeof Anchor;
		label: string;
	}

	interface Props {
		groups: Group[];
		activeGroup: string | null;
		/** Group containing the current route; highlighted when no drawer is open. */
		currentGroup?: string | null;
		/** How a label wider than the rail is handled: truncated with "…" or wrapped onto more lines. */
		labelOverflow?: 'ellipsis' | 'wrap';
		onSelectGroup: (key: string | null) => void;
	}

	let { groups, activeGroup = $bindable(), currentGroup = null, labelOverflow = 'ellipsis', onSelectGroup }: Props = $props();

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
			{@const Icon = g.icon}
			<div class="ui-nav-rail-group">
				<button
					type="button"
					class="ui-nav-rail-btn"
					class:ui-nav-rail-btn--active={activeGroup === g.key || (activeGroup === null && currentGroup === g.key)}
					onclick={() => handleClick(g.key)}
					aria-label={g.label}
					aria-expanded={activeGroup === g.key}
				>
					<Icon size={20} />
				</button>
				<span
					class="ui-nav-rail-label ui-nav-rail-label--{labelOverflow}"
					title={labelOverflow === 'ellipsis' ? g.label : undefined}
				>{g.label}</span>
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
		width: 100%;
		padding: 0 3px;
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
		max-width: 100%;
		text-align: center;
	}
	.ui-nav-rail-label--ellipsis {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.ui-nav-rail-label--wrap {
		white-space: normal;
		overflow-wrap: anywhere;
		line-height: 1.2;
	}
</style>
