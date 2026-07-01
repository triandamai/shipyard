<script lang="ts">
	import { api } from '$lib/api/client';
	import { PERMISSION_GROUPS } from '$lib/api/types';
	import type { OrgMember, MemberRole, Project, MemberProjectAssignment } from '$lib/api/types';
	import {
		Check, Loader2, AlertCircle, Lock, Folder, FolderOpen,
		X, Plus, Trash2, Crown, Eye, ChevronDown
	} from '@lucide/svelte';

	const PROJECT_PERM_OPTIONS = [
		{ id: 'view',   label: 'View',   desc: 'Read-only access to services and deployments' },
		{ id: 'deploy', label: 'Deploy', desc: 'Trigger deployments and restarts' },
		{ id: 'manage', label: 'Manage', desc: 'Create, edit, and delete services' },
	];

	const ROLES: { value: MemberRole; label: string }[] = [
		{ value: 'owner',  label: 'Owner' },
		{ value: 'admin',  label: 'Admin' },
		{ value: 'member', label: 'Member' },
		{ value: 'viewer', label: 'Viewer' },
	];

	interface Props {
		member: OrgMember;
		orgId: string;
		isOwner: boolean;
		allProjects: Project[];
		onClose: () => void;
		onRoleChanged?: (userId: string, newRole: MemberRole) => void;
		onPermissionsChanged?: (userId: string, perms: string[]) => void;
	}

	let {
		member,
		orgId,
		isOwner,
		allProjects,
		onClose,
		onRoleChanged,
		onPermissionsChanged,
	}: Props = $props();

	// ── Role ──────────────────────────────────────────────────────────
	let role       = $state<MemberRole>(member.role as MemberRole);
	let savingRole = $state(false);
	let roleError  = $state('');

	async function saveRole() {
		if (role === member.role || savingRole) return;
		savingRole = true;
		roleError  = '';
		const res = await api.changeMemberRole(orgId, member.user_id, role);
		if (res.error) {
			roleError = res.error.message;
			role = member.role as MemberRole;
		} else {
			onRoleChanged?.(member.user_id, role);
		}
		savingRole = false;
	}

	// ── Org Permissions ───────────────────────────────────────────────
	let orgPerms     = $state<Set<string>>(new Set(member.permissions));
	let savingPerms  = $state(false);
	let permsError   = $state('');
	let permsSaved   = $state(false);

	function togglePerm(id: string) {
		const next = new Set(orgPerms);
		if (next.has(id)) next.delete(id); else next.add(id);
		orgPerms = next;
	}

	async function savePerms() {
		savingPerms = true;
		permsError  = '';
		permsSaved  = false;
		const res = await api.setMemberPermissions(orgId, member.user_id, [...orgPerms]);
		if (res.error) {
			permsError = res.error.message;
		} else {
			permsSaved = true;
			onPermissionsChanged?.(member.user_id, [...orgPerms]);
			setTimeout(() => (permsSaved = false), 2500);
		}
		savingPerms = false;
	}

	// ── Project assignments ───────────────────────────────────────────
	let projectAssignments = $state<MemberProjectAssignment[]>([]);
	let loadingProjects    = $state(true);
	let projectsError      = $state('');
	let savingProjects     = $state(false);
	let projectsSaved      = $state(false);

	// Projects not yet assigned to this member
	let unassignedProjects = $derived(
		allProjects.filter(p => !projectAssignments.some(a => a.project_id === p.id))
	);

	// Pending edits (mirror of projectAssignments for in-panel editing)
	let pendingAssignments = $state<{ project_id: string; project_name: string; project_slug: string; permissions: Set<string> }[]>([]);

	async function loadProjectAssignments() {
		loadingProjects = true;
		projectsError   = '';
		const res = await api.getMemberProjects(orgId, member.user_id);
		if (res.data) {
			projectAssignments = res.data;
			pendingAssignments = res.data.map(a => ({
				project_id:   a.project_id,
				project_name: a.project_name,
				project_slug: a.project_slug,
				permissions:  new Set(a.permissions),
			}));
		} else {
			projectsError = res.error?.message ?? 'Failed to load project assignments';
		}
		loadingProjects = false;
	}

	function addProject(project: Project) {
		pendingAssignments = [
			...pendingAssignments,
			{
				project_id:   project.id,
				project_name: project.name,
				project_slug: project.slug,
				permissions:  new Set(['view']),
			},
		];
	}

	function removeProject(projectId: string) {
		pendingAssignments = pendingAssignments.filter(a => a.project_id !== projectId);
	}

	function toggleProjectPerm(projectId: string, permId: string) {
		pendingAssignments = pendingAssignments.map(a => {
			if (a.project_id !== projectId) return a;
			const next = new Set(a.permissions);
			if (next.has(permId)) next.delete(permId); else next.add(permId);
			return { ...a, permissions: next };
		});
	}

	async function saveProjectAssignments() {
		savingProjects = true;
		projectsSaved  = false;
		projectsError  = '';
		const assignments = pendingAssignments.map(a => ({
			project_id:  a.project_id,
			permissions: [...a.permissions],
		}));
		const res = await api.setMemberProjects(orgId, member.user_id, assignments);
		if (res.error) {
			projectsError = res.error.message;
		} else {
			projectAssignments = res.data ?? [];
			pendingAssignments = (res.data ?? []).map(a => ({
				project_id:   a.project_id,
				project_name: a.project_name,
				project_slug: a.project_slug,
				permissions:  new Set(a.permissions),
			}));
			projectsSaved = true;
			setTimeout(() => (projectsSaved = false), 2500);
		}
		savingProjects = false;
	}

	// Load on mount
	$effect(() => {
		loadProjectAssignments();
	});

	function roleColor(r: string) {
		switch (r) {
			case 'owner':  return 'role-owner';
			case 'admin':  return 'role-admin';
			case 'member': return 'role-member';
			default:       return 'role-viewer';
		}
	}

	function assignableRoles(): MemberRole[] {
		return isOwner ? ['owner', 'admin', 'member', 'viewer'] : ['admin', 'member', 'viewer'];
	}
