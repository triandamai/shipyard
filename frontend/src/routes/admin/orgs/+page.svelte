<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AdminOrg } from '$lib/api/types';

	let orgs = $state<AdminOrg[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let patchingId = $state<string | null>(null);
	let search = $state('');

	onMount(() => load());

	async function load() {
		loading = true;
		const res = await api.getAdminOrgs();
		if (res.data) orgs = res.data;
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	}

	async function toggleSuspend(org: AdminOrg) {
		patchingId = org.id;
		const next = org.sub_status === 'suspended' ? 'active' : 'suspended';
		await api.patchAdminOrg(org.id, { sub_status: next });
		await load();
		patchingId = null;
	}

	let filtered = $derived(
		search.trim()
			? orgs.filter(
					(o) =>
						o.name.toLowerCase().includes(search.toLowerCase()) ||
						o.slug.toLowerCase().includes(search.toLowerCase())
				)
			: orgs
	);

	const PAGE = 25;
	let page = $state(0);
	let rows = $derived(filtered.slice(page * PAGE, (page + 1) * PAGE));
	let totalPages = $derived(Math.ceil(filtered.length / PAGE));
	$effect(() => { filtered; page = 0; });

	type Meta = { label: string; color: string; bg: string };

	function tierMeta(t: string | null): Meta {
		if (!t || t === 'free') return { label: 'Free',  color: 'var(--ok)',   bg: 'var(--ok-soft)' };
		if (t === 'pro')        return { label: 'Pro',   color: 'var(--accent)', bg: 'var(--accent-soft)' };
		                        return { label: 'Max',   color: '#a855f7',       bg: 'rgba(168,85,247,0.09)' };
	}

	function statusMeta(s: string | null): { label: string; dot: string; color: string } {
		if (s === 'active')    return { label: 'Active',    dot: 'var(--ok)',     color: 'var(--ok)' };
		if (s === 'suspended') return { label: 'Suspended', dot: 'var(--danger)', color: 'var(--danger)' };
		if (s === 'past_due')  return { label: 'Past due',  dot: 'var(--warn)',   color: 'var(--warn)' };
		                       return { label: s ?? 'Free', dot: 'var(--text-3)', color: 'var(--text-3)' };
	}

	const palette = ['#1c1c1c','#252525','#1a1f2e','#1e1a2e','#1a2420','#2a1a1a'];
	function orgColor(name: string): string {
		return palette[name.charCodeAt(0) % palette.length];
	}
	function initials(name: string): string {
		return name.split(/\s+/).slice(0,2).map(w => w[0]?.toUpperCase() ?? '').join('');
	}
</script>

<div class="p">
	<header class="hdr">
		<div class="hdr-l">
			<h1 class="ttl">Organizations</h1>
			<span class="pill">{orgs.length}</span>
		</div>
		<label class="search">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13">
				<path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/>
			</svg>
			<input type="text" placeholder="Search…" bind:value={search} />
		</label>
	</header>

	{#if loading}
		<div class="tbl">
			{#each [0,1,2,3] as _}
				<div class="sk-row">
					<div class="sk sk-ava"></div>
					<div style="display:flex;flex-direction:column;gap:6px">
						<div class="sk sk-l"></div>
						<div class="sk sk-xs"></div>
					</div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if rows.length === 0}
		<div class="empty">
			<svg viewBox="0 0 20 20" fill="currentColor" width="28" height="28"><path fill-rule="evenodd" d="M4 4a2 2 0 012-2h8a2 2 0 012 2v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4zm3 1h2v2H7V5zm2 4H7v2h2V9zm2-4h2v2h-2V5zm2 4h-2v2h2V9z" clip-rule="evenodd"/></svg>
			{search ? 'No results.' : 'No organizations yet.'}
		</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:2.2">Organization</span>
				<span style="flex:0.9">Tier</span>
				<span style="flex:1.1">Status</span>
				<span class="r" style="flex:0.7">Members</span>
				<span class="r" style="flex:0.7">Nodes</span>
				<span style="flex:1">Created</span>
				<span class="r" style="flex:1.4">Actions</span>
			</div>
			{#each rows as org}
				{@const tm = tierMeta(org.tier)}
				{@const sm = statusMeta(org.sub_status)}
				{@const oc = orgColor(org.name)}
				<div class="trow">
					<div class="org-c" style="flex:2.2">
						<div class="ava" style="background:{oc}">{initials(org.name)}</div>
						<div class="org-info">
							<span class="org-name">{org.name}</span>
							<span class="org-slug">{org.slug}</span>
						</div>
					</div>
					<div style="flex:0.9">
						<span class="chip" style="background:{tm.bg};color:{tm.color}">{tm.label}</span>
					</div>
					<div style="flex:1.1">
						<span class="status">
							<span class="dot" style="background:{sm.dot}"></span>
							<span style="color:{sm.color}">{sm.label}</span>
						</span>
					</div>
					<div class="n r" style="flex:0.7">{org.member_count}</div>
					<div class="n r" style="flex:0.7">{org.node_count}</div>
					<div class="d" style="flex:1">{new Date(org.created_at).toLocaleDateString()}</div>
					<div style="flex:1.4;display:flex;justify-content:flex-end;gap:6px">
						<a class="act act-open" href="/orgs/{org.slug}" target="_blank" rel="noopener">Open</a>
						<button
							class="act"
							class:act-danger={org.sub_status !== 'suspended'}
							class:act-ok={org.sub_status === 'suspended'}
							onclick={() => toggleSuspend(org)}
							disabled={patchingId === org.id}
						>
							{patchingId === org.id ? '…' : org.sub_status === 'suspended' ? 'Restore' : 'Suspend'}
						</button>
					</div>
				</div>
			{/each}
		</div>
		{#if totalPages > 1}
			<div class="pager">
				<button class="pg-btn" disabled={page === 0} onclick={() => page--}>Prev</button>
				<span class="pg-info">Page {page + 1} of {totalPages} &bull; {filtered.length} total</span>
				<button class="pg-btn" disabled={page >= totalPages - 1} onclick={() => page++}>Next</button>
			</div>
		{/if}
		<!-- Mobile cards -->
		<div class="card-list">
			{#each rows as org}
				{@const tm = tierMeta(org.tier)}
				{@const sm = statusMeta(org.sub_status)}
				{@const oc = orgColor(org.name)}
				<div class="m-card">
					<div class="m-card-hdr">
						<div class="ava" style="background:{oc}">{initials(org.name)}</div>
						<div class="org-info">
							<span class="org-name">{org.name}</span>
							<span class="org-slug">{org.slug}</span>
						</div>
					</div>
					<div class="m-card-row"><span class="m-key">Tier</span><span class="chip" style="background:{tm.bg};color:{tm.color}">{tm.label}</span></div>
					<div class="m-card-row"><span class="m-key">Status</span><span style="color:{sm.color}">{sm.label}</span></div>
					<div class="m-card-row"><span class="m-key">Members</span><span>{org.member_count}</span></div>
					<div class="m-card-row"><span class="m-key">Nodes</span><span>{org.node_count}</span></div>
					<div class="m-card-foot" style="gap:6px">
						<a class="act act-open" href="/orgs/{org.slug}" target="_blank" rel="noopener">Open</a>
						<button
							class="act"
							class:act-danger={org.sub_status !== 'suspended'}
							class:act-ok={org.sub_status === 'suspended'}
							onclick={() => toggleSuspend(org)}
							disabled={patchingId === org.id}
						>
							{patchingId === org.id ? '…' : org.sub_status === 'suspended' ? 'Restore' : 'Suspend'}
						</button>
					</div>
				</div>
			{/each}
			{#if totalPages > 1}
				<div class="pager">
					<button class="pg-btn" disabled={page === 0} onclick={() => page--}>Prev</button>
					<span class="pg-info">{page + 1} / {totalPages}</span>
					<button class="pg-btn" disabled={page >= totalPages - 1} onclick={() => page++}>Next</button>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.p { padding: 40px 36px; }

	/* Header */
	.hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:20px; gap:12px; }
	.hdr-l { display:flex; align-items:center; gap:8px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0; letter-spacing:-0.02em; }
	.pill {
		display:inline-flex; align-items:center; justify-content:center;
		height:20px; padding:0 7px; border-radius:999px;
		font-size:11px; font-weight:700;
		background:var(--surface-2); color:var(--text-3);
		border:1px solid var(--border);
	}

	/* Search */
	.search {
		position:relative; display:flex; align-items:center; cursor:text;
	}
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search input {
		height:32px; padding:0 10px 0 28px;
		background:var(--surface); border:1px solid var(--border);
		border-radius:var(--radius-sm); font-size:12.5px; color:var(--text);
		outline:none; width:190px;
		transition:border-color .15s, box-shadow .15s;
		font-family:var(--font);
	}
	.search input::placeholder { color:var(--text-3); }
	.search input:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	/* Skeleton */
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-ava { width:32px; height:32px; border-radius:8px; flex-shrink:0; }
	.sk-l { width:110px; height:12px; }
	.sk-xs { width:70px; height:10px; }
	.sk-row { display:flex; align-items:center; gap:10px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	/* Error / empty */
	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:56px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	/* Table */
	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.thead {
		display:flex; align-items:center; gap:10px; padding:9px 16px;
		background:var(--surface-2); border-bottom:1px solid var(--border);
		font-size:10.5px; font-weight:700; color:var(--text-3);
		text-transform:uppercase; letter-spacing:0.065em;
	}
	.trow {
		display:flex; align-items:center; gap:10px; padding:11px 16px;
		border-bottom:1px solid var(--border); transition:background .1s;
	}
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }

	/* Cells */
	.org-c { display:flex; align-items:center; gap:9px; min-width:0; }
	.ava {
		width:32px; height:32px; border-radius:8px; flex-shrink:0;
		display:flex; align-items:center; justify-content:center;
		font-size:10.5px; font-weight:800; color:rgba(255,255,255,0.7);
		letter-spacing:0.02em;
	}
	.org-info { display:flex; flex-direction:column; gap:1px; min-width:0; }
	.org-name { font-size:12.5px; font-weight:600; color:var(--text); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
	.org-slug { font-size:10.5px; color:var(--text-3); font-family:var(--mono); }
	.n { font-size:12.5px; font-variant-numeric:tabular-nums; color:var(--text-2); }
	.r { text-align:right; }
	.d { font-size:11.5px; color:var(--text-3); white-space:nowrap; }

	/* Chips */
	.chip {
		display:inline-flex; align-items:center; font-size:10.5px; font-weight:700;
		padding:2px 8px; border-radius:999px; white-space:nowrap;
		letter-spacing:0.01em;
	}
	.status { display:inline-flex; align-items:center; gap:5px; font-size:12px; font-weight:500; }
	.dot { width:6px; height:6px; border-radius:50%; flex-shrink:0; }

	/* Action buttons */
	.act {
		padding:4px 11px; height:28px; border-radius:var(--radius-sm);
		font-size:11.5px; font-weight:600; cursor:pointer;
		border:1px solid transparent; white-space:nowrap;
		transition:background .15s, border-color .15s, color .15s;
		font-family:var(--font);
	}
	.act:disabled { opacity:.45; cursor:not-allowed; }
	.act-danger { background:var(--danger-soft); color:var(--danger); border-color:rgba(220,38,38,0.18); }
	.act-danger:hover:not(:disabled) { background:rgba(220,38,38,0.14); border-color:rgba(220,38,38,0.32); }
	.act-ok { background:var(--ok-soft); color:var(--ok); border-color:rgba(22,163,74,0.18); }
	.act-ok:hover:not(:disabled) { background:rgba(22,163,74,0.14); border-color:rgba(22,163,74,0.32); }
	.act-open { background:var(--surface-2); color:var(--text-2); border-color:var(--border); text-decoration:none; display:inline-flex; align-items:center; }
	.act-open:hover { background:var(--accent); color:#000; border-color:var(--accent); }

	.pager { display:flex; align-items:center; gap:10px; padding:12px 0 4px; justify-content:center; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }

	.card-list { display:none; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-hdr { display:flex; align-items:center; gap:10px; margin-bottom:10px; }
	.m-card-row { display:flex; justify-content:space-between; align-items:center; padding:5px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); }
	.m-card-row:last-of-type { border-bottom:none; }
	.m-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; }
	.m-card-foot { padding-top:10px; display:flex; justify-content:flex-end; }

	@media (max-width: 1024px) {
		.p { padding:28px 20px; }
	}
	@media (max-width: 768px) {
		.p { padding:20px 14px; }
		.search input { width:140px; }
	}
	@media (max-width: 640px) {
		.p { padding:16px 12px; }
		.tbl { display:none; }
		.card-list { display:block; }
		.hdr { flex-direction:column; align-items:flex-start; }
		.search input { width:100%; }
	}
</style>
