<script lang="ts">
	import { toastStore, type Toast } from '$lib/stores/toast.store';
	import { fly, fade } from 'svelte/transition';

	const icons: Record<Toast['type'], string> = {
		success: '✓',
		error: '✕',
		warning: '⚠',
		info: 'ℹ'
	};
</script>

{#if $toastStore.length > 0}
	<div class="toast-container" role="region" aria-label="Notifications" aria-live="polite">
		{#each $toastStore as toast (toast.id)}
			<div
				class="toast toast--{toast.type}"
				in:fly={{ y: -20, duration: 200 }}
				out:fade={{ duration: 150 }}
			>
				<span class="toast__icon">{icons[toast.type]}</span>
				<div class="toast__body">
					<span class="toast__title">{toast.title}</span>
					{#if toast.message}
						<span class="toast__msg">{toast.message}</span>
					{/if}
				</div>
				<button class="toast__close" onclick={() => toastStore.remove(toast.id)} aria-label="Dismiss">
					✕
				</button>
			</div>
		{/each}
	</div>
{/if}

<style>
	.toast-container {
		position: fixed;
		top: 1.25rem;
		left: 50%;
		transform: translateX(-50%);
		z-index: 9999;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 0.5rem;
		width: max-content;
		max-width: min(420px, calc(100vw - 2rem));
		pointer-events: none;
	}

	.toast {
		display: flex;
		align-items: flex-start;
		gap: 0.625rem;
		padding: 0.75rem 1rem;
		border-radius: 8px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		box-shadow: var(--shadow-lg);
		border-left: 3px solid transparent;
		font-size: 0.8125rem;
		pointer-events: all;
		width: 100%;
	}

	.toast--success { border-left-color: #16a34a; }
	.toast--error   { border-left-color: #dc2626; }
	.toast--warning { border-left-color: #d97706; }
	.toast--info    { border-left-color: var(--accent, #2563eb); }

	.toast__icon {
		flex-shrink: 0;
		font-size: 0.875rem;
		margin-top: 1px;
	}

	.toast--success .toast__icon { color: #16a34a; }
	.toast--error   .toast__icon { color: #dc2626; }
	.toast--warning .toast__icon { color: #d97706; }
	.toast--info    .toast__icon { color: var(--accent, #2563eb); }

	.toast__body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 0.125rem;
	}

	.toast__title {
		font-weight: 500;
		color: var(--text-primary);
	}

	.toast__msg {
		color: var(--text-muted);
		font-size: 0.75rem;
		line-height: 1.4;
	}

	.toast__close {
		flex-shrink: 0;
		background: none;
		border: none;
		padding: 0;
		cursor: pointer;
		color: var(--text-muted);
		font-size: 0.75rem;
		line-height: 1;
		opacity: 0.6;
	}

	.toast__close:hover { opacity: 1; }
</style>
