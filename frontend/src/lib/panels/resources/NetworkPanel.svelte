<script lang="ts">
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';

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
		<div class="form-group">
			<label class="form-label" for="net-name">Network Name</label>
			<input id="net-name" class="form-input" type="text" bind:value={name}
				placeholder="my-network" required />
		</div>
		<div class="form-group">
			<label class="form-label" for="net-driver">Driver</label>
			<select id="net-driver" class="form-select" bind:value={driver}>
				<option value="overlay">overlay (Swarm)</option>
				<option value="bridge">bridge (local)</option>
			</select>
		</div>
		<div class="form-group">
			<label class="form-label" for="net-subnet">Subnet (optional)</label>
			<input id="net-subnet" class="form-input font-mono" type="text" bind:value={subnet}
				placeholder="10.0.0.0/24" />
		</div>

		{#if submitError}
			<div class="error-msg">{submitError}</div>
		{/if}

		<button class="btn btn-primary submit-btn" type="submit" disabled={isSubmitting}>
			{#if isSubmitting}
				<div class="btn-spinner"></div> Creating…
			{:else}
				Add Network
			{/if}
		</button>
	</form>
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 14px; }
	.form-group { display: flex; flex-direction: column; gap: 4px; }

	.form-label {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
	}

	.form-input, .form-select {
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary);
		font-size: 13px; font-family: var(--font-sans); padding: 8px 10px;
		outline: none; transition: border-color var(--transition-fast);
	}
	.form-input:focus, .form-select:focus { border-color: var(--accent); }
	.font-mono { font-family: var(--font-mono); }

	.error-msg {
		font-size: 12px; color: var(--accent-red); padding: 8px 10px;
		background: color-mix(in srgb, var(--accent-red) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent-red) 30%, transparent);
		border-radius: var(--radius-sm);
	}

	.submit-btn { margin-top: 4px; display: flex; align-items: center; gap: 6px; justify-content: center; }

	.btn-spinner {
		width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3);
		border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite;
	}

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
