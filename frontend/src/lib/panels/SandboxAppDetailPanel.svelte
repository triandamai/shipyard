<script lang="ts">
	import { onMount } from 'svelte';
	import { Play, Square, ExternalLink, Code2, Trash2, AlertTriangle } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { goto } from '$app/navigation';
	import { uiStore } from '$lib/stores/ui.store';
	import { orgStore } from '$lib/stores/org.store';
	import { can, permProject } from '$lib/auth/permissions';
	import type { SandboxInstance, Service } from '$lib/api/types';

	interface Props {
		serviceId: string;
		projectId: string;
		orgSlug: string;
		projectSlug: string;
		onDeleted?: () => void;
	}

	let { serviceId, projectId, orgSlug, projectSlug, onDeleted }: Props = $props();

	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let canDeleteApp = $derived(
		can($orgStore.myMembership?.role ?? null, $orgStore.myMembership?.permissions ?? [], permProject(orgId, projectId, 'service', 'delete'))
	);

	let instance = $state<SandboxInstance | null>(null);
	let service = $state<Service | null>(null);
	let isLoading = $state(true);
	let error = $state<string | null>(null);
	let isStarting = $state(false);
	let isStopping = $state(false);

	// Danger zone
	let confirmSlug = $state('');
	let isDeleting = $state(false);
	let deleteError = $state('');
	let canDelete = $derived(confirmSlug.trim() === (service?.slug ?? ''));

	async function load() {
		isLoading = true;
		const [instanceRes, serviceRes] = await Promise.all([
			api.getSandboxInstance(serviceId),
			api.getService(projectId, serviceId)
		]);
		if (instanceRes.data) instance = instanceRes.data;
		else error = instanceRes.error?.message ?? 'Failed to load sandbox status';
		if (serviceRes.data) service = serviceRes.data;
		isLoading = false;
	}

	async function start() {
		isStarting = true;
		const res = await api.startSandbox(serviceId);
		if (res.data) await load();
		else error = res.error?.message ?? 'Failed to start sandbox';
		isStarting = false;
	}

	async function stop() {
		isStopping = true;
		const res = await api.stopSandbox(serviceId);
		if (res.data) await load();
		else error = res.error?.message ?? 'Failed to stop sandbox';
		isStopping = false;
	}

	function openEditor() {
		goto(`/orgs/${orgSlug}/projects/${projectSlug}/apps/${serviceId}/editor`);
	}

	async function deleteApp() {
		if (!canDelete || isDeleting) return;
		isDeleting = true;
		deleteError = '';
		const res = await api.deleteService(projectId, serviceId);
		isDeleting = false;
		if (res.error) {
			deleteError = res.error.message;
			return;
		}
		onDeleted?.();
		uiStore.popPanel();
	}

	onMount(load);
</script>

<div class="sandbox-panel">
	{#if isLoading}
		<p class="muted">Loading…</p>
	{:else if error}
		<p class="error">{error}</p>
	{:else if instance}
		<div class="status-row">
			<span class="status-dot" class:running={instance.status === 'running'}></span>
			<span class="status-text">{instance.status}</span>
		</div>

		{#if instance.preview_url}
			<a class="preview-link" href={instance.preview_url} target="_blank" rel="noopener noreferrer">
				{instance.preview_url} <ExternalLink size={12} />
			</a>
		{/if}

		<div class="actions">
			{#if instance.status === 'running'}
				<button class="btn btn-secondary btn-sm" onclick={stop} disabled={isStopping}>
					<Square size={13} /> Stop
				</button>
			{:else}
				<button class="btn btn-primary btn-sm" onclick={start} disabled={isStarting}>
					<Play size={13} /> Start
				</button>
			{/if}
			<button class="btn btn-secondary btn-sm" onclick={openEditor}>
				<Code2 size={13} /> Open Editor
			</button>
		</div>

		{#if canDeleteApp && service}
			<section class="danger-zone">
				<div class="danger-header">
					<AlertTriangle size={14} />
					<h3 class="danger-title">Danger Zone</h3>
				</div>
				<p class="danger-desc">
					Deleting this app removes its container, its dedicated volume, and its preview domain
					(including its DNS record, if one was synced). <strong>This cannot be undone.</strong>
				</p>
				<div class="danger-confirm">
					<label class="danger-label" for="confirm-sandbox-slug">
						Type <strong>{service.slug}</strong> to confirm
					</label>
					<input
						id="confirm-sandbox-slug"
						class="danger-input"
						type="text"
						placeholder={service.slug}
						bind:value={confirmSlug}
						autocomplete="off"
						spellcheck="false"
					/>
				</div>
				{#if deleteError}
					<div class="delete-error">{deleteError}</div>
				{/if}
				<button class="btn-delete" type="button" disabled={!canDelete || isDeleting} onclick={deleteApp}>
					<Trash2 size={13} />
					{isDeleting ? 'Deleting…' : 'Delete App'}
				</button>
			</section>
		{/if}
	{/if}
</div>

<style>
	.sandbox-panel {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
	}
	.status-row {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.status-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--text-dim);
	}
	.status-dot.running {
		background: var(--status-running, #22c55e);
	}
	.status-text {
		font-size: 12px;
		text-transform: capitalize;
		color: var(--text-secondary);
	}
	.preview-link {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--accent);
		font-family: var(--font-mono);
	}
	.actions {
		display: flex;
		gap: 8px;
	}
	.muted {
		color: var(--text-dim);
		font-size: 12px;
	}
	.error {
		color: var(--accent-red);
		font-size: 12px;
	}

	/* Danger Zone */
	.danger-zone {
		background: color-mix(in srgb, var(--accent-red, #EF4444) 4%, var(--bg-elevated));
		border: 1px solid color-mix(in srgb, var(--accent-red, #EF4444) 25%, transparent);
		border-radius: var(--radius-md);
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 12px;
		margin-top: 8px;
	}

	.danger-header {
		display: flex;
		align-items: center;
		gap: 7px;
		color: var(--accent-red, #EF4444);
	}

	.danger-title {
		font-size: 13px;
		font-weight: 700;
		color: var(--accent-red, #EF4444);
		margin: 0;
	}

	.danger-desc {
		font-size: 12px;
		color: var(--text-muted);
		margin: 0;
		line-height: 1.5;
	}

	.danger-confirm {
		display: flex;
		flex-direction: column;
		gap: 5px;
	}

	.danger-label {
		font-size: 11px;
		color: var(--text-dim);
	}
	.danger-label strong {
		color: var(--text-secondary);
		font-family: var(--font-mono);
	}

	.danger-input {
		background: var(--bg-base);
		border: 1px solid color-mix(in srgb, var(--accent-red, #EF4444) 30%, transparent);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-mono);
		padding: 7px 10px;
		outline: none;
	}
	.danger-input:focus {
		border-color: var(--accent-red, #EF4444);
	}

	.delete-error {
		font-size: 12px;
		color: var(--accent-red);
		padding: 7px 10px;
		background: color-mix(in srgb, var(--accent-red) 10%, transparent);
		border-radius: var(--radius-sm);
	}

	.btn-delete {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 6px;
		font-size: 12px;
		font-weight: 600;
		color: white;
		background: var(--accent-red, #EF4444);
		border: none;
		border-radius: var(--radius-sm);
		padding: 8px 12px;
		cursor: pointer;
		transition: opacity var(--transition-fast);
	}
	.btn-delete:hover:not(:disabled) {
		opacity: 0.85;
	}
	.btn-delete:disabled {
		opacity: 0.5;
		cursor: default;
	}
</style>
