<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { DataTable, InlineAlert } from '$lib/components/ui';

	let clients = $state<any[]>([]);
	let loading = $state(false);
	let error = $state('');

	async function load() {
		loading = true; error = '';
		const r = await api.get<any>('/admin/mqtt/clients');
		if (r.data) {
			clients = Array.isArray(r.data) ? r.data : (r.data.items ?? r.data.data ?? []);
		} else {
			error = r.error?.message ?? 'Failed to load clients';
		}
		loading = false;
	}

	function connectedAt(ts: any): string {
		if (!ts) return '—';
		try { return new Date(typeof ts === 'number' ? ts * 1000 : ts).toLocaleString(); }
		catch { return String(ts); }
	}

	let tableItems = $derived(
		clients.map((c, i) => ({
			key: String(c.client_id ?? c.clientid ?? i),
			clientId: c.client_id ?? c.clientid ?? '—',
			username: c.username ?? '—',
			address: c.remote_addr ?? c.ip_address ?? c.ipaddress ?? '—',
			protocol: c.proto_ver ?? c.protocol ?? '—',
			connectedAt: connectedAt(c.connected_at ?? c.connected_epoch)
		}))
	);

	onMount(load);
</script>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={tableItems}
		rowKey={(c) => c.key}
		searchFields={['clientId', 'username', 'address']}
		columns={[
			{ key: 'clientId', label: 'Client ID', width: '26%' },
			{ key: 'username', label: 'Username', width: '20%' },
			{ key: 'address', label: 'Address', width: '20%' },
			{ key: 'protocol', label: 'Protocol', width: '14%' },
			{ key: 'connectedAt', label: 'Connected at', width: '20%' }
		]}
		emptyMessage="No connected clients."
	>
		{#snippet row(c)}
			<tr>
				<td class="mc-mono">{c.clientId}</td>
				<td>{c.username}</td>
				<td class="mc-mono">{c.address}</td>
				<td>{c.protocol}</td>
				<td>{c.connectedAt}</td>
			</tr>
		{/snippet}
	</DataTable>
{/if}

<style>
	.mc-mono { font-family: var(--font-mono); }
</style>
