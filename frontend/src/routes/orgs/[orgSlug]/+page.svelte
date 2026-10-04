<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import {
		FolderOpen, Server, Activity, Users, Plus, ArrowRight,
		Zap, GitBranch, Box, Settings, ChevronRight,
		CheckCircle2, XCircle, Clock
	} from '@lucide/svelte';
	import {
		PageHeader, StatCard, Card, ListRow, ActivityList, Button, EmptyState,
		Badge, Avatar, Skeleton, Spinner, SectionLabel
	} from '$lib/components/ui';
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

	function statusTone(status: string): 'green' | 'red' | 'blue' | 'yellow' {
		switch (status) {
			case 'success': return 'green';
			case 'failed':  return 'red';
			case 'running': return 'blue';
			default:        return 'yellow';
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

	// Deterministic avatar tone from project name
	const AVATAR_TONES = ['blue', 'green', 'yellow', 'red', 'purple'] as const;
	function projectTone(name: string) {
		let h = 0;
		for (let i = 0; i < name.length; i++) h = (h * 31 + name.charCodeAt(i)) & 0xffff;
		return AVATAR_TONES[h % AVATAR_TONES.length];
	}
</script>

<div class="dash">

	<!-- ── Header ─────────────────────────────────────────────────────── -->
	<div class="dash-header">
		<Avatar initials={org?.name?.charAt(0) ?? '?'} size={44} />
		<div class="dash-header-main">
			<PageHeader
				title={org?.name ?? 'Organization'}
				subtitle="{projects.length} project{projects.length !== 1 ? 's' : ''} · {totalServices} service{totalServices !== 1 ? 's' : ''}{runningServices > 0 ? ` · ${runningServices} running` : ''}"
			>
				{#snippet actions()}
					<Button variant="ghost" size="sm" onclick={() => goto(`/orgs/${orgSlug}/settings`)}>
						<Settings size={13} />
						Settings
					</Button>
					<Button size="sm" onclick={() => goto(`/orgs/${orgSlug}/projects`)}>
						<Plus size={13} />
						New Project
					</Button>
				{/snippet}
			</PageHeader>
		</div>
	</div>

	<!-- ── Stat cards ─────────────────────────────────────────────────── -->
	<div class="stats-grid">
		<StatCard tone="blue" value={projects.length} label="Projects · Total workspaces">
			{#snippet icon()}<FolderOpen size={15} />{/snippet}
		</StatCard>

		<StatCard
			tone="green"
			value={loading ? '—' : totalServices}
			label={loading ? 'Services · Loading…' : `Services · ${runningServices} up${failedServices > 0 ? ` · ${failedServices} failed` : ''}`}
		>
			{#snippet icon()}<Server size={15} />{/snippet}
		</StatCard>

		<StatCard
			tone="yellow"
			value={loading ? '—' : recentDeployments.length}
			label={loading ? 'Deployments · Loading…' : `Deployments · ${successDeploys} succeeded`}
		>
			{#snippet icon()}<Activity size={15} />{/snippet}
		</StatCard>

		<a class="stat-link" href="/orgs/{orgSlug}/settings/members" aria-label="Members: manage team">
			<StatCard tone="red" value={memberCount || '—'} label="Members · Manage team">
				{#snippet icon()}<Users size={15} />{/snippet}
			</StatCard>
		</a>
	</div>

	<!-- ── Main grid ──────────────────────────────────────────────────── -->
	<div class="main-grid">

		<!-- Projects panel -->
		<Card padding="0">
			<div class="panel-header">
				<span class="panel-title"><FolderOpen size={14} />Projects</span>
				<Button variant="ghost" size="sm" href="/orgs/{orgSlug}/projects">
					View all <ArrowRight size={11} />
				</Button>
			</div>

			{#if projects.length === 0}
				<EmptyState message="No projects yet">
					{#snippet icon()}<FolderOpen size={28} />{/snippet}
				</EmptyState>
				<div class="empty-action">
					<Button size="sm" onclick={() => goto(`/orgs/${orgSlug}/projects`)}>
						Create first project
					</Button>
				</div>
			{:else}
				<div class="project-grid">
					{#each projects.slice(0, 6) as project (project.id)}
						{@const pSvcs = servicesByProject[project.id] ?? []}
						{@const running = pSvcs.filter(s => s.status === 'running').length}
						<a class="project-card" href="/orgs/{orgSlug}/projects/{project.slug}">
							<div class="project-card-top">
								<Avatar initials={project.name.split(/\s+/).map(w => w[0]).join('')} tone={projectTone(project.name)} size={34} />
								<ChevronRight size={13} class="project-chevron" />
							</div>
							<div class="project-card-body">
								<span class="project-name">{project.name}</span>
								<span class="project-slug">{project.slug}</span>
							</div>
							<div class="project-card-footer">
								<span class="project-stat">
									<Box size={11} />
									{loading ? '…' : pSvcs.length} service{pSvcs.length !== 1 ? 's' : ''}
								</span>
								{#if running > 0}
									<Badge tone="green">{running} up</Badge>
								{/if}
							</div>
						</a>
					{/each}
				</div>
			{/if}
		</Card>

		<!-- Right column -->
		<div class="right-col">

			<!-- Recent Deployments -->
			<section>
				<div class="section-head">
					<SectionLabel><span class="sl"><Activity size={12} /> Recent Deployments</span></SectionLabel>
					{#if loading}<Spinner size={13} />{/if}
				</div>

				{#if loading}
					<div class="skeleton-list">
						{#each [1,2,3,4] as _}
							<Skeleton variant="row" />
						{/each}
					</div>
				{:else if recentDeployments.length === 0}
					<Card padding="0">
						<EmptyState message="No deployments yet." sub="Deploy a service to get started.">
							{#snippet icon()}<Activity size={24} />{/snippet}
						</EmptyState>
					</Card>
				{:else}
					<ActivityList>
						{#each recentDeployments as dep (dep.id)}
							<ListRow
								iconTone={statusTone(dep.status)}
								title={dep.serviceName}
								meta="{dep.projectName} · {dep.source_ref}"
							>
								{#snippet icon()}
									{#if dep.status === 'success'}<CheckCircle2 size={14} />
									{:else if dep.status === 'failed'}<XCircle size={14} />
									{:else if dep.status === 'running'}<Activity size={14} />
									{:else}<Clock size={14} />{/if}
								{/snippet}
								{#snippet trailing()}
									<span class="dep-right">
										<Badge tone={statusTone(dep.status)}>{dep.status}</Badge>
										<span class="dep-time">{timeAgo(dep.created_at)}</span>
									</span>
								{/snippet}
							</ListRow>
						{/each}
					</ActivityList>
				{/if}
			</section>

			<!-- Quick actions -->
			<section>
				<SectionLabel><span class="sl"><Zap size={12} /> Quick Actions</span></SectionLabel>
				<div class="qa-grid">
					<Button variant="secondary" onclick={() => goto(`/orgs/${orgSlug}/projects`)}>
						<FolderOpen size={14} /> New Project
					</Button>
					<Button variant="secondary" onclick={() => goto(`/orgs/${orgSlug}/settings/members`)}>
						<Users size={14} /> Invite Member
					</Button>
					<Button variant="secondary" onclick={() => goto(`/orgs/${orgSlug}/settings`)}>
						<Settings size={14} /> Settings
					</Button>
					<Button variant="secondary" onclick={() => goto(`/orgs/${orgSlug}/settings/api-keys`)}>
						<GitBranch size={14} /> API Keys
					</Button>
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

	.dash-header {
		display: flex;
		align-items: flex-start;
		gap: 14px;
	}
	.dash-header-main { flex: 1; min-width: 0; }
	.dash-header-main :global(.ui-page-header) { margin-bottom: 0; flex-wrap: wrap; }

	.stats-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 12px;
	}
	@media (max-width: 900px) {
		.stats-grid { grid-template-columns: repeat(2, 1fr); }
	}
	.stat-link { text-decoration: none; display: block; }
	.stat-link:hover :global(.ui-stat-card) { border-color: var(--border-hover); }

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

	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px 8px 16px;
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
	.empty-action { display: flex; justify-content: center; padding: 0 0 28px; }

	.sl { display: inline-flex; align-items: center; gap: 6px; }
	.section-head { display: flex; align-items: flex-start; justify-content: space-between; }
	.skeleton-list { display: flex; flex-direction: column; gap: 8px; }

	.project-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(180px, 1fr));
		gap: 8px;
		padding: 12px;
	}
	.project-card {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 14px;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		text-decoration: none;
		color: inherit;
		transition: background var(--transition-fast), border-color var(--transition-fast);
	}
	.project-card:hover { background: var(--bg-hover); border-color: var(--border-hover); }
	.project-card-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	:global(.project-chevron) { color: var(--text-dim); }
	.project-card:hover :global(.project-chevron) { color: var(--accent); }
	.project-card-body {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
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
		font-family: var(--font-mono);
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

	.dep-right {
		display: flex;
		flex-direction: column;
		align-items: flex-end;
		gap: 3px;
	}
	.dep-time {
		font-size: 10px;
		color: var(--text-dim);
	}

	.qa-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 8px;
	}
</style>
