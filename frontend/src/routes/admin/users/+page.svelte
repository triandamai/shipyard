<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AdminUser } from '$lib/api/types';

	let users = $state<AdminUser[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let patchingId = $state<string | null>(null);
	let search = $state('');

	onMount(() => load());

	async function load() {
		loading = true;
		const res = await api.getAdminUsers();
		if (res.data) users = res.data;
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	}

	async function toggleAdmin(user: AdminUser) {
		patchingId = user.id;
		await api.patchAdminUser(user.id, { is_superadmin: !user.is_superadmin });
		await load();
		patchingId = null;
	}

	async function toggleSuspend(user: AdminUser) {
		patchingId = user.id;
		await api.patchAdminUser(user.id, { is_suspended: !user.is_suspended });
		await load();
		patchingId = null;
	}

	let filtered = $derived(
		search.trim() ? users.filter(u => u.email.toLowerCase().includes(search.toLowerCase())) : users
	);
	let adminCount = $derived(users.filter(u => u.is_superadmin).length);

	const PAGE = 25;
	let page = $state(0);
	let rows = $derived(filtered.slice(page * PAGE, (page + 1) * PAGE));
	let totalPages = $derived(Math.ceil(filtered.length / PAGE));
	$effect(() => { filtered; page = 0; });

	const avaColors: [string, string][] = [
		['#1e1e1e','rgba(255,255,255,0.55)'],
		['#1c1f28','rgba(255,255,255,0.55)'],
		['#1a1e1a','rgba(255,255,255,0.55)'],
		['#201a1a','rgba(255,255,255,0.55)'],
		['#1a1a24','rgba(255,255,255,0.55)'],
	];
	function avaStyle(email: string): [string, string] {
		return avaColors[email.charCodeAt(0) % avaColors.length];
	}
</script>

<div class="p">
	<header class="hdr">
		<div class="hdr-l">
			<h1 class="ttl">Users</h1>
			<span class="pill">{users.length}</span>
			{#if adminCount > 0}
				<span class="admin-pill">{adminCount} admin{adminCount !== 1 ? 's' : ''}</span>
			{/if}
		</div>
		<label class="search">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13">
				<path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/>
			</svg>
			<input type="text" placeholder="Filter email…" bind:value={search} />
		</label>
	</header>

	{#if loading}
		<div class="tbl">
			{#each [0,1,2,3] as _}
				<div class="sk-row">
					<div class="sk sk-ava"></div>
					<div style="flex:1;display:flex;flex-direction:column;gap:6px">
						<div class="sk sk-l"></div>
						<div class="sk sk-xs"></div>
					</div>
					<div class="sk sk-chip"></div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if rows.length === 0}
		<div class="empty">
			<svg viewBox="0 0 20 20" fill="currentColor" width="28" height="28"><path fill-rule="evenodd" d="M10 9a3 3 0 100-6 3 3 0 000 6zm-7 9a7 7 0 1114 0H3z" clip-rule="evenodd"/></svg>
			{search ? 'No users match.' : 'No users found.'}
		</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:3">User</span>
				<span style="flex:1.1">Role</span>
				<span class="r" style="flex:0.7">Orgs</span>
				<span style="flex:1.1">Joined</span>
				<span class="r" style="flex:1.8">Actions</span>
			</div>
			{#each rows as user}
				{@const [bg, fg] = avaStyle(user.email)}
				<div class="trow">
					<div class="user-c" style="flex:3">
						<div class="ava" style="background:{bg};color:{fg}">{user.email[0].toUpperCase()}</div>
						<div class="user-info">
							<span class="user-email">{user.email}</span>
							<span class="user-id">{user.id.slice(0,8)}…</span>
						</div>
					</div>
					<div style="flex:1.1">
						{#if user.is_superadmin}
							<span class="role-admin">Superadmin</span>
						{:else}
							<span class="role-user">User</span>
						{/if}
					</div>
					<div class="n r" style="flex:0.7">{user.org_count}</div>
					<div class="d" style="flex:1.1">{new Date(user.created_at).toLocaleDateString()}</div>
					<div style="flex:1.8;display:flex;justify-content:flex-end;gap:6px">
						<button
							class="act"
							class:act-warn={!user.is_suspended}
							class:act-ok={user.is_suspended}
							onclick={() => toggleSuspend(user)}
							disabled={patchingId === user.id}
						>
							{patchingId === user.id ? '…' : user.is_suspended ? 'Unsuspend' : 'Suspend'}
						</button>
						<button
							class="act"
							class:act-primary={!user.is_superadmin}
							class:act-danger={user.is_superadmin}
							onclick={() => toggleAdmin(user)}
							disabled={patchingId === user.id}
						>
							{patchingId === user.id ? '…' : user.is_superadmin ? 'Revoke' : 'Grant Admin'}
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
			{#each rows as user}
				{@const [bg, fg] = avaStyle(user.email)}
				<div class="m-card">
					<div class="m-card-hdr">
						<div class="ava" style="background:{bg};color:{fg}">{user.email[0].toUpperCase()}</div>
						<div class="user-info">
							<span class="user-email">{user.email}</span>
							<span class="user-id">{user.id.slice(0,8)}…</span>
						</div>
					</div>
					<div class="m-card-row">
						<span class="m-key">Role</span>
						{#if user.is_superadmin}<span class="role-admin">Superadmin</span>{:else}<span class="role-user">User</span>{/if}
					</div>
					<div class="m-card-row"><span class="m-key">Orgs</span><span>{user.org_count}</span></div>
					<div class="m-card-row"><span class="m-key">Joined</span><span>{new Date(user.created_at).toLocaleDateString()}</span></div>
					<div class="m-card-foot">
						<button
							class="act"
							class:act-warn={!user.is_suspended}
							class:act-ok={user.is_suspended}
							onclick={() => toggleSuspend(user)}
							disabled={patchingId === user.id}
						>
							{patchingId === user.id ? '…' : user.is_suspended ? 'Unsuspend' : 'Suspend'}
						</button>
						<button
							class="act"
							class:act-primary={!user.is_superadmin}
							class:act-danger={user.is_superadmin}
							onclick={() => toggleAdmin(user)}
							disabled={patchingId === user.id}
						>
							{patchingId === user.id ? '…' : user.is_superadmin ? 'Revoke' : 'Grant Admin'}
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
	.p { padding:40px 36px; }

	.hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:20px; gap:12px; flex-wrap:wrap; }
	.hdr-l { display:flex; align-items:center; gap:8px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0; letter-spacing:-0.02em; }
	.pill { display:inline-flex; align-items:center; justify-content:center; height:20px; padding:0 7px; border-radius:999px; font-size:11px; font-weight:700; background:var(--surface-2); color:var(--text-3); border:1px solid var(--border); }
	.admin-pill { display:inline-flex; align-items:center; height:20px; padding:0 8px; border-radius:999px; font-size:11px; font-weight:600; background:var(--danger-soft); color:var(--danger); border:1px solid rgba(220,38,38,0.2); }

	.search { position:relative; display:flex; align-items:center; cursor:text; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search input { height:32px; padding:0 10px 0 28px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:200px; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search input::placeholder { color:var(--text-3); }
	.search input:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-ava { width:32px; height:32px; border-radius:8px; flex-shrink:0; }
	.sk-l { width:150px; height:12px; }
	.sk-xs { width:80px; height:10px; }
	.sk-chip { width:72px; height:20px; border-radius:999px; }
	.sk-row { display:flex; align-items:center; gap:10px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { display:flex; flex-direction:column; align-items:center; justify-content:center; gap:10px; padding:56px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:0.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:11px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }

	.user-c { display:flex; align-items:center; gap:9px; min-width:0; }
	.ava { width:32px; height:32px; border-radius:8px; flex-shrink:0; display:flex; align-items:center; justify-content:center; font-size:12px; font-weight:800; }
	.user-info { display:flex; flex-direction:column; gap:1px; min-width:0; }
	.user-email { font-size:12.5px; font-weight:500; color:var(--text); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
	.user-id { font-size:10px; color:var(--text-3); font-family:var(--mono); }
	.n { font-size:12.5px; font-variant-numeric:tabular-nums; color:var(--text-2); }
	.r { text-align:right; }
	.d { font-size:11.5px; color:var(--text-3); white-space:nowrap; }

	.role-admin { display:inline-flex; align-items:center; gap:4px; font-size:10.5px; font-weight:700; padding:2px 8px; border-radius:999px; background:var(--danger-soft); color:var(--danger); border:1px solid rgba(220,38,38,0.18); }
	.role-user { display:inline-flex; align-items:center; font-size:10.5px; font-weight:600; padding:2px 8px; border-radius:999px; background:var(--surface-2); color:var(--text-3); border:1px solid var(--border); }

	.act { padding:4px 11px; height:28px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:600; cursor:pointer; border:1px solid transparent; white-space:nowrap; transition:background .15s, border-color .15s, color .15s; font-family:var(--font); }
	.act:disabled { opacity:.45; cursor:not-allowed; }
	.act-primary { background:var(--accent-soft); color:var(--accent); border-color:var(--accent-ring); }
	.act-primary:hover:not(:disabled) { background:var(--accent); color:#000; border-color:var(--accent); }
	.act-danger { background:var(--danger-soft); color:var(--danger); border-color:rgba(220,38,38,0.18); }
	.act-danger:hover:not(:disabled) { background:rgba(220,38,38,0.14); border-color:rgba(220,38,38,0.32); }
	.act-warn { background:var(--warn-soft); color:var(--warn); border-color:rgba(180,83,9,0.18); }
	.act-warn:hover:not(:disabled) { background:rgba(180,83,9,0.14); border-color:rgba(180,83,9,0.32); }
	.act-ok { background:var(--ok-soft); color:var(--ok); border-color:rgba(22,163,74,0.18); }
	.act-ok:hover:not(:disabled) { background:rgba(22,163,74,0.14); border-color:rgba(22,163,74,0.32); }

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
	.m-card-foot { padding-top:10px; display:flex; justify-content:flex-end; gap:6px; }

	@media (max-width: 1024px) {
		.p { padding:28px 20px; }
	}
	@media (max-width: 768px) {
		.p { padding:20px 14px; }
	}
	@media (max-width: 640px) {
		.p { padding:16px 12px; }
		.tbl { display:none; }
		.card-list { display:block; }
		.hdr { flex-direction:column; align-items:flex-start; }
	}
</style>
