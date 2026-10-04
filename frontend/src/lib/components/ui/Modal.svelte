<script lang="ts" module>
	let modalCounter = 0;
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		open: boolean;
		title: string;
		children: Snippet;
		footer?: Snippet;
	}

	let { open = $bindable(), title, children, footer }: Props = $props();

	const titleId = `ui-modal-title-${++modalCounter}`;
	const FOCUSABLE =
		'a[href], button:not([disabled]), input:not([disabled]), select:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])';

	let dialogEl: HTMLDivElement | undefined = $state();
	let returnFocusTo: HTMLElement | null = null;

	function close() {
		open = false;
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') {
			e.stopPropagation();
			close();
			return;
		}
		if (e.key !== 'Tab' || !dialogEl) return;
		const items = [...dialogEl.querySelectorAll<HTMLElement>(FOCUSABLE)];
		if (items.length === 0) {
			e.preventDefault();
			dialogEl.focus();
			return;
		}
		const first = items[0];
		const last = items[items.length - 1];
		const active = document.activeElement;
		if (e.shiftKey && (active === first || active === dialogEl)) {
			e.preventDefault();
			last.focus();
		} else if (!e.shiftKey && active === last) {
			e.preventDefault();
			first.focus();
		}
	}

	$effect(() => {
		if (!open) return;
		returnFocusTo = document.activeElement as HTMLElement | null;
		dialogEl?.focus();
		return () => returnFocusTo?.focus();
	});
</script>

{#if open}
	<div class="ui-modal-scrim" role="presentation" onclick={close}>
		<div
			class="ui-modal"
			role="dialog"
			aria-modal="true"
			aria-labelledby={titleId}
			tabindex="-1"
			bind:this={dialogEl}
			onclick={(e) => e.stopPropagation()}
			onkeydown={onKeydown}
		>
			<div class="ui-modal-header">
				<span id={titleId}>{title}</span>
			</div>
			<div class="ui-modal-body">
				{@render children()}
			</div>
			{#if footer}
				<div class="ui-modal-footer">
					{@render footer()}
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	.ui-modal-scrim {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.55);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 500;
		padding: 16px;
	}
	.ui-modal {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		width: 100%;
		max-width: 420px;
		box-shadow: var(--shadow-lg);
		outline: none;
	}
	.ui-modal-header {
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
		font-weight: 700;
		color: var(--text-primary);
	}
	.ui-modal-body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.ui-modal-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 12px 16px;
		border-top: 1px solid var(--border);
	}
</style>
