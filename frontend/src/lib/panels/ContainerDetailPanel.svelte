<script lang="ts">
	import { onMount } from 'svelte';
	import {
		Box, Globe, Terminal, ChevronRight, Loader2,
		RefreshCw, AlertCircle, CheckCircle, XCircle, Clock,
		Play, Square, Trash2, AlertTriangle
	} from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { uiStore } from '$lib/stores/ui.store';
	import type { Container, ContainerInspect } from '$lib/api/types';

	interface Props {
		containerId: string;
		serviceId: string;
		onDeleted?: () => void;
	}

	let { containerId, serviceId, onDeleted }: Props = $props();

	let container = $state<Container | null>(null);
	let inspect = $state<ContainerInspect | null>(null);
	let loadingBase = $state(true);
	let loadingInspect = $state(false);
	let inspectError = $state('');

	let stopping = $state(false);
	let restarting = $state(false);
	let deleting = $state(false);
	let actionError = $state('');
	let deleteConfirm = $state('');

	let isRunning = $derived(container?.status === 'running');
	let isTerminal = $derived(
		['shutdown', 'failed', 'orphan', 'complete', 'rejected'].includes(container?.status ?? '')
	);
	let canDelete = $derived(deleteConfirm.trim() === 'delete');

	function fmtDate(iso: string | null | undefined) {
		if (!iso) return '—';
		return new Date(iso).toLocaleString();
	}

	function relativeTime(iso: string | null | undefined) {
		if (!iso) return '—';
		const d = new Date(iso);
		const diff = Date.now() - d.getTime();
		const s = Math.floor(diff / 1000);
		if (s < 60) return `${s}s ago`;
		const m = Math.floor(s / 60);
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		return `${Math.floor(h / 24)}d ago`;
	}

	onMount(async () => {
		const res = await api.get<Container>(`/services/${serviceId}/containers/${containerId}`);
		if (res.data) container = res.data;
		loadingBase = false;
	});

	async function loadInspect() {
		loadingInspect = true;
		inspectError = '';
		const res = await api.inspectContainer(serviceId, containerId);
		loadingInspect = false;
		if (res.error) {
			inspectError = res.error.message;
		} else if (res.data) {
			inspect = res.data;
		}
	}

	async function handleStop() {
		stopping = true;
		actionError = '';
		const res = await api.stopContainer(serviceId, containerId);
		stopping = false;
		if (res.error) {
			actionError = res.error.message;
		} else if (container) {
			container = { ...container, status: 'shutdown' };
		}
	}

	async function handleRestart() {
		restarting = true;
		actionError = '';
		const res = await api.restartContainer(serviceId, containerId);
		restarting = false;
		if (res.error) {
			actionError = res.error.message;
		} else if (container) {
			container = { ...container, status: 'running' };
		}
	}

	async function handleDelete() {
		if (!canDelete) return;
		deleting = true;
		actionError = '';
		const res = await api.deleteContainer(serviceId, containerId);
		deleting = false;
		if (res.error) {
			actionError = res.error.message;
			return;
		}
		onDeleted?.();
		uiStore.popPanel();
	}

	function statusIcon(status: string) {
		switch (status) {
			case 'running': return CheckCircle;
			case 'failed': return XCircle;
			case 'pending': return Clock;
			default: return AlertCircle;
		}
	}

	function statusClass(status: string) {
		switch (status) {
			case 'running': return 'status-running';
			case 'failed': return 'status-failed';
			case 'pending': return 'status-pending';
			default: return 'status-default';
		}
	}
</script>

