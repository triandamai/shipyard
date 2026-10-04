<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Volume } from '$lib/api/types';
	import { SearchInput, ActivityList, ListRow, Button, Spinner, EmptyState, InlineAlert } from '$lib/components/ui';
	import { HardDrive, Check } from '@lucide/svelte';

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
		<SearchInput bind:value={search} placeholder="Search volumes…" />
	</div>

	{#if loading}
		<div class="state-msg"><Spinner size={16} /> Loading volumes…</div>
	{:else if error}
		<div class="state-pad" role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{:else if filtered.length === 0}
		<EmptyState message={search.trim() ? 'No volumes match your search.' : 'No standalone volumes found in this project.'} />
	{:else}
		<div class="list">
			<ActivityList>
				{#each filtered as vol (vol.id)}
					{@const isSelected = selected.has(vol.id)}
					<ListRow
						onclick={() => toggle(vol.id)}
						title={vol.name}
						meta={`${vol.mount_path || '—'}${vol.size_mb > 0 ? ` · ${fmtSize(vol.size_mb)}` : ''}`}
						iconTone="yellow"
					>
						{#snippet icon()}<HardDrive size={14} />{/snippet}
						{#snippet trailing()}
							{#if isSelected}<span class="sel-check"><Check size={14} /><span class="sr-only">Selected</span></span>{/if}
						{/snippet}
					</ListRow>
				{/each}
			</ActivityList>
		</div>
	{/if}

	<div class="footer">
		<span class="footer-hint">{selected.size} selected</span>
		<Button variant="primary" onclick={confirm}>Confirm Selection</Button>
	</div>
</div>

<style>
	.picker-wrap { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
	.search-bar { padding: 12px 16px; border-bottom: 1px solid var(--border); flex-shrink: 0; }
	.state-msg {
		display: flex; align-items: center; gap: 10px;
		padding: 32px 16px; font-size: 13px; color: var(--text-muted); justify-content: center;
	}
	.state-pad { padding: 16px; }
	.list { flex: 1; overflow-y: auto; padding: 0 16px 16px; }
	.sel-check { color: var(--accent); display: flex; align-items: center; }
	.sr-only { position: absolute; width: 1px; height: 1px; overflow: hidden; clip: rect(0 0 0 0); white-space: nowrap; }
	.footer {
		display: flex; align-items: center; justify-content: space-between;
		padding: 12px 16px; border-top: 1px solid var(--border); flex-shrink: 0;
		background: var(--bg-surface);
	}
	.footer-hint { font-size: 12px; color: var(--text-dim); }
</style>
