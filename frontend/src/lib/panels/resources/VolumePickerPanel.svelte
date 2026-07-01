<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Volume } from '$lib/api/types';
	import { Search, HardDrive, Check } from '@lucide/svelte';

	interface Props {
		projectId: string;
		initialSelected?: string[];
		onConfirm: (ids: string[], items: Volume[]) => void;
	}

	let { projectId, initialSelected = [], onConfirm }: Props = $props();

	let volumes = $state<Volume[]>([]);
	let loading = $state(true);
	let error = $state('');
	let search = $state('');
	let selected = $state<Set<string>>(new Set(initialSelected));

	let filtered = $derived(
		search.trim()
			? volumes.filter(v => v.name.toLowerCase().includes(search.toLowerCase()))
			: volumes
	);

	function fmtSize(mb: number) {
		return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
	}

	onMount(async () => {
		const res = await api.getProjectVolumes(projectId);
		if (res.error) error = res.error.message;
		else if (res.data) volumes = res.data;
		loading = false;
	});

	function toggle(id: string) {
		const next = new Set(selected);
		next.has(id) ? next.delete(id) : next.add(id);
		selected = next;
	}

	function confirm() {
		const items = volumes.filter(v => selected.has(v.id));
		onConfirm([...selected], items);
		uiStore.popPanel();
	}
</script>

<div class="picker-wrap">
	<div class="search-bar">
		<Search size={14} class="search-icon" />
		<input class="search-input" type="text" placeholder="Search volumes…" bind:value={search} />
	</div>

	{#if loading}
		<div class="state-msg"><div class="spinner"></div> Loading volumes…</div>
	{:else if error}
		<div class="state-msg error">{error}</div>
	{:else if filtered.length === 0}
		<div class="state-msg">
			{search.trim() ? 'No volumes match your search.' : 'No standalone volumes found in this project.'}
		</div>
	{:else}
		<div class="list">
			{#each filtered as vol (vol.id)}
				{@const isSelected = selected.has(vol.id)}
				<button type="button" class="list-row" class:sel={isSelected} onclick={() => toggle(vol.id)}>
					<div class="row-icon"><HardDrive size={14} /></div>
					<div class="row-info">
						<span class="row-name">{vol.name}</span>
						<span class="row-sub">
							{vol.mount_path || '—'}
							{#if vol.size_mb > 0} · {fmtSize(vol.size_mb)}{/if}
						</span>
					</div>
					{#if isSelected}
						<div class="check"><Check size={13} /></div>
					{/if}
				</button>
			{/each}
		</div>
	{/if}

	<div class="footer">
		<span class="footer-hint">{selected.size} selected</span>
		<button type="button" class="btn btn-primary confirm-btn" onclick={confirm}>
			Confirm Selection
		</button>
	</div>
</div>

<style>
	.picker-wrap { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

	.search-bar {
		display: flex; align-items: center; gap: 8px;
		padding: 12px 16px; border-bottom: 1px solid var(--border); flex-shrink: 0;
	}
	:global(.search-icon) { color: var(--text-dim); flex-shrink: 0; }
	.search-input {
		flex: 1; background: transparent; border: none; outline: none;
		color: var(--text-primary); font-size: 13px; font-family: var(--font-sans);
	}
	.search-input::placeholder { color: var(--text-dim); }

	.state-msg {
		display: flex; align-items: center; gap: 10px;
		padding: 32px 16px; font-size: 13px; color: var(--text-muted); justify-content: center;
	}
	.state-msg.error { color: var(--accent-red); }

	.spinner {
		width: 16px; height: 16px; border: 2px solid var(--border);
		border-top-color: var(--accent); border-radius: 50%;
		animation: spin 0.7s linear infinite; flex-shrink: 0;
	}

	.list { flex: 1; overflow-y: auto; }

	.list-row {
		display: flex; align-items: center; gap: 10px;
		padding: 11px 16px; width: 100%; background: transparent; border: none;
		border-bottom: 1px solid var(--border); cursor: pointer;
		text-align: left; transition: background var(--transition-fast);
	}
	.list-row:hover { background: var(--bg-elevated); }
	.list-row.sel { background: color-mix(in srgb, var(--accent) 7%, transparent); }
	.list-row:last-child { border-bottom: none; }

	.row-icon { color: var(--accent-yellow); flex-shrink: 0; display: flex; align-items: center; }

	.row-info { display: flex; flex-direction: column; gap: 1px; flex: 1; min-width: 0; }
	.row-name { font-size: 13px; font-weight: 600; color: var(--text-primary); font-family: var(--font-mono); }
	.row-sub  { font-size: 11px; color: var(--text-dim); }

	.check { color: var(--accent); display: flex; align-items: center; flex-shrink: 0; }

	.footer {
		display: flex; align-items: center; justify-content: space-between;
		padding: 12px 16px; border-top: 1px solid var(--border); flex-shrink: 0;
		background: var(--bg-surface);
	}
	.footer-hint { font-size: 12px; color: var(--text-dim); }
	.confirm-btn { padding: 7px 18px; font-size: 13px; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
