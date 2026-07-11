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

	async function load(cursor?: string) {
		if (cursor) loadingMore = true;
		else { loading = true; items = []; nextCursor = null; }
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
			{#each filtered as entry}
				<div class="trow">
					<div style="flex:1.8">
						<span class="action-chip" style="color:{actionColor(entry.action)}">{entry.action}</span>
					</div>
					<div class="cell" style="flex:1.2">{entry.resource_type}</div>
					<div class="mono muted" style="flex:2;font-size:11px">{entry.resource_id ?? '—'}</div>
					<div class="mono muted" style="flex:2;font-size:11px">{entry.user_id ?? '—'}</div>
					<div class="mono muted" style="flex:1;font-size:11px">{entry.ip_address ?? '—'}</div>
					<div class="cell muted" style="flex:1.8;font-size:11.5px">{fmtDate(entry.created_at)}</div>
				</div>
			{/each}
		</div>

		{#if nextCursor}
			<div class="load-more-wrap">
				<button class="load-more" onclick={() => load(nextCursor!)} disabled={loadingMore}>
					{loadingMore ? 'Loading…' : 'Load more'}
				</button>
			</div>
		{/if}

		<div class="count-note">Showing {filtered.length} entries{search ? ' (filtered)' : ''}.</div>
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
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.mono { font-family:var(--mono); color:var(--text); }

	.action-chip { font-size:12px; font-weight:600; font-family:var(--mono); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-row { display:flex; align-items:center; gap:12px; padding:12px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err-banner { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.load-more-wrap { display:flex; justify-content:center; padding:16px 0 4px; }
	.load-more { padding:7px 24px; border-radius:var(--radius-sm); font-size:12.5px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.load-more:hover:not(:disabled) { background:var(--surface-2); }
	.load-more:disabled { opacity:.5; cursor:not-allowed; }

	.count-note { font-size:11.5px; color:var(--text-3); padding:8px 0 0; text-align:right; }
</style>
