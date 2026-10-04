<script lang="ts">
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import {
		UserPlus, Shield, Trash2, Crown, Eye,
		X, Clock, SlidersHorizontal,
		Link, CheckCheck, Folder
	} from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Avatar from '$lib/components/ui/Avatar.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import { formatDistanceToNow, isPast } from 'date-fns';
	import type { OrgMember, MemberRole, Invitation, Project } from '$lib/api/types';
	import SlidePanel from '$lib/components/SlidePanel.svelte';
	import MemberManagePanel from '$lib/panels/MemberManagePanel.svelte';
	import InvitePanel from '$lib/panels/InvitePanel.svelte';
	import { eventBus } from '$lib/mqtt/eventBus';
	import type { MqttPayload } from '$lib/api/types';

	let orgId         = $derived($orgStore.activeOrg?.id ?? '');
	let currentUserId = $derived($authStore.user?.id ?? '');

	// Permission check using the already-loaded membership from the org layout.
	let myMembership    = $derived($orgStore.myMembership);
	let membershipRole  = $derived(myMembership?.role ?? null);
	let membershipPerms = $derived(myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	// read, invite, or manage all imply the user can see the member list.
	let canViewMembers = $derived(
		can(membershipRole, membershipPerms, perm(orgId, 'members', 'read')) ||
		can(membershipRole, membershipPerms, perm(orgId, 'members', 'invite')) ||
		can(membershipRole, membershipPerms, perm(orgId, 'members', 'manage'))
	);

	// ── Members ───────────────────────────────────────────────────────
	let members        = $state<OrgMember[]>([]);
	let loadingMembers = $state(true);
	let membersError   = $state('');

	let myRole    = $derived(members.find(m => m.user_id === currentUserId)?.role ?? membershipRole ?? 'viewer');
	let canManage = $derived(myRole === 'owner' || myRole === 'admin');
	let isOwner   = $derived(myRole === 'owner');
	// Invite button is visible for admins/owners AND members with explicit members:invite perm.
	let canInvite = $derived(can(membershipRole, membershipPerms, perm(orgId, 'members', 'invite')));

	// ── Projects (for project-assignment picker) ──────────────────────
	let projects        = $state<Project[]>([]);
	let loadingProjects = $state(false);

	// ── Invitations ───────────────────────────────────────────────────
	let invitations      = $state<Invitation[]>([]);
	let loadingInvites   = $state(false);
	let cancellingInvite = $state('');
	let copiedInviteId   = $state('');

	// ── Invite panel ──────────────────────────────────────────────────
	let showInvitePanel = $state(false);

	// ── Member manage panel ───────────────────────────────────────────
	let panelMember = $state<OrgMember | null>(null);

	// ── Role change / remove trackers ─────────────────────────────────
	let changingRoleFor = $state('');
	let removingMember  = $state('');

	// ── ROLES table ───────────────────────────────────────────────────
	const ROLES: { value: MemberRole; label: string; desc: string }[] = [
		{ value: 'owner',  label: 'Owner',  desc: 'Full control, can manage billing and delete org' },
		{ value: 'admin',  label: 'Admin',  desc: 'Manage members, projects, and settings' },
		{ value: 'member', label: 'Member', desc: 'Deploy and manage services in projects' },
		{ value: 'viewer', label: 'Viewer', desc: 'Read-only access to all resources' },
	];

	function roleLabel(role: string) {
		return ROLES.find(r => r.value === role)?.label ?? role;
	}

	function roleTone(role: string): 'yellow' | 'blue' | 'green' | 'neutral' {
		switch (role) {
			case 'owner':  return 'yellow';
			case 'admin':  return 'blue';
			case 'member': return 'green';
			default:       return 'neutral';
		}
	}

	function assignableRoles(): MemberRole[] {
		return isOwner ? ['owner', 'admin', 'member', 'viewer'] : ['admin', 'member', 'viewer'];
	}

	function canChangeRole(target: OrgMember): boolean {
		if (!canManage) return false;
		if (target.user_id === currentUserId) return false;
		if (target.role === 'owner' && !isOwner) return false;
		return true;
	}

	function canRemove(target: OrgMember): boolean {
		if (target.user_id === currentUserId) return false;
		if (!canManage) return false;
		if (target.role === 'owner' && !isOwner) return false;
		return true;
	}

	function formatTime(ts: string) {
		try { return formatDistanceToNow(new Date(ts), { addSuffix: true }); }
		catch { return ts; }
	}

	function expiresLabel(ts: string) {
		try {
			const d = new Date(ts);
			return isPast(d) ? 'Expired' : `Expires ${formatDistanceToNow(d, { addSuffix: true })}`;
		} catch { return ts; }
	}

	// ── Data loading ──────────────────────────────────────────────────
	async function loadMembers(id = orgId) {
		if (!id) return;
		loadingMembers = true;
		membersError = '';
		const res = await api.getMembers(id);
		if (res.data) {
			members = res.data;
			const myMember = res.data.find(m => m.user_id === currentUserId);
			const role = myMember?.role ?? 'viewer';
			if (role === 'owner' || role === 'admin' || canInvite) {
				loadInvitations(id);
				loadProjects(id);
			}
		} else if (res.error) {
			membersError = res.error.message;
		}
		loadingMembers = false;
	}

	async function loadInvitations(id = orgId) {
		if (!id) return;
		loadingInvites = true;
		const res = await api.getInvitations(id);
		if (res.data) invitations = res.data;
		loadingInvites = false;
	}

	async function loadProjects(id = orgId) {
		if (!id) return;
		loadingProjects = true;
		const res = await api.getProjects(id);
		if (res.data) projects = res.data;
		loadingProjects = false;
	}

	// ── Invitation actions ────────────────────────────────────────────
	async function copyInviteLink(inv: Invitation) {
		const link = `${window.location.origin}/accept-invite/${inv.token}`;
		await navigator.clipboard.writeText(link);
		copiedInviteId = inv.id;
		setTimeout(() => { if (copiedInviteId === inv.id) copiedInviteId = ''; }, 2000);
	}

	async function cancelInvite(inv: Invitation) {
		cancellingInvite = inv.id;
		const res = await api.cancelInvitation(orgId, inv.id);
		if (!res.error) {
			invitations = invitations.filter(i => i.id !== inv.id);
		}
		cancellingInvite = '';
	}

	// ── Member actions ────────────────────────────────────────────────
	async function handleRoleChange(member: OrgMember, newRole: MemberRole) {
		if (changingRoleFor) return;
		changingRoleFor = member.user_id;
		const res = await api.changeMemberRole(orgId, member.user_id, newRole);
		if (res.data) {
			members = members.map(m => m.user_id === member.user_id ? { ...m, role: newRole } : m);
		}
		changingRoleFor = '';
	}

	async function handleRemove(member: OrgMember) {
		if (removingMember) return;
		removingMember = member.user_id;
		const res = await api.removeMember(orgId, member.user_id);
		if (!res.error) {
			members = members.filter(m => m.user_id !== member.user_id);
		}
		removingMember = '';
	}

	// ── Member manage panel ───────────────────────────────────────────
	function openMemberPanel(member: OrgMember) {
		panelMember = member;
	}

	function closeMemberPanel() {
		panelMember = null;
	}

	function handleRoleChangedFromPanel(userId: string, newRole: MemberRole) {
		members = members.map(m => m.user_id === userId ? { ...m, role: newRole } : m);
	}

	function handlePermsChangedFromPanel(userId: string, perms: string[]) {
		members = members.map(m => m.user_id === userId ? { ...m, permissions: perms } : m);
	}

	// Wait for membership to be loaded and confirmed before fetching the list.
	$effect(() => {
		if (orgId && membershipLoaded && canViewMembers) loadMembers(orgId);
		else if (membershipLoaded && !canViewMembers) loadingMembers = false;
	});

	// Live updates via MQTT — react to member/invitation changes pushed by the server.
	$effect(() => {
		const id = orgId;
		if (!id) return;

		const memberTopic = `platform/orgs/${id}/members`;

		const handler = (topic: string, payload: MqttPayload) => {
			if (topic !== memberTopic) return;

			switch (payload.event) {
				case 'org.member.joined':
					// New member accepted invite — full reload to get email etc.
					loadMembers(id);
					loadInvitations(id);
					break;

				case 'org.member.removed': {
					const uid = payload.meta?.user_id as string | undefined;
					if (uid) members = members.filter(m => m.user_id !== uid);
					break;
				}

				case 'org.member.updated': {
					const uid  = payload.meta?.user_id as string | undefined;
					const role = payload.meta?.role as string | undefined;
					const perms = payload.meta?.permissions as string[] | undefined;
					if (!uid) break;
					members = members.map(m => {
						if (m.user_id !== uid) return m;
						return {
							...m,
							...(role  ? { role: role as MemberRole } : {}),
							...(perms ? { permissions: perms }       : {}),
						};
					});
					break;
				}

				case 'org.invitation.sent':
					// Someone was invited — reload the pending list.
					if (canInvite) loadInvitations(id);
					break;

				case 'org.invitation.declined': {
					// Invitee declined — drop it from the pending list immediately.
					const email = payload.meta?.email as string | undefined;
					if (email) invitations = invitations.filter(i => i.email !== email);
					break;
				}
			}
		};

		eventBus.on('*', handler as any);
		return () => eventBus.off('*', handler as any);
	});
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canViewMembers}
	message="You need the 'View members' permission to see this page."
	onDismiss={() => history.back()}
