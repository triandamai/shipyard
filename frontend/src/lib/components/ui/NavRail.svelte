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

	function itemClass(g: Group): string {
		return isHighlighted(g) ? 'ui-nav-rail-item ui-nav-rail-item--active' : 'ui-nav-rail-item';
	}
</script>

<aside class={phoneBar ? 'ui-nav-rail ui-nav-rail--phone-bar' : 'ui-nav-rail'}>
	<div class="ui-nav-rail-logo" aria-hidden="true">
		<Anchor size={15} strokeWidth={2.5} />
	</div>
	<nav class="ui-nav-rail-items">
		{#each groups as g (g.key)}
			{@const Icon = g.icon}
			{#if g.href}
				<a href={g.href} class={itemClass(g)} aria-current={g.active ? 'page' : undefined} title={labelOverflow === 'ellipsis' ? g.label : undefined}>
					<span class="ui-nav-rail-icon"><Icon size={18} strokeWidth={isHighlighted(g) ? 2.25 : 1.75} /></span>
					<span class="ui-nav-rail-label ui-nav-rail-label--{labelOverflow}">{g.label}</span>
				</a>
			{:else}
				<button
					type="button"
					class={itemClass(g)}
					onclick={() => handleClick(g.key)}
					aria-expanded={activeGroup === g.key}
					title={labelOverflow === 'ellipsis' ? g.label : undefined}
				>
					<span class="ui-nav-rail-icon"><Icon size={18} strokeWidth={isHighlighted(g) ? 2.25 : 1.75} /></span>
					<span class="ui-nav-rail-label ui-nav-rail-label--{labelOverflow}">{g.label}</span>
				</button>
			{/if}
		{/each}
	</nav>
	{#if footer}
		<div class="ui-nav-rail-footer">{@render footer()}</div>
	{/if}
</aside>

<style>
	.ui-nav-rail {
		width: var(--rail-width);
		flex-shrink: 0;
		background: var(--rail-bg);
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 12px 0;
		height: 100vh;
		height: 100dvh;
		position: relative;
		z-index: 5;
		border-right: 1px solid var(--rail-hover-bg);
	}
	.ui-nav-rail-logo {
		width: 32px;
		height: 32px;
		border-radius: var(--radius-md);
		background: var(--accent);
		color: var(--accent-fg);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.ui-nav-rail-items {
		display: flex;
		flex-direction: column;
		gap: 2px;
		width: 100%;
		margin-top: 12px;
		padding-top: 12px;
		border-top: 1px solid var(--rail-hover-bg);
	}

	/* One target per destination: icon and label are both inside it. */
	.ui-nav-rail-item {
		position: relative;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		width: 100%;
		padding: 6px 4px;
		background: none;
		border: none;
		color: var(--rail-fg);
		font-family: var(--font-sans);
		text-decoration: none;
		cursor: pointer;
	}
	.ui-nav-rail-icon {
		width: 40px;
		height: 30px;
		border-radius: var(--radius-md);
		display: flex;
		align-items: center;
		justify-content: center;
		transition: background var(--transition-fast), color var(--transition-fast);
	}
	.ui-nav-rail-label {
		font-size: 10px;
		font-weight: 500;
		line-height: 1.2;
		max-width: 100%;
		text-align: center;
		transition: color var(--transition-fast);
	}
	.ui-nav-rail-label--ellipsis { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
	.ui-nav-rail-label--wrap { white-space: normal; overflow-wrap: anywhere; }

	/* Indicator: a signal bar flush with the rail's leading edge, outside the
	   rounded icon tile so its corners never clip it. Grows in on activation;
	   hover previews it as a short tick. */
	.ui-nav-rail-item::before {
		content: '';
		position: absolute;
		left: 0;
		top: 21px;
		width: 3px;
		height: 0;
		border-radius: 0 3px 3px 0;
		background: var(--accent);
		transform: translateY(-50%);
		transition: height var(--transition-normal), background var(--transition-fast);
	}
	.ui-nav-rail-item:hover { color: var(--rail-fg-active); }
	.ui-nav-rail-item:hover .ui-nav-rail-icon { background: var(--rail-hover-bg); }
	.ui-nav-rail-item:hover::before { height: 8px; background: var(--rail-fg); }
	.ui-nav-rail-item:focus-visible { outline: none; }
	.ui-nav-rail-item:focus-visible .ui-nav-rail-icon { outline: 2px solid var(--accent); outline-offset: 1px; }

	.ui-nav-rail-item--active,
	.ui-nav-rail-item--active:hover { color: var(--rail-fg-active); }
	.ui-nav-rail-item--active .ui-nav-rail-icon,
	.ui-nav-rail-item--active:hover .ui-nav-rail-icon { background: var(--rail-active-bg); }
	.ui-nav-rail-item--active::before,
	.ui-nav-rail-item--active:hover::before { height: 22px; background: var(--accent); }
	.ui-nav-rail-item--active .ui-nav-rail-label { font-weight: 600; }

	.ui-nav-rail-footer {
		margin-top: auto;
		padding-top: 10px;
		border-top: 1px solid var(--rail-hover-bg);
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
			border-right: none;
			border-top: 1px solid var(--rail-hover-bg);
			z-index: 60;
		}
		.ui-nav-rail--phone-bar .ui-nav-rail-logo { display: none; }
		.ui-nav-rail--phone-bar .ui-nav-rail-items {
			flex-direction: row;
			justify-content: space-around;
			flex: 1;
			gap: 0;
			margin: 0;
			padding: 0;
			border: none;
			height: 100%;
		}
		.ui-nav-rail--phone-bar .ui-nav-rail-item { width: auto; flex: 1; justify-content: center; height: 100%; gap: 2px; }
		.ui-nav-rail--phone-bar .ui-nav-rail-icon { width: 36px; height: 26px; }
		/* Indicator moves to the top edge of the bar. */
		.ui-nav-rail--phone-bar .ui-nav-rail-item::before {
			left: 50%;
			top: 0;
			width: 0;
			height: 3px;
			border-radius: 0 0 3px 3px;
			transform: translateX(-50%);
			transition: width var(--transition-normal), background var(--transition-fast);
		}
		.ui-nav-rail--phone-bar .ui-nav-rail-item:hover::before { width: 0; height: 3px; }
		.ui-nav-rail--phone-bar .ui-nav-rail-item--active::before,
		.ui-nav-rail--phone-bar .ui-nav-rail-item--active:hover::before { width: 24px; height: 3px; }
		.ui-nav-rail--phone-bar .ui-nav-rail-footer {
			flex-direction: row;
			margin-top: 0;
			padding: 0;
			border: none;
			width: auto;
			gap: 2px;
		}
	}
</style>
