<script lang="ts">
	import { api } from '$lib/api/client';
	import { PERMISSION_GROUPS, PROJECT_PERM_OPTIONS, buildOrgPermission, parseOrgPermissionSuffix, expandProjectPermissions, collapseProjectPermissions } from '$lib/api/types';
	import type { OrgMember, MemberRole, Project, MemberProjectAssignment, ProjectPermTier } from '$lib/api/types';
	import { Check, Lock, Folder, FolderOpen, X, Plus, Crown } from '@lucide/svelte';
	import Avatar from '$lib/components/ui/Avatar.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Checkbox from '$lib/components/ui/Checkbox.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';

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
	// The panel works with suffix ids (e.g. "settings:read") for display.
	// Full shipyard:<orgId>: strings are built only when saving.
	function toSuffixSet(fullPerms: string[]): Set<string> {
		const out = new Set<string>();
		for (const p of fullPerms) {
			const suffix = parseOrgPermissionSuffix(orgId, p);
			if (suffix) out.add(suffix);
		}
		return out;
	}

	let orgPerms     = $state<Set<string>>(toSuffixSet(member.permissions));
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
		// Build full shipyard: strings from suffix ids
		const fullPerms = [...orgPerms].map(suffix => buildOrgPermission(orgId, suffix));
		const res = await api.setMemberPermissions(orgId, member.user_id, fullPerms);
		if (res.error) {
			permsError = res.error.message;
		} else {
			permsSaved = true;
			onPermissionsChanged?.(member.user_id, fullPerms);
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
	let pendingAssignments = $state<{ project_id: string; project_name: string; project_slug: string; permissions: Set<ProjectPermTier> }[]>([]);

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
				permissions:  collapseProjectPermissions(orgId, a.project_id, a.permissions),
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
				permissions:  new Set<ProjectPermTier>(['view']),
			},
		];
	}

	function removeProject(projectId: string) {
		pendingAssignments = pendingAssignments.filter(a => a.project_id !== projectId);
	}

	function toggleProjectPerm(projectId: string, permId: ProjectPermTier) {
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
			permissions: expandProjectPermissions(orgId, a.project_id, a.permissions),
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
				permissions:  collapseProjectPermissions(orgId, a.project_id, a.permissions),
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

	function assignableRoles(): MemberRole[] {
		return isOwner ? ['owner', 'admin', 'member', 'viewer'] : ['admin', 'member', 'viewer'];
	}
</script>

<div class="panel-wrap">
	<!-- ── Member header ── -->
	<div class="member-header">
		<Avatar initials={member.email[0] ?? '?'} size={38} />
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
				<Select
					bind:value={role}
					options={assignableRoles().map((r) => ({ value: r, label: ROLES.find((x) => x.value === r)?.label ?? r }))}
					disabled={savingRole}
					onchange={saveRole}
					aria-label="Role"
				/>
				{#if savingRole}<span class="role-loading"><Spinner size={12} /></span>{/if}
			</div>
			{#if roleError}
				<div role="alert"><InlineAlert tone="error">{roleError}</InlineAlert></div>
			{/if}
		</div>
	</section>

	<!-- ── Org Permissions ── -->
	<section class="panel-section">
		<div class="section-label"><Lock size={12} />Org Permissions
			{#if orgPerms.size > 0}
				<Badge tone="blue">{orgPerms.size}</Badge>
			{/if}
		</div>

		<div class="perm-groups">
			{#each PERMISSION_GROUPS as group}
				<div class="perm-group">
					<div class="perm-group-name">{group.group}</div>
					<div class="perm-grid">
						{#each group.permissions as perm}
							<span class="perm-check" title={perm.description}>
								<Checkbox checked={orgPerms.has(perm.id)} label={perm.label} onchange={() => togglePerm(perm.id)} />
							</span>
						{/each}
					</div>
				</div>
			{/each}
		</div>

		{#if permsError}
			<div role="alert"><InlineAlert tone="error">{permsError}</InlineAlert></div>
		{/if}
		<div class="section-actions">
			<Button size="sm" disabled={savingPerms} onclick={savePerms}>
				{#if savingPerms}<Spinner size={12} tone="current" />Saving…
				{:else if permsSaved}<Check size={12} />Saved
				{:else}<Check size={12} />Save permissions{/if}
			</Button>
		</div>
	</section>

	<!-- ── Project Assignments ── -->
	<section class="panel-section">
		<div class="section-label"><Folder size={12} />Project Access
			{#if pendingAssignments.length > 0}
				<Badge tone="blue">{pendingAssignments.length}</Badge>
			{/if}
		</div>

		{#if loadingProjects}
			<div class="loading-row"><Spinner size={12} /><span>Loading…</span></div>
		{:else if projectsError}
			<div role="alert"><InlineAlert tone="error">{projectsError}</InlineAlert></div>
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
								<Button
									variant="ghost"
									size="icon"
									onclick={() => removeProject(assignment.project_id)}
									title="Remove project access"
									aria-label="Remove project access"
								>
									<X size={12} />
								</Button>
							</div>
							<div class="perm-chip-row">
								{#each PROJECT_PERM_OPTIONS as opt}
									<span title={opt.desc}>
										<Checkbox
											checked={assignment.permissions.has(opt.id)}
											label={opt.label}
											onchange={() => toggleProjectPerm(assignment.project_id, opt.id)}
										/>
									</span>
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
							<Button variant="secondary" size="sm" onclick={() => addProject(project)}>
								<Folder size={11} />{project.name}
							</Button>
						{/each}
					</div>
				</div>
			{/if}

			{#if projectsError}
				<div role="alert"><InlineAlert tone="error">{projectsError}</InlineAlert></div>
			{/if}
			<div class="section-actions">
				<Button size="sm" disabled={savingProjects} onclick={saveProjectAssignments}>
					{#if savingProjects}<Spinner size={12} tone="current" />Saving…
					{:else if projectsSaved}<Check size={12} />Saved
					{:else}<Check size={12} />Save project access{/if}
				</Button>
			</div>
		{/if}
	</section>
</div>

<style>
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

	/* ── Role ── */
	.role-row { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
	.role-select-wrap { display: inline-flex; align-items: center; gap: 8px; min-width: 140px; }

	/* ── Permissions ── */
	.perm-groups { display: flex; flex-direction: column; gap: 10px; }
	.perm-group { display: flex; flex-direction: column; gap: 5px; }
	.perm-group-name { font-size: 10px; font-weight: 700; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.08em; }
	.perm-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 3px; }
	.perm-check { display: flex; padding: 4px 7px; font-size: 11px; }
	.perm-check :global(.ui-checkbox), .perm-chip-row :global(.ui-checkbox) { font-size: 11px; }

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

	.perm-chip-row {
		display: flex; gap: 4px 14px; flex-wrap: wrap;
		padding: 8px 10px;
	}

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

	/* ── Common ── */
	.section-actions { display: flex; justify-content: flex-end; }
	.loading-row { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-dim); }
</style>
