<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { DataTable, Badge, InlineAlert } from '$lib/components/ui';

	let subscriptions = $state<any[]>([]);
	let loading = $state(false);
	let error = $state('');

	async function load() {
		loading = true; error = '';
		const r = await api.get<any>('/admin/mqtt/subscriptions');
		if (r.data) {
			subscriptions = Array.isArray(r.data) ? r.data : (r.data.items ?? r.data.data ?? []);
		} else {
			error = r.error?.message ?? 'Failed to load subscriptions';
		}
		loading = false;
	}

	let tableItems = $derived(
		subscriptions.map((s, i) => ({
			key: String(s.id ?? `${s.client_id ?? s.clientid ?? i}-${s.topic ?? i}`),
			topic: s.topic ?? '—',
			clientId: s.client_id ?? s.clientid ?? '—',
			qos: s.qos ?? '—'
		}))
	);

	onMount(load);
</script>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={tableItems}
		rowKey={(s) => s.key}
		searchFields={['topic', 'clientId']}
		columns={[
			{ key: 'topic', label: 'Topic', width: '46%' },
			{ key: 'clientId', label: 'Client ID', width: '34%' },
			{ key: 'qos', label: 'QoS', width: '20%' }
		]}
		emptyMessage="No subscriptions."
	>
		{#snippet row(s)}
			<tr>
				<td class="ms-mono">{s.topic}</td>
				<td class="ms-mono">{s.clientId}</td>
				<td><Badge tone="blue">{s.qos}</Badge></td>
			</tr>
		{/snippet}
	</DataTable>
{/if}

<style>
	.ms-mono { font-family: var(--font-mono); }
</style>
