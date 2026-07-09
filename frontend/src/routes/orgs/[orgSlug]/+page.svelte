<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import {
		FolderOpen, Server, Activity, Users, Plus, ArrowRight,
		Loader2, Zap, GitBranch, Box, Settings, ChevronRight, Circle
	} from '@lucide/svelte';
	import type { Deployment, Project, Service } from '$lib/api/types';

	let orgSlug = $derived(page.params.orgSlug ?? '');
	let org     = $derived($orgStore.activeOrg);
	let projects = $derived($projectStore.projects);

	// ── Data state ────────────────────────────────────────────────────
	type RichDeployment = Deployment & { serviceName: string; projectName: string; projectSlug: string };

	let recentDeployments = $state<RichDeployment[]>([]);
	let allServices       = $state<Service[]>([]);
	let memberCount       = $state(0);
	// projectId → service[]
	let servicesByProject = $state<Record<string, Service[]>>({});
	let loading           = $state(true);

	// ── Derived stats ─────────────────────────────────────────────────
	let totalServices   = $derived(allServices.length);
	let runningServices = $derived(allServices.filter(s => s.status === 'running').length);
	let failedServices  = $derived(allServices.filter(s => s.status === 'failed').length);
	let successDeploys  = $derived(recentDeployments.filter(d => d.status === 'success').length);

	async function fetchStats(activeOrgId: string, projectsList: Project[]) {
		loading = true;
		try {
			// Members count
			const membersRes = await api.getMembers(activeOrgId);
			if (membersRes.data) memberCount = membersRes.data.length;

			// Services per project + recent deployments
			const svcs: Service[] = [];
			const deploys: RichDeployment[] = [];
			const byProject: Record<string, Service[]> = {};

			await Promise.all(
				projectsList.slice(0, 10).map(async (p: Project) => {
					const svcsRes = await api.getServices(p.id);
					const pSvcs = svcsRes.data ?? [];
					svcs.push(...pSvcs);
					byProject[p.id] = pSvcs;

					await Promise.all(
						pSvcs.slice(0, 4).map(async (svc) => {
							const depRes = await api.getDeployments(svc.id);
							const recent = (depRes.data ?? []).slice(0, 2);
							deploys.push(...recent.map(d => ({
								...d,
								serviceName: svc.name,
								projectName: p.name,
								projectSlug: p.slug,
							})));
						})
					);
				})
			);

			allServices = svcs;
			servicesByProject = byProject;
			recentDeployments = deploys
				.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
				.slice(0, 10);
		} catch (err) {
			console.error("Failed to fetch homepage stats:", err);
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		const activeOrgId = org?.id;
		const projectsList = projects;

		if (activeOrgId) {
			void fetchStats(activeOrgId, projectsList);
		}
	});

	function statusColor(status: string) {
		switch (status) {
			case 'success': return 'dep-success';
			case 'failed':  return 'dep-failed';
			case 'running': return 'dep-running';
			default:        return 'dep-pending';
		}
	}

	function timeAgo(dateStr: string | null | undefined): string {
		if (!dateStr) return '';
		const diff = Date.now() - new Date(dateStr).getTime();
		const m = Math.floor(diff / 60000);
		if (m < 1)  return 'just now';
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		return `${Math.floor(h / 24)}d ago`;
	}

	function projectInitials(name: string) {
		return name.split(/\s+/).map(w => w[0]).join('').toUpperCase().slice(0, 2);
	}

	// Deterministic hue from project name for avatar colour
	function projectHue(name: string) {
		let h = 0;
		for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) & 0xffff;
		return h % 360;
	}
</script>

