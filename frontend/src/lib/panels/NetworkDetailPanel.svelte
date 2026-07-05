<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, permProject } from '$lib/auth/permissions';
	import { Network, Trash2, AlertTriangle, Copy, Check } from '@lucide/svelte';

	interface Props {
		networkId: string;
		projectId: string;
		onDeleted?: () => void;
	}

	let { networkId, projectId, onDeleted }: Props = $props();

	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let canNetworkWrite = $derived(can($orgStore.myMembership?.role ?? null, $orgStore.myMembership?.permissions ?? [], permProject(orgId, projectId, 'network', 'write')));

	interface NetworkDetail {
		id: string;
		project_id: string;
		name: string;
		driver: string;
		subnet: string;
		docker_network_id?: string;
		created_at: string;
	}

	let network = $state<NetworkDetail | null>(null);
	let loading = $state(true);
	let loadError = $state('');

	// Danger zone state
	let confirmName = $state('');
	let deleting = $state(false);
	let deleteError = $state('');

	let canDelete = $derived(confirmName.trim() === (network?.name ?? ''));

	// Copy-to-clipboard state
	let copied = $state('');
	function copyText(text: string, key: string) {
		navigator.clipboard.writeText(text);
		copied = key;
		setTimeout(() => (copied = ''), 2000);
	}

	onMount(async () => {
		loading = true;
		const res = await api.get<NetworkDetail>(`/projects/${projectId}/networks/${networkId}`);
		if (res.error) {
			loadError = res.error.message;
		} else if (res.data) {
			network = res.data;
		}
		loading = false;
	});

	async function deleteNetwork() {
		if (!canDelete) return;
		deleting = true;
		deleteError = '';
		const res = await api.delete(`/projects/${projectId}/networks/${networkId}`);
		deleting = false;
		if (res.error) {
			deleteError = res.error.message;
			return;
		}
		onDeleted?.();
		uiStore.popPanel();
	}

	function fmtDate(iso: string) {
		return new Date(iso).toLocaleString();
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
	{:else if network}
		<!-- Header -->
		<div class="detail-header">
			<div class="detail-icon">
				<Network size={18} />
			</div>
			<div class="detail-title-block">
				<h2 class="detail-name">{network.name}</h2>
				<span class="detail-badge">{network.driver}</span>
			</div>
		</div>

		<!-- Properties -->
		<section class="detail-section">
			<h3 class="section-title">Properties</h3>
			<div class="prop-list">
				<div class="prop-row">
					<span class="prop-key">Name</span>
					<span class="prop-value mono">{network.name}</span>
				</div>
				<div class="prop-row">
					<span class="prop-key">Driver</span>
					<span class="prop-value">{network.driver}</span>
				</div>
				{#if network.subnet}
					<div class="prop-row">
						<span class="prop-key">Subnet</span>
						<span class="prop-value mono">{network.subnet}</span>
					</div>
				{/if}
				{#if network.docker_network_id}
					<div class="prop-row">
						<span class="prop-key">Docker ID</span>
						<div class="prop-value-copy">
							<span class="prop-value mono truncate">{network.docker_network_id.slice(0, 12)}</span>
							<button
								type="button"
								class="copy-btn"
								onclick={() => copyText(network!.docker_network_id!, 'dockerId')}
							>
								{#if copied === 'dockerId'}
									<Check size={11} />
								{:else}
									<Copy size={11} />
								{/if}
							</button>
						</div>
					</div>
				{/if}
				<div class="prop-row">
					<span class="prop-key">Created</span>
					<span class="prop-value">{fmtDate(network.created_at)}</span>
				</div>
			</div>
		</section>

		<!-- Danger Zone -->
		{#if canNetworkWrite}
		<section class="danger-zone">
			<div class="danger-header">
				<AlertTriangle size={14} />
				<h3 class="danger-title">Danger Zone</h3>
			</div>
			<p class="danger-desc">
				Deleting this network will remove it from Docker and disconnect any attached services.
				This action cannot be undone.
			</p>
			<div class="danger-confirm">
				<label class="danger-label" for="confirm-net-name">
					Type <strong>{network.name}</strong> to confirm
				</label>
				<input
					id="confirm-net-name"
					class="danger-input"
					type="text"
					placeholder={network.name}
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
				onclick={deleteNetwork}
			>
				<Trash2 size={13} />
				{deleting ? 'Deleting…' : 'Delete Network'}
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
		display: flex;
		align-items: center;
		gap: 10px;
		color: var(--text-muted);
		font-size: 13px;
		padding: 40px 0;
		justify-content: center;
	}
	.center-state.error { color: var(--accent-red); }

	.spinner {
		width: 18px; height: 18px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
		flex-shrink: 0;
	}

	/* Header */
	.detail-header {
		display: flex;
		align-items: center;
		gap: 14px;
		padding-bottom: 16px;
		border-bottom: 1px solid var(--border);
	}

	.detail-icon {
		width: 42px; height: 42px;
		border-radius: var(--radius-md);
		background: var(--accent-blue-muted);
		color: var(--accent-blue);
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0;
	}

	.detail-title-block { display: flex; flex-direction: column; gap: 4px; min-width: 0; }

	.detail-name {
		font-size: 17px; font-weight: 700;
		color: var(--text-primary); margin: 0;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	.detail-badge {
		display: inline-block;
		font-size: 10px; font-weight: 600; font-family: var(--font-mono);
		padding: 2px 7px; border-radius: 99px;
		background: var(--accent-blue-muted); color: var(--accent-blue);
		text-transform: uppercase; letter-spacing: 0.04em;
	}

	/* Properties section */
	.detail-section {
		display: flex; flex-direction: column; gap: 10px;
	}

	.section-title {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em; margin: 0;
	}

	.prop-list {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.prop-row {
		display: flex; align-items: center;
		padding: 9px 14px;
		border-bottom: 1px solid var(--border);
		gap: 12px;
	}
	.prop-row:last-child { border-bottom: none; }

	.prop-key {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		width: 90px; flex-shrink: 0; text-transform: uppercase; letter-spacing: 0.05em;
	}

	.prop-value {
		font-size: 12px; color: var(--text-secondary); flex: 1; min-width: 0;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}
	.prop-value.mono { font-family: var(--font-mono); }

	.prop-value-copy {
		display: flex; align-items: center; gap: 6px; flex: 1; min-width: 0;
	}

	.truncate {
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	.copy-btn {
		background: none; border: none; padding: 2px; cursor: pointer;
		color: var(--text-dim); display: flex; align-items: center;
		border-radius: 3px; flex-shrink: 0;
	}
	.copy-btn:hover { color: var(--accent); }

	/* Danger Zone */
	.danger-zone {
		background: color-mix(in srgb, var(--accent-red, #EF4444) 4%, var(--bg-elevated));
		border: 1px solid color-mix(in srgb, var(--accent-red, #EF4444) 25%, transparent);
		border-radius: var(--radius-md);
		padding: 16px;
		display: flex; flex-direction: column; gap: 12px;
		margin-top: auto;
	}

	.danger-header {
		display: flex; align-items: center; gap: 7px;
		color: var(--accent-red, #EF4444);
	}

	.danger-title {
		font-size: 13px; font-weight: 700;
		color: var(--accent-red, #EF4444); margin: 0;
	}

	.danger-desc {
		font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5;
	}

	.danger-confirm { display: flex; flex-direction: column; gap: 5px; }

	.danger-label {
		font-size: 11px; color: var(--text-dim);
	}
	.danger-label strong { color: var(--text-secondary); font-family: var(--font-mono); }

	.danger-input {
		background: var(--bg-base);
		border: 1px solid color-mix(in srgb, var(--accent-red, #EF4444) 30%, transparent);
		border-radius: var(--radius-sm);
		color: var(--text-primary); font-size: 13px; font-family: var(--font-mono);
		padding: 7px 10px; outline: none;
	}
	.danger-input:focus {
		border-color: var(--accent-red, #EF4444);
	}

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
