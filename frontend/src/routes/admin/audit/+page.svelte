<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AuditLogEntry } from '$lib/api/types';

	let items       = $state<AuditLogEntry[]>([]);
	let nextCursor  = $state<string | null>(null);
	let loading     = $state(true);
	let loadingMore = $state(false);
	let error       = $state('');
	let orgFilter   = $state('');
	let search      = $state('');

	let page = $state(0);
	const LIMIT = 25;

	let expanded = $state(new Set<string>());
	function toggleExpand(id: string) {
		if (expanded.has(id)) expanded.delete(id);
		else expanded.add(id);
		expanded = new Set(expanded);
	}

	async function load(cursor?: string) {
		if (cursor) loadingMore = true;
		else { loading = true; items = []; nextCursor = null; page = 0; }
		error = '';
		const res = await api.getAdminAuditLogs({
			cursor,
			limit: 50,
			org_id: orgFilter.trim() || undefined,
		});
		if (res.data) {
			items = cursor ? [...items, ...res.data.items] : res.data.items;
			nextCursor = res.data.next_cursor;
		} else {
			error = res.error?.message ?? 'Failed to load audit logs';
		}
		loading = false;
		loadingMore = false;
	}

	let filtered = $derived(
		search.trim()
			? items.filter(e =>
				e.action.toLowerCase().includes(search.toLowerCase()) ||
				(e.resource_type ?? '').toLowerCase().includes(search.toLowerCase()) ||
				(e.user_id ?? '').toLowerCase().includes(search.toLowerCase())
			)
			: items
	);

	let totalPages = $derived(Math.ceil(filtered.length / LIMIT));
	let paged = $derived(filtered.slice(page * LIMIT, (page + 1) * LIMIT));

	$effect(() => {
		search;
		page = 0;
	});

	function fmtDate(ts: string): string {
		try { return new Date(ts).toLocaleString(); }
		catch { return ts; }
	}

	function actionColor(action: string): string {
		const a = action.toLowerCase();
		if (a.includes('delete') || a.includes('remove')) return 'var(--danger)';
		if (a.includes('create') || a.includes('add') || a.includes('invite')) return 'var(--ok)';
		if (a.includes('update') || a.includes('patch') || a.includes('revoke')) return 'var(--accent)';
		return 'var(--text-2)';
	}

	onMount(() => load());
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Audit Log</h1>
			<p class="sub">Platform-wide activity across all organizations.</p>
		</div>
	</header>

	<div class="filters">
		<label class="search-wrap">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
			<input class="search-inp" placeholder="Search action, type, user ID…" bind:value={search} />
		</label>
		<div class="org-filter">
			<input class="org-inp" placeholder="Org ID filter (optional)" bind:value={orgFilter} />
			<button class="filter-btn" onclick={() => load()}>Apply</button>
		</div>
	</div>

	{#if loading}
		<div class="tbl">
			{#each [0,1,2,3,4] as _}
				<div class="sk-row">
					<div class="sk" style="width:110px;height:11px"></div>
					<div class="sk" style="width:70px;height:11px"></div>
					<div class="sk" style="flex:1;height:11px"></div>
					<div class="sk" style="width:130px;height:11px"></div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err-banner">{error}</div>
	{:else if filtered.length === 0}
		<div class="empty">No audit entries found.</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:1.8">Action</span>
				<span style="flex:1.2">Resource Type</span>
				<span style="flex:2">Resource ID</span>
				<span style="flex:2">User ID</span>
				<span style="flex:1">IP</span>
				<span style="flex:1.8">Time</span>
			</div>
			{#each paged as entry}
				<!-- svelte-ignore a11y_click_events_have_key_events -->
				<!-- svelte-ignore a11y_no_static_element_interactions -->
				<div class="trow-wrapper">
					<div class="trow" onclick={() => toggleExpand(entry.id)}>
						<div style="flex:1.8;min-width:0;display:flex;align-items:center;gap:6px">
							<span class="chevron" class:expanded={expanded.has(entry.id)}>▶</span>
							<span class="action-chip" style="color:{actionColor(entry.action)}">{entry.action}</span>
						</div>
						<div class="cell trunc" style="flex:1.2">{entry.resource_type}</div>
						<div class="mono muted trunc" style="flex:2;font-size:11px">{entry.resource_id ?? '—'}</div>
						<div class="mono muted trunc" style="flex:2;font-size:11px">{entry.user_id ?? '—'}</div>
						<div class="mono muted trunc" style="flex:1;font-size:11px">{entry.ip_address ?? '—'}</div>
						<div class="cell muted trunc" style="flex:1.8;font-size:11.5px">{fmtDate(entry.created_at)}</div>
					</div>
					{#if expanded.has(entry.id)}
						<div class="details-panel">
							<div class="details-grid">
								<div><strong>Resource ID:</strong> <span class="mono">{entry.resource_id ?? '—'}</span></div>
								<div><strong>User ID:</strong> <span class="mono">{entry.user_id ?? '—'}</span></div>
								<div><strong>IP Address:</strong> <span class="mono">{entry.ip_address ?? '—'}</span></div>
								{#if entry.metadata && Object.keys(entry.metadata).length > 0}
									<div style="grid-column: 1 / -1">
										<strong>Metadata Details:</strong>
										<pre class="details-pre mono">{JSON.stringify(entry.metadata, null, 2)}</pre>
									</div>
								{/if}
							</div>
						</div>
					{/if}
				</div>
			{/each}
		</div>

		<div class="card-list">
			{#each paged as entry}
				<div class="m-card">
					<div class="m-card-title">
						<span class="action-chip" style="color:{actionColor(entry.action)}">{entry.action}</span>
					</div>
					<div class="m-card-row"><span class="m-card-key">Type</span><span class="cell">{entry.resource_type}</span></div>
					<div class="m-card-row"><span class="m-card-key">Resource ID</span><span class="mono cell">{entry.resource_id ?? '—'}</span></div>
					<div class="m-card-row"><span class="m-card-key">User ID</span><span class="mono cell">{entry.user_id ?? '—'}</span></div>
					<div class="m-card-row"><span class="m-card-key">IP Address</span><span class="mono cell">{entry.ip_address ?? '—'}</span></div>
					<div class="m-card-row"><span class="m-card-key">Time</span><span class="cell muted" style="font-size:11.5px">{fmtDate(entry.created_at)}</span></div>
					{#if entry.metadata && Object.keys(entry.metadata).length > 0}
						<div style="margin-top: 8px">
							<span class="m-card-key">Metadata Details</span>
							<pre class="details-pre mono" style="font-size: 10px; padding: 6px">{JSON.stringify(entry.metadata, null, 2)}</pre>
						</div>
					{/if}
				</div>
			{/each}
		</div>

		<div class="pager-section">
			{#if totalPages > 1}
				<div class="pager">
					<button class="pg-btn" disabled={page === 0} onclick={() => page--}>Prev</button>
					<span class="pg-info">Page {page + 1} of {totalPages} &bull; {filtered.length} total</span>
					<button class="pg-btn" disabled={page >= totalPages - 1} onclick={() => page++}>Next</button>
				</div>
			{/if}

			{#if nextCursor}
				<div class="load-more-wrap">
					<button class="load-more" onclick={() => load(nextCursor!)} disabled={loadingMore}>
						{loadingMore ? 'Loading…' : 'Load more from server'}
					</button>
				</div>
			{/if}
		</div>

		<div class="count-note">Showing {filtered.length} entries loaded{search ? ' (filtered)' : ''}.</div>
	{/if}
</div>

<style>
	.p { max-width:1100px; margin:0 auto; padding:40px 36px; }
	.hdr { margin-bottom:16px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }

	.filters { display:flex; align-items:center; gap:10px; margin-bottom:14px; flex-wrap:wrap; }
	.search-wrap { position:relative; display:flex; align-items:center; flex:1; min-width:200px; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search-inp { height:32px; padding:0 10px 0 28px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:100%; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search-inp::placeholder { color:var(--text-3); }
	.search-inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }
	.org-filter { display:flex; align-items:center; gap:6px; }
	.org-inp { height:32px; padding:0 10px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12px; color:var(--text); outline:none; width:240px; font-family:var(--mono); transition:border-color .15s; }
	.org-inp::placeholder { color:var(--text-3); font-family:var(--font); }
	.org-inp:focus { border-color:var(--accent); }
	.filter-btn { padding:0 14px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface-2); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.filter-btn:hover { background:var(--accent); border-color:var(--accent); color:#000; }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow-wrapper { border-bottom:1px solid var(--border); display:flex; flex-direction:column; }
	.trow-wrapper:last-child { border-bottom:none; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; transition:background .1s; cursor:pointer; }
	.trow:hover { background:var(--row-hover); }
	.chevron { display:inline-block; font-size:8px; color:var(--text-3); transition:transform .2s; margin-right:2px; flex-shrink:0; }
	.chevron.expanded { transform:rotate(90deg); }

	.details-panel { padding:14px 20px 16px; background:var(--surface-2); border-top:1px solid var(--border); font-size:12.5px; border-bottom:1px solid var(--border); }
	.details-grid { display:grid; grid-template-columns:1fr 1fr; gap:8px; }
	.details-pre { margin-top:6px; padding:8px 12px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:11px; max-height:220px; overflow-y:auto; color:var(--text); white-space:pre-wrap; word-break:break-all; }

	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.mono { font-family:var(--mono); color:var(--text); }
	.trunc { text-overflow:ellipsis; white-space:nowrap; overflow:hidden; min-width:0; }

	.action-chip { font-size:12px; font-weight:600; font-family:var(--mono); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-row { display:flex; align-items:center; gap:12px; padding:12px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err-banner { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.pager-section { display:flex; flex-direction:column; align-items:center; gap:8px; padding:16px 0 4px; }
	.pager { display:flex; align-items:center; gap:10px; justify-content:center; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }

	.load-more-wrap { display:flex; justify-content:center; }
	.load-more { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.load-more:hover:not(:disabled) { background:var(--surface-2); }
	.load-more:disabled { opacity:.5; cursor:not-allowed; }

	.count-note { font-size:11.5px; color:var(--text-3); padding:8px 0 0; text-align:right; }

	.card-list { display:none; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-title { font-size:13px; font-weight:600; color:var(--text); margin-bottom:8px; }
	.m-card-row { display:flex; justify-content:space-between; align-items:center; padding:4px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); gap:8px; }
	.m-card-row:last-child { border-bottom:none; }
	.m-card-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; flex-shrink:0; }

	@media (max-width: 860px) {
		.tbl { display:none; }
		.card-list { display:block; }
		.org-filter { width: 100%; }
		.org-inp { width: 100%; }
		.filter-btn { width: 100%; }
	}
	@media (max-width: 768px) {
		.p { padding:20px 14px; }
	}
	@media (max-width: 640px) {
		.p { padding:16px 12px; }
	}
</style>
