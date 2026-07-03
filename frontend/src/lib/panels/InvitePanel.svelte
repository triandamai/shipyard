<script lang="ts">
	import { api } from '$lib/api/client';
	import { PERMISSION_GROUPS } from '$lib/api/types';
	import type { MemberRole, Project, ProjectAssignment } from '$lib/api/types';
	import {
		Mail, ChevronDown, Check, Lock, Folder, FolderOpen,
		Search, X, Loader2, AlertCircle, UserPlus, ChevronRight
	} from '@lucide/svelte';

	const PROJECT_PERM_OPTIONS = [
		{ id: 'view',   label: 'View',   desc: 'Read-only access to services and deployments' },
		{ id: 'deploy', label: 'Deploy', desc: 'Trigger deployments and restarts' },
		{ id: 'manage', label: 'Manage', desc: 'Create, edit, and delete services' },
	];

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
	let projectPerms      = $state<Record<string, Set<string>>>({});
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
			projectPerms = { ...projectPerms, [projectId]: new Set(['view']) };
		}
		selectedProjects = sel;
		expandedProjects = exp;
	}

	function toggleExpandProject(projectId: string) {
		const next = new Set(expandedProjects);
		next.has(projectId) ? next.delete(projectId) : next.add(projectId);
		expandedProjects = next;
	}

	function toggleProjectPerm(projectId: string, permId: string) {
		const current = projectPerms[projectId] ?? new Set<string>();
		const next = new Set(current);
		next.has(permId) ? next.delete(permId) : next.add(permId);
		projectPerms = { ...projectPerms, [projectId]: next };
	}

	function buildAssignments(): ProjectAssignment[] {
		return [...selectedProjects].map(pid => ({
			project_id: pid,
			permissions: [...(projectPerms[pid] ?? [])],
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
			[...permissions],
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
		<div class="section-label"><Mail size={13} />Details</div>

		<div class="field">
			<label class="field-label" for="invite-email">Email address</label>
			<input
				id="invite-email"
				class="field-input"
				type="email"
				placeholder="colleague@example.com"
				bind:value={email}
				onkeydown={(e) => e.key === 'Enter' && handleInvite()}
			/>
		</div>

		<div class="field">
			<label class="field-label" for="invite-role">Role</label>
			<div class="select-wrap">
				<select id="invite-role" class="field-input field-select" bind:value={role}>
					{#each assignableRoles as r}
						<option value={r.value}>{r.label} — {r.desc}</option>
					{/each}
				</select>
				<ChevronDown size={13} class="select-chevron" />
			</div>
		</div>
	</section>

	<!-- ── Org permissions ─────────────────────────────────────────── -->
	<section class="section">
		<div class="section-label">
			<Lock size={13} />
			Organization permissions
			{#if permissions.size > 0}
				<span class="count-badge">{permissions.size}</span>
			{/if}
		</div>

		<div class="perm-groups">
			{#each PERMISSION_GROUPS as group}
				<div class="perm-group">
					<div class="group-name">{group.group}</div>
					<div class="perm-grid">
						{#each group.permissions as perm}
							{@const active = permissions.has(perm.id)}
							<button
								class="perm-row"
								class:active
								type="button"
								onclick={() => togglePerm(perm.id)}
								title={perm.description}
							>
								<span class="check-box" class:checked={active}>
									{#if active}<Check size={9} />{/if}
								</span>
								<span class="perm-label-text">{perm.label}</span>
							</button>
						{/each}
					</div>
				</div>
			{/each}
		</div>
	</section>

	<!-- ── Project access ──────────────────────────────────────────── -->
	<section class="section">
		<div class="section-label">
			<Folder size={13} />
			Project access
			{#if selectedProjects.size > 0}
				<span class="count-badge">{selectedProjects.size} project{selectedProjects.size === 1 ? '' : 's'}</span>
			{/if}
		</div>

		{#if allProjects.length === 0}
			<p class="empty-hint">No projects yet — create one first.</p>
		{:else}
			<!-- Search -->
			<div class="search-wrap">
				<Search size={13} class="search-icon" />
				<input
					class="search-input"
					type="text"
					placeholder="Search projects…"
					bind:value={projectSearch}
				/>
				{#if projectSearch}
					<button class="search-clear" onclick={() => (projectSearch = '')} type="button">
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
								<button
									class="check-box-btn"
									type="button"
									onclick={() => toggleProject(project.id)}
									aria-label="{isSelected ? 'Remove' : 'Add'} {project.name}"
								>
									<span class="check-box" class:checked={isSelected}>
										{#if isSelected}<Check size={9} />{/if}
									</span>
								</button>

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
										<button
											class="perm-row perm-row-sm"
											class:active={hasPerm}
											type="button"
											onclick={() => toggleProjectPerm(project.id, opt.id)}
											title={opt.desc}
										>
											<span class="check-box" class:checked={hasPerm}>
												{#if hasPerm}<Check size={9} />{/if}
											</span>
											<span class="perm-label-text">{opt.label}</span>
											<span class="perm-desc">{opt.desc}</span>
										</button>
									{/each}
								</div>
							{/if}
						</div>
					{/each}
				{/if}
			</div>
		{/if}
	</section>

	<!-- ── Footer ──────────────────────────────────────────────────── -->
	<div class="footer">
		{#if inviteError}
			<div class="status-msg error"><AlertCircle size={13} />{inviteError}</div>
		{/if}
		{#if inviteOk}
			<div class="status-msg success"><Check size={13} />{inviteOk}</div>
		{/if}
		<div class="footer-actions">
			<button class="btn-cancel" type="button" onclick={onClose}>Cancel</button>
			<button
				class="btn-invite"
				type="button"
				onclick={handleInvite}
				disabled={!email.trim() || inviting}
			>
				{#if inviting}
					<Loader2 size={14} class="spin" />Sending…
				{:else}
					<UserPlus size={14} />Send Invitation
				{/if}
			</button>
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
		gap: 12px;
		padding: 16px;
		border-bottom: 1px solid var(--border);
	}

	.section-label {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11px;
		font-weight: 700;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.count-badge {
		margin-left: 2px;
		padding: 1px 6px;
		background: rgba(37,99,235,.1);
		color: var(--accent);
		border: 1px solid rgba(37,99,235,.2);
		border-radius: 999px;
		font-size: 10px;
		font-weight: 700;
	}

	/* ── Fields ── */
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 12px; font-weight: 500; color: var(--text-muted); }
	.field-input {
		width: 100%;
		padding: 8px 10px;
		background: var(--bg-muted);
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 13px;
		color: var(--text-primary);
		font-family: var(--font-sans);
		outline: none;
		box-sizing: border-box;
		transition: border-color .15s;
	}
	.field-input:focus { border-color: var(--accent); }

	.select-wrap { position: relative; }
	.field-select { appearance: none; -webkit-appearance: none; cursor: pointer; padding-right: 30px; }
	:global(.select-chevron) {
		position: absolute; right: 10px; top: 50%; transform: translateY(-50%);
		color: var(--text-muted); pointer-events: none;
	}

	/* ── Permission groups ── */
	.perm-groups { display: flex; flex-direction: column; gap: 12px; }
	.perm-group  { display: flex; flex-direction: column; gap: 6px; }
	.group-name  {
		font-size: 10px; font-weight: 700; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em;
	}
	.perm-grid { display: flex; flex-direction: column; gap: 2px; }

	.perm-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 8px;
		background: transparent;
		border: 1px solid transparent;
		border-radius: 6px;
		cursor: pointer;
		text-align: left;
		width: 100%;
		transition: background .12s, border-color .12s;
	}
	.perm-row:hover { background: var(--bg-muted); }
	.perm-row.active { background: rgba(37,99,235,.05); border-color: rgba(37,99,235,.2); }

	.perm-row-sm { padding: 5px 8px; }

	.check-box {
		width: 14px; height: 14px; flex-shrink: 0;
		border: 1.5px solid var(--border);
		border-radius: 3px;
		background: var(--bg-surface);
		display: flex; align-items: center; justify-content: center;
		color: #fff;
		transition: background .12s, border-color .12s;
	}
	.check-box.checked { background: var(--accent); border-color: var(--accent); }

	.perm-label-text { font-size: 12px; color: var(--text-primary); flex: 1; }
	.perm-desc { font-size: 11px; color: var(--text-muted); }

	/* ── Search ── */
	.search-wrap {
		position: relative;
		display: flex;
		align-items: center;
	}
	:global(.search-icon) {
		position: absolute; left: 10px;
		color: var(--text-muted); pointer-events: none;
	}
	.search-input {
		width: 100%;
		padding: 7px 32px;
		background: var(--bg-muted);
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 13px;
		color: var(--text-primary);
		outline: none;
		box-sizing: border-box;
	}
	.search-input:focus { border-color: var(--accent); }
	.search-clear {
		position: absolute; right: 8px;
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
		border-radius: 7px;
		overflow: hidden;
		transition: border-color .15s;
	}
	.project-card.selected { border-color: rgba(37,99,235,.35); }

	.project-row {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 10px;
		background: transparent;
	}
	.project-card.selected .project-row { background: rgba(37,99,235,.02); }

	.check-box-btn {
		background: transparent; border: none; cursor: pointer;
		padding: 0; display: flex; align-items: center; flex-shrink: 0;
	}

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
		transition: color .12s, background .12s;
	}
	.expand-btn:hover { color: var(--text-primary); background: var(--bg-muted); }
	:global(.expand-btn .rotated) { transform: rotate(90deg); }

	.project-perms {
		display: flex;
		flex-direction: column;
		gap: 2px;
		padding: 6px 10px 8px;
		border-top: 1px solid var(--border);
		background: rgba(37,99,235,.02);
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

	.status-msg {
		display: flex; align-items: center; gap: 6px;
		font-size: 12px; padding: 8px 10px; border-radius: 6px;
	}
	.status-msg.error   { background: rgba(239,68,68,.08);  color: #ef4444; border: 1px solid rgba(239,68,68,.2); }
	.status-msg.success { background: rgba(22,163,74,.08);  color: #16a34a; border: 1px solid rgba(22,163,74,.2); }

	.footer-actions { display: flex; gap: 8px; }

	.btn-invite {
		flex: 1;
		display: flex; align-items: center; justify-content: center; gap: 6px;
		padding: 8px 16px;
		background: var(--accent);
		color: #fff;
		border: none; border-radius: 6px;
		font-size: 13px; font-weight: 500;
		cursor: pointer;
		transition: opacity .15s;
	}
	.btn-invite:disabled { opacity: .55; cursor: not-allowed; }

	.btn-cancel {
		padding: 8px 14px;
		background: var(--bg-muted);
		color: var(--text-muted);
		border: 1px solid var(--border); border-radius: 6px;
		font-size: 13px; cursor: pointer;
		transition: color .15s, background .15s;
	}
	.btn-cancel:hover { color: var(--text-primary); background: var(--bg-elevated); }

	:global(.spin) { animation: spin .8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
