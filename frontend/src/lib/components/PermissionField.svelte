<script lang="ts">
	import { Plus } from '@lucide/svelte';
	import { Badge, Button, BottomSheet } from '$lib/components/ui';
	import PermissionPicker from './PermissionPicker.svelte';
	import { permissionLabel, type PermResource } from '$lib/auth/permissionCatalog';

	interface Props {
		catalog: PermResource[];
		/** Applied permission ids. */
		value: string[];
		onchange: (next: string[]) => void;
		title?: string;
		subtitle?: string;
		emptyText?: string;
		disabled?: boolean;
	}

	let {
		catalog,
		value,
		onchange,
		title = 'Permissions',
		subtitle = 'Search for a permission, add it, then tick what it allows.',
		emptyText = 'No permissions selected.',
		disabled = false
	}: Props = $props();

	let open = $state(false);
	let draft = $state<string[]>([]);

	function edit() {
		draft = [...value];
		open = true;
	}

	function apply() {
		onchange([...draft]);
		open = false;
	}
</script>

<div class="pf">
	{#if value.length === 0}
		<p class="pf-empty">{emptyText}</p>
	{:else}
		<div class="pf-chips">
			{#each value as id (id)}
				<Badge tone="blue">{permissionLabel(catalog, id)}</Badge>
			{/each}
		</div>
	{/if}
	<div>
		<Button variant="secondary" size="sm" {disabled} onclick={edit}>
			<Plus size={12} />
			{value.length === 0 ? 'Add permissions' : 'Edit permissions'}
		</Button>
	</div>
</div>

<BottomSheet bind:open {title} {subtitle}>
	{#snippet children()}
		<PermissionPicker {catalog} bind:value={draft} />
	{/snippet}
	{#snippet footer()}
		<Button variant="secondary" size="sm" onclick={() => (open = false)}>Cancel</Button>
		<Button size="sm" onclick={apply}>Apply{draft.length ? ` (${draft.length})` : ''}</Button>
	{/snippet}
</BottomSheet>

<style>
	.pf { display: flex; flex-direction: column; gap: 10px; align-items: flex-start; }
	.pf-empty { margin: 0; font-size: 12px; color: var(--text-muted); }
	.pf-chips { display: flex; flex-wrap: wrap; gap: 4px; }
</style>