</script>

<div class="panel-wrap">
	<!-- ── Member header ── -->
	<div class="member-header">
		<div class="member-avatar">{member.email[0]?.toUpperCase() ?? '?'}</div>
		<div class="member-info">
			<span class="member-email">{member.email}</span>
			<span class="member-since">Member</span>
		</div>
	</div>

	<!-- ── Role ── -->
	<section class="panel-section">
		<div class="section-label"><Crown size={12} />Role</div>
		<div class="role-row">
			<div class="role-select-wrap">
				<select
					class="role-select {roleColor(role)}"
					bind:value={role}
					disabled={savingRole}
					onchange={saveRole}
				>
					{#each assignableRoles() as r}
						<option value={r}>{ROLES.find(x => x.value === r)?.label ?? r}</option>
					{/each}
				</select>
				<ChevronDown size={11} class="role-chevron" />
				{#if savingRole}<Loader2 size={12} class="spin role-loading" />{/if}
			</div>
			{#if roleError}
				<span class="inline-error"><AlertCircle size={11} />{roleError}</span>
			{/if}
		</div>
	</section>

	<!-- ── Org Permissions ── -->
	<section class="panel-section">
		<div class="section-label"><Lock size={12} />Org Permissions
			{#if orgPerms.size > 0}
				<span class="count-chip">{orgPerms.size}</span>
			{/if}
		</div>

		<div class="perm-groups">
			{#each PERMISSION_GROUPS as group}
				<div class="perm-group">
					<div class="perm-group-name">{group.group}</div>
					<div class="perm-grid">
						{#each group.permissions as perm}
							<label class="perm-check" class:checked={orgPerms.has(perm.id)} title={perm.description}>
								<input type="checkbox" checked={orgPerms.has(perm.id)} onchange={() => togglePerm(perm.id)} />
								<span class="perm-check-box">{#if orgPerms.has(perm.id)}<Check size={9} />{/if}</span>
								<span class="perm-label">{perm.label}</span>
							</label>
						{/each}
					</div>
				</div>
			{/each}
		</div>

		{#if permsError}
			<div class="error-banner"><AlertCircle size={12} />{permsError}</div>
		{/if}
		<div class="section-actions">
			<button class="btn btn-primary btn-sm" disabled={savingPerms} onclick={savePerms}>
				{#if savingPerms}<Loader2 size={12} class="spin" />Saving…
				{:else if permsSaved}<Check size={12} />Saved
				{:else}<Check size={12} />Save permissions{/if}
			</button>
		</div>
	</section>

	<!-- ── Project Assignments ── -->
	<section class="panel-section">
		<div class="section-label"><Folder size={12} />Project Access
			{#if pendingAssignments.length > 0}
				<span class="count-chip">{pendingAssignments.length}</span>
			{/if}
		</div>

		{#if loadingProjects}
			<div class="loading-row"><div class="spinner"></div><span>Loading…</span></div>
		{:else if projectsError}
			<div class="error-banner"><AlertCircle size={12} />{projectsError}</div>
		{:else}
			<!-- Current assignments -->
			{#if pendingAssignments.length === 0}
				<p class="empty-hint">No project assignments yet.</p>
			{:else}
				<div class="assignment-list">
					{#each pendingAssignments as assignment (assignment.project_id)}
						<div class="assignment-item">
							<div class="assignment-header">
								<FolderOpen size={13} class="folder-icon" />
								<span class="assignment-name">{assignment.project_name}</span>
								<button
									class="revoke-btn"
									onclick={() => removeProject(assignment.project_id)}
									title="Remove project access"
								>
									<X size={12} />
								</button>
							</div>
							<div class="perm-chip-row">
								{#each PROJECT_PERM_OPTIONS as opt}
									{@const has = assignment.permissions.has(opt.id)}
									<label class="perm-chip" class:active={has} title={opt.desc}>
										<input
											type="checkbox"
											checked={has}
											onchange={() => toggleProjectPerm(assignment.project_id, opt.id)}
										/>
										{#if has}<Check size={8} />{/if}
										{opt.label}
									</label>
								{/each}
							</div>
						</div>
					{/each}
				</div>
			{/if}

			<!-- Add project -->
			{#if unassignedProjects.length > 0}
				<div class="add-project-wrap">
					<span class="add-label"><Plus size={11} />Add project</span>
					<div class="project-chips">
						{#each unassignedProjects as project}
							<button
								class="project-add-chip"
								onclick={() => addProject(project)}
							>
								<Folder size={11} />{project.name}
							</button>
						{/each}
					</div>
				</div>
			{/if}

			{#if projectsError}
				<div class="error-banner"><AlertCircle size={12} />{projectsError}</div>
			{/if}
			<div class="section-actions">
				<button class="btn btn-primary btn-sm" disabled={savingProjects} onclick={saveProjectAssignments}>
					{#if savingProjects}<Loader2 size={12} class="spin" />Saving…
					{:else if projectsSaved}<Check size={12} />Saved
					{:else}<Check size={12} />Save project access{/if}
				</button>
			</div>
		{/if}
	</section>
</div>

<style>
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.8s linear infinite; }

	.panel-wrap {
		display: flex; flex-direction: column;
		padding: 0;
	}

	/* ── Member header ── */
	.member-header {
		display: flex; align-items: center; gap: 12px;
		padding: 16px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.member-avatar {
		width: 38px; height: 38px; border-radius: 50%;
		background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 60%, #7C3AED));
		color: white; font-size: 14px; font-weight: 700;
		display: flex; align-items: center; justify-content: center; flex-shrink: 0;
	}

	.member-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
	.member-email { font-size: 13px; font-weight: 600; color: var(--text-primary); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.member-since { font-size: 11px; color: var(--text-dim); }

	/* ── Sections ── */
	.panel-section {
		border-bottom: 1px solid var(--border);
		padding: 16px 20px;
		display: flex; flex-direction: column; gap: 12px;
	}

	.section-label {
		display: flex; align-items: center; gap: 6px;
		font-size: 10px; font-weight: 700; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.08em;
	}

	.count-chip {
		font-size: 10px; font-weight: 700;
		padding: 1px 6px; border-radius: 999px;
		background: rgba(37,99,235,0.1); color: var(--accent);
		border: 1px solid rgba(37,99,235,0.2);
	}

	/* ── Role ── */
	.role-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }

	.role-select-wrap { position: relative; display: inline-flex; align-items: center; }

	.role-select {
		appearance: none; -webkit-appearance: none;
		font-size: 12px; font-weight: 600;
		padding: 5px 28px 5px 10px;
		border-radius: 999px; border: 1px solid;
		cursor: pointer; outline: none;
		background: transparent; font-family: var(--font-sans);
		transition: opacity var(--transition-fast);
	}
	.role-select:disabled { opacity: 0.6; cursor: default; }
	.role-owner  { color: #D97706; border-color: rgba(245,158,11,0.4); background: rgba(245,158,11,0.08) !important; }
	.role-admin  { color: #6366F1; border-color: rgba(99,102,241,0.4); background: rgba(99,102,241,0.08) !important; }
	.role-member { color: #10B981; border-color: rgba(16,185,129,0.3); background: rgba(16,185,129,0.08) !important; }
	.role-viewer { color: var(--text-muted); border-color: var(--border); background: var(--bg-elevated) !important; }

	:global(.role-chevron) { position: absolute; right: 8px; pointer-events: none; color: currentColor; opacity: 0.6; }
	:global(.role-loading) { margin-left: 8px; color: var(--text-muted); }

	/* ── Permissions ── */
	.perm-groups { display: flex; flex-direction: column; gap: 10px; }
	.perm-group { display: flex; flex-direction: column; gap: 5px; }
	.perm-group-name { font-size: 10px; font-weight: 700; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.08em; }
	.perm-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 3px; }

	.perm-check {
		display: flex; align-items: center; gap: 6px;
		padding: 4px 7px; border-radius: var(--radius-sm);
		cursor: pointer; user-select: none;
		font-size: 11px; color: var(--text-muted);
		border: 1px solid transparent;
		transition: background var(--transition-fast), border-color var(--transition-fast), color var(--transition-fast);
	}
	.perm-check:hover { background: var(--bg-elevated); color: var(--text-primary); }
	.perm-check.checked { background: rgba(37,99,235,0.06); border-color: rgba(37,99,235,0.2); color: var(--text-primary); }
	.perm-check input { display: none; }

	.perm-check-box {
		width: 13px; height: 13px; flex-shrink: 0;
		border: 1.5px solid var(--border);
		border-radius: 3px;
		display: flex; align-items: center; justify-content: center;
		background: var(--bg-base); color: white;
		transition: background var(--transition-fast), border-color var(--transition-fast);
	}
	.perm-check.checked .perm-check-box { background: var(--accent); border-color: var(--accent); }
	.perm-label { line-height: 1; font-size: 11px; }

	/* ── Project assignments ── */
	.assignment-list { display: flex; flex-direction: column; gap: 6px; }

	.assignment-item {
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-base);
	}

	.assignment-header {
		display: flex; align-items: center; gap: 8px;
		padding: 7px 10px;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
	}
	:global(.folder-icon) { color: var(--accent); flex-shrink: 0; }
	.assignment-name { flex: 1; font-size: 12px; font-weight: 500; color: var(--text-primary); }

	.revoke-btn {
		width: 22px; height: 22px; background: transparent; border: none;
		cursor: pointer; color: var(--text-dim);
		display: flex; align-items: center; justify-content: center;
		border-radius: var(--radius-sm);
		transition: color var(--transition-fast), background var(--transition-fast);
		flex-shrink: 0;
	}
	.revoke-btn:hover { color: #EF4444; background: rgba(239,68,68,0.08); }

	.perm-chip-row {
		display: flex; gap: 4px; flex-wrap: wrap;
		padding: 7px 10px;
	}

	.perm-chip {
		display: inline-flex; align-items: center; gap: 4px;
		font-size: 11px; font-weight: 500;
		padding: 3px 8px; border-radius: 999px;
		border: 1px solid var(--border);
		cursor: pointer; user-select: none;
		background: var(--bg-elevated); color: var(--text-dim);
		transition: all var(--transition-fast);
	}
	.perm-chip input { display: none; }
	.perm-chip:hover { border-color: rgba(37,99,235,0.3); color: var(--accent); }
	.perm-chip.active { background: rgba(37,99,235,0.08); border-color: rgba(37,99,235,0.3); color: var(--accent); }

	/* ── Add project ── */
	.empty-hint { font-size: 12px; color: var(--text-dim); font-style: italic; margin: 0; }

	.add-project-wrap {
		border-top: 1px dashed var(--border);
		padding-top: 10px;
		display: flex; flex-direction: column; gap: 6px;
	}
	.add-label {
		display: flex; align-items: center; gap: 5px;
		font-size: 10px; font-weight: 700; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em;
	}
	.project-chips { display: flex; gap: 4px; flex-wrap: wrap; }
	.project-add-chip {
		display: inline-flex; align-items: center; gap: 5px;
		font-size: 11px; font-weight: 500; padding: 4px 9px;
		border-radius: 999px;
		border: 1px dashed var(--border); background: transparent;
		cursor: pointer; color: var(--text-muted);
		transition: all var(--transition-fast);
	}
	.project-add-chip:hover { border-color: var(--accent); color: var(--accent); background: rgba(37,99,235,0.06); border-style: solid; }

	/* ── Common ── */
	.section-actions { display: flex; justify-content: flex-end; }
	.loading-row { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-dim); }
	.spinner { width: 12px; height: 12px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }
	.error-banner { display: flex; align-items: center; gap: 6px; padding: 8px 10px; font-size: 12px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.2); border-radius: var(--radius-sm); color: #EF4444; }
	.inline-error { display: flex; align-items: center; gap: 4px; font-size: 11px; color: #EF4444; }
</style>
