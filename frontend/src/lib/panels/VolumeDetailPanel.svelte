<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, permProject } from '$lib/auth/permissions';
	import { HardDrive, Trash2, AlertTriangle } from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import FormField from '$lib/components/ui/FormField.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import KeyValueList from '$lib/components/ui/KeyValueList.svelte';
	import ListRow from '$lib/components/ui/ListRow.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';

	interface Props {
		volumeId: string;
		projectId: string;
		onDeleted?: () => void;
	}

	let { volumeId, projectId, onDeleted }: Props = $props();

	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let canVolumeWrite = $derived(can($orgStore.myMembership?.role ?? null, $orgStore.myMembership?.permissions ?? [], permProject(orgId, projectId, 'volume', 'write')));

	interface VolumeDetail {
		id: string;
		project_id?: string;
		service_id?: string;
		name: string;
		mount_path: string;
		driver: string;
		size_mb: number;
		created_at: string;
	}

	let volume = $state<VolumeDetail | null>(null);
	let loading = $state(true);
	let loadError = $state('');

	// Danger zone
	let confirmName = $state('');
	let deleting = $state(false);
	let deleteError = $state('');

	let canDelete = $derived(confirmName.trim() === (volume?.name ?? ''));

	function fmtSize(mb: number): string {
		return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
	}

	function fmtDate(iso: string) {
		return new Date(iso).toLocaleString();
	}

	onMount(async () => {
		loading = true;
		const res = await api.get<VolumeDetail>(`/projects/${projectId}/volumes/${volumeId}`);
		if (res.error) {
			loadError = res.error.message;
		} else if (res.data) {
			volume = res.data;
		}
		loading = false;
	});

	async function deleteVolume() {
		if (!canDelete) return;
		deleting = true;
		deleteError = '';
		const res = await api.delete(`/projects/${projectId}/volumes/${volumeId}`);
		deleting = false;
		if (res.error) {
			deleteError = res.error.message;
			return;
		}
		onDeleted?.();
		uiStore.popPanel();
	}
</script>

<div class="panel-wrap">
	{#if loading}
		<div class="center-state">
			<Spinner size={18} />
			<span>Loading…</span>
		</div>
	{:else if loadError}
		<div class="center-state error">{loadError}</div>
	{:else if volume}
		<!-- Header -->
		<Card padding="12px 16px">
			<ListRow title={volume.name} meta={volume.driver} iconTone="yellow">
				{#snippet icon()}<HardDrive size={18} />{/snippet}
			</ListRow>
		</Card>

		<!-- Properties -->
		<section class="detail-section">
			<h3 class="section-title">Properties</h3>
			<KeyValueList
				keyWidth="70px"
				items={[
					{ key: 'Name', value: volume.name, mono: true },
					{ key: 'Driver', value: volume.driver },
					{ key: 'Mount', value: volume.mount_path || '—', mono: true },
					...(volume.size_mb > 0 ? [{ key: 'Size', value: fmtSize(volume.size_mb) }] : []),
					{ key: 'Scope', value: volume.service_id ? 'Service' : 'Project' },
					{ key: 'Created', value: fmtDate(volume.created_at) }
				]}
			/>
		</section>

		<!-- Danger Zone -->
		{#if canVolumeWrite}
		<div class="danger-wrap">
		<Card tone="danger">
			<div class="danger-zone">
				<div class="danger-header">
					<AlertTriangle size={14} />
					<h3 class="danger-title">Danger Zone</h3>
				</div>
				<p class="danger-desc">
					Deleting this volume will remove it from Docker and permanently destroy any data stored on it.
					This action cannot be undone.
				</p>
				<FormField label={`Type ${volume.name} to confirm`} for="confirm-vol-name">
					<TextField
						id="confirm-vol-name"
						type="text"
						placeholder={volume.name}
						bind:value={confirmName}
					/>
				</FormField>
				{#if deleteError}
					<div role="alert"><InlineAlert tone="error">{deleteError}</InlineAlert></div>
				{/if}
				<Button variant="danger-outline" disabled={!canDelete || deleting} onclick={deleteVolume}>
					<Trash2 size={13} />
					{deleting ? 'Deleting…' : 'Delete Volume'}
				</Button>
			</div>
		</Card>
		</div>
		{/if}
	{/if}
</div>

<style>
	.panel-wrap {
		padding: 16px;
		height: 100%;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	.center-state {
		display: flex; align-items: center; gap: 10px;
		color: var(--text-muted); font-size: 13px;
		padding: 40px 0; justify-content: center;
	}
	.center-state.error { color: var(--accent-red); }

	.detail-section { display: flex; flex-direction: column; gap: 10px; }

	.section-title {
		font-size: 11px; font-weight: 600; color: var(--text-dim); margin: 0;
	}

	/* Danger Zone */
	.danger-wrap { margin-top: auto; }
	.danger-zone { display: flex; flex-direction: column; gap: 12px; }
	.danger-header { display: flex; align-items: center; gap: 7px; color: var(--accent-red); }
	.danger-title { font-size: 13px; font-weight: 700; color: var(--accent-red); margin: 0; }
	.danger-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }
</style>
