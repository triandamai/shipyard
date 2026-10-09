<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'danger-outline';
		size?: 'sm' | 'md' | 'icon';
		type?: 'button' | 'submit';
		disabled?: boolean;
		onclick?: (e: MouseEvent) => void;
		href?: string;
		target?: string;
		rel?: string;
		title?: string;
		children: Snippet;
		'aria-label'?: string;
	}

	let {
		variant = 'primary',
		size = 'md',
		type = 'button',
		disabled = false,
		onclick,
		href,
		target,
		rel,
		title,
		children,
		'aria-label': ariaLabel
	}: Props = $props();
</script>

{#if href}
	<a
		{href}
		{target}
		{rel}
		{title}
		{onclick}
		class="ui-btn ui-btn--{variant} ui-btn--{size}"
		aria-label={ariaLabel}
	>
		{@render children()}
	</a>
{:else}
	<button
		{type}
		{disabled}
		{title}
		class="ui-btn ui-btn--{variant} ui-btn--{size}"
		{onclick}
		aria-label={ariaLabel}
	>
		{@render children()}
	</button>
{/if}

<style>
	.ui-btn {
		display: inline-flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		border-radius: var(--radius-md);
		font-family: var(--font-sans);
		font-weight: 500;
		cursor: pointer;
		border: 1px solid transparent;
		transition: background var(--transition-fast), border-color var(--transition-fast), opacity var(--transition-fast);
		white-space: nowrap;
		position: relative;
		text-decoration: none;
	}
	.ui-btn:disabled { opacity: 0.5; cursor: not-allowed; }

	/* Sizes */
	.ui-btn--sm { padding: 5px 12px; font-size: 12px; height: 30px; }
	.ui-btn--md { padding: 7px 16px; font-size: 13px; height: 36px; }
	.ui-btn--icon { padding: 0; width: 32px; height: 32px; }

	/* Variants — state layers via a pseudo-element overlay (M3 pattern: a
	   semi-transparent layer on top of the base color, not a hue shift) so
	   hover/pressed feedback is consistent across every variant. */
	.ui-btn::after {
		content: '';
		position: absolute;
		inset: 0;
		border-radius: inherit;
		background: currentColor;
		opacity: 0;
		transition: opacity var(--transition-fast);
		pointer-events: none;
	}
	.ui-btn:hover:not(:disabled)::after { opacity: 0.08; }
	.ui-btn:active:not(:disabled)::after { opacity: 0.12; }

	.ui-btn--primary {
		background: var(--accent);
		color: var(--accent-fg);
		border-color: var(--accent);
	}
	.ui-btn--primary:hover:not(:disabled) { background: var(--accent-hover); border-color: var(--accent-hover); }

	.ui-btn--secondary {
		background: var(--bg-surface);
		color: var(--text-secondary);
		border-color: var(--border);
	}
	.ui-btn--secondary:hover:not(:disabled) { background: var(--bg-hover); }

	.ui-btn--ghost {
		background: transparent;
		color: var(--text-secondary);
	}
	.ui-btn--ghost:hover:not(:disabled) { background: var(--bg-hover); color: var(--text-primary); }

	.ui-btn--danger {
		background: var(--accent-red);
		color: var(--accent-fg);
		border-color: var(--accent-red);
	}

	.ui-btn--danger-outline {
		background: transparent;
		color: var(--accent-red);
		border-color: var(--accent-red);
	}
	.ui-btn--danger-outline:hover:not(:disabled) { background: var(--accent-red-muted); }
</style>
