<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Network } from '$lib/api/types';
	import { SearchInput, ActivityList, ListRow, Button, Spinner, EmptyState, InlineAlert } from '$lib/components/ui';
	import { Network as NetworkIcon, Check } from '@lucide/svelte';

	interface Props {
		projectId: string;
		initialSelected?: string[];
		onConfirm: (ids: string[], items: Network[]) => void;
	}

	let { projectId, initialSelected = [], onConfirm }: Props = $props();

	let networks = $state<Network[]>([]);
	let loading = $state(true);
	let error = $state('');
	let search = $state('');
	let selected = $state<Set<string>>(new Set(initialSelected));

	let filtered = $derived(
		search.trim()
			? networks.filter(n => n.name.toLowerCase().includes(search.toLowerCase()))
			: networks
	);

	onMount(async () => {
		const res = await api.getNetworks(projectId);
		if (res.error) error = res.error.message;
		else if (res.data) networks = res.data;
		loading = false;
	});

	function toggle(id: string) {
		const next = new Set(selected);
		next.has(id) ? next.delete(id) : next.add(id);
		selected = next;
	}

	function confirm() {
		const items = networks.filter(n => selected.has(n.id));
		onConfirm([...selected], items);
		uiStore.popPanel();
	}
</script>

<div class="picker-wrap">
	<div class="search-bar">
		<SearchInput bind:value={search} placeholder="Search networks…" />
	</div>

	{#if loading}
		<div class="state-msg"><Spinner size={16} /> Loading networks…</div>
	{:else if error}
		<div class="state-pad" role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{:else if filtered.length === 0}
		<EmptyState message={search.trim() ? 'No networks match your search.' : 'No networks found in this project.'} />
	{:else}
		<div class="list">
			<ActivityList>
				{#each filtered as net (net.id)}
					{@const isSelected = selected.has(net.id)}
					<ListRow
						onclick={() => toggle(net.id)}
						title={net.name}
						meta={`${net.driver}${net.subnet ? ` · ${net.subnet}` : ''}`}
						iconTone="blue"
					>
						{#snippet icon()}<NetworkIcon size={14} />{/snippet}
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
