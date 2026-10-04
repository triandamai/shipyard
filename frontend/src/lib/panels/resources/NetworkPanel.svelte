<script lang="ts">
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { FormField, TextField, Select, Button, InlineAlert, Spinner } from '$lib/components/ui';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: () => void;
	}

	let { projectId, onCreated }: Props = $props();

	let name = $state('');
	let driver = $state('overlay');
	let subnet = $state('');
	let isSubmitting = $state(false);
	let submitError = $state('');

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		submitError = '';
		isSubmitting = true;
		try {
			const res = await api.createNetwork(projectId, { name, driver, subnet });
			if (res.error) { submitError = res.error.message; return; }
			onCreated?.();
			uiStore.clearPanels();
		} finally {
			isSubmitting = false;
		}
	}
</script>

<div class="panel-wrap">
	<form class="form" onsubmit={handleSubmit}>
		<FormField label="Network Name" for="net-name">
			<TextField id="net-name" type="text" bind:value={name} placeholder="my-network" required />
		</FormField>
		<FormField label="Driver" for="net-driver">
			<Select id="net-driver" bind:value={driver} options={[
				{ value: 'overlay', label: 'overlay (Swarm)' },
				{ value: 'bridge', label: 'bridge (local)' },
			]} />
		</FormField>
		<FormField label="Subnet (optional)" for="net-subnet">
			<div class="mono-field"><TextField id="net-subnet" type="text" bind:value={subnet} placeholder="10.0.0.0/24" /></div>
		</FormField>

		{#if submitError}
			<div role="alert"><InlineAlert tone="error">{submitError}</InlineAlert></div>
		{/if}

		<div class="submit-row">
			<Button variant="primary" type="submit" disabled={isSubmitting}>
				{#if isSubmitting}
					<Spinner size={12} tone="current" /> Creating…
				{:else}
					Add Network
				{/if}
			</Button>
		</div>
	</form>
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 14px; }
	.submit-row { margin-top: 4px; }
	.mono-field :global(input) { font-family: var(--font-mono); }
</style>
