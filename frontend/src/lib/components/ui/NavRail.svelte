<script lang="ts">
	import type { Snippet } from 'svelte';
	import { Anchor } from '@lucide/svelte';

	interface Group {
		key: string;
		icon: typeof Anchor;
		label: string;
		href?: string;
		active?: boolean;
	}

	interface Props {
		groups: Group[];
		activeGroup?: string | null;
		/** Group containing the current route; highlighted when no drawer is open. */
		currentGroup?: string | null;
		/** How a label wider than the rail is handled: truncated with "…" or wrapped onto more lines. */
		labelOverflow?: 'ellipsis' | 'wrap';
		onSelectGroup?: (key: string | null) => void;
		footer?: Snippet;
		/** At ≤639px render as a bottom navigation bar instead of a side rail. */
		phoneBar?: boolean;
	}

	let {
		groups,
		activeGroup = $bindable(null),
		currentGroup = null,
		labelOverflow = 'ellipsis',
		onSelectGroup,
		footer,
		phoneBar = false
	}: Props = $props();

	function handleClick(key: string) {
		onSelectGroup?.(activeGroup === key ? null : key);
	}

	function isHighlighted(g: Group): boolean {
		if (g.href) return !!g.active;
		return activeGroup === g.key || (activeGroup === null && currentGroup === g.key);
	}
</script>

<aside class="ui-nav-rail" class:ui-nav-rail--phone-bar={phoneBar}>
	<div class="ui-nav-rail-logo">
		<Anchor size={16} strokeWidth={2.5} />
	</div>
	<nav class="ui-nav-rail-items">
		{#each groups as g (g.key)}
			{@const Icon = g.icon}
			<div class="ui-nav-rail-group">
				{#if g.href}
					<a
						href={g.href}
						class="ui-nav-rail-btn"
						class:ui-nav-rail-btn--active={isHighlighted(g)}
						aria-label={g.label}
						aria-current={g.active ? 'page' : undefined}
					>
						<Icon size={20} />
					</a>
				{:else}
					<button
						type="button"
						class="ui-nav-rail-btn"
						class:ui-nav-rail-btn--active={isHighlighted(g)}
						onclick={() => handleClick(g.key)}
						aria-label={g.label}
						aria-expanded={activeGroup === g.key}
					>
						<Icon size={20} />
					</button>
				{/if}
				<span
					class="ui-nav-rail-label ui-nav-rail-label--{labelOverflow}"
					title={labelOverflow === 'ellipsis' ? g.label : undefined}
				>{g.label}</span>
			</div>
		{/each}
	</nav>
	{#if footer}
		<div class="ui-nav-rail-footer">{@render footer()}</div>
	{/if}
</aside>

<style>
	.ui-nav-rail {
		width: 60px;
		flex-shrink: 0;
		background: var(--rail-bg);
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
		border-radius: var(--radius-md);
		background: var(--rail-active-bg);
		color: var(--accent);
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
		color: var(--rail-fg);
		background: none;
		border: none;
		cursor: pointer;
		text-decoration: none;
		transition: background var(--transition-fast), color var(--transition-fast);
	}
	.ui-nav-rail-btn:hover { background: var(--rail-hover-bg); color: var(--rail-fg-active); }
	.ui-nav-rail-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
	/* Active: lit icon plus a signal bar on the leading edge. */
	.ui-nav-rail-btn--active {
		background: var(--rail-active-bg);
		color: var(--rail-fg-active);
		box-shadow: inset 2px 0 0 var(--accent);
	}
	.ui-nav-rail-label {
		font-size: 9px;
		font-weight: 500;
		color: var(--rail-fg);
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
	.ui-nav-rail-footer {
		margin-top: auto;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		width: 100%;
	}
	/* Footer controls sit on graphite, not on a surface. */
	.ui-nav-rail-footer :global(.ui-btn--ghost) { color: var(--rail-fg); }
	.ui-nav-rail-footer :global(.ui-btn--ghost:hover:not(:disabled)) {
		background: var(--rail-hover-bg);
		color: var(--rail-fg-active);
	}

	@media (max-width: 639px) {
		.ui-nav-rail--phone-bar {
			position: fixed;
			left: 0;
			right: 0;
			bottom: 0;
			width: 100%;
			height: 56px;
			flex-direction: row;
			padding: 0 4px;
			gap: 0;
			border-top: 1px solid var(--rail-hover-bg);
			z-index: 60;
		}
		.ui-nav-rail--phone-bar .ui-nav-rail-logo { display: none; }
		.ui-nav-rail--phone-bar .ui-nav-rail-items {
			flex-direction: row;
			justify-content: space-around;
			flex: 1;
			gap: 0;
		}
		.ui-nav-rail--phone-bar .ui-nav-rail-group { width: auto; padding: 0 6px; }
		.ui-nav-rail--phone-bar .ui-nav-rail-btn { width: 36px; height: 32px; }
		.ui-nav-rail--phone-bar .ui-nav-rail-btn--active { box-shadow: inset 0 2px 0 var(--accent); }
		.ui-nav-rail--phone-bar .ui-nav-rail-footer {
			flex-direction: row;
			margin-top: 0;
			width: auto;
			gap: 2px;
		}
	}
</style>
