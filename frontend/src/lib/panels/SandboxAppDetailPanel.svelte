<script lang="ts">
	import { onMount } from 'svelte';
	import { Play, Square, ExternalLink, Code2, Trash2, AlertTriangle } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { goto } from '$app/navigation';
	import { uiStore } from '$lib/stores/ui.store';
	import { orgStore } from '$lib/stores/org.store';
	import { can, permProject } from '$lib/auth/permissions';
	import type { SandboxInstance, Service } from '$lib/api/types';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import FormField from '$lib/components/ui/FormField.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import KeyValueList from '$lib/components/ui/KeyValueList.svelte';
	import StatusDot from '$lib/components/ui/StatusDot.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';

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
		<KeyValueList
			keyWidth="70px"
			items={[
				{ key: 'Status', value: instance.status },
				...(instance.preview_url ? [{ key: 'Preview', value: instance.preview_url, mono: true }] : [])
			]}
		>
			{#snippet value(item)}
				{#if item.key === 'Status'}
					<span class="status-row">
						<!-- Old dot colored only "running"; every other status was neutral grey. -->
						<StatusDot status={instance!.status === 'running' ? 'running' : 'stopped'} />
						<span class="status-text">{item.value}</span>
					</span>
				{:else}
					<a class="preview-link" href={instance!.preview_url} target="_blank" rel="noopener noreferrer">
						<span class="preview-text">{item.value}</span> <ExternalLink size={12} />
					</a>
				{/if}
			{/snippet}
		</KeyValueList>

		<div class="actions">
			{#if instance.status === 'running'}
				<Button variant="secondary" size="sm" onclick={stop} disabled={isStopping}>
					<Square size={13} /> Stop
				</Button>
			{:else}
				<Button size="sm" onclick={start} disabled={isStarting}>
					<Play size={13} /> Start
				</Button>
			{/if}
			<Button variant="secondary" size="sm" onclick={openEditor}>
				<Code2 size={13} /> Open Editor
			</Button>
		</div>

		{#if canDeleteApp && service}
			<div class="danger-wrap">
			<Card tone="danger">
				<div class="danger-zone">
					<div class="danger-header">
						<AlertTriangle size={14} />
						<h3 class="danger-title">Danger Zone</h3>
					</div>
					<p class="danger-desc">
						Deleting this app removes its container, its dedicated volume, and its preview domain
						(including its DNS record, if one was synced). <strong>This cannot be undone.</strong>
					</p>
					<FormField label={`Type ${service.slug} to confirm`} for="confirm-sandbox-slug">
						<TextField
							id="confirm-sandbox-slug"
							type="text"
							placeholder={service.slug}
							bind:value={confirmSlug}
							autocomplete="off"
							spellcheck={false}
						/>
					</FormField>
					{#if deleteError}
						<div role="alert"><InlineAlert tone="error">{deleteError}</InlineAlert></div>
					{/if}
					<Button variant="danger" disabled={!canDelete || isDeleting} onclick={deleteApp}>
						<Trash2 size={13} />
						{isDeleting ? 'Deleting…' : 'Delete App'}
					</Button>
				</div>
			</Card>
			</div>
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
	.status-row { display: flex; align-items: center; gap: 6px; }
	.status-text { text-transform: capitalize; }
	.preview-link {
		display: flex;
		align-items: center;
		gap: 4px;
		min-width: 0;
		color: var(--accent);
	}
	.preview-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.actions { display: flex; gap: 8px; flex-wrap: wrap; }
	.muted { color: var(--text-dim); font-size: 12px; }
	.error { color: var(--accent-red); font-size: 12px; }

	/* Danger Zone */
	.danger-wrap { margin-top: 8px; }
	.danger-zone { display: flex; flex-direction: column; gap: 12px; }
	.danger-header { display: flex; align-items: center; gap: 7px; color: var(--accent-red); }
	.danger-title { font-size: 13px; font-weight: 700; color: var(--accent-red); margin: 0; }
	.danger-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }
</style>
