<script lang="ts">
	import { api } from '$lib/api/client';
	import { PERMISSION_GROUPS, PROJECT_PERM_OPTIONS, buildOrgPermission, expandProjectPermissions } from '$lib/api/types';
	import type { MemberRole, Project, ProjectAssignment, ProjectPermTier } from '$lib/api/types';
	import {
		Mail, Lock, Folder, FolderOpen,
		Search, X, UserPlus, ChevronRight
	} from '@lucide/svelte';
	import { Button, Badge, Checkbox, FormField, InlineAlert, SectionLabel, Select, Spinner, TextField } from '$lib/components/ui';

	const ROLES: { value: MemberRole; label: string; desc: string }[] = [
		{ value: 'owner',  label: 'Owner',  desc: 'Full control, can manage billing and delete org' },
		{ value: 'admin',  label: 'Admin',  desc: 'Manage members, projects, and settings' },
		{ value: 'member', label: 'Member', desc: 'Deploy and manage services in projects' },
		{ value: 'viewer', label: 'Viewer', desc: 'Read-only access to all resources' },
	];

	interface Props {
		orgId: string;
		allProjects: Project[];
		isOwner: boolean;
		onClose: () => void;
		onInvited?: () => void;
	}

	let { orgId, allProjects, isOwner, onClose, onInvited }: Props = $props();

	// ── Form state ────────────────────────────────────────────────────
	let email       = $state('');
	let role        = $state<MemberRole>('member');
	let permissions = $state<Set<string>>(new Set());

	// Project assignment
	let projectSearch     = $state('');
	let selectedProjects  = $state<Set<string>>(new Set());
	let projectPerms      = $state<Record<string, Set<ProjectPermTier>>>({});
	let expandedProjects  = $state<Set<string>>(new Set());

	// Submit
	let inviting    = $state(false);
	let inviteError = $state('');
	let inviteOk    = $state('');

	// ── Derived ───────────────────────────────────────────────────────
	let filteredProjects = $derived(
		allProjects.filter(p =>
			p.name.toLowerCase().includes(projectSearch.toLowerCase()) ||
			p.slug.toLowerCase().includes(projectSearch.toLowerCase())
		)
	);

	let assignableRoles = $derived(
		isOwner
			? ROLES
			: ROLES.filter(r => r.value !== 'owner')
	);

	// ── Helpers ───────────────────────────────────────────────────────
	function togglePerm(id: string) {
		const next = new Set(permissions);
		next.has(id) ? next.delete(id) : next.add(id);
		permissions = next;
	}

	function toggleProject(projectId: string) {
		const sel = new Set(selectedProjects);
		const exp = new Set(expandedProjects);
		if (sel.has(projectId)) {
			sel.delete(projectId);
			exp.delete(projectId);
			const { [projectId]: _, ...rest } = projectPerms;
			projectPerms = rest;
		} else {
			sel.add(projectId);
			exp.add(projectId);
			projectPerms = { ...projectPerms, [projectId]: new Set<ProjectPermTier>(['view']) };
		}
		selectedProjects = sel;
		expandedProjects = exp;
	}

	function toggleExpandProject(projectId: string) {
		const next = new Set(expandedProjects);
		next.has(projectId) ? next.delete(projectId) : next.add(projectId);
		expandedProjects = next;
	}

	function toggleProjectPerm(projectId: string, permId: ProjectPermTier) {
		const current = projectPerms[projectId] ?? new Set<ProjectPermTier>();
		const next = new Set(current);
		next.has(permId) ? next.delete(permId) : next.add(permId);
		projectPerms = { ...projectPerms, [projectId]: next };
	}

	function buildAssignments(): ProjectAssignment[] {
		return [...selectedProjects].map(pid => ({
			project_id: pid,
			permissions: expandProjectPermissions(orgId, pid, projectPerms[pid] ?? new Set()),
		}));
	}

	// ── Submit ────────────────────────────────────────────────────────
	async function handleInvite() {
		if (!email.trim() || inviting) return;
		inviting = true;
		inviteError = '';
		inviteOk = '';

		const res = await api.inviteMember(
			orgId,
			email.trim(),
			role,
			[...permissions].map(suffix => buildOrgPermission(orgId, suffix)),
			buildAssignments()
		);

		if (res.error) {
			inviteError = res.error.message;
		} else {
			inviteOk = `Invitation sent to ${email.trim()}`;
			email = '';
			role = 'member';
			permissions = new Set();
			selectedProjects = new Set();
			projectPerms = {};
			expandedProjects = new Set();
			onInvited?.();
			setTimeout(() => { inviteOk = ''; }, 4000);
		}
		inviting = false;
	}
</script>

