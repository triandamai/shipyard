<script lang="ts">
	import { ChevronsLeft, ChevronLeft, ChevronRight, ChevronsRight } from '@lucide/svelte';
	import Button from './Button.svelte';
	import Select from './Select.svelte';

	interface Props {
		page: number;
		pageSize: number;
		total: number;
		pageSizeOptions?: number[];
		onPageChange: (page: number) => void;
		onPageSizeChange: (size: number) => void;
	}

	let {
		page,
		pageSize,
		total,
		pageSizeOptions = [10, 25, 50, 100],
		onPageChange,
		onPageSizeChange
	}: Props = $props();

	let lastPage = $derived(Math.max(0, Math.ceil(total / pageSize) - 1));
	let rangeStart = $derived(total === 0 ? 0 : page * pageSize + 1);
	let rangeEnd = $derived(Math.min(total, (page + 1) * pageSize));

	let pageSizeOpts = $derived(pageSizeOptions.map((n) => ({ value: String(n), label: String(n) })));

	let pageSizeStr = $state(String(pageSize));

	$effect(() => {
		pageSizeStr = String(pageSize);
	});

	$effect(() => {
		const n = Number(pageSizeStr);
		if (n !== pageSize) onPageSizeChange(n);
	});
</script>

<div class="ui-pagination">
	<div class="ui-pagination-size">
		<span class="ui-pagination-size-label">Rows per page</span>
		<Select bind:value={pageSizeStr} options={pageSizeOpts} id="ui-pagination-size-select" />
	</div>
	<span class="ui-pagination-range">{rangeStart}–{rangeEnd} of {total}</span>
	<div class="ui-pagination-nav">
		<Button variant="ghost" size="icon" disabled={page === 0} onclick={() => onPageChange(0)}>
			<ChevronsLeft size={15} />
		</Button>
		<Button variant="ghost" size="icon" disabled={page === 0} onclick={() => onPageChange(page - 1)}>
			<ChevronLeft size={15} />
		</Button>
		<Button variant="ghost" size="icon" disabled={page >= lastPage} onclick={() => onPageChange(page + 1)}>
			<ChevronRight size={15} />
		</Button>
		<Button variant="ghost" size="icon" disabled={page >= lastPage} onclick={() => onPageChange(lastPage)}>
			<ChevronsRight size={15} />
		</Button>
	</div>
</div>

<style>
	.ui-pagination {
		display: flex;
		align-items: center;
		gap: 20px;
		padding: 10px 4px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.ui-pagination-size { display: flex; align-items: center; gap: 8px; margin-left: auto; }
	.ui-pagination-size-label { white-space: nowrap; }
	.ui-pagination-size :global(select) { width: 68px; height: 30px; }
	.ui-pagination-range { font-variant-numeric: tabular-nums; white-space: nowrap; }
	.ui-pagination-nav { display: flex; gap: 2px; }
</style>
