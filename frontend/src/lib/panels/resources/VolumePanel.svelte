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
	let mountPath = $state('/data');
	let isSubmitting = $state(false);
	let submitError = $state('');

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		submitError = '';
		isSubmitting = true;
		try {
			const res = await api.post<unknown>(`/projects/${projectId}/volumes`, {
				name,
				mount_path: mountPath,
				driver: 'local',
			});
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
		<FormField label="Volume Name" for="vol-name">
			<TextField id="vol-name" type="text" bind:value={name} placeholder="my-data" required />
		</FormField>
		<FormField label="Mount Path" for="vol-mount" hint="Container path where the volume is mounted">
			<div class="mono-field"><TextField id="vol-mount" type="text" bind:value={mountPath} placeholder="/data" required /></div>
		</FormField>

		{#if submitError}
			<div role="alert"><InlineAlert tone="error">{submitError}</InlineAlert></div>
		{/if}

		<div class="submit-row">
			<Button variant="primary" type="submit" disabled={isSubmitting}>
				{#if isSubmitting}
					<Spinner size={12} tone="current" /> Creating…
				{:else}
					Add Volume
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