<div class="panel-body">
	{#if loadingBase}
		<div class="center-state">
			<Loader2 size={20} class="spin" />
			<span>Loading…</span>
		</div>
	{:else if !container}
		<div class="center-state error">Container not found.</div>
	{:else}
		<!-- Actions -->
		<section class="section">
			<div class="action-bar">
				<button
					class="btn btn-secondary btn-sm action-btn"
					onclick={handleRestart}
					disabled={restarting || stopping}
					title="Restart container"
				>
					{#if restarting}
						<Loader2 size={14} class="spin" />
						Restarting…
					{:else}
						<Play size={14} />
						Restart
					{/if}
				</button>

				<button
					class="btn btn-secondary btn-sm action-btn"
					onclick={handleStop}
					disabled={stopping || restarting || !isRunning}
					title={isRunning ? 'Stop container' : 'Container is not running'}
				>
					{#if stopping}
						<Loader2 size={14} class="spin" />
						Stopping…
					{:else}
						<Square size={14} />
						Stop
					{/if}
				</button>
			</div>
			{#if actionError}
			<p class="action-error">{actionError}</p>
			{/if}
		</section>

		<!-- Overview -->
		<section class="section">
			<h3 class="section-title">Overview</h3>
			<div class="info-grid">
				<div class="info-row">
					<span class="label">Status</span>
					<span class="value {statusClass(container.status)}">
						<svelte:component this={statusIcon(container.status)} size={13} />
						{container.status}
					</span>
				</div>
				<div class="info-row">
					<span class="label">Image</span>
					<span class="value mono">{container.image || '—'}</span>
				</div>
				<div class="info-row">
					<span class="label">Replica</span>
					<span class="value">#{container.replica_index}</span>
				</div>
				<div class="info-row">
					<span class="label">Node</span>
					<span class="value mono">{container.node_id || '—'}</span>
				</div>
				<div class="info-row">
					<span class="label">Started</span>
					<span class="value">{relativeTime(container.started_at)}</span>
				</div>
				{#if container.finished_at}
				<div class="info-row">
					<span class="label">Finished</span>
					<span class="value">{fmtDate(container.finished_at)}</span>
				</div>
				{/if}
				{#if container.exit_code !== null && container.exit_code !== undefined}
				<div class="info-row">
					<span class="label">Exit code</span>
					<span class="value {container.exit_code === 0 ? 'status-running' : 'status-failed'}">
						{container.exit_code}
					</span>
				</div>
				{/if}
				{#if container.status_message}
				<div class="info-row">
					<span class="label">Message</span>
					<span class="value status-message">{container.status_message}</span>
				</div>
				{/if}
				<div class="info-row">
					<span class="label">Docker ID</span>
					<span class="value mono small">{container.docker_container_id || '—'}</span>
				</div>
			</div>
		</section>

		<!-- Port mappings (from inspect) -->
		{#if inspect && inspect.port_bindings.length > 0}
		<section class="section">
			<h3 class="section-title">
				<Globe size={14} />
				Port Mappings
			</h3>
			<div class="port-list">
				{#each inspect.port_bindings as pb}
				<div class="port-row">
					<span class="port-host">{pb.host_ip || '0.0.0.0'}:{pb.host_port}</span>
					<ChevronRight size={12} />
					<span class="port-container">{pb.container_port}/{pb.protocol}</span>
				</div>
				{/each}
			</div>
		</section>
		{/if}

		<!-- Docker Inspect -->
		<section class="section">
			<h3 class="section-title">
				<Terminal size={14} />
				Docker Inspect
			</h3>

			{#if !inspect}
			<button class="btn btn-secondary btn-sm inline-btn" onclick={loadInspect} disabled={loadingInspect}>
				{#if loadingInspect}
					<Loader2 size={14} class="spin" />
					Inspecting…
				{:else}
					<Box size={14} />
					Inspect Container
				{/if}
			</button>
			{/if}

			{#if inspectError}
			<p class="action-error">{inspectError}</p>
			{/if}

			{#if inspect}
			<div class="inspect-meta">
				<div class="info-grid">
					<div class="info-row">
						<span class="label">State</span>
						<span class="value">{inspect.state}</span>
					</div>
					<div class="info-row">
						<span class="label">Platform</span>
						<span class="value">{inspect.platform || '—'}</span>
					</div>
					<div class="info-row">
						<span class="label">Restarts</span>
						<span class="value">{inspect.restart_count}</span>
					</div>
					<div class="info-row">
						<span class="label">Created</span>
						<span class="value">{fmtDate(inspect.created)}</span>
					</div>
				</div>

				{#if inspect.env.length > 0}
				<details class="env-details">
					<summary>Environment ({inspect.env.length} vars)</summary>
					<div class="env-list">
						{#each inspect.env as e}
						<div class="env-row mono small">{e}</div>
						{/each}
					</div>
				</details>
				{/if}

				<button class="btn btn-ghost btn-sm inline-btn muted" onclick={loadInspect} disabled={loadingInspect}>
					<RefreshCw size={12} />
					Refresh
				</button>
			</div>
			{/if}
		</section>

		<!-- Danger Zone -->
		<section class="section danger-zone">
			<h3 class="section-title danger-title">
				<AlertTriangle size={14} />
				Danger Zone
			</h3>
			<p class="danger-desc">
				Delete this container record. Only allowed for stopped/failed containers.
				Type <strong>delete</strong> to confirm.
			</p>
			<div class="danger-row">
				<input
					class="input danger-input"
					type="text"
					placeholder="delete"
					bind:value={deleteConfirm}
					disabled={deleting || !isTerminal}
				/>
				<button
					class="btn btn-danger btn-sm"
					onclick={handleDelete}
					disabled={!canDelete || deleting || !isTerminal}
					title={!isTerminal ? 'Stop the container before deleting its record' : ''}
				>
					{#if deleting}
						<Loader2 size={14} class="spin" />
					{:else}
						<Trash2 size={14} />
					{/if}
					Delete
				</button>
			</div>
			{#if !isTerminal}
			<p class="danger-note">Stop the container first before deleting its record.</p>
			{/if}
		</section>
	{/if}
</div>

<style>
	.panel-body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	.center-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 40px 0;
		color: var(--text-muted);
		font-size: 13px;
	}

	.center-state.error { color: var(--color-error, #e74c3c); }

	.section {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.section-title {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
		margin: 0;
	}

	/* Actions */
	.action-bar {
		display: flex;
		gap: 8px;
	}

	.action-btn {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.action-error {
		font-size: 12px;
		color: var(--color-error, #e74c3c);
		margin: 0;
	}

	/* Info grid */
	.info-grid {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.info-row {
		display: flex;
		align-items: baseline;
		gap: 8px;
		font-size: 13px;
	}

	.label {
		width: 90px;
		flex-shrink: 0;
		color: var(--text-muted);
		font-size: 12px;
	}

	.value {
		color: var(--text-primary);
		display: flex;
		align-items: center;
		gap: 4px;
		flex: 1;
		min-width: 0;
		word-break: break-all;
	}

	.value.mono { font-family: var(--font-mono, monospace); font-size: 12px; }
	.value.small { font-size: 11px; }

	.status-running { color: var(--color-success, #27ae60); }
	.status-failed  { color: var(--color-error, #e74c3c); }
	.status-pending { color: var(--color-warning, #f39c12); }
	.status-default { color: var(--text-muted); }
	.status-message { color: var(--text-secondary); font-size: 12px; }

	/* Ports */
	.port-list {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.port-row {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		font-family: var(--font-mono, monospace);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm, 4px);
		padding: 6px 10px;
	}

	.port-host   { color: var(--accent); }
	.port-container { color: var(--text-primary); }

	/* Inspect */
	.inline-btn {
		align-self: flex-start;
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.inline-btn.muted { color: var(--text-muted); font-size: 12px; }

	.inspect-meta {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.env-details {
		font-size: 12px;
		color: var(--text-muted);
	}

	.env-details summary {
		cursor: pointer;
		padding: 4px 0;
		color: var(--text-secondary);
	}

	.env-list {
		margin-top: 6px;
		display: flex;
		flex-direction: column;
		gap: 3px;
		max-height: 200px;
		overflow-y: auto;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm, 4px);
		padding: 8px;
	}

	.env-row { word-break: break-all; }

	/* Danger zone */
	.danger-zone {
		border: 1px solid color-mix(in srgb, var(--color-error, #e74c3c) 30%, transparent);
		border-radius: var(--radius-md, 6px);
		padding: 14px;
		background: color-mix(in srgb, var(--color-error, #e74c3c) 4%, transparent);
	}

	.danger-title { color: var(--color-error, #e74c3c); }

	.danger-desc {
		font-size: 12px;
		color: var(--text-secondary);
		margin: 0;
		line-height: 1.5;
	}

	.danger-row {
		display: flex;
		gap: 8px;
		align-items: center;
	}

	.danger-input {
		flex: 1;
		min-width: 0;
	}

	.danger-note {
		font-size: 11px;
		color: var(--text-muted);
		margin: 0;
	}

	:global(.spin) {
		animation: spin 0.7s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
