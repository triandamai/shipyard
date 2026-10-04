<script lang="ts">
	import { api } from '$lib/api/client';
	import { DataTable, Badge, Button, PageHeader } from '$lib/components/ui';

	interface Payment {
		id: string;
		org_id: string | null;
		org_name: string | null;
		stripe_payment_intent_id: string | null;
		amount: number;
		currency: string;
		status: string;
		description: string | null;
		created_at: string;
	}

	let totalPayments = $state(0);

	// Page-local segmented-control filter. A segmented control (not Select/
	// Dropdown) since it's visually/interactionally distinct — a small set of
	// mutually-exclusive pill buttons rather than an open/close menu. Wired to
	// force a DataTable refetch via the {#key} remount technique (Task 45).
	let statusFilter = $state('');

	const STATUS_OPTIONS = [
		{ value: '', label: 'All' },
		{ value: 'success', label: 'Success' },
		{ value: 'pending', label: 'Pending' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'canceled', label: 'Canceled' }
	];

	// Backend (/admin/payments) paginates via page/per_page, not page/limit like
	// the other server-mode admin endpoints — confirmed against the pre-migration
	// fetch call in this file and the handler in backend/crates/api/src/admin/mod.rs.
	// It does not accept a `q`/search param, so `searchable={false}` below.
	async function fetchPaymentsPage(params: { page: number; pageSize: number; search: string }) {
		const qs = new URLSearchParams({
			page: String(params.page),
			per_page: String(params.pageSize)
		});
		if (statusFilter) qs.set('status', statusFilter);
		const res = await api.get<{ items: Payment[]; total: number }>(`/admin/payments?${qs}`);
		if (!res.data) throw new Error(res.error?.message ?? 'Failed to load payments');
		totalPayments = res.data.total;
		return { rows: res.data.items, total: res.data.total };
	}

	// Unchanged from the pre-migration version of this page.
	function fmtAmount(cents: number, currency: string): string {
		return (cents / 100).toLocaleString('en-US', { style: 'currency', currency: currency.toUpperCase() });
	}

	// Maps payment status onto Badge's fixed tone enum (green/red/yellow/blue/neutral),
	// preserving the same groupings the old hand-rolled statusMeta() used.
	function statusTone(s: string): 'green' | 'red' | 'yellow' | 'blue' | 'neutral' {
		if (s === 'success') return 'green';
		if (s === 'pending') return 'yellow';
		if (s === 'failed') return 'red';
		return 'neutral';
	}
	function statusLabel(s: string): string {
		return s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
	}
</script>

<PageHeader title="Payments" subtitle="Billing payment records across all organizations.">
	{#snippet actions()}
		<Badge tone="neutral">{totalPayments} total</Badge>
	{/snippet}
</PageHeader>

<div class="pm-toolbar">
	<span class="pm-filter-label">Status</span>
	<div class="pm-seg">
		{#each STATUS_OPTIONS as opt}
			<Button
				variant={statusFilter === opt.value ? 'primary' : 'ghost'}
				size="sm"
				onclick={() => { statusFilter = opt.value; }}
			>
				{opt.label}
			</Button>
		{/each}
	</div>
</div>

{#key statusFilter}
	<DataTable
		fetchPage={fetchPaymentsPage}
		rowKey={(p) => p.id}
		searchable={false}
		columns={[
			{ key: 'org', label: 'Organization' },
			{ key: 'amount', label: 'Amount' },
			{ key: 'status', label: 'Status' },
			{ key: 'description', label: 'Description' },
			{ key: 'payment_id', label: 'Payment ID' },
			{ key: 'date', label: 'Date' }
		]}
		emptyMessage={statusFilter ? `No ${statusFilter} payments found.` : 'No payment records yet.'}
	>
		{#snippet row(p)}
			<tr>
				<td>{p.org_name ?? '—'}</td>
				<td class="pm-amount">{fmtAmount(p.amount, p.currency)}</td>
				<td><Badge tone={statusTone(p.status)}>{statusLabel(p.status)}</Badge></td>
				<td class="pm-dim">{p.description ?? '—'}</td>
				<td class="pm-mono pm-dim" title={p.stripe_payment_intent_id ?? undefined}>{p.stripe_payment_intent_id ?? '—'}</td>
				<td class="pm-dim">{new Date(p.created_at).toLocaleDateString()}</td>
			</tr>
		{/snippet}
	</DataTable>
{/key}

<style>
	.pm-toolbar {
		display: flex;
		align-items: center;
		gap: 10px;
		margin-bottom: 14px;
		flex-wrap: wrap;
	}
	.pm-filter-label {
		font-size: 11.5px;
		font-weight: 600;
		color: var(--text-muted);
	}
	.pm-seg {
		display: flex;
		align-items: center;
		gap: 2px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 3px;
	}
	.pm-amount {
		font-weight: 700;
		color: var(--text-primary);
		font-variant-numeric: tabular-nums;
	}
	.pm-mono {
		font-family: var(--font-mono);
		font-size: 11px;
		max-width: 160px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.pm-dim {
		color: var(--text-muted);
	}

	@media (max-width: 640px) {
		.pm-toolbar { flex-direction: column; align-items: flex-start; }
		.pm-seg { flex-wrap: wrap; }
	}
</style>
