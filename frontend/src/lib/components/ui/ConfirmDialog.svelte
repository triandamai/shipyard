<script lang="ts" module>
	let confirmCounter = 0;
</script>

<script lang="ts">
	import Modal from './Modal.svelte';
	import Button from './Button.svelte';
	import TextField from './TextField.svelte';

	interface Props {
		open: boolean;
		title: string;
		message: string;
		confirmLabel?: string;
		danger?: boolean;
		confirmText?: string;
		onConfirm: () => void | Promise<void>;
	}

	let {
		open = $bindable(),
		title,
		message,
		confirmLabel = 'Delete',
		danger = true,
		confirmText,
		onConfirm
	}: Props = $props();

	const inputId = `ui-confirm-type-input-${++confirmCounter}`;

	let typedConfirm = $state('');
	let confirming = $state(false);

	let canConfirm = $derived(!confirmText || typedConfirm === confirmText);

	async function handleConfirm() {
		if (!canConfirm || confirming) return;
		confirming = true;
		try {
			await onConfirm();
			typedConfirm = '';
			open = false;
		} finally {
			confirming = false;
		}
	}

	function handleCancel() {
		typedConfirm = '';
		open = false;
	}
</script>

<Modal bind:open {title}>
	<p class="ui-confirm-message">{message}</p>
	{#if confirmText}
		<div class="ui-confirm-type-field">
			<label class="ui-confirm-type-label" for={inputId}>
				Type <code class="ui-confirm-code">{confirmText}</code> to confirm
			</label>
			<TextField id={inputId} bind:value={typedConfirm} />
		</div>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" onclick={handleCancel}>Cancel</Button>
		<Button variant={danger ? 'danger' : 'primary'} disabled={!canConfirm || confirming} onclick={handleConfirm}>
			{confirming ? 'Working…' : confirmLabel}
		</Button>
	{/snippet}
</Modal>

<style>
	.ui-confirm-message {
		font-size: 12.5px;
		color: var(--text-primary);
		line-height: 1.5;
		margin: 0;
	}
	.ui-confirm-type-field { display: flex; flex-direction: column; gap: 6px; }
	.ui-confirm-type-label { font-size: 11px; color: var(--text-muted); }
	.ui-confirm-code {
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-elevated);
		padding: 1px 5px;
		border-radius: 3px;
		border: 1px solid var(--border);
	}
</style>
