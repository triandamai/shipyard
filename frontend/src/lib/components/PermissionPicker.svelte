<script lang="ts">
	import { X } from '@lucide/svelte';
	import { Autocomplete, Checkbox } from '$lib/components/ui';
	import type { AutocompleteOption } from '$lib/components/ui';
	import type { PermResource } from '$lib/auth/permissionCatalog';

	interface Props {
		catalog: PermResource[];
		/** Selected permission ids. */
		value: string[];
		placeholder?: string;
	}

	let { catalog, value = $bindable(), placeholder = 'Search permissions…' }: Props = $props();

	let query = $state('');
	// Resources the user has picked, in the order they picked them. Seeded from
	// the existing selection so edits show what's already granted.
	// Seeded once on mount; the parent remounts the picker for each edit session.
	// svelte-ignore state_referenced_locally
	let picked = $state<string[]>(
		catalog.filter((r) => r.actions.some((a) => value.includes(a.id))).map((r) => r.key)
	);

	let pickedResources = $derived(
		picked.map((k) => catalog.find((r) => r.key === k)).filter((r): r is PermResource => !!r)
	);

	let options = $derived.by<AutocompleteOption[]>(() => {
		const q = query.trim().toLowerCase();
		return catalog
			.filter((r) => !picked.includes(r.key))
			.filter(
				(r) =>
					!q ||
					r.label.toLowerCase().includes(q) ||
					r.key.toLowerCase().includes(q) ||
					r.actions.some((a) => a.label.toLowerCase().includes(q))
			)
			.slice(0, 30)
			.map((r) => ({ value: r.key, label: r.label, meta: r.actions.map((a) => a.label).join(' · ') }));
	});

	function onSelect(opt: AutocompleteOption) {
		if (!picked.includes(opt.value)) picked = [...picked, opt.value];
		// Autocomplete writes the option value into the input; reset for the next search.
		queueMicrotask(() => (query = ''));
	}

	function toggle(id: string, on: boolean) {
		value = on ? [...new Set([...value, id])] : value.filter((v) => v !== id);
	}

	function remove(res: PermResource) {
		const ids = new Set(res.actions.map((a) => a.id));
		value = value.filter((v) => !ids.has(v));
		picked = picked.filter((k) => k !== res.key);
	}
</script>

<div class="pp">
	<Autocomplete
		bind:value={query}
		{options}
		{placeholder}
		emptyText="No matching permissions."
		{onSelect}
	/>

	{#if pickedResources.length === 0}
		<p class="pp-empty">Search above and select a permission to add it.</p>
	{:else}
		<ul class="pp-list">
			{#each pickedResources as res (res.key)}
				<li class="pp-item">
					<div class="pp-head">
						<span class="pp-name">{res.label}</span>
						<button type="button" class="pp-remove" aria-label={`Remove ${res.label}`} onclick={() => remove(res)}>
							<X size={13} />
						</button>
					</div>
					<div class="pp-actions">
						{#each res.actions as a (a.id)}
							<span title={a.description}>
								<Checkbox
									label={a.label}
									checked={value.includes(a.id)}
									onchange={(c) => toggle(a.id, c)}
								/>
							</span>
						{/each}
					</div>
				</li>
			{/each}
		</ul>
	{/if}
</div>

<style>
	.pp { display: flex; flex-direction: column; gap: 12px; }
	.pp-empty { margin: 0; font-size: 12px; color: var(--text-muted); }
	.pp-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
	.pp-item {
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.pp-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	.pp-name { font-size: 12.5px; font-weight: 600; color: var(--text-primary); min-width: 0; }
	.pp-remove {
		display: flex; align-items: center; justify-content: center;
		width: 26px; height: 26px; border: none; border-radius: var(--radius-sm);
		background: none; color: var(--text-muted); cursor: pointer; flex-shrink: 0;
	}
	.pp-remove:hover { background: var(--bg-hover); color: var(--text-primary); }
	.pp-actions { display: flex; flex-wrap: wrap; gap: 8px 18px; }
</style>
