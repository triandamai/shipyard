<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface StaffUser { id: string; email: string; staff_permissions: string[]; created_at: string; }

	let staff = $state<StaffUser[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let revokingId = $state<string | null>(null);
	let search = $state('');
	let showGrant = $state(false);
	let grantEmail = $state('');
	let grantPerms = $state<string[]>([]);
	let granting = $state(false);
	let grantError = $state('');

	const PERM_GROUPS = [
		{ label: 'Organizations', perms: [
			{ id: 'shipyard:admin:organization:view',   label: 'View' },
			{ id: 'shipyard:admin:organization:manage', label: 'Manage' },
		]},
		{ label: 'Users', perms: [
			{ id: 'shipyard:admin:users:view',   label: 'View' },
			{ id: 'shipyard:admin:users:manage', label: 'Manage' },
		]},
		{ label: 'Staff', perms: [
			{ id: 'shipyard:admin:staff:view',   label: 'View' },
			{ id: 'shipyard:admin:staff:manage', label: 'Manage' },
		]},
		{ label: 'Projects', perms: [
			{ id: 'shipyard:admin:projects:view',   label: 'View' },
			{ id: 'shipyard:admin:projects:manage', label: 'Manage' },
		]},
		{ label: 'Deployments', perms: [
			{ id: 'shipyard:deployments:projects:view',   label: 'View' },
			{ id: 'shipyard:deployments:projects:manage', label: 'Manage' },
		]},
		{ label: 'Provisioning', perms: [
			{ id: 'shipyard:deployments:orgs:view',   label: 'View' },
			{ id: 'shipyard:deployments:orgs:manage', label: 'Manage' },
		]},
		{ label: 'Nodes', perms: [
			{ id: 'shipyard:admin:nodes:view',   label: 'View' },
			{ id: 'shipyard:admin:nodes:manage', label: 'Manage' },
		]},
		{ label: 'Infrastructure', perms: [
			{ id: 'shipyard:admin:infra:view',   label: 'View' },
			{ id: 'shipyard:admin:infra:manage', label: 'Manage' },
		]},
		{ label: 'Docker', perms: [
			{ id: 'shipyard:admin:infra:docker:view',   label: 'View' },
			{ id: 'shipyard:admin:infra:docker:manage', label: 'Manage' },
		]},
		{ label: 'Traefik', perms: [
			{ id: 'shipyard:admin:infra:traefik:view',   label: 'View' },
			{ id: 'shipyard:admin:infra:traefik:manage', label: 'Manage' },
		]},
		{ label: 'MQTT', perms: [
			{ id: 'shipyard:admin:infra:mqtt:view',   label: 'View' },
			{ id: 'shipyard:admin:infra:mqtt:manage', label: 'Manage' },
		]},
		{ label: 'Static Sites', perms: [
			{ id: 'shipyard:admin:infra:static:view',   label: 'View' },
			{ id: 'shipyard:admin:infra:static:manage', label: 'Manage' },
		]},
		{ label: 'SMTP', perms: [
			{ id: 'shipyard:admin:smtp:view',   label: 'View' },
			{ id: 'shipyard:admin:smtp:manage', label: 'Manage' },
		]},
		{ label: 'Database (Postgres)', perms: [
			{ id: 'shipyard:admin:infra:postgres:view',   label: 'View' },
			{ id: 'shipyard:admin:infra:postgres:manage', label: 'Manage' },
		]},
		{ label: 'Database (Redis)', perms: [
			{ id: 'shipyard:admin:infra:redis:view',   label: 'View' },
			{ id: 'shipyard:admin:infra:redis:manage', label: 'Manage' },
		]},
		{ label: 'Audit Log', perms: [
			{ id: 'shipyard:admin:audit:view',   label: 'View' },
			{ id: 'shipyard:admin:audit:manage', label: 'Manage' },
		]},
		{ label: 'Plans', perms: [
			{ id: 'shipyard:admin:plan:view',   label: 'View' },
			{ id: 'shipyard:admin:plan:manage', label: 'Manage' },
		]},
		{ label: 'Updates', perms: [
			{ id: 'shipyard:admin:system:update:view',   label: 'View' },
			{ id: 'shipyard:admin:system:update:manage', label: 'Manage' },
		]},
		{ label: 'Config', perms: [
			{ id: 'shipyard:admin:system:config:view',   label: 'View' },
			{ id: 'shipyard:admin:system:config:manage', label: 'Manage' },
		]},
	];

	onMount(() => load());

	async function load() {
		loading = true;
		const res = await api.get<StaffUser[]>('/admin/staff');
		if (res.data) staff = Array.isArray(res.data) ? res.data : [];
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	}

	let filtered = $derived(
		search.trim()
			? staff.filter(u => u.email.toLowerCase().includes(search.toLowerCase()))
			: staff
	);

	async function revoke(user: StaffUser) {
		if (!confirm(`Remove staff access from ${user.email}?`)) return;
		revokingId = user.id;
		await api.post(`/admin/staff/${user.id}/revoke`, {});
		await load();
		revokingId = null;
	}

	async function grantAdmin() {
		granting = true;
		grantError = '';
		const res = await api.post('/admin/staff/grant', {
			email: grantEmail,
			permissions: grantPerms,
		});
		if (res.error) {
			grantError = res.error.message;
		} else {
			showGrant = false;
			grantEmail = '';
			grantPerms = [];
			await load();
		}
		granting = false;
	}

	function togglePerm(id: string) {
		if (grantPerms.includes(id)) grantPerms = grantPerms.filter(p => p !== id);
		else grantPerms = [...grantPerms, id];
	}

	function permLabel(id: string): string {
		for (const g of PERM_GROUPS) {
			for (const p of g.perms) {
				if (p.id === id) return `${g.label}: ${p.label}`;
			}
		}
		return id;
	}

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
			<h1 class="ttl">Staff</h1>
			<span class="pill">{staff.length}</span>
		</div>
		<div style="display:flex;align-items:center;gap:8px">
			<label class="search">
				<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13">
					<path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/>
				</svg>
				<input type="text" placeholder="Filter email…" bind:value={search} />
			</label>
			<button class="add-btn" onclick={() => (showGrant = true)}>+ Add Staff</button>
		</div>
	</header>

	{#if loading}
		<div class="tbl">
			{#each [0,1,2] as _}
				<div class="sk-row">
					<div class="sk sk-ava"></div>
					<div style="flex:1;display:flex;flex-direction:column;gap:6px">
						<div class="sk sk-l"></div>
						<div class="sk sk-xs"></div>
					</div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if filtered.length === 0}
		<div class="empty">
			<svg viewBox="0 0 20 20" fill="currentColor" width="28" height="28"><path fill-rule="evenodd" d="M10 9a3 3 0 100-6 3 3 0 000 6zm-7 9a7 7 0 1114 0H3z" clip-rule="evenodd"/></svg>
			{search ? 'No staff match.' : 'No staff members yet. Add one above.'}
		</div>
	{:else}
		<div class="tbl">
			<div class="thead">
				<span style="flex:3">User</span>
				<span style="flex:3">Permissions</span>
				<span style="flex:1.1">Joined</span>
				<span class="r" style="flex:1">Action</span>
			</div>
			{#each filtered as user}
				{@const [bg, fg] = avaStyle(user.email)}
				<div class="trow">
					<div class="user-c" style="flex:3">
						<div class="ava" style="background:{bg};color:{fg}">{user.email[0].toUpperCase()}</div>
						<div class="user-info">
							<span class="user-email">{user.email}</span>
							<span class="user-id">{user.id.slice(0,8)}…</span>
						</div>
					</div>
					<div style="flex:3;display:flex;flex-wrap:wrap;gap:4px;align-items:center">
						{#each user.staff_permissions.slice(0, 3) as p}
							<span class="perm-chip">{permLabel(p)}</span>
						{/each}
						{#if user.staff_permissions.length > 3}
							<span class="perm-more">+{user.staff_permissions.length - 3} more</span>
						{/if}
					</div>
					<div class="d" style="flex:1.1">{new Date(user.created_at).toLocaleDateString()}</div>
					<div style="flex:1;display:flex;justify-content:flex-end">
						<button class="act act-danger" onclick={() => revoke(user)} disabled={revokingId === user.id}>
							{revokingId === user.id ? '…' : 'Revoke'}
						</button>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

{#if showGrant}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-bg" onclick={() => (showGrant = false)}>
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="modal" onclick={(e) => e.stopPropagation()}>
			<div class="modal-hdr">
				<h2 class="modal-title">Promote to Admin</h2>
				<button class="modal-close" onclick={() => (showGrant = false)}>✕</button>
			</div>
			<div class="modal-body">
				<div class="field">
					<label class="lbl">User Email</label>
					<input class="inp" type="email" placeholder="user@example.com" bind:value={grantEmail} />
				</div>
				<div class="field">
					<label class="lbl">Permissions</label>
					<div class="perm-groups">
						{#each PERM_GROUPS as group}
							<div class="perm-group">
								<span class="perm-group-label">{group.label}</span>
								<div class="perm-row">
									{#each group.perms as p}
										<label class="perm-item">
											<input type="checkbox" checked={grantPerms.includes(p.id)} onchange={() => togglePerm(p.id)} />
											<span>{p.label}</span>
										</label>
									{/each}
								</div>
							</div>
						{/each}
					</div>
				</div>
				{#if grantError}
					<div class="err" style="margin-top:8px">{grantError}</div>
				{/if}
			</div>
			<div class="modal-foot">
				{#if grantEmail && grantPerms.length === 0}
					<span class="perm-hint">Select at least one permission</span>
				{/if}
				<button class="btn-cancel" onclick={() => (showGrant = false)}>Cancel</button>
				<button class="btn-confirm" onclick={grantAdmin} disabled={granting || !grantEmail || grantPerms.length === 0}>
					{granting ? 'Granting…' : 'Grant Admin Access'}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.p { max-width:900px; margin:0 auto; padding:40px 36px; }

	.hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:20px; gap:12px; flex-wrap:wrap; }
	.hdr-l { display:flex; align-items:center; gap:8px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0; letter-spacing:-0.02em; }
	.pill { display:inline-flex; align-items:center; justify-content:center; height:20px; padding:0 7px; border-radius:999px; font-size:11px; font-weight:700; background:var(--surface-2); color:var(--text-3); border:1px solid var(--border); }

	.search { position:relative; display:flex; align-items:center; cursor:text; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search input { height:32px; padding:0 10px 0 28px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:190px; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search input::placeholder { color:var(--text-3); }
	.search input:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.add-btn { padding:6px 14px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; font-family:var(--font); }
	.add-btn:hover { opacity:.88; }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-ava { width:32px; height:32px; border-radius:8px; flex-shrink:0; }
	.sk-l { width:150px; height:12px; }
	.sk-xs { width:80px; height:10px; }
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
	.d { font-size:11.5px; color:var(--text-3); white-space:nowrap; }
	.r { text-align:right; }

	.role-admin { display:inline-flex; align-items:center; font-size:10.5px; font-weight:700; padding:2px 8px; border-radius:999px; background:var(--danger-soft); color:var(--danger); border:1px solid rgba(220,38,38,0.18); }

	.act { padding:4px 11px; height:28px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:600; cursor:pointer; border:1px solid transparent; white-space:nowrap; transition:background .15s; font-family:var(--font); }
	.act:disabled { opacity:.45; cursor:not-allowed; }
	.act-danger { background:var(--danger-soft); color:var(--danger); border-color:rgba(220,38,38,0.18); }
	.act-danger:hover:not(:disabled) { background:rgba(220,38,38,0.14); border-color:rgba(220,38,38,0.32); }

	/* Modal */
	.modal-bg { position:fixed; inset:0; z-index:200; background:rgba(0,0,0,0.45); display:flex; align-items:center; justify-content:center; padding:20px; }
	.modal { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); box-shadow:var(--shadow-md); width:100%; max-width:460px; display:flex; flex-direction:column; }
	.modal-hdr { display:flex; align-items:center; justify-content:space-between; padding:16px 20px; border-bottom:1px solid var(--border); }
	.modal-title { font-size:14px; font-weight:700; color:var(--text); margin:0; }
	.modal-close { background:none; border:none; color:var(--text-3); font-size:14px; cursor:pointer; padding:4px; border-radius:4px; }
	.modal-close:hover { color:var(--text); background:var(--surface-2); }
	.modal-body { padding:20px; display:flex; flex-direction:column; gap:16px; }
	.modal-foot { display:flex; justify-content:flex-end; gap:8px; padding:14px 20px; border-top:1px solid var(--border); background:var(--surface-2); }

	.field { display:flex; flex-direction:column; gap:5px; }
	.lbl { font-size:11.5px; font-weight:600; color:var(--text-2); }
	.inp { height:34px; padding:0 10px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:100%; box-sizing:border-box; font-family:var(--font); transition:border-color .15s; }
	.inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.perm-groups { display:flex; flex-direction:column; gap:10px; max-height:360px; overflow-y:auto; }
	.perm-group { display:flex; flex-direction:column; gap:4px; }
	.perm-group-label { font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.06em; }
	.perm-row { display:flex; gap:16px; }
	.perm-item { display:flex; align-items:center; gap:7px; font-size:12.5px; color:var(--text-2); cursor:pointer; }
	.perm-item input { width:14px; height:14px; accent-color:var(--accent); cursor:pointer; }
	.perm-chip { display:inline-flex; padding:1px 7px; border-radius:999px; font-size:10px; font-weight:600; background:var(--accent-soft); color:var(--accent); border:1px solid var(--accent-ring); white-space:nowrap; }
	.perm-more { font-size:10px; color:var(--text-3); }

	.perm-hint { font-size:11px; color:var(--text-3); flex:1; display:flex; align-items:center; }
	.btn-cancel { padding:6px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); }
	.btn-cancel:hover { background:var(--surface-2); }
	.btn-confirm { padding:6px 16px; border-radius:var(--radius-sm); font-size:12px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; font-family:var(--font); }
	.btn-confirm:hover:not(:disabled) { opacity:.88; }
	.btn-confirm:disabled { opacity:.5; cursor:not-allowed; }
</style>
