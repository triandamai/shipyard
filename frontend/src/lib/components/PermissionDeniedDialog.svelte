<script lang="ts">
	import { ShieldOff, ArrowLeft } from '@lucide/svelte';
	import { Modal, Button } from '$lib/components/ui';

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
</script>

<!-- `open` is store/page-driven: Modal asks to close (Escape / scrim click) and we forward it to onDismiss. -->
<Modal bind:open={() => open, (v) => { if (!v) onDismiss(); }} title="Access Restricted">
	<div class="pd-content">
		<div class="icon-wrap">
			<ShieldOff size={24} />
		</div>
		<p class="body">{message}</p>
	</div>
	{#snippet footer()}
		{#if onBack}
			<Button variant="secondary" onclick={onBack}>
				<ArrowLeft size={14} />
				Go back
			</Button>
		{/if}
		<Button variant="primary" onclick={onDismiss}>Dismiss</Button>
	{/snippet}
</Modal>

<style>
	.pd-content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		padding: 4px 0;
	}

	.icon-wrap {
		width: 52px;
		height: 52px;
		border-radius: 50%;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-muted);
	}

	.body {
		font-size: 13px;
		color: var(--text-muted);
		margin: 0;
		text-align: center;
		line-height: 1.5;
	}
</style>