/>

<div class="members-page">

	<!-- ── Role info note ─────────────────────────────────────────── -->
	{#if membershipLoaded && myRole}
		<div class="role-info-note">
			<span class="role-info-label">Your role:</span>
			<span class="role-info-badge"><Badge tone={roleTone(myRole)}>{roleLabel(myRole)}</Badge></span>
			<span class="role-info-desc">
				{#if myRole === 'owner'}
					Full access — manage all members, roles, and invitations.
				{:else if myRole === 'admin'}
					Can invite members and remove member-role users. Cannot remove admins or the owner.
				{:else if myRole === 'member'}
					Read-only view. Contact an admin to change member settings.
				{:else}
					Read-only view of organization members.
				{/if}
			</span>
		</div>
	{/if}

	<!-- ── Invite button ───────────────────────────────────────────── -->
	<div class="invite-bar">
		<div class="invite-bar-text">
			<h2 class="invite-bar-title">Members</h2>
			<p class="invite-bar-desc">Manage who has access to this organization.</p>
		</div>
		{#if canInvite}
			<Button onclick={() => (showInvitePanel = true)}>
				<UserPlus size={14} />
				Invite Member
			</Button>
		{/if}
	</div>

	<!-- ── Pending Invitations ─────────────────────────────────────── -->
	{#if canInvite}
		<section class="settings-section">
			<div class="section-header">
				<div class="section-icon"><Clock size={16} /></div>
				<div>
					<h2 class="section-title">Pending Invitations</h2>
					<p class="section-desc">
						{invitations.length} pending invitation{invitations.length === 1 ? '' : 's'}
					</p>
				</div>
			</div>

			{#if loadingInvites}
				<div class="list-empty">
					<Spinner size={14} />
					<span>Loading…</span>
				</div>
			{:else if invitations.length === 0}
				<div class="list-empty muted">No pending invitations</div>
			{:else}
				<ul class="invite-list">
					{#each invitations as inv (inv.id)}
						{@const assignmentCount = Array.isArray(inv.project_assignments) ? inv.project_assignments.length : 0}
						<li class="invite-item">
							<Avatar initials={inv.email[0] ?? '?'} tone="blue" size={30} />
							<div class="invite-info">
								<span class="invite-email-text">{inv.email}</span>
								<div class="invite-meta">
									<Badge tone={roleTone(inv.role)}>{roleLabel(inv.role)}</Badge>
									{#if inv.permissions.length > 0}
										<Badge tone="blue">{inv.permissions.length} perm{inv.permissions.length === 1 ? '' : 's'}</Badge>
									{/if}
									{#if assignmentCount > 0}
										<Badge tone="green">
											<span class="pill-icon"><Folder size={9} />{assignmentCount} project{assignmentCount === 1 ? '' : 's'}</span>
										</Badge>
									{/if}
									<span class="invite-expiry">{expiresLabel(inv.expires_at)}</span>
								</div>
							</div>
							<Button
								variant="ghost"
								size="icon"
								onclick={() => copyInviteLink(inv)}
								title="Copy invitation link"
								aria-label="Copy invitation link"
							>
								{#if copiedInviteId === inv.id}
									<CheckCheck size={13} />
								{:else}
									<Link size={13} />
								{/if}
							</Button>
							<Button
								variant="danger-outline"
								size="icon"
								disabled={cancellingInvite === inv.id}
								onclick={() => cancelInvite(inv)}
								title="Cancel invitation"
								aria-label="Cancel invitation"
							>
								{#if cancellingInvite === inv.id}
									<Spinner size={13} tone="current" />
								{:else}
									<X size={13} />
								{/if}
							</Button>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	{/if}

	<!-- ── Invite slide panel ──────────────────────────────────────── -->
	{#if showInvitePanel}
		<SlidePanel title="Invite Member" onClose={() => (showInvitePanel = false)} zIndex={70}>
			<InvitePanel
				{orgId}
				allProjects={projects}
				{isOwner}
				onClose={() => (showInvitePanel = false)}
				onInvited={() => loadInvitations()}
			/>
		</SlidePanel>
	{/if}

	<!-- ── Member manage slide panel ─────────────────────────────────── -->
	{#if panelMember}
		<SlidePanel
			title="Manage — {panelMember.email}"
			onClose={closeMemberPanel}
			zIndex={70}
		>
			<MemberManagePanel
				member={panelMember}
				{orgId}
				{isOwner}
				allProjects={projects}
				onClose={closeMemberPanel}
				onRoleChanged={handleRoleChangedFromPanel}
				onPermissionsChanged={handlePermsChangedFromPanel}
			/>
		</SlidePanel>
	{/if}

	<!-- ── Member list ─────────────────────────────────────────────── -->
	<section class="settings-section">
		<div class="section-header">
			<div class="section-icon"><Shield size={16} /></div>
			<div>
				<h2 class="section-title">{members.length} member{members.length === 1 ? '' : 's'}</h2>
				<p class="section-desc">Current members of this organization</p>
			</div>
		</div>

		{#if loadingMembers}
			<div class="list-empty">
				<Spinner size={14} />
				<span>Loading members…</span>
			</div>
		{:else if membersError}
			<div class="members-error" role="alert">
				<InlineAlert tone="error">{membersError}</InlineAlert>
				<Button variant="ghost" size="sm" onclick={() => loadMembers()}>Retry</Button>
			</div>
		{:else}
			<DataTable
				items={members}
				rowKey={(m) => m.id}
				columns={[
					{ key: 'member', label: 'Member' },
					{ key: 'role', label: 'Role' },
					{ key: 'actions', label: '', width: '90px' }
				]}
				searchable={false}
				emptyMessage="No members."
			>
				{#snippet row(member)}
					{@const isSelf     = member.user_id === currentUserId}
					{@const isChanging = changingRoleFor === member.user_id}
					{@const isRemoving = removingMember  === member.user_id}
					{@const isPanelOpen = panelMember?.user_id === member.user_id}
					<tr class={isSelf ? 'member-self' : ''}>
						<td>
							<div class="member-cell">
								<Avatar initials={member.email[0] ?? '?'} tone="blue" size={34} />
								<div class="member-info">
									<div class="member-email-row">
										<span class="member-email">{member.email}</span>
										{#if isSelf}<Badge tone="blue">You</Badge>{/if}
									</div>
									<div class="member-sub">
										<span class="member-since">Joined {formatTime(member.created_at)}</span>
										{#if member.permissions.length > 0}
											<Badge tone="blue">{member.permissions.length} permission{member.permissions.length === 1 ? '' : 's'}</Badge>
										{:else}
											<Badge tone="neutral">No custom permissions</Badge>
										{/if}
									</div>
								</div>
							</div>
						</td>
						<td class="role-cell">
							{#if canChangeRole(member)}
								<div class="role-select">
									<Select
										value={member.role}
										disabled={isChanging}
										aria-label="Role for {member.email}"
										onchange={(e) => handleRoleChange(member, (e.target as HTMLSelectElement).value as MemberRole)}
										options={assignableRoles().map((r) => ({ value: r, label: roleLabel(r) }))}
									/>
									{#if isChanging}<Spinner size={12} />{/if}
								</div>
							{:else}
								<Badge tone={roleTone(member.role)}>
									<span class="pill-icon">
										{#if member.role === 'owner'}<Crown size={10} />{/if}
										{#if member.role === 'viewer'}<Eye size={10} />{/if}
										{roleLabel(member.role)}
									</span>
								</Badge>
							{/if}
						</td>
						<td>
							<div class="member-actions">
								{#if canManage && !isSelf}
									<Button
										variant={isPanelOpen ? 'secondary' : 'ghost'}
										size="icon"
										onclick={() => isPanelOpen ? closeMemberPanel() : openMemberPanel(member)}
										title="Manage permissions & project access"
										aria-label="Manage permissions & project access"
									>
										<SlidersHorizontal size={13} />
									</Button>
								{/if}
								{#if canRemove(member)}
									<Button
										variant="danger-outline"
										size="icon"
										disabled={isRemoving}
										onclick={() => handleRemove(member)}
										title="Remove member"
										aria-label="Remove member"
									>
										{#if isRemoving}<Spinner size={13} tone="current" />{:else}<Trash2 size={13} />{/if}
									</Button>
								{/if}
							</div>
						</td>
					</tr>
				{/snippet}
			</DataTable>
		{/if}
	</section>
</div>

<style>
	.members-page { display: flex; flex-direction: column; gap: 20px; }

	.pill-icon { display: inline-flex; align-items: center; gap: 4px; }

	/* ── Role info note ── */
	.role-info-note {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 9px 14px;
		border-radius: var(--radius-md);
		border: 1px solid var(--border);
		background: var(--bg-elevated);
		font-size: 12px;
		flex-wrap: wrap;
	}
	.role-info-label { font-weight: 600; color: var(--text-muted); flex-shrink: 0; }
	.role-info-badge { flex-shrink: 0; display: inline-flex; }
	.role-info-desc { color: var(--text-muted); flex: 1; min-width: 180px; }

	/* ── Invite bar ── */
	.invite-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.invite-bar-title { font-size: 16px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.invite-bar-desc  { font-size: 13px; color: var(--text-muted); margin: 0; }

	/* ── Shared section chrome ── */
	.settings-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.section-header {
		display: flex; gap: 14px; padding: 18px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.section-icon {
		width: 32px; height: 32px; border-radius: var(--radius-md);
		background: var(--accent-muted); color: var(--accent);
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0; margin-top: 1px;
	}
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc  { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.list-empty {
		display: flex; align-items: center; gap: 8px;
		padding: 20px; color: var(--text-muted); font-size: 13px;
	}
	.list-empty.muted { color: var(--text-dim); font-size: 12px; font-style: italic; }
	.members-error { display: flex; align-items: center; gap: 8px; margin: 12px 20px 16px; }
	.members-error :global(.ui-alert) { flex: 1; }

	/* ── Pending invitations ── */
	.invite-list { list-style: none; margin: 0; padding: 0; }
	.invite-item {
		display: flex; align-items: center; gap: 12px;
		padding: 11px 20px; border-bottom: 1px solid var(--border);
	}
	.invite-item:last-child { border-bottom: none; }
	.invite-info { flex: 1; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
	.invite-email-text { font-size: 13px; font-weight: 500; color: var(--text-primary); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.invite-meta { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
	.invite-expiry { font-size: 11px; color: var(--text-dim); }

	/* ── Member table ── */
	.member-cell { display: flex; align-items: center; gap: 12px; }
	.member-info { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
	.member-email-row { display: flex; align-items: center; gap: 7px; }
	.member-email { font-size: 13px; font-weight: 500; color: var(--text-primary); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.member-sub { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
	.member-since { font-size: 11px; color: var(--text-dim); }
	:global(tr.member-self) { background: color-mix(in srgb, var(--accent) 3%, transparent); }

	.role-select { display: flex; align-items: center; gap: 6px; width: 130px; }
	.member-actions { display: flex; align-items: center; justify-content: flex-end; gap: 4px; }

	@media (max-width: 639px) {
		.members-page { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.invite-bar { flex-direction: column; align-items: flex-start; gap: 10px; }
		.member-email { font-size: 12px; max-width: 140px; }
		.member-sub { max-width: 200px; }
		.role-cell { display: none; }
		.settings-section :global(.ui-data-table-el thead th:nth-child(2)) { display: none; }
		.invite-item { padding: 10px 16px; }
	}
</style>