<div class="panel">
	<!-- ── Details ─────────────────────────────────────────────────── -->
	<section class="section">
		<SectionLabel><span class="label-inner"><Mail size={13} />Details</span></SectionLabel>

		<div class="section-body">
			<FormField label="Email address" for="invite-email">
				<TextField
					id="invite-email"
					type="email"
					placeholder="colleague@example.com"
					bind:value={email}
					onkeydown={(e) => e.key === 'Enter' && handleInvite()}
				/>
			</FormField>

			<FormField label="Role" for="invite-role">
				<Select
					id="invite-role"
					bind:value={() => role, (v) => (role = v as MemberRole)}
					options={assignableRoles.map(r => ({ value: r.value, label: `${r.label} — ${r.desc}` }))}
				/>
			</FormField>
		</div>
	</section>

	<!-- ── Org permissions ─────────────────────────────────────────── -->
	<section class="section">
		<SectionLabel>
			<span class="label-inner">
				<Lock size={13} />
				Organization permissions
				{#if permissions.size > 0}
					<Badge tone="blue">{permissions.size}</Badge>
				{/if}
			</span>
		</SectionLabel>

		<div class="section-body perm-groups">
			{#each PERMISSION_GROUPS as group}
				<div class="perm-group">
					<div class="group-name">{group.group}</div>
					<div class="perm-grid">
						{#each group.permissions as perm}
							{@const active = permissions.has(perm.id)}
							<div class="perm-row" class:active title={perm.description}>
								<Checkbox checked={active} label={perm.label} onchange={() => togglePerm(perm.id)} />
							</div>
						{/each}
					</div>
				</div>
			{/each}
		</div>
	</section>

	<!-- ── Project access ──────────────────────────────────────────── -->
	<section class="section">
		<SectionLabel>
			<span class="label-inner">
				<Folder size={13} />
				Project access
				{#if selectedProjects.size > 0}
					<Badge tone="blue">{selectedProjects.size} project{selectedProjects.size === 1 ? '' : 's'}</Badge>
				{/if}
			</span>
		</SectionLabel>

		<div class="section-body">
		{#if allProjects.length === 0}
			<p class="empty-hint">No projects yet — create one first.</p>
		{:else}
			<!-- Search -->
			<div class="search-wrap">
				<TextField type="text" placeholder="Search projects…" bind:value={projectSearch}>
					{#snippet icon()}<Search size={13} />{/snippet}
				</TextField>
				{#if projectSearch}
					<button class="search-clear" onclick={() => (projectSearch = '')} type="button" aria-label="Clear search">
						<X size={11} />
					</button>
				{/if}
			</div>

			<!-- Project list -->
			<div class="project-list">
				{#if filteredProjects.length === 0}
					<p class="empty-hint">No projects match "{projectSearch}"</p>
				{:else}
					{#each filteredProjects as project (project.id)}
						{@const isSelected = selectedProjects.has(project.id)}
						{@const isExpanded = expandedProjects.has(project.id)}

						<div class="project-card" class:selected={isSelected}>
							<!-- Project row -->
							<div class="project-row">
								<!-- Checkbox -->
								<span role="group" aria-label="{isSelected ? 'Remove' : 'Add'} {project.name}">
									<Checkbox checked={isSelected} onchange={() => toggleProject(project.id)} />
								</span>

								<!-- Name -->
								<button
									class="project-name-btn"
									type="button"
									onclick={() => isSelected ? toggleExpandProject(project.id) : toggleProject(project.id)}
								>
									{#if isSelected && isExpanded}
										<FolderOpen size={13} class="folder-icon selected" />
									{:else if isSelected}
										<Folder size={13} class="folder-icon selected" />
									{:else}
										<Folder size={13} class="folder-icon" />
									{/if}
									<span class="proj-name">{project.name}</span>
									<span class="proj-slug">{project.slug}</span>
								</button>

								<!-- Expand toggle (only when selected) -->
								{#if isSelected}
									<button
										class="expand-btn"
										type="button"
										onclick={() => toggleExpandProject(project.id)}
										aria-label="{isExpanded ? 'Collapse' : 'Expand'} permissions"
									>
										<ChevronRight size={13} class={isExpanded ? 'rotated' : ''} />
									</button>
								{/if}
							</div>

							<!-- Permission options (expanded) -->
							{#if isSelected && isExpanded}
								<div class="project-perms">
									{#each PROJECT_PERM_OPTIONS as opt}
										{@const hasPerm = projectPerms[project.id]?.has(opt.id) ?? false}
										<div class="perm-row perm-row-sm" class:active={hasPerm} title={opt.desc}>
											<Checkbox checked={hasPerm} label={opt.label} onchange={() => toggleProjectPerm(project.id, opt.id)} />
											<span class="perm-desc">{opt.desc}</span>
										</div>
									{/each}
								</div>
							{/if}
						</div>
					{/each}
				{/if}
			</div>
		{/if}
		</div>
	</section>

	<!-- ── Footer ──────────────────────────────────────────────────── -->
	<div class="footer">
		{#if inviteError}
			<div role="alert"><InlineAlert tone="error">{inviteError}</InlineAlert></div>
		{/if}
		{#if inviteOk}
			<div role="status"><InlineAlert tone="success">{inviteOk}</InlineAlert></div>
		{/if}
		<div class="footer-actions">
			<Button variant="secondary" onclick={onClose}>Cancel</Button>
			<div class="invite-btn">
				<Button variant="primary" onclick={handleInvite} disabled={!email.trim() || inviting}>
					{#if inviting}
						<Spinner size={14} tone="current" />Sending…
					{:else}
						<UserPlus size={14} />Send Invitation
					{/if}
				</Button>
			</div>
		</div>
	</div>
</div>

<style>
	.panel {
		display: flex;
		flex-direction: column;
		min-height: 100%;
	}

	/* ── Sections ── */
	.section {
		display: flex;
		flex-direction: column;
		padding: 16px;
		border-bottom: 1px solid var(--border);
	}

	.section-body {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.label-inner {
		display: inline-flex;
		align-items: center;
		gap: 6px;
	}

	/* ── Permission groups ── */
	.perm-groups { gap: 12px; }
	.perm-group  { display: flex; flex-direction: column; gap: 6px; }
	.group-name  {
		font-size: 10px; font-weight: 700; color: var(--text-dim);
	}
	.perm-grid { display: flex; flex-direction: column; gap: 2px; }

	.perm-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 8px;
		border: 1px solid transparent;
		border-radius: var(--radius-sm);
		transition: background var(--transition-fast), border-color var(--transition-fast);
	}
	.perm-row:hover { background: var(--bg-hover); }
	.perm-row.active {
		background: color-mix(in srgb, var(--accent) 5%, transparent);
		border-color: color-mix(in srgb, var(--accent) 20%, transparent);
	}

	.perm-row-sm { padding: 5px 8px; }

	.perm-desc { font-size: 11px; color: var(--text-muted); }

	/* ── Search ── */
	.search-wrap {
		position: relative;
	}
	.search-clear {
		position: absolute; right: 8px; top: 50%; transform: translateY(-50%);
		background: transparent; border: none;
		cursor: pointer; color: var(--text-muted);
		display: flex; align-items: center; padding: 2px;
		border-radius: 3px;
	}
	.search-clear:hover { color: var(--text-primary); }

	/* ── Project list ── */
	.project-list { display: flex; flex-direction: column; gap: 4px; }
	.empty-hint { font-size: 12px; color: var(--text-muted); font-style: italic; margin: 0; }

	.project-card {
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
		transition: border-color var(--transition-fast);
	}
	.project-card.selected { border-color: color-mix(in srgb, var(--accent) 35%, transparent); }

	.project-row {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 10px;
		background: transparent;
	}
	.project-card.selected .project-row { background: color-mix(in srgb, var(--accent) 2%, transparent); }

	.project-name-btn {
		display: flex; align-items: center; gap: 7px;
		flex: 1; background: transparent; border: none;
		cursor: pointer; text-align: left; padding: 0; min-width: 0;
	}
	:global(.folder-icon) { color: var(--text-muted); flex-shrink: 0; }
	:global(.folder-icon.selected) { color: var(--accent); }

	.proj-name { font-size: 13px; font-weight: 500; color: var(--text-primary); flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.proj-slug { font-size: 11px; color: var(--text-muted); font-family: var(--font-mono); flex-shrink: 0; }

	.expand-btn {
		background: transparent; border: none; cursor: pointer;
		color: var(--text-muted); display: flex; align-items: center;
		padding: 4px; border-radius: 4px; flex-shrink: 0;
		transition: color var(--transition-fast), background var(--transition-fast);
	}
	.expand-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
	:global(.expand-btn .rotated) { transform: rotate(90deg); }

	.project-perms {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 6px 10px 8px;
		border-top: 1px solid var(--border);
		background: color-mix(in srgb, var(--accent) 2%, transparent);
	}

	/* ── Footer ── */
	.footer {
		margin-top: auto;
		padding: 14px 16px;
		border-top: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		gap: 10px;
		background: var(--bg-surface);
		position: sticky;
		bottom: 0;
	}

	.footer-actions { display: flex; gap: 8px; }
	.invite-btn { flex: 1; display: flex; }
	.invite-btn :global(.ui-btn) { width: 100%; }
</style>
