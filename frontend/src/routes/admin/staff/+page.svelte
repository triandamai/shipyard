<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import {
		DataTable,
		Avatar,
		Badge,
		Button,
		Modal,
		FormField,
		TextField,
		Checkbox,
		PageHeader,
		InlineAlert,
		ConfirmDialog
	} from '$lib/components/ui';

	interface StaffUser { id: string; email: string; staff_permissions: string[]; created_at: string; }

	let staff = $state<StaffUser[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let revokingId = $state<string | null>(null);

	// ── Revoke confirm ───────────────────────────────────────────────────────
	let revokeConfirmOpen = $state(false);
	let revokeTarget = $state<StaffUser | null>(null);

	// ── Promote-to-admin modal ───────────────────────────────────────────────
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
		{ label: 'Registry', perms: [
			{ id: 'shipyard:admin:registry:view',   label: 'View' },
			{ id: 'shipyard:admin:registry:manage', label: 'Manage' },
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

	function requestRevoke(user: StaffUser) {
		revokeTarget = user;
		revokeConfirmOpen = true;
	}

	// Modal can also close itself (Escape key / backdrop click) without going
	// through a confirm/cancel handler — keep revokeTarget in sync either way.
	$effect(() => {
		if (!revokeConfirmOpen && revokeTarget) revokeTarget = null;
	});

	async function confirmRevoke() {
		if (!revokeTarget) return;
		revokingId = revokeTarget.id;
		await api.post(`/admin/staff/${revokeTarget.id}/revoke`, {});
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

	// Same deterministic index pattern as the Users page (Task 40): email
	// charCodeAt(0) % length picks one of Avatar's fixed `tone` values.
	const avaTones: Array<'blue' | 'green' | 'red' | 'yellow' | 'purple'> = ['blue', 'purple', 'green', 'yellow', 'red'];
	function avaTone(email: string): 'blue' | 'green' | 'red' | 'yellow' | 'purple' {
		return avaTones[email.charCodeAt(0) % avaTones.length];
	}
</script>

<PageHeader title="Staff">
	{#snippet actions()}
		<Badge tone="neutral">{staff.length} total</Badge>
		<Button size="sm" onclick={() => (showGrant = true)}>+ Add Staff</Button>
	{/snippet}
</PageHeader>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={staff}
		rowKey={(u) => u.id}
		searchFields={['email']}
		columns={[
			{ key: 'user', label: 'User', width: '28%' },
			{ key: 'permissions', label: 'Permissions', width: '36%' },
			{ key: 'joined', label: 'Joined', width: '14%' },
			{ key: 'actions', label: 'Action', width: '22%' }
		]}
		emptyMessage="No staff members yet. Add one above."
	>
		{#snippet row(user)}
			<tr>
				<td>
					<div class="st-user-cell">
						<Avatar initials={user.email[0]} tone={avaTone(user.email)} size={32} />
						<div class="st-user-info">
							<span class="st-email">{user.email}</span>
							<span class="st-id">{user.id.slice(0, 8)}…</span>
						</div>
					</div>
				</td>
				<td>
					<div class="st-perms">
						{#each user.staff_permissions.slice(0, 3) as p}
							<Badge tone="blue">{permLabel(p)}</Badge>
						{/each}
						{#if user.staff_permissions.length > 3}
							<Badge tone="neutral">+{user.staff_permissions.length - 3} more</Badge>
						{/if}
					</div>
				</td>
				<td class="st-date">{new Date(user.created_at).toLocaleDateString()}</td>
				<td>
					<div class="st-actions">
						<Button
							variant="danger-outline"
							size="sm"
							disabled={revokingId === user.id}
							onclick={() => requestRevoke(user)}
						>
							{revokingId === user.id ? '…' : 'Revoke'}
						</Button>
					</div>
				</td>
			</tr>
		{/snippet}
	</DataTable>
{/if}

<ConfirmDialog
	bind:open={revokeConfirmOpen}
	title="Remove staff access"
	message={revokeTarget ? `Remove staff access from ${revokeTarget.email}?` : ''}
	confirmLabel="Revoke"
	onConfirm={confirmRevoke}
/>

<!-- ── Promote to Admin Modal ──────────────────────────────────────────────── -->
<Modal bind:open={showGrant} title="Promote to Admin">
	{#snippet children()}
		<FormField label="User Email" for="st-grant-email">
			<TextField id="st-grant-email" type="email" placeholder="user@example.com" bind:value={grantEmail} />
		</FormField>
		<div class="st-perm-groups">
			{#each PERM_GROUPS as group}
				<div class="st-perm-group">
					<span class="st-perm-group-label">{group.label}</span>
					<div class="st-perm-row">
						{#each group.perms as p}
							<Checkbox
								label={p.label}
								bind:checked={() => grantPerms.includes(p.id), () => togglePerm(p.id)}
							/>
						{/each}
					</div>
				</div>
			{/each}
		</div>
		{#if grantError}
			<InlineAlert tone="error">{grantError}</InlineAlert>
		{/if}
	{/snippet}
	{#snippet footer()}
		{#if grantEmail && grantPerms.length === 0}
			<span class="st-perm-hint">Select at least one permission</span>
		{/if}
		<Button variant="secondary" size="sm" onclick={() => (showGrant = false)}>Cancel</Button>
		<Button size="sm" onclick={grantAdmin} disabled={granting || !grantEmail || grantPerms.length === 0}>
			{granting ? 'Granting…' : 'Grant Admin Access'}
		</Button>
	{/snippet}
</Modal>

<style>
	.st-user-cell { display: flex; align-items: center; gap: 9px; min-width: 0; }
	.st-user-info { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
	.st-email { font-size: 12.5px; font-weight: 500; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
	.st-id { font-size: 10px; color: var(--text-dim); font-family: var(--font-mono); }
	.st-date { font-size: 11.5px; color: var(--text-dim); white-space: nowrap; }
	.st-perms { display: flex; flex-wrap: wrap; gap: 4px; align-items: center; }
	.st-actions { display: flex; justify-content: flex-end; gap: 6px; }

	/* Promote-to-admin modal: same grouped (one heading per permission
	   category, two Checkboxes per row) layout the page always used — only
	   rebuilt on Modal + Checkbox (Task 43). */
	.st-perm-groups { display: flex; flex-direction: column; gap: 10px; max-height: 360px; overflow-y: auto; }
	.st-perm-group { display: flex; flex-direction: column; gap: 6px; }
	.st-perm-group-label { font-size: 10.5px; font-weight: 700; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
	.st-perm-row { display: flex; gap: 16px; }
	.st-perm-hint { font-size: 11px; color: var(--text-dim); flex: 1; display: flex; align-items: center; }
</style>
