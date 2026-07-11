<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

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

	let items     = $state<Payment[]>([]);
	let total     = $state(0);
	let loading   = $state(true);
	let error     = $state('');
	let page      = $state(0);
	let perPage   = 25;
	let statusFilter = $state('');

	const STATUS_OPTIONS = ['', 'success', 'pending', 'failed', 'canceled'];

	async function load() {
		loading = true;
		error   = '';
		const qs = new URLSearchParams({ page: String(page), per_page: String(perPage) });
		if (statusFilter) qs.set('status', statusFilter);
		const r = await api.get<{ items: Payment[]; total: number; page: number }>(`/admin/payments?${qs}`);
		if (r.data) { items = r.data.items; total = r.data.total; }
		else error = r.error?.message ?? 'Failed to load payments';
		loading = false;
	}

	let totalPages = $derived(Math.ceil(total / perPage));

	function fmtAmount(cents: number, currency: string): string {
		return (cents / 100).toLocaleString('en-US', { style: 'currency', currency: currency.toUpperCase() });
	}

	function statusMeta(s: string): { label: string; color: string; bg: string } {
		if (s === 'success')  return { label: 'Success',  color: 'var(--ok)',     bg: 'var(--ok-soft)' };
		if (s === 'pending')  return { label: 'Pending',  color: 'var(--warn)',   bg: 'var(--warn-soft)' };
		if (s === 'failed')   return { label: 'Failed',   color: 'var(--danger)', bg: 'var(--danger-soft)' };
		if (s === 'canceled') return { label: 'Canceled', color: 'var(--text-3)', bg: 'var(--surface-2)' };
		return { label: s, color: 'var(--text-3)', bg: 'var(--surface-2)' };
	}

	$effect(() => { statusFilter; page = 0; });

	onMount(load);
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Payments</h1>
			<p class="sub">Billing payment records across all organizations.</p>
		</div>
	</header>

	<div class="toolbar">
		<div class="filter-row">
			<label class="filter-label">Status</label>
			<div class="seg">
				{#each STATUS_OPTIONS as opt}
					<button
						class="seg-btn"
						class:active={statusFilter === opt}
						onclick={() => { statusFilter = opt; load(); }}
					>
						{opt === '' ? 'All' : opt.charAt(0).toUpperCase() + opt.slice(1)}
					</button>
				{/each}
			</div>
		</div>
		<span class="total-label">{total} total</span>
	</div>

	{#if loading}
		<div class="tbl">
			{#each [0,1,2,3,4] as _}
				<div class="sk-row">
					<div class="sk" style="width:100px;height:11px"></div>
					<div class="sk" style="flex:1;height:11px"></div>
					<div class="sk" style="width:60px;height:18px;border-radius:999px"></div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if items.length === 0}
		<div class="empty">
			<svg viewBox="0 0 20 20" fill="currentColor" width="28" height="28"><path d="M4 4a2 2 0 00-2 2v1h16V6a2 2 0 00-2-2H4z"/><path fill-rule="evenodd" d="M18 9H2v5a2 2 0 002 2h12a2 2 0 002-2V9zM4 13a1 1 0 011-1h1a1 1 0 110 2H5a1 1 0 01-1-1zm5-1a1 1 0 100 2h1a1 1 0 100-2H9z" clip-rule="evenodd"/></svg>
			{statusFilter ? `No ${statusFilter} payments found.` : 'No payment records yet.'}
		</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:1.8">Organization</span>
				<span style="flex:1.2">Amount</span>
				<span style="flex:1">Status</span>
				<span style="flex:1.5">Description</span>
				<span style="flex:1.8">Payment ID</span>
				<span style="flex:1.2">Date</span>
			</div>
			{#each items as p}
				{@const sm = statusMeta(p.status)}
				<div class="trow">
					<div class="cell" style="flex:1.8">{#if p.org_name}{p.org_name}{:else}<span class="muted">—</span>{/if}</div>
					<div class="amount" style="flex:1.2">{fmtAmount(p.amount, p.currency)}</div>
					<div style="flex:1">
						<span class="status-chip" style="background:{sm.bg};color:{sm.color}">{sm.label}</span>
					</div>
					<div class="muted cell" style="flex:1.5;font-size:11.5px">{p.description ?? '—'}</div>
					<div class="mono muted" style="flex:1.8;font-size:10.5px;overflow:hidden;text-overflow:ellipsis;white-space:nowrap">{p.stripe_payment_intent_id ?? '—'}</div>
					<div class="date" style="flex:1.2">{new Date(p.created_at).toLocaleDateString()}</div>
				</div>
			{/each}
		</div>

		<!-- Mobile cards -->
		<div class="card-list">
			{#each items as p}
				{@const sm = statusMeta(p.status)}
				<div class="m-card">
					<div class="m-card-hdr">
						<span class="m-org">{p.org_name ?? '—'}</span>
						<span class="status-chip" style="background:{sm.bg};color:{sm.color}">{sm.label}</span>
					</div>
					<div class="m-card-row"><span class="m-key">Amount</span><span class="amount">{fmtAmount(p.amount, p.currency)}</span></div>
					<div class="m-card-row"><span class="m-key">Date</span><span class="date">{new Date(p.created_at).toLocaleDateString()}</span></div>
					{#if p.description}<div class="m-card-row"><span class="m-key">Note</span><span class="muted">{p.description}</span></div>{/if}
					{#if p.stripe_payment_intent_id}<div class="m-card-row"><span class="m-key">ID</span><span class="mono muted" style="font-size:10px;word-break:break-all">{p.stripe_payment_intent_id}</span></div>{/if}
				</div>
			{/each}
		</div>
	{/if}

	{#if totalPages > 1}
		<div class="pager">
			<button class="pg-btn" disabled={page === 0} onclick={() => { page--; load(); }}>Prev</button>
			<span class="pg-info">Page {page + 1} of {totalPages}</span>
			<button class="pg-btn" disabled={page >= totalPages - 1} onclick={() => { page++; load(); }}>Next</button>
		</div>
	{/if}
</div>

<style>
	.p { max-width:1100px; margin:0 auto; padding:40px 36px; }
	.hdr { margin-bottom:16px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }

	.toolbar { display:flex; align-items:center; justify-content:space-between; margin-bottom:14px; flex-wrap:wrap; gap:10px; }
	.filter-row { display:flex; align-items:center; gap:10px; }
	.filter-label { font-size:11.5px; font-weight:600; color:var(--text-3); }
	.seg { display:flex; gap:2px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); padding:3px; }
	.seg-btn { padding:4px 11px; border-radius:4px; font-size:12px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.seg-btn.active { background:var(--surface); color:var(--text); box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.total-label { font-size:11.5px; color:var(--text-3); }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:11px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.mono { font-family:var(--mono); }
	.amount { font-size:13px; font-weight:700; color:var(--text); font-variant-numeric:tabular-nums; }
	.date { font-size:11.5px; color:var(--text-3); white-space:nowrap; }
	.status-chip { display:inline-flex; align-items:center; padding:2px 8px; border-radius:999px; font-size:10.5px; font-weight:700; white-space:nowrap; }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-row { display:flex; align-items:center; gap:12px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:56px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.pager { display:flex; align-items:center; gap:10px; padding:12px 0 4px; justify-content:center; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }

	.card-list { display:none; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:10px; }
	.m-org { font-size:13px; font-weight:600; color:var(--text); }
	.m-card-row { display:flex; justify-content:space-between; align-items:flex-start; padding:5px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); gap:8px; }
	.m-card-row:last-child { border-bottom:none; }
	.m-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; flex-shrink:0; }

	@media (max-width: 768px) {
		.p { padding:20px 14px; }
		.toolbar { flex-direction:column; align-items:flex-start; }
		.seg { flex-wrap:wrap; }
	}
	@media (max-width: 640px) {
		.p { padding:16px 12px; }
		.tbl { display:none; }
		.card-list { display:block; }
	}
</style>
