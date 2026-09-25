<script lang="ts">
	import { onMount } from 'svelte';
	import { Play, Square, ExternalLink, Code2 } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { goto } from '$app/navigation';
	import type { SandboxInstance } from '$lib/api/types';

	interface Props {
		serviceId: string;
		orgSlug: string;
		projectSlug: string;
		onDeleted?: () => void;
	}

	let { serviceId, orgSlug, projectSlug }: Props = $props();

	let instance = $state<SandboxInstance | null>(null);
	let isLoading = $state(true);
	let error = $state<string | null>(null);
	let isStarting = $state(false);
	let isStopping = $state(false);

	async function load() {
		isLoading = true;
		const res = await api.getSandboxInstance(serviceId);
		if (res.data) instance = res.data;
		else error = res.error?.message ?? 'Failed to load sandbox status';
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
</style>
