<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import { FolderOpen, Server, Activity, Users, Plus, ArrowRight, CheckCircle, XCircle, Clock } from '@lucide/svelte';
	import type { Deployment, Project } from '$lib/api/types';

	let orgSlug = $derived(page.params.orgSlug ?? '');
	let org = $derived($orgStore.activeOrg);
	let projects = $derived($projectStore.projects);

	let recentDeployments = $state<(Deployment & { serviceName?: string; projectName?: string })[]>([]);
	let loadingDeployments = $state(false);

	let stats = $derived({
		projects: projects.length,
		services: 0,
	});

	onMount(async () => {
		// Load recent deployments across all projects
		loadingDeployments = true;
		try {
			const deployPromises = projects.slice(0, 5).map(async (p: Project) => {
				const svcsRes = await api.getServices(p.id);
				if (!svcsRes.data) return [];
				const deplPromises = svcsRes.data.slice(0, 3).map(async (svc) => {
					const res = await api.getDeployments(svc.id);
					return (res.data ?? []).slice(0, 2).map((d) => ({
						...d,
						serviceName: svc.name,
						projectName: p.name,
					}));
				});
				return (await Promise.all(deplPromises)).flat();
			});
			const all = (await Promise.all(deployPromises)).flat();
			recentDeployments = all
				.sort((a, b) => new Date(b.started_at ?? b.created_at).getTime() - new Date(a.started_at ?? a.created_at).getTime())
				.slice(0, 8);
		} finally {
			loadingDeployments = false;
		}
	});

	function statusIcon(status: string) {
		switch (status) {
			case 'success': return CheckCircle;
			case 'failed':  return XCircle;
			default:        return Clock;
		}
	}

	function statusClass(status: string) {
		switch (status) {
			case 'success': return 'success';
			case 'failed':  return 'failed';
			case 'running': return 'running';
			default:        return 'pending';
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
</script>

<div class="home-page">
	<!-- Header -->
	<div class="page-header">
		<div class="header-left">
			<h1 class="page-title">{org?.name ?? 'Organization'}</h1>
			<p class="page-subtitle">Overview of your platform</p>
		</div>
		<button class="btn btn-primary" onclick={() => goto(`/orgs/${orgSlug}/projects`)}>
			<Plus size={14} />
			New Project
		</button>
	</div>

	<!-- Stats row -->
	<div class="stats-row">
		<div class="stat-card">
			<div class="stat-icon projects-color">
				<FolderOpen size={18} />
			</div>
			<div class="stat-body">
				<span class="stat-value">{stats.projects}</span>
				<span class="stat-label">Projects</span>
			</div>
		</div>
		<div class="stat-card">
			<div class="stat-icon services-color">
				<Server size={18} />
			</div>
			<div class="stat-body">
				<span class="stat-value">{recentDeployments.length > 0 ? '—' : '0'}</span>
				<span class="stat-label">Services</span>
			</div>
		</div>
		<div class="stat-card">
			<div class="stat-icon deploys-color">
				<Activity size={18} />
			</div>
			<div class="stat-body">
				<span class="stat-value">{recentDeployments.length}</span>
				<span class="stat-label">Recent deploys</span>
			</div>
		</div>
	</div>

	<div class="content-grid">
		<!-- Projects -->
		<section class="card">
			<div class="card-header">
				<span class="card-title">Projects</span>
				<a href="/orgs/{orgSlug}/projects" class="card-link">
					View all <ArrowRight size={12} />
				</a>
			</div>
			{#if projects.length === 0}
				<div class="empty-card">
					<FolderOpen size={32} />
					<p>No projects yet</p>
					<button class="btn btn-primary" onclick={() => goto(`/orgs/${orgSlug}/projects`)}>
						Create your first project
					</button>
				</div>
			{:else}
				<ul class="project-list">
					{#each projects.slice(0, 6) as project (project.id)}
						<li>
							<a class="project-row" href="/orgs/{orgSlug}/projects/{project.slug}">
								<div class="project-icon">{project.name.charAt(0).toUpperCase()}</div>
								<div class="project-info">
									<span class="project-name">{project.name}</span>
									<span class="project-slug">{project.slug}</span>
								</div>
								<ArrowRight size={14} class="project-arrow" />
							</a>
						</li>
					{/each}
				</ul>
			{/if}
		</section>

		<!-- Recent Deployments -->
		<section class="card">
			<div class="card-header">
				<span class="card-title">Recent Deployments</span>
			</div>
			{#if loadingDeployments}
				<div class="empty-card">
					<div class="spinner"></div>
					<p>Loading…</p>
				</div>
			{:else if recentDeployments.length === 0}
				<div class="empty-card">
					<Activity size={32} />
					<p>No deployments yet</p>
				</div>
			{:else}
				<ul class="deploy-list">
					{#each recentDeployments as dep (dep.id)}
						{@const Icon = statusIcon(dep.status)}
						<li class="deploy-row">
							<span class="deploy-status {statusClass(dep.status)}">
								<Icon size={14} />
							</span>
							<div class="deploy-info">
								<span class="deploy-service">{dep.serviceName}</span>
								<span class="deploy-project">{dep.projectName}</span>
							</div>
							<span class="deploy-time">{timeAgo(dep.started_at ?? dep.created_at)}</span>
						</li>
					{/each}
				</ul>
			{/if}
		</section>
	</div>
</div>

<style>
	.home-page {
		padding: 28px 32px;
		height: 100%;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	@media (max-width: 639px) {
		.home-page { padding: 16px 16px 72px; }
	}

	.page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
	}

	.header-left {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.page-title {
		font-size: 22px;
		font-weight: 700;
		color: var(--text-primary);
		letter-spacing: -0.02em;
		margin: 0;
	}

	.page-subtitle {
		font-size: 13px;
		color: var(--text-muted);
		margin: 0;
	}

	/* Stats */
	.stats-row {
		display: flex;
		gap: 12px;
	}

	.stat-card {
		flex: 1;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 16px;
		display: flex;
		align-items: center;
		gap: 14px;
	}

	.stat-icon {
		width: 40px;
		height: 40px;
		border-radius: var(--radius-md);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.projects-color { background: rgba(37, 99, 235, 0.12); color: #3B82F6; }
	.services-color { background: rgba(16, 185, 129, 0.12); color: #10B981; }
	.deploys-color  { background: rgba(245, 158, 11, 0.12); color: #F59E0B; }

	.stat-body {
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.stat-value {
		font-size: 24px;
		font-weight: 700;
		color: var(--text-primary);
		line-height: 1;
	}

	.stat-label {
		font-size: 12px;
		color: var(--text-muted);
	}

	/* Content grid */
	.content-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 16px;
	}

	@media (max-width: 900px) {
		.content-grid { grid-template-columns: 1fr; }
		.stats-row { flex-direction: column; }
	}

	.card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	.card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
	}

	.card-title {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.card-link {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--accent);
		text-decoration: none;
	}

	.card-link:hover { color: var(--accent-hover); }

	.empty-card {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 12px;
		padding: 40px 20px;
		color: var(--text-muted);
	}

	.empty-card p { font-size: 13px; margin: 0; }

	/* Projects list */
	.project-list {
		list-style: none;
		margin: 0;
		padding: 4px 0;
	}

	.project-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 16px;
		text-decoration: none;
		transition: background var(--transition-fast);
		color: inherit;
	}

	.project-row:hover { background: var(--bg-elevated); }

	.project-icon {
		width: 28px;
		height: 28px;
		border-radius: var(--radius-sm);
		background: rgba(37, 99, 235, 0.1);
		color: var(--accent);
		font-size: 12px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.project-info {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}

	.project-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.project-slug {
		font-size: 11px;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}

	:global(.project-arrow) { color: var(--text-dim); }
	.project-row:hover :global(.project-arrow) { color: var(--accent); }

	/* Deploy list */
	.deploy-list {
		list-style: none;
		margin: 0;
		padding: 4px 0;
	}

	.deploy-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 16px;
	}

	.deploy-status {
		width: 24px;
		height: 24px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: 50%;
		flex-shrink: 0;
	}

	.deploy-status.success { color: #10B981; background: rgba(16,185,129,0.1); }
	.deploy-status.failed  { color: #EF4444; background: rgba(239,68,68,0.1); }
	.deploy-status.running { color: #3B82F6; background: rgba(59,130,246,0.1); }
	.deploy-status.pending { color: var(--text-muted); background: var(--bg-elevated); }

	.deploy-info {
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 1px;
		min-width: 0;
	}

	.deploy-service {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.deploy-project {
		font-size: 11px;
		color: var(--text-muted);
	}

	.deploy-time {
		font-size: 11px;
		color: var(--text-dim);
		white-space: nowrap;
	}

	.spinner {
		width: 24px;
		height: 24px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
