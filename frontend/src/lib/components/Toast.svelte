<script lang="ts">
	import { toastStore, type Toast } from '$lib/stores/toast.store';
	import { fly, fade } from 'svelte/transition';
	import { Check, X, TriangleAlert, Info } from '@lucide/svelte';
	import { Button } from '$lib/components/ui';

	const icons: Record<Toast['type'], typeof Check> = {
		success: Check,
		error: X,
		warning: TriangleAlert,
		info: Info
	};
</script>

{#if $toastStore.length > 0}
	<div class="toast-container" role="region" aria-label="Notifications" aria-live="polite">
		{#each $toastStore as toast (toast.id)}
			{@const Icon = icons[toast.type]}
			<div
				class="toast toast--{toast.type}"
				in:fly={{ y: -20, duration: 200 }}
				out:fade={{ duration: 150 }}
			>
				<span class="toast__icon"><Icon size={15} /></span>
				<div class="toast__body">
					<span class="toast__title">{toast.title}</span>
					{#if toast.message}
						<span class="toast__msg">{toast.message}</span>
					{/if}
				</div>
				<Button variant="ghost" size="icon" onclick={() => toastStore.remove(toast.id)} aria-label="Dismiss">
					<X size={14} />
				</Button>
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
		padding: 0.625rem 0.5rem 0.625rem 1rem;
		border-radius: var(--radius-lg);
		background: var(--bg-surface);
		border: 1px solid var(--border);
		box-shadow: var(--shadow-lg);
		border-left: 3px solid transparent;
		font-size: 0.8125rem;
		pointer-events: all;
		width: 100%;
	}

	.toast--success { border-left-color: var(--accent-green); }
	.toast--error   { border-left-color: var(--accent-red); }
	.toast--warning { border-left-color: var(--accent-yellow); }
	.toast--info    { border-left-color: var(--accent); }

	.toast__icon {
		flex-shrink: 0;
		display: flex;
		margin-top: 1px;
	}

	.toast--success .toast__icon { color: var(--accent-green); }
	.toast--error   .toast__icon { color: var(--accent-red); }
	.toast--warning .toast__icon { color: var(--accent-yellow); }
	.toast--info    .toast__icon { color: var(--accent); }

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
</style>
