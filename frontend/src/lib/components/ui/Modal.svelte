<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		open: boolean;
		title: string;
		children: Snippet;
		footer?: Snippet;
	}

	let { open = $bindable(), title, children, footer }: Props = $props();

	let dialogEl: HTMLDivElement | undefined = $state();

	function close() {
		open = false;
	}

	function onBackdropKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') close();
	}

	$effect(() => {
		if (open) dialogEl?.focus();
	});
</script>

{#if open}
	<div class="ui-modal-scrim" role="presentation" onclick={close}>
		<div
			class="ui-modal"
			role="dialog"
			aria-modal="true"
			aria-labelledby="ui-modal-title"
			tabindex="-1"
			bind:this={dialogEl}
			onclick={(e) => e.stopPropagation()}
			onkeydown={onBackdropKeydown}
		>
			<div class="ui-modal-header">
				<span id="ui-modal-title">{title}</span>
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
