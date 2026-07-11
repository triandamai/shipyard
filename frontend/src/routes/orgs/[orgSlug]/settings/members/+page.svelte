<script lang="ts">
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import {
		UserPlus, Shield, Trash2, Crown, Eye, ChevronDown,
		Loader2, AlertCircle, Check, X, Clock, SlidersHorizontal,
		Link, CheckCheck, Folder
	} from '@lucide/svelte';
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

	function roleColor(role: string) {
		switch (role) {
			case 'owner':  return 'role-owner';
			case 'admin':  return 'role-admin';
			case 'member': return 'role-member';
			default:       return 'role-viewer';
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
		<div class="role-info-note role-info-note--{myRole}">
			<span class="role-info-label">Your role:</span>
			<span class="role-info-badge role-badge {roleColor(myRole)}">{roleLabel(myRole)}</span>
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
			<button class="btn-invite-open" onclick={() => (showInvitePanel = true)}>
				<UserPlus size={14} />
				Invite Member
			</button>
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
					<div class="spinner"></div>
					<span>Loading…</span>
				</div>
			{:else if invitations.length === 0}
				<div class="list-empty muted">No pending invitations</div>
			{:else}
				<ul class="invite-list">
					{#each invitations as inv (inv.id)}
						{@const assignmentCount = Array.isArray(inv.project_assignments) ? inv.project_assignments.length : 0}
						<li class="invite-item">
							<div class="invite-avatar">{inv.email[0]?.toUpperCase()}</div>
							<div class="invite-info">
								<span class="invite-email-text">{inv.email}</span>
								<div class="invite-meta">
									<span class="role-badge {roleColor(inv.role)}">{roleLabel(inv.role)}</span>
									{#if inv.permissions.length > 0}
										<span class="perm-pill">{inv.permissions.length} perm{inv.permissions.length === 1 ? '' : 's'}</span>
									{/if}
									{#if assignmentCount > 0}
										<span class="perm-pill project-pill">
											<Folder size={9} />{assignmentCount} project{assignmentCount === 1 ? '' : 's'}
										</span>
									{/if}
									<span class="invite-expiry">{expiresLabel(inv.expires_at)}</span>
								</div>
							</div>
							<button
								class="copy-link-btn"
								class:copied={copiedInviteId === inv.id}
								onclick={() => copyInviteLink(inv)}
								title="Copy invitation link"
							>
								{#if copiedInviteId === inv.id}
									<CheckCheck size={13} />
								{:else}
									<Link size={13} />
								{/if}
							</button>
							<button
								class="cancel-invite-btn"
								disabled={cancellingInvite === inv.id}
								onclick={() => cancelInvite(inv)}
								title="Cancel invitation"
							>
								{#if cancellingInvite === inv.id}
									<Loader2 size={13} class="spin" />
								{:else}
									<X size={13} />
								{/if}
							</button>
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
				<div class="spinner"></div>
				<span>Loading members…</span>
			</div>
		{:else if membersError}
			<div class="error-banner" style="margin: 12px 20px 16px;">
				<AlertCircle size={14} />{membersError}
				<button class="btn btn-ghost btn-sm" onclick={() => loadMembers()}>Retry</button>
			</div>
		{:else}
			<ul class="member-list">
				{#each members as member (member.id)}
					{@const isSelf     = member.user_id === currentUserId}
					{@const isChanging = changingRoleFor === member.user_id}
					{@const isRemoving = removingMember  === member.user_id}
					{@const isPanelOpen = panelMember?.user_id === member.user_id}

					<li class="member-item" class:member-self={isSelf}>
						<!-- Main row -->
						<div class="member-row">
							<div class="member-avatar">{member.email[0]?.toUpperCase() ?? '?'}</div>

							<div class="member-info">
								<div class="member-email-row">
									<span class="member-email">{member.email}</span>
									{#if isSelf}<span class="self-badge">You</span>{/if}
								</div>
								<div class="member-sub">
									<span class="member-since">Joined {formatTime(member.created_at)}</span>
									{#if member.permissions.length > 0}
										<span class="perm-pill">{member.permissions.length} permission{member.permissions.length === 1 ? '' : 's'}</span>
									{:else}
										<span class="perm-pill perm-none">No custom permissions</span>
									{/if}
								</div>
							</div>

							<!-- Role badge / selector -->
							<div class="member-role-wrap">
								{#if canChangeRole(member)}
									<div class="role-select-wrap">
										{#if isChanging}<Loader2 size={12} class="spin role-loading" />{/if}
										<select
											class="role-select-inline {roleColor(member.role)}"
											value={member.role}
											disabled={isChanging}
											onchange={(e) => handleRoleChange(member, (e.target as HTMLSelectElement).value as MemberRole)}
										>
											{#each assignableRoles() as r}
												<option value={r}>{roleLabel(r)}</option>
											{/each}
										</select>
										<ChevronDown size={11} class="role-chevron" />
									</div>
								{:else}
									<span class="role-badge {roleColor(member.role)}">
										{#if member.role === 'owner'}<Crown size={10} />{/if}
										{#if member.role === 'viewer'}<Eye size={10} />{/if}
										{roleLabel(member.role)}
									</span>
								{/if}
							</div>

							<!-- Action buttons -->
							<div class="member-actions">
								{#if canManage && !isSelf}
									<button
										class="action-btn"
										class:active={isPanelOpen}
										onclick={() => isPanelOpen ? closeMemberPanel() : openMemberPanel(member)}
										title="Manage permissions & project access"
									>
										<SlidersHorizontal size={13} />
									</button>
								{/if}
								{#if canRemove(member)}
									<button
										class="action-btn danger"
										disabled={isRemoving}
										onclick={() => handleRemove(member)}
										title="Remove member"
									>
										{#if isRemoving}<Loader2 size={13} class="spin" />{:else}<Trash2 size={13} />{/if}
									</button>
								{/if}
							</div>
						</div>
					</li>
				{/each}
			</ul>
		{/if}
	</section>
</div>

<style>
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.8s linear infinite; }

	.members-page { display: flex; flex-direction: column; gap: 20px; }

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
	.role-info-label {
		font-weight: 600;
		color: var(--text-muted);
		flex-shrink: 0;
	}
	.role-info-badge { flex-shrink: 0; }
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
	.btn-invite-open {
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 8px 16px;
		background: var(--accent);
		color: #fff;
		border: none;
		border-radius: 7px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		white-space: nowrap;
		flex-shrink: 0;
		transition: opacity .15s;
	}
	.btn-invite-open:hover { opacity: .88; }

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
		background: rgba(37,99,235,0.1); color: var(--accent);
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
	.spinner { width: 14px; height: 14px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }
	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	/* ── Field inputs ── */
	.field-input {
		background: var(--bg-base); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary);
		font-size: 13px; font-family: var(--font-sans);
		padding: 8px 10px; outline: none;
		transition: border-color var(--transition-fast);
	}
	.field-input:focus { border-color: var(--accent); }


	/* ── Pending invitations ── */
	.invite-list { list-style: none; margin: 0; padding: 0; }
	.invite-item {
		display: flex; align-items: center; gap: 12px;
		padding: 11px 20px; border-bottom: 1px solid var(--border);
	}
	.invite-item:last-child { border-bottom: none; }

	.invite-avatar {
		width: 30px; height: 30px; border-radius: 50%;
		background: var(--bg-elevated); border: 1.5px dashed var(--border);
		color: var(--text-dim); font-size: 12px; font-weight: 600;
		display: flex; align-items: center; justify-content: center; flex-shrink: 0;
	}

	.invite-info { flex: 1; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
	.invite-email-text { font-size: 13px; font-weight: 500; color: var(--text-primary); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.invite-meta { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
	.invite-expiry { font-size: 11px; color: var(--text-dim); }

	.project-pill { display: flex; align-items: center; gap: 4px; background: rgba(16,185,129,0.1); color: #10B981; border-color: rgba(16,185,129,0.25); }

	.copy-link-btn, .cancel-invite-btn {
		width: 26px; height: 26px; background: transparent; border: none; cursor: pointer;
		color: var(--text-dim); display: flex; align-items: center; justify-content: center;
		border-radius: var(--radius-sm);
		transition: color var(--transition-fast), background var(--transition-fast);
		flex-shrink: 0;
	}
	.copy-link-btn:hover { color: var(--accent); background: rgba(37,99,235,0.08); }
	.copy-link-btn.copied { color: #22C55E; }
	.cancel-invite-btn:hover { color: #EF4444; background: rgba(239,68,68,0.08); }
	.cancel-invite-btn:disabled { opacity: 0.4; cursor: default; }

	/* ── Member list ── */
	.member-list { list-style: none; margin: 0; padding: 0; }
	.member-item { border-bottom: 1px solid var(--border); }
	.member-item:last-child { border-bottom: none; }
	.member-item.member-self .member-row { background: color-mix(in srgb, var(--accent) 3%, transparent); }

	.member-row {
		display: flex; align-items: center; gap: 12px; padding: 12px 20px;
		transition: background var(--transition-fast);
	}
	.member-row:hover { background: var(--bg-elevated); }

	.member-avatar {
		width: 34px; height: 34px; border-radius: 50%;
		background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 60%, #7C3AED));
		color: white; font-size: 13px; font-weight: 700;
		display: flex; align-items: center; justify-content: center; flex-shrink: 0;
	}

	.member-info { flex: 1; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
	.member-email-row { display: flex; align-items: center; gap: 7px; }
	.member-email { font-size: 13px; font-weight: 500; color: var(--text-primary); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.self-badge { font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: 999px; background: rgba(37,99,235,0.1); color: var(--accent); border: 1px solid rgba(37,99,235,0.2); flex-shrink: 0; }
	.member-sub { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
	.member-since { font-size: 11px; color: var(--text-dim); }

	/* Permission count pill */
	.perm-pill {
		font-size: 10px; font-weight: 600; padding: 2px 7px; border-radius: 999px;
		background: rgba(99,102,241,0.1); color: #6366F1;
		border: 1px solid rgba(99,102,241,0.2);
	}
	.perm-pill.perm-none { background: var(--bg-elevated); color: var(--text-dim); border-color: var(--border); font-weight: 400; }

	/* Role badge */
	.role-badge {
		display: inline-flex; align-items: center; gap: 4px;
		font-size: 11px; font-weight: 600; padding: 3px 9px;
		border-radius: 999px; flex-shrink: 0;
	}
	.role-owner  { background: rgba(245,158,11,0.12); color: #D97706; border: 1px solid rgba(245,158,11,0.3); }
	.role-admin  { background: rgba(99,102,241,0.12); color: #6366F1; border: 1px solid rgba(99,102,241,0.3); }
	.role-member { background: rgba(16,185,129,0.12); color: #10B981; border: 1px solid rgba(16,185,129,0.25); }
	.role-viewer { background: var(--bg-elevated); color: var(--text-muted); border: 1px solid var(--border); }

	/* Inline role selector */
	.member-role-wrap { display: flex; align-items: center; flex-shrink: 0; }
	.role-select-wrap { position: relative; display: flex; align-items: center; }
	.role-select-inline {
		appearance: none; -webkit-appearance: none;
		font-size: 11px; font-weight: 600; padding: 3px 26px 3px 9px;
		border-radius: 999px; border: 1px solid; cursor: pointer; outline: none;
		background: transparent; font-family: var(--font-sans);
		transition: opacity var(--transition-fast);
	}
	.role-select-inline:disabled { opacity: 0.6; cursor: default; }
	:global(.role-chevron) { position: absolute; right: 8px; pointer-events: none; color: currentColor; opacity: 0.7; }
	:global(.role-loading) { position: absolute; left: -20px; color: var(--text-muted); }

	/* Member action buttons */
	.member-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
	.action-btn {
		width: 28px; height: 28px; background: transparent; border: 1px solid transparent;
		cursor: pointer; color: var(--text-dim);
		display: flex; align-items: center; justify-content: center;
		border-radius: var(--radius-sm);
		transition: color var(--transition-fast), background var(--transition-fast), border-color var(--transition-fast);
	}
	.action-btn:hover { color: var(--text-primary); background: var(--bg-elevated); border-color: var(--border); }
	.action-btn.active { color: var(--accent); background: rgba(37,99,235,0.08); border-color: rgba(37,99,235,0.2); }
	.action-btn.danger:hover { color: #EF4444; background: rgba(239,68,68,0.08); border-color: rgba(239,68,68,0.2); }
	.action-btn:disabled { opacity: 0.4; cursor: default; }

	/* ── Permission groups (used by MemberManagePanel via shared classes) ── */
	.perm-editor-groups { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-md); padding: 14px; }

	@media (max-width: 639px) {
		.members-page { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.invite-bar { flex-direction: column; align-items: flex-start; gap: 10px; }
		.btn-invite-open { align-self: flex-start; }
		.member-row { padding: 10px 16px; gap: 10px; }
		.member-email { font-size: 12px; }
		.member-role-wrap { display: none; }
		.invite-item { padding: 10px 16px; }
	}
</style>
