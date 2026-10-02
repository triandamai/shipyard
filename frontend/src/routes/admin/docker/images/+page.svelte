<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Trash2, RefreshCw } from '@lucide/svelte';
	import { DataTable, Button, InlineAlert, ConfirmDialog } from '$lib/components/ui';

	interface ImageSummary { id: string; tags: string[]; size: number; created: number; }

	let images = $state<ImageSummary[]>([]);
	let loading = $state(false);
	let error = $state('');

	let pruning = $state(false);
	let pruneMsg = $state('');
	let showPruneConfirm = $state(false);

	async function load() {
		loading = true; error = '';
		const r = await api.get<ImageSummary[]>('/admin/docker/images');
		if (r.data) images = r.data;
		else error = r.error?.message ?? 'Failed to load images';
		loading = false;
	}

	async function prune() {
		pruning = true; pruneMsg = '';
		const r = await api.post<{ message: string }>('/admin/docker/prune/images');
		pruneMsg = r.data?.message ?? r.error?.message ?? 'Done';
		pruning = false;
		setTimeout(() => (pruneMsg = ''), 4000);
		await load();
	}

	function fmtBytes(bytes: number): string {
		if (bytes === 0) return '0 B';
		const k = 1024;
		const s = ['B','KB','MB','GB','TB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${(bytes / Math.pow(k, i)).toFixed(1)} ${s[i]}`;
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
	title="Prune unused images"
	message="This cannot be undone."
	confirmLabel="Prune"
	onConfirm={prune}
/>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={images}
		rowKey={(i) => i.id}
		searchFields={['id', 'tags']}
		columns={[
			{ key: 'id', label: 'ID', width: '20%' },
			{ key: 'tags', label: 'Tags', width: '60%' },
			{ key: 'size', label: 'Size', width: '20%' }
		]}
		emptyMessage="No images found."
	>
		{#snippet row(img)}
			<tr>
				<td class="ct-mono ct-trunc">{img.id.replace('sha256:', '').slice(0, 12)}</td>
				<td class="ct-trunc">{img.tags.join(', ') || '—'}</td>
				<td>{fmtBytes(img.size)}</td>
			</tr>
		{/snippet}
	</DataTable>
{/if}

<style>
	.ct-toolbar { display: flex; align-items: center; gap: 12px; margin-bottom: 14px; }
	.ct-toolbar-right { margin-left: auto; }
	.ct-prune-msg { font-size: 11.5px; color: var(--accent-green); font-weight: 500; }
	.ct-mono { font-family: var(--font-mono); }
	.ct-trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; max-width: 0; }
</style>
