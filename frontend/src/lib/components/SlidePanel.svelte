<script lang="ts">
	import { X } from '@lucide/svelte';

	interface Props {
		title: string;
		onClose: () => void;
		zIndex?: number;
		children?: import('svelte').Snippet;
	}

	let { title, onClose, zIndex = 60, children }: Props = $props();
</script>

<div
	class="slide-panel"
	style="z-index: {zIndex};"
	role="dialog"
	aria-label={title}
	aria-modal="true"
>
	<div class="panel-header">
		<h2 class="panel-title">{title}</h2>
		<button class="close-btn btn btn-ghost btn-icon" onclick={onClose} aria-label="Close panel">
			<X size={16} />
		</button>
	</div>

	<div class="panel-body">
		{#if children}
			{@render children()}
		{/if}
	</div>
</div>

<style>
	.slide-panel {
		position: fixed;
		right: 0;
		top: 0;
		width: var(--slide-panel-width);
		height: 100vh;
		background: var(--bg-surface);
		border-left: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		box-shadow: var(--shadow-lg);
		overflow: hidden;
	}

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 16px;
		height: 48px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.panel-title {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn { color: var(--text-muted); }
	.close-btn:hover { color: var(--text-primary); }

	.panel-body {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
	}

	@media (max-width: 639px) {
		.slide-panel {
			width: 100vw;
			left: 0;
			border-left: none;
		}
	}
</style>