<div class="dash">

	<!-- ── Header ─────────────────────────────────────────────────────── -->
	<div class="dash-header">
		<div class="dash-header-left">
			<div class="org-avatar">{org?.name?.charAt(0).toUpperCase() ?? '?'}</div>
			<div>
				<h1 class="dash-title">{org?.name ?? 'Organization'}</h1>
				<p class="dash-subtitle">
					{projects.length} project{projects.length !== 1 ? 's' : ''} ·
					{totalServices} service{totalServices !== 1 ? 's' : ''}
					{#if runningServices > 0}
						· <span class="running-pill">{runningServices} running</span>
					{/if}
				</p>
			</div>
		</div>
		<div class="dash-actions">
			<button class="btn btn-ghost btn-sm" onclick={() => goto(`/orgs/${orgSlug}/settings`)}>
				<Settings size={13} />
				Settings
			</button>
			<button class="btn btn-primary btn-sm" onclick={() => goto(`/orgs/${orgSlug}/projects`)}>
				<Plus size={13} />
				New Project
			</button>
		</div>
	</div>

	<!-- ── Stat cards ─────────────────────────────────────────────────── -->
	<div class="stats-grid">
		<div class="stat-card">
			<div class="stat-top">
				<span class="stat-label">Projects</span>
				<div class="stat-icon" style="background:rgba(99,102,241,.12);color:#6366f1">
					<FolderOpen size={15} />
				</div>
			</div>
			<span class="stat-value">{projects.length}</span>
			<span class="stat-sub">Total workspaces</span>
		</div>

		<div class="stat-card">
			<div class="stat-top">
				<span class="stat-label">Services</span>
				<div class="stat-icon" style="background:rgba(16,185,129,.12);color:#10B981">
					<Server size={15} />
				</div>
			</div>
			<span class="stat-value">{loading ? '—' : totalServices}</span>
			<span class="stat-sub">
				{#if !loading}
					<span class="pill-green">{runningServices} up</span>
					{#if failedServices > 0}
						<span class="pill-red">{failedServices} failed</span>
					{/if}
				{:else}
					Loading…
				{/if}
			</span>
		</div>

		<div class="stat-card">
			<div class="stat-top">
				<span class="stat-label">Deployments</span>
				<div class="stat-icon" style="background:rgba(245,158,11,.12);color:#F59E0B">
					<Activity size={15} />
				</div>
			</div>
			<span class="stat-value">{loading ? '—' : recentDeployments.length}</span>
			<span class="stat-sub">
				{#if !loading}
					<span class="pill-green">{successDeploys} succeeded</span>
				{:else}
					Loading…
				{/if}
			</span>
		</div>

		<div class="stat-card">
			<div class="stat-top">
				<span class="stat-label">Members</span>
				<div class="stat-icon" style="background:rgba(236,72,153,.12);color:#EC4899">
					<Users size={15} />
				</div>
			</div>
			<span class="stat-value">{memberCount || '—'}</span>
			<span class="stat-sub">
				<a class="stat-link" href="/orgs/{orgSlug}/settings/members">Manage team</a>
			</span>
		</div>
	</div>

	<!-- ── Main grid ──────────────────────────────────────────────────── -->
	<div class="main-grid">

		<!-- Projects panel -->
		<section class="panel">
			<div class="panel-header">
				<span class="panel-title"><FolderOpen size={14} />Projects</span>
				<a class="panel-link" href="/orgs/{orgSlug}/projects">
					View all <ArrowRight size={11} />
				</a>
			</div>

			{#if projects.length === 0}
				<div class="empty-state">
					<FolderOpen size={28} />
					<p>No projects yet</p>
					<button class="btn btn-primary btn-sm" onclick={() => goto(`/orgs/${orgSlug}/projects`)}>
						Create first project
					</button>
				</div>
			{:else}
				<div class="project-grid">
					{#each projects.slice(0, 6) as project (project.id)}
						{@const hue = projectHue(project.name)}
						{@const pSvcs = servicesByProject[project.id] ?? []}
						{@const running = pSvcs.filter(s => s.status === 'running').length}
						<a class="project-card" href="/orgs/{orgSlug}/projects/{project.slug}">
							<div class="project-card-top">
								<div class="project-avatar" style="background:hsl({hue},70%,20%);color:hsl({hue},80%,70%)">
									{projectInitials(project.name)}
								</div>
								<ChevronRight size={13} class="project-chevron" />
							</div>
							<div class="project-card-body">
								<span class="project-name">{project.name}</span>
								<span class="project-slug font-mono">{project.slug}</span>
							</div>
							<div class="project-card-footer">
								<span class="project-stat">
									<Box size={11} />
									{loading ? '…' : pSvcs.length} service{pSvcs.length !== 1 ? 's' : ''}
								</span>
								{#if running > 0}
									<span class="project-running">
										<Circle size={7} fill="#10B981" color="#10B981" />
										{running} up
									</span>
								{/if}
							</div>
						</a>
					{/each}
				</div>
			{/if}
		</section>

		<!-- Right column -->
		<div class="right-col">

			<!-- Recent Deployments -->
			<section class="panel">
				<div class="panel-header">
					<span class="panel-title"><Activity size={14} />Recent Deployments</span>
					{#if loading}
						<Loader2 size={13} class="spin-icon" />
					{/if}
				</div>

				{#if loading}
					<div class="skeleton-list">
						{#each [1,2,3,4] as _}
							<div class="skeleton-row">
								<div class="skel skel-circle"></div>
								<div class="skel-lines">
									<div class="skel skel-line w60"></div>
									<div class="skel skel-line w40"></div>
								</div>
								<div class="skel skel-line w20"></div>
							</div>
						{/each}
					</div>
				{:else if recentDeployments.length === 0}
					<div class="empty-state">
						<Activity size={24} />
						<p>No deployments yet.<br/>Deploy a service to get started.</p>
					</div>
				{:else}
					<ul class="dep-list">
						{#each recentDeployments as dep (dep.id)}
							<li class="dep-row">
								<span class="dep-dot {statusColor(dep.status)}"></span>
								<div class="dep-body">
									<span class="dep-name">{dep.serviceName}</span>
									<span class="dep-meta">{dep.projectName} · {dep.source_ref}</span>
								</div>
								<div class="dep-right">
									<span class="dep-status-text {statusColor(dep.status)}">{dep.status}</span>
									<span class="dep-time">{timeAgo(dep.created_at)}</span>
								</div>
							</li>
						{/each}
					</ul>
				{/if}
			</section>

			<!-- Quick actions -->
			<section class="panel quick-actions">
				<div class="panel-header">
					<span class="panel-title"><Zap size={14} />Quick Actions</span>
				</div>
				<div class="qa-grid">
					<button class="qa-btn" onclick={() => goto(`/orgs/${orgSlug}/projects`)}>
						<div class="qa-icon" style="background:rgba(99,102,241,.12);color:#6366f1"><FolderOpen size={16} /></div>
						<span>New Project</span>
					</button>
					<button class="qa-btn" onclick={() => goto(`/orgs/${orgSlug}/settings/members`)}>
						<div class="qa-icon" style="background:rgba(236,72,153,.12);color:#EC4899"><Users size={16} /></div>
						<span>Invite Member</span>
					</button>
					<button class="qa-btn" onclick={() => goto(`/orgs/${orgSlug}/settings`)}>
						<div class="qa-icon" style="background:rgba(245,158,11,.12);color:#F59E0B"><Settings size={16} /></div>
						<span>Settings</span>
					</button>
					<button class="qa-btn" onclick={() => goto(`/orgs/${orgSlug}/settings/api-keys`)}>
						<div class="qa-icon" style="background:rgba(16,185,129,.12);color:#10B981"><GitBranch size={16} /></div>
						<span>API Keys</span>
					</button>
				</div>
			</section>

		</div>
	</div>
</div>

<style>
	.dash {
		padding: 28px 32px;
		height: 100%;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	@media (max-width: 639px) {
		.dash { padding: 16px 16px 80px; }
	}

	/* ── Header ───────────────────────────────────────────────────────── */
	.dash-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.dash-header-left {
		display: flex;
		align-items: center;
		gap: 14px;
	}
	.org-avatar {
		width: 44px;
		height: 44px;
		border-radius: 12px;
		background: linear-gradient(135deg, #6366f1, #8b5cf6);
		color: #fff;
		font-size: 18px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.dash-title {
		font-size: 20px;
		font-weight: 700;
		color: var(--text-primary);
		margin: 0;
		letter-spacing: -0.02em;
	}
	.dash-subtitle {
		font-size: 12px;
		color: var(--text-muted);
		margin: 2px 0 0;
	}
	.running-pill {
		color: #10B981;
		font-weight: 600;
	}
	.dash-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
	}

	/* ── Stats ────────────────────────────────────────────────────────── */
	.stats-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 12px;
	}
	@media (max-width: 900px) {
		.stats-grid { grid-template-columns: repeat(2, 1fr); }
	}

	.stat-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.stat-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.stat-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.07em;
	}
	.stat-icon {
		width: 28px;
		height: 28px;
		border-radius: 8px;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.stat-value {
		font-size: 28px;
		font-weight: 700;
		color: var(--text-primary);
		line-height: 1;
	}
	.stat-sub {
		font-size: 11px;
		color: var(--text-muted);
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.pill-green {
		background: rgba(16,185,129,.12);
		color: #10B981;
		border-radius: 20px;
		padding: 1px 7px;
		font-weight: 600;
		font-size: 10px;
	}
	.pill-red {
		background: rgba(239,68,68,.12);
		color: #EF4444;
		border-radius: 20px;
		padding: 1px 7px;
		font-weight: 600;
		font-size: 10px;
	}
	.stat-link {
		color: var(--accent);
		text-decoration: none;
		font-size: 11px;
	}
	.stat-link:hover { text-decoration: underline; }

	/* ── Main grid ────────────────────────────────────────────────────── */
	.main-grid {
		display: grid;
		grid-template-columns: 1fr 380px;
		gap: 16px;
		align-items: start;
	}
	@media (max-width: 1024px) {
		.main-grid { grid-template-columns: 1fr; }
	}

	.right-col {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	/* ── Panel ────────────────────────────────────────────────────────── */
	.panel {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
	}
	.panel-title {
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
	}
	.panel-link {
		display: flex;
		align-items: center;
		gap: 3px;
		font-size: 11px;
		color: var(--accent);
		text-decoration: none;
	}
	.panel-link:hover { opacity: 0.8; }
	:global(.spin-icon) { animation: spin .8s linear infinite; color: var(--text-dim); }
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ── Project cards ────────────────────────────────────────────────── */
	.project-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 1px;
		background: var(--border);
	}
	.project-card {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px;
		background: var(--bg-surface);
		text-decoration: none;
		color: inherit;
		transition: background var(--transition-fast);
	}
	.project-card:hover { background: var(--bg-elevated); }
	.project-card-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.project-avatar {
		width: 34px;
		height: 34px;
		border-radius: 8px;
		font-size: 13px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	:global(.project-chevron) { color: var(--text-dim); }
	.project-card:hover :global(.project-chevron) { color: var(--accent); }
	.project-card-body {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.project-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.project-slug {
		font-size: 10px;
		color: var(--text-dim);
	}
	.project-card-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.project-stat {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		color: var(--text-muted);
	}
	.project-running {
		display: flex;
		align-items: center;
		gap: 3px;
		font-size: 10px;
		color: #10B981;
		font-weight: 600;
	}

	/* ── Deployments list ─────────────────────────────────────────────── */
	.dep-list {
		list-style: none;
		margin: 0;
		padding: 4px 0;
	}
	.dep-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 9px 16px;
		border-bottom: 1px solid var(--border);
	}
	.dep-row:last-child { border-bottom: none; }
	.dep-dot {
		width: 8px;
		height: 8px;
		border-radius: 50%;
		flex-shrink: 0;
	}
	.dep-dot.dep-success { background: #10B981; }
	.dep-dot.dep-failed  { background: #EF4444; }
	.dep-dot.dep-running { background: #3B82F6; animation: pulse 1.5s ease infinite; }
	.dep-dot.dep-pending { background: #6B7280; }
	@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:.4} }
	.dep-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}
	.dep-name {
		font-size: 12px;
		font-weight: 500;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.dep-meta {
		font-size: 10px;
		color: var(--text-dim);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.dep-right {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 2px;
		flex-shrink: 0;
	}
	.dep-status-text {
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.dep-status-text.dep-success { color: #10B981; }
	.dep-status-text.dep-failed  { color: #EF4444; }
	.dep-status-text.dep-running { color: #3B82F6; }
	.dep-status-text.dep-pending { color: #6B7280; }
	.dep-time {
		font-size: 10px;
		color: var(--text-dim);
	}

	/* ── Skeleton ─────────────────────────────────────────────────────── */
	.skeleton-list { padding: 8px 16px; display: flex; flex-direction: column; gap: 12px; }
	.skeleton-row  { display: flex; align-items: center; gap: 10px; }
	.skel { background: var(--bg-elevated); border-radius: 4px; animation: shimmer 1.4s ease infinite; }
	.skel-circle { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
	.skel-lines { flex: 1; display: flex; flex-direction: column; gap: 5px; }
	.skel-line { height: 10px; }
	.w60 { width: 60%; }
	.w40 { width: 40%; }
	.w20 { width: 50px; }
	@keyframes shimmer { 0%,100%{opacity:.6} 50%{opacity:.3} }

	/* ── Quick actions ────────────────────────────────────────────────── */
	.qa-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1px;
		background: var(--border);
	}
	.qa-btn {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		padding: 16px 12px;
		background: var(--bg-surface);
		border: none;
		cursor: pointer;
		color: var(--text-secondary);
		font-size: 12px;
		font-weight: 500;
		transition: background var(--transition-fast);
	}
	.qa-btn:hover { background: var(--bg-elevated); color: var(--text-primary); }
	.qa-icon {
		width: 36px;
		height: 36px;
		border-radius: 10px;
		display: flex;
		align-items: center;
		justify-content: center;
	}

	/* ── Empty state ──────────────────────────────────────────────────── */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 10px;
		padding: 36px 24px;
		color: var(--text-muted);
		font-size: 12px;
		text-align: center;
	}
	.empty-state p { margin: 0; line-height: 1.6; }

	.font-mono { font-family: var(--font-mono); }
</style>
