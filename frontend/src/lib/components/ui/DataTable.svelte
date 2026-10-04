<script lang="ts" generics="T">
	import type { Snippet } from 'svelte';
	import SearchInput from './SearchInput.svelte';
	import Pagination from './Pagination.svelte';
	import Skeleton from './Skeleton.svelte';
	import EmptyState from './EmptyState.svelte';
	import InlineAlert from './InlineAlert.svelte';

	interface Column {
		key: string;
		label: string;
		width?: string;
	}

	interface FetchPageParams {
		page: number;
		pageSize: number;
		search: string;
	}

	interface FetchPageResult<T> {
		rows: T[];
		total: number;
	}

	interface Props {
		columns: Column[];
		rowKey: (row: T) => string;
		items?: T[];
		fetchPage?: (params: FetchPageParams) => Promise<FetchPageResult<T>>;
		searchable?: boolean;
		searchFields?: (keyof T)[];
		pageSize?: number;
		emptyMessage?: string;
		row: Snippet<[T]>;
	}

	let {
		columns,
		rowKey,
		items,
		fetchPage,
		searchable = true,
		searchFields,
		pageSize = 25,
		emptyMessage = 'No results.',
		row
	}: Props = $props();

	if (import.meta.env.DEV) {
		if ((items && fetchPage) || (!items && !fetchPage)) {
			console.error('DataTable: pass exactly one of `items` (client mode) or `fetchPage` (server mode), not both or neither.');
		}
		if (items && searchable && !searchFields) {
			console.error('DataTable: `searchFields` is required in client mode when `searchable` is true.');
		}
	}

	let search = $state('');
	let page = $state(0);
	let currentPageSize = $state(pageSize);

	// Server-mode state
	let serverRows = $state<T[]>([]);
	let serverTotal = $state(0);
	let loading = $state(false);
	let error = $state('');
	// Server mode fetches with the debounced search term, not the raw input value.
	let appliedSearch = $state('');
	// Monotonic id so a superseded (slower, older) response never overwrites a newer one.
	let requestId = 0;

	async function loadServerPage() {
		if (!fetchPage) return;
		const id = ++requestId;
		loading = true;
		error = '';
		try {
			const result = await fetchPage({ page, pageSize: currentPageSize, search: appliedSearch });
			if (id !== requestId) return;
			serverRows = result.rows;
			serverTotal = result.total;
			// Rows vanished from under us (e.g. the last row of the last page was deleted):
			// step back to the last real page; the page change triggers the next fetch.
			if (result.rows.length === 0 && result.total > 0 && page > 0) {
				const last = Math.max(0, Math.ceil(result.total / currentPageSize) - 1);
				if (last < page) {
					page = last;
					return;
				}
			}
		} catch (e) {
			if (id !== requestId) return;
			error = e instanceof Error ? e.message : 'Failed to load data.';
		}
		if (id === requestId) loading = false;
	}

	$effect(() => {
		// Re-fetch whenever page, page size, or (debounced) search changes.
		page; currentPageSize; appliedSearch;
		if (fetchPage) loadServerPage();
	});

	// Client-mode derived state
	let clientFiltered = $derived.by(() => {
		if (!items) return [];
		if (!search || !searchFields) return items;
		const q = search.toLowerCase();
		return items.filter((it) =>
			searchFields!.some((f) => String(it[f] ?? '').toLowerCase().includes(q))
		);
	});
	let clientTotal = $derived(items ? clientFiltered.length : 0);
	let clientPageRows = $derived(
		items ? clientFiltered.slice(page * currentPageSize, (page + 1) * currentPageSize) : []
	);

	// Client mode: reset to page 0 whenever the search term changes.
	// Server mode: debounce 300ms, then reset the page and apply the term in one
	// batched update so exactly one fetch fires.
	$effect(() => {
		const s = search;
		if (!fetchPage) {
			page = 0;
			return;
		}
		const t = setTimeout(() => {
			if (s !== appliedSearch) {
				page = 0;
				appliedSearch = s;
			}
		}, 300);
		return () => clearTimeout(t);
	});

	// Client mode: never leave the user on a page past the end (items shrank or were replaced).
	$effect(() => {
		if (!items) return;
		const last = Math.max(0, Math.ceil(clientTotal / currentPageSize) - 1);
		if (page > last) page = last;
	});

	let displayRows = $derived(items ? clientPageRows : serverRows);
	let displayTotal = $derived(items ? clientTotal : serverTotal);
	let isEmpty = $derived(!loading && !error && displayRows.length === 0);
</script>

<div class="ui-data-table">
	{#if searchable}
		<div class="ui-data-table-toolbar">
			<SearchInput bind:value={search} />
		</div>
	{/if}

	{#if loading && displayRows.length === 0}
		<div class="ui-data-table-skeleton">
			{#each Array(5) as _}
				<Skeleton variant="row" />
			{/each}
		</div>
	{:else if error}
		<InlineAlert tone="error">{error}</InlineAlert>
	{:else if isEmpty}
		<EmptyState message={emptyMessage} />
	{:else}
		<div class="ui-data-table-scroll">
			<table class="ui-data-table-el">
				<thead>
					<tr>
						{#each columns as col (col.key)}
							<th style={col.width ? `width:${col.width}` : undefined}>{col.label}</th>
						{/each}
					</tr>
				</thead>
				<tbody>
					{#each displayRows as r (rowKey(r))}
						{@render row(r)}
					{/each}
				</tbody>
			</table>
		</div>
		<Pagination
			{page}
			pageSize={currentPageSize}
			total={displayTotal}
			onPageChange={(p) => (page = p)}
			onPageSizeChange={(s) => (currentPageSize = s)}
		/>
	{/if}
</div>

<style>
	.ui-data-table {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.ui-data-table-toolbar {
		display: flex;
		justify-content: flex-end;
		padding: 14px 16px 6px;
	}
	.ui-data-table-toolbar :global(.ui-search) { max-width: 220px; }
	.ui-data-table-skeleton {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px 16px;
	}
	.ui-data-table-scroll { overflow-x: auto; }
	.ui-data-table-el {
		width: 100%;
		border-collapse: collapse;
		font-size: 12.5px;
	}
	.ui-data-table-el thead th {
		text-align: left;
		padding: 9px 16px;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		font-size: 10.5px;
		font-weight: 700;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		white-space: nowrap;
	}
	.ui-data-table-el :global(tbody tr) {
		border-bottom: 1px solid var(--border);
	}
	.ui-data-table-el :global(tbody tr:last-child) {
		border-bottom: none;
	}
	.ui-data-table-el :global(tbody tr:hover) {
		background: var(--bg-hover);
	}
	.ui-data-table-el :global(td) {
		padding: 10px 16px;
		color: var(--text-secondary);
	}
	/* Expanded detail rows (a full-width colspan cell) must never resize the
	   table: zero their intrinsic width so only the header columns size the
	   table, and wrap long unbroken values inside the available width. */
	.ui-data-table-el :global(td[colspan] > *) {
		contain: inline-size;
		overflow-wrap: anywhere;
	}
	:global(.ui-data-table) :global(.ui-pagination) {
		padding: 10px 16px;
		border-top: 1px solid var(--border);
	}
</style>
