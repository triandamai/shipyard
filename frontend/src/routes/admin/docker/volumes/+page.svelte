<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Trash2, RefreshCw, ChevronRight } from '@lucide/svelte';
	import { DataTable, Button, InlineAlert, ConfirmDialog } from '$lib/components/ui';

	interface VolumeSummary {
		name: string; driver: string; mountpoint: string;
		scope: string; labels: Record<string, string>; created_at: string | null;
	}

	let volumes = $state<VolumeSummary[]>([]);
	let loading = $state(false);
	let error = $state('');

	let expanded = $state(new Set<string>());
	function toggleExpand(name: string) {
		if (expanded.has(name)) expanded.delete(name);
		else expanded.add(name);
		expanded = new Set(expanded);
	}

	let pruning = $state(false);
	let pruneMsg = $state('');
	let showPruneConfirm = $state(false);

	async function load() {
		loading = true; error = '';
		const r = await api.get<VolumeSummary[]>('/admin/docker/volumes');
		if (r.data) volumes = r.data;
		else error = r.error?.message ?? 'Failed to load volumes';
		loading = false;
	}

	async function prune() {
		pruning = true; pruneMsg = '';
		const r = await api.post<{ message: string }>('/admin/docker/prune/volumes');
		pruneMsg = r.data?.message ?? r.error?.message ?? 'Done';
		pruning = false;
		setTimeout(() => (pruneMsg = ''), 4000);
		await load();
	}

	onMount(load);
</script>

<div class="ct-toolbar">
	<Button variant="danger-outline" size="sm" disabled={pruning} onclick={() => (showPruneConfirm = true)}>
		<Trash2 size={12} />
		{pruning ? 'Pruning…' : 'Prune Unused'}
	</Button>
	{#if pruneMsg}<span class="ct-prune-msg">{pruneMsg}</span>{/if}
	<div class="ct-toolbar-right">
		<Button variant="secondary" size="icon" onclick={load}>
			<RefreshCw size={13} />
		</Button>
	</div>
</div>

<ConfirmDialog
	bind:open={showPruneConfirm}
	title="Prune unused volumes"
	message="This cannot be undone."
	confirmLabel="Prune"
	onConfirm={prune}
/>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={volumes}
		rowKey={(v) => v.name}
		searchFields={['name']}
		columns={[
			{ key: 'name', label: 'Name', width: '25%' },
			{ key: 'driver', label: 'Driver', width: '15%' },
			{ key: 'scope', label: 'Scope', width: '15%' },
			{ key: 'mountpoint', label: 'Mountpoint', width: '45%' }
		]}
		emptyMessage="No volumes found."
	>
		{#snippet row(v)}
			<!-- svelte-ignore a11y_click_events_have_key_events -->
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<tr class="ct-row" onclick={() => toggleExpand(v.name)}>
				<td class="ct-mono ct-trunc">
					<span class="ct-expand-cell">
						<ChevronRight size={12} class={expanded.has(v.name) ? 'ct-chevron ct-chevron-expanded' : 'ct-chevron'} />
						{v.name}
					</span>
				</td>
				<td>{v.driver}</td>
				<td>{v.scope}</td>
				<td class="ct-mono ct-trunc">{v.mountpoint}</td>
			</tr>
			{#if expanded.has(v.name)}
				<tr class="ct-detail-row">
					<td colspan="4">
						<div class="ct-details-grid">
							<div style="grid-column: 1 / -1"><strong>Mountpoint:</strong> <span class="ct-mono">{v.mountpoint}</span></div>
							<div><strong>Driver:</strong> {v.driver}</div>
							<div><strong>Scope:</strong> {v.scope}</div>
							{#if v.created_at}
								<div><strong>Created At:</strong> {new Date(v.created_at).toLocaleString()}</div>
							{/if}
							{#if Object.keys(v.labels).length > 0}
								<div style="grid-column: 1 / -1">
									<strong>Labels:</strong>
									<div class="ct-labels-box ct-mono">
										{#each Object.entries(v.labels) as [k, val]}
											<div><span class="ct-lbl-key">{k}:</span> {val}</div>
										{/each}
									</div>
								</div>
							{/if}
						</div>
					</td>
				</tr>
			{/if}
		{/snippet}
	</DataTable>
{/if}

<style>
	.ct-toolbar { display: flex; align-items: center; gap: 12px; margin-bottom: 14px; }
	.ct-toolbar-right { margin-left: auto; }
	.ct-prune-msg { font-size: 11.5px; color: var(--accent-green); font-weight: 500; }
	.ct-mono { font-family: var(--font-mono); }
	.ct-trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; max-width: 0; }

	.ct-row { cursor: pointer; }
	.ct-expand-cell { display: flex; align-items: center; gap: 6px; min-width: 0; }
	:global(.ct-chevron) { color: var(--text-dim); transition: transform 0.2s; flex-shrink: 0; }
	:global(.ct-chevron-expanded) { transform: rotate(90deg); }
	.ct-detail-row td { background: var(--bg-elevated); border-top: 1px solid var(--border); padding: 14px 20px 16px; font-size: 12.5px; }
	.ct-details-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
	.ct-labels-box { margin-top: 6px; padding: 8px 12px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 11px; max-height: 120px; overflow-y: auto; }
	.ct-lbl-key { color: var(--accent); font-weight: 500; }
</style>
