<script lang="ts" module>
	let confirmCounter = 0;
</script>

<script lang="ts">
	import Modal from './Modal.svelte';
	import Button from './Button.svelte';
	import TextField from './TextField.svelte';
	import InlineAlert from './InlineAlert.svelte';

	interface Props {
		open: boolean;
		title: string;
		message: string;
		confirmLabel?: string;
		danger?: boolean;
		confirmText?: string;
		error?: string;
		onConfirm: () => void | boolean | Promise<void | boolean>;
	}

	let {
		open = $bindable(),
		title,
		message,
		confirmLabel = 'Delete',
		danger = true,
		confirmText,
		error,
		onConfirm
	}: Props = $props();

	const inputId = `ui-confirm-type-input-${++confirmCounter}`;

	let typedConfirm = $state('');
	let confirming = $state(false);

	// confirmText === undefined is the only "no typing" mode; a defined but empty
	// value (e.g. a slug still loading) must never be confirmable.
	let canConfirm = $derived(
		confirmText === undefined || (confirmText !== '' && typedConfirm === confirmText)
	);

	async function handleConfirm() {
		if (!canConfirm || confirming) return;
		confirming = true;
		try {
			const result = await onConfirm();
			// Returning false means the action failed: stay open, keep the typed text.
			if (result === false) return;
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

<Modal bind:open {title} dismissible={!confirming}>
	<p class="ui-confirm-message">{message}</p>
	{#if confirmText !== undefined}
		<div class="ui-confirm-type-field">
			<label class="ui-confirm-type-label" for={inputId}>
				Type <code class="ui-confirm-code">{confirmText}</code> to confirm
			</label>
			<TextField id={inputId} bind:value={typedConfirm} />
		</div>
	{/if}
	{#if error}
		<div role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{/if}
	{#snippet footer()}
		<Button variant="ghost" disabled={confirming} onclick={handleCancel}>Cancel</Button>
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
