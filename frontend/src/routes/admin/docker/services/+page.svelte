<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { RefreshCw, ChevronRight } from '@lucide/svelte';
	import { DataTable, Badge, Button, InlineAlert } from '$lib/components/ui';

	interface ServiceSummary {
		id: string; name: string; image: string;
		replicas_running: number; replicas_desired: number;
		mode: string; ports: string[]; labels: Record<string, string>;
		created_at: string | null; updated_at: string | null;
	}

	let services = $state<ServiceSummary[]>([]);
	let loading = $state(false);
	let error = $state('');

	let expanded = $state(new Set<string>());
	function toggleExpand(id: string) {
		if (expanded.has(id)) expanded.delete(id);
		else expanded.add(id);
		expanded = new Set(expanded);
	}

	async function load() {
		loading = true; error = '';
		const r = await api.get<ServiceSummary[]>('/admin/docker/services');
		if (r.data) services = r.data;
		else error = r.error?.message ?? 'Failed to load services';
		loading = false;
	}

	onMount(load);
</script>

<div class="sv-toolbar">
	<div class="sv-toolbar-right">
		<Button variant="secondary" size="icon" onclick={load}>
			<RefreshCw size={13} />
		</Button>
	</div>
</div>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={services}
		rowKey={(s) => s.id}
		searchFields={['name', 'image']}
		columns={[
			{ key: 'name', label: 'Name', width: '30%' },
			{ key: 'image', label: 'Image', width: '36%' },
			{ key: 'mode', label: 'Mode', width: '14%' },
			{ key: 'replicas', label: 'Replicas', width: '20%' }
		]}
		emptyMessage="No services found."
	>
		{#snippet row(s)}
			<!-- svelte-ignore a11y_click_events_have_key_events -->
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<tr class="sv-row" onclick={() => toggleExpand(s.id)}>
				<td class="sv-mono sv-trunc">
					<span class="sv-expand-cell">
						<ChevronRight size={12} class={expanded.has(s.id) ? 'sv-chevron sv-chevron-expanded' : 'sv-chevron'} />
						{s.name}
					</span>
				</td>
				<td class="sv-mono sv-trunc">{s.image}</td>
				<td>{s.mode}</td>
				<td>
					<Badge tone={s.replicas_running === s.replicas_desired ? 'green' : 'red'}>
						{s.replicas_running}/{s.replicas_desired}
					</Badge>
				</td>
			</tr>
			{#if expanded.has(s.id)}
				<tr class="sv-detail-row">
					<td colspan="4">
						<div class="sv-details-grid">
							<div><strong>Service ID:</strong> <span class="sv-mono">{s.id}</span></div>
							<div><strong>Mode:</strong> {s.mode}</div>
							<div style="grid-column: 1 / -1"><strong>Image:</strong> <span class="sv-mono">{s.image}</span></div>
							{#if s.ports && s.ports.length > 0}
								<div style="grid-column: 1 / -1"><strong>Published Ports:</strong> <span class="sv-mono">{s.ports.join(', ')}</span></div>
							{/if}
							{#if s.created_at}
								<div><strong>Created At:</strong> {new Date(s.created_at).toLocaleString()}</div>
							{/if}
							{#if s.updated_at}
								<div><strong>Updated At:</strong> {new Date(s.updated_at).toLocaleString()}</div>
							{/if}
							{#if Object.keys(s.labels).length > 0}
								<div style="grid-column: 1 / -1">
									<strong>Labels:</strong>
									<div class="sv-labels-box sv-mono">
										{#each Object.entries(s.labels) as [k, val]}
											<div><span class="sv-lbl-key">{k}:</span> {val}</div>
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
	.sv-toolbar { display: flex; align-items: center; gap: 12px; margin-bottom: 14px; }
	.sv-toolbar-right { margin-left: auto; }
	.sv-mono { font-family: var(--font-mono); }
	.sv-trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; max-width: 0; }

	.sv-row { cursor: pointer; }
	.sv-expand-cell { display: flex; align-items: center; gap: 6px; min-width: 0; }
	:global(.sv-chevron) { color: var(--text-dim); transition: transform 0.2s; flex-shrink: 0; }
	:global(.sv-chevron-expanded) { transform: rotate(90deg); }
	.sv-detail-row td { background: var(--bg-elevated); border-top: 1px solid var(--border); padding: 14px 20px 16px; font-size: 12.5px; }
	.sv-details-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
	.sv-labels-box { margin-top: 6px; padding: 8px 12px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 11px; max-height: 120px; overflow-y: auto; }
	.sv-lbl-key { color: var(--accent); font-weight: 500; }
</style>
