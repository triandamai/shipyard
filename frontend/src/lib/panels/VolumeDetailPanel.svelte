<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can } from '$lib/auth/permissions';
	import { HardDrive, Trash2, AlertTriangle } from '@lucide/svelte';

	interface Props {
		volumeId: string;
		projectId: string;
		onDeleted?: () => void;
	}

	let { volumeId, projectId, onDeleted }: Props = $props();

	let canVolumeWrite = $derived(can($orgStore.myMembership?.role ?? null, $orgStore.myMembership?.permissions ?? [], 'volume:write'));

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
			<div class="spinner"></div>
			<span>Loading…</span>
		</div>
	{:else if loadError}
		<div class="center-state error">{loadError}</div>
	{:else if volume}
		<!-- Header -->
		<div class="detail-header">
			<div class="detail-icon">
				<HardDrive size={18} />
			</div>
			<div class="detail-title-block">
				<h2 class="detail-name">{volume.name}</h2>
				<span class="detail-badge">{volume.driver}</span>
			</div>
		</div>

		<!-- Properties -->
		<section class="detail-section">
			<h3 class="section-title">Properties</h3>
			<div class="prop-list">
				<div class="prop-row">
					<span class="prop-key">Name</span>
					<span class="prop-value mono">{volume.name}</span>
				</div>
				<div class="prop-row">
					<span class="prop-key">Driver</span>
					<span class="prop-value">{volume.driver}</span>
				</div>
				<div class="prop-row">
					<span class="prop-key">Mount</span>
					<span class="prop-value mono">{volume.mount_path || '—'}</span>
				</div>
				{#if volume.size_mb > 0}
					<div class="prop-row">
						<span class="prop-key">Size</span>
						<span class="prop-value">{fmtSize(volume.size_mb)}</span>
					</div>
				{/if}
				<div class="prop-row">
					<span class="prop-key">Scope</span>
					<span class="prop-value">{volume.service_id ? 'Service' : 'Project'}</span>
				</div>
				<div class="prop-row">
					<span class="prop-key">Created</span>
					<span class="prop-value">{fmtDate(volume.created_at)}</span>
				</div>
			</div>
		</section>

		<!-- Danger Zone -->
		{#if canVolumeWrite}
		<section class="danger-zone">
			<div class="danger-header">
				<AlertTriangle size={14} />
				<h3 class="danger-title">Danger Zone</h3>
			</div>
			<p class="danger-desc">
				Deleting this volume will remove it from Docker and permanently destroy any data stored on it.
				This action cannot be undone.
			</p>
			<div class="danger-confirm">
				<label class="danger-label" for="confirm-vol-name">
					Type <strong>{volume.name}</strong> to confirm
				</label>
				<input
					id="confirm-vol-name"
					class="danger-input"
					type="text"
					placeholder={volume.name}
					bind:value={confirmName}
				/>
			</div>
			{#if deleteError}
				<div class="delete-error">{deleteError}</div>
			{/if}
			<button
				class="btn-delete"
				type="button"
				disabled={!canDelete || deleting}
				onclick={deleteVolume}
			>
				<Trash2 size={13} />
				{deleting ? 'Deleting…' : 'Delete Volume'}
			</button>
		</section>
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

	.spinner {
		width: 18px; height: 18px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite; flex-shrink: 0;
	}

	/* Header */
	.detail-header {
		display: flex; align-items: center; gap: 14px;
		padding-bottom: 16px; border-bottom: 1px solid var(--border);
	}

	.detail-icon {
		width: 42px; height: 42px; border-radius: var(--radius-md);
		background: var(--accent-yellow-muted); color: var(--accent-yellow);
		display: flex; align-items: center; justify-content: center; flex-shrink: 0;
	}

	.detail-title-block { display: flex; flex-direction: column; gap: 4px; min-width: 0; }

	.detail-name {
		font-size: 17px; font-weight: 700; color: var(--text-primary); margin: 0;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	.detail-badge {
		display: inline-block;
		font-size: 10px; font-weight: 600; font-family: var(--font-mono);
		padding: 2px 7px; border-radius: 99px;
		background: var(--accent-yellow-muted); color: var(--accent-yellow);
		text-transform: uppercase; letter-spacing: 0.04em;
	}

	/* Properties */
	.detail-section { display: flex; flex-direction: column; gap: 10px; }

	.section-title {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em; margin: 0;
	}

	.prop-list {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-md); overflow: hidden;
	}

	.prop-row {
		display: flex; align-items: center;
		padding: 9px 14px; border-bottom: 1px solid var(--border); gap: 12px;
	}
	.prop-row:last-child { border-bottom: none; }

	.prop-key {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		width: 70px; flex-shrink: 0; text-transform: uppercase; letter-spacing: 0.05em;
	}

	.prop-value {
		font-size: 12px; color: var(--text-secondary); flex: 1; min-width: 0;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}
	.prop-value.mono { font-family: var(--font-mono); }

	/* Danger Zone */
	.danger-zone {
		background: color-mix(in srgb, var(--accent-red, #EF4444) 4%, var(--bg-elevated));
		border: 1px solid color-mix(in srgb, var(--accent-red, #EF4444) 25%, transparent);
		border-radius: var(--radius-md); padding: 16px;
		display: flex; flex-direction: column; gap: 12px;
		margin-top: auto;
	}

	.danger-header { display: flex; align-items: center; gap: 7px; color: var(--accent-red, #EF4444); }

	.danger-title {
		font-size: 13px; font-weight: 700; color: var(--accent-red, #EF4444); margin: 0;
	}

	.danger-desc {
		font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5;
	}

	.danger-confirm { display: flex; flex-direction: column; gap: 5px; }

	.danger-label { font-size: 11px; color: var(--text-dim); }
	.danger-label strong { color: var(--text-secondary); font-family: var(--font-mono); }

	.danger-input {
		background: var(--bg-base);
		border: 1px solid color-mix(in srgb, var(--accent-red, #EF4444) 30%, transparent);
		border-radius: var(--radius-sm);
		color: var(--text-primary); font-size: 13px; font-family: var(--font-mono);
		padding: 7px 10px; outline: none;
	}
	.danger-input:focus { border-color: var(--accent-red, #EF4444); }

	.delete-error {
		font-size: 12px; color: var(--accent-red);
		padding: 7px 10px;
		background: color-mix(in srgb, var(--accent-red) 10%, transparent);
		border-radius: var(--radius-sm);
	}

	.btn-delete {
		display: flex; align-items: center; gap: 6px; justify-content: center;
		padding: 8px 16px; border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--accent-red, #EF4444) 12%, transparent);
		color: var(--accent-red, #EF4444);
		border: 1px solid color-mix(in srgb, var(--accent-red, #EF4444) 35%, transparent);
		font-size: 13px; font-weight: 600; cursor: pointer;
		transition: all var(--transition-fast);
	}
	.btn-delete:hover:not(:disabled) {
		background: color-mix(in srgb, var(--accent-red, #EF4444) 20%, transparent);
	}
	.btn-delete:disabled { opacity: 0.4; cursor: default; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
