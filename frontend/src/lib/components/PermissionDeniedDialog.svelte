<script lang="ts">
	import { ShieldOff, ArrowLeft, X } from '@lucide/svelte';

	interface Props {
		open: boolean;
		message?: string;
		onDismiss: () => void;
		onBack?: () => void;
	}

	let {
		open,
		message = 'You don\'t have permission to perform this action.',
		onDismiss,
		onBack,
	}: Props = $props();

	function handleBackdropClick(e: MouseEvent) {
		if (e.target === e.currentTarget) onDismiss();
	}

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Escape') onDismiss();
	}
</script>

{#if open}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div
		class="backdrop"
		onclick={handleBackdropClick}
		onkeydown={handleKeydown}
		role="dialog"
		aria-modal="true"
		aria-label="Access restricted"
		tabindex="-1"
	>
		<div class="dialog">
			<button class="close-btn" onclick={onDismiss} aria-label="Dismiss">
				<X size={14} />
			</button>

			<div class="icon-wrap">
				<ShieldOff size={24} />
			</div>

			<h3 class="title">Access Restricted</h3>
			<p class="body">{message}</p>

			<div class="actions">
				{#if onBack}
					<button class="btn btn-secondary" onclick={onBack}>
						<ArrowLeft size={14} />
						Go back
					</button>
				{/if}
				<button class="btn btn-primary" onclick={onDismiss}>
					Dismiss
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.45);
		backdrop-filter: blur(2px);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
	}

	.dialog {
		position: relative;
		background: var(--bg-surface, #fff);
		border: 1px solid var(--border, #e2e8f0);
		border-radius: 12px;
		padding: 32px 28px 24px;
		width: 100%;
		max-width: 380px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		box-shadow: 0 8px 32px rgba(0, 0, 0, 0.18);
	}

	.close-btn {
		position: absolute;
		top: 12px;
		right: 12px;
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-muted);
		padding: 4px;
		border-radius: 4px;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.close-btn:hover { color: var(--text-primary); background: var(--bg-hover, #f1f5f9); }

	.icon-wrap {
		width: 52px;
		height: 52px;
		border-radius: 50%;
		background: var(--bg-elevated, #f8fafc);
		border: 1px solid var(--border, #e2e8f0);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-muted);
		margin-bottom: 4px;
	}

	.title {
		font-size: 16px;
		font-weight: 600;
		color: var(--text-primary);
		margin: 0;
		text-align: center;
	}

	.body {
		font-size: 13px;
		color: var(--text-muted);
		margin: 0;
		text-align: center;
		line-height: 1.5;
		max-width: 300px;
	}

	.actions {
		display: flex;
		gap: 8px;
		margin-top: 6px;
		justify-content: center;
	}
</style>
