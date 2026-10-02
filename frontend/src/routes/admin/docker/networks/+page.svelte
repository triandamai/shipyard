<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { RefreshCw } from '@lucide/svelte';
	import { DataTable, Button, InlineAlert } from '$lib/components/ui';

	interface NetworkSummary {
		id: string; name: string; driver: string; scope: string;
		internal: boolean; attachable: boolean; ipam_subnet: string | null;
		labels: Record<string, string>; containers: number;
	}

	let networks = $state<NetworkSummary[]>([]);
	let loading = $state(false);
	let error = $state('');

	async function load() {
		loading = true; error = '';
		const r = await api.get<NetworkSummary[]>('/admin/docker/networks');
		if (r.data) networks = r.data;
		else error = r.error?.message ?? 'Failed to load networks';
		loading = false;
	}

	onMount(load);
</script>

<div class="ct-toolbar">
	<div class="ct-toolbar-right">
		<Button variant="secondary" size="icon" onclick={load}>
			<RefreshCw size={13} />
		</Button>
	</div>
</div>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={networks}
		rowKey={(n) => n.id}
		searchFields={['name', 'driver']}
		columns={[
			{ key: 'name', label: 'Name', width: '30%' },
			{ key: 'driver', label: 'Driver', width: '18%' },
			{ key: 'scope', label: 'Scope', width: '18%' },
			{ key: 'subnet', label: 'Subnet', width: '22%' },
			{ key: 'containers', label: 'Containers', width: '12%' }
		]}
		emptyMessage="No networks found."
	>
		{#snippet row(n)}
			<tr>
				<td class="ct-mono ct-trunc">{n.name}</td>
				<td>{n.driver}</td>
				<td>{n.scope}</td>
				<td class="ct-mono">{n.ipam_subnet ?? '—'}</td>
				<td>{n.containers}</td>
			</tr>
		{/snippet}
	</DataTable>
{/if}

<style>
	.ct-toolbar { display: flex; align-items: center; gap: 12px; margin-bottom: 14px; }
	.ct-toolbar-right { margin-left: auto; }
	.ct-mono { font-family: var(--font-mono); }
	.ct-trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; max-width: 0; }
</style>
