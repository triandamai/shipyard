<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { DataTable, InlineAlert } from '$lib/components/ui';

	let topics = $state<any[]>([]);
	let error = $state('');

	async function load() {
		error = '';
		const r = await api.get<any>('/admin/mqtt/topics');
		if (r.data) {
			topics = Array.isArray(r.data) ? r.data : (r.data.items ?? r.data.data ?? []);
		} else {
			error = r.error?.message ?? 'Failed to load topics';
		}
	}

	let tableItems = $derived(
		topics.map((t, i) => ({
			key: String(t.id ?? t.topic ?? t.name ?? i),
			topic: t.topic ?? t.name ?? '—'
		}))
	);

	onMount(load);
</script>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={tableItems}
		rowKey={(t) => t.key}
		searchFields={['topic']}
		columns={[{ key: 'topic', label: 'Topic' }]}
		emptyMessage="No topics."
	>
		{#snippet row(t)}
			<tr>
				<td class="ms-mono">{t.topic}</td>
			</tr>
		{/snippet}
	</DataTable>
{/if}

<style>
	.ms-mono { font-family: var(--font-mono); }
</style>
