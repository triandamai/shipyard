<script lang="ts" module>
	let sheetCounter = 0;
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';
	import { fade, fly } from 'svelte/transition';
	import { X } from '@lucide/svelte';

	interface Props {
		open: boolean;
		title: string;
		subtitle?: string;
		children: Snippet;
		footer?: Snippet;
	}

	let { open = $bindable(), title, subtitle, children, footer }: Props = $props();

	const titleId = `ui-sheet-title-${++sheetCounter}`;
	let sheetEl: HTMLDivElement | undefined = $state();
	let returnFocusTo: HTMLElement | null = null;

	function close() {
		open = false;
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') close();
	}

	$effect(() => {
		if (open) {
			returnFocusTo = document.activeElement as HTMLElement | null;
			sheetEl?.focus();
			return () => returnFocusTo?.focus();
		}
	});
</script>

{#if open}
	<div class="ui-sheet-scrim" role="presentation" onclick={close} transition:fade={{ duration: 150 }}></div>
	<div
		class="ui-sheet"
		role="dialog"
		aria-modal="true"
		aria-labelledby={titleId}
		tabindex="-1"
		bind:this={sheetEl}
		onkeydown={onKeydown}
		transition:fly={{ y: 320, duration: 220 }}
	>
		<div class="ui-sheet-handle" aria-hidden="true"></div>
		<div class="ui-sheet-header">
			<div class="ui-sheet-titles">
				<span id={titleId} class="ui-sheet-title">{title}</span>
				{#if subtitle}<span class="ui-sheet-subtitle">{subtitle}</span>{/if}
			</div>
			<button type="button" class="ui-sheet-close" aria-label="Close" onclick={close}>
				<X size={16} />
			</button>
		</div>
		<div class="ui-sheet-body">
			{@render children()}
		</div>
		{#if footer}
			<div class="ui-sheet-footer">
				{@render footer()}
			</div>
		{/if}
	</div>
{/if}

<style>
	.ui-sheet-scrim {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.45);
		z-index: 500;
	}
	.ui-sheet {
		position: fixed;
		left: 50%;
		bottom: 0;
		transform: translateX(-50%);
		width: 100%;
		max-width: 640px;
		max-height: 88vh;
		display: flex;
		flex-direction: column;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-bottom: none;
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
		box-shadow: var(--shadow-lg);
		z-index: 501;
		outline: none;
	}
	.ui-sheet-handle {
		width: 36px;
		height: 4px;
		border-radius: 2px;
		background: var(--border-hover);
		margin: 8px auto 2px;
		flex-shrink: 0;
	}
	.ui-sheet-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 20px 12px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}
	.ui-sheet-titles { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
	.ui-sheet-title { font-size: 14px; font-weight: 700; color: var(--text-primary); }
	.ui-sheet-subtitle { font-size: 11.5px; color: var(--text-muted); }
	.ui-sheet-close {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: none;
		border-radius: var(--radius-md);
		background: none;
		color: var(--text-muted);
		cursor: pointer;
		flex-shrink: 0;
		transition: background var(--transition-fast), color var(--transition-fast);
	}
	.ui-sheet-close:hover { background: var(--bg-hover); color: var(--text-primary); }
	.ui-sheet-body {
		padding: 16px 20px;
		display: flex;
		flex-direction: column;
		gap: 14px;
		overflow-y: auto;
		min-height: 0;
	}
	.ui-sheet-footer {
		display: flex;
		justify-content: flex-end;
		align-items: center;
		gap: 8px;
		padding: 12px 20px;
		border-top: 1px solid var(--border);
		flex-shrink: 0;
	}
</style>
