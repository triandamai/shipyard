<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import {
		ArrowLeft, Settings2, Calendar, FolderOpen,
		Trash2, AlertTriangle, Loader2, X, RefreshCw,
		Layers, Network, HardDrive, Globe
	} from '@lucide/svelte';
	import type { Project, Service } from '$lib/api/types';

	let orgSlug    = $derived(page.params.orgSlug ?? '');
	let projectSlug = $derived(page.params.projectSlug ?? '');
	let orgId      = $derived($orgStore.activeOrg?.id ?? '');
	let projectId  = $derived(
		$projectStore.projects.find(p => p.slug === projectSlug)?.id ?? ''
	);

	let project     = $state<Project | null>(null);
	let services    = $state<Service[]>([]);
	let loading     = $state(true);
	let loadError   = $state('');

	// Resource counts fetched in parallel
	let networkCount = $state(0);
	let volumeCount  = $state(0);
	let domainCount  = $state(0);

	// Confirmation dialog
	let confirmInput = $state('');
	let deleting     = $state(false);
	let deleteError  = $state('');
	let showConfirm  = $state(false);

	onMount(async () => {
		if (!orgId || !projectId) { loadError = 'Project not found.'; loading = false; return; }

		const [projRes, svcRes, netRes, volRes] = await Promise.all([
			api.getProject(orgId, projectId),
			api.getServices(projectId),
			api.getNetworks(projectId),
			api.get<{ id: string }[]>(`/projects/${projectId}/volumes`),
		]);

		if (projRes.error) { loadError = projRes.error.message; loading = false; return; }
		project  = projRes.data ?? null;
		services = svcRes.data ?? [];
		networkCount = (netRes.data ?? []).length;
		volumeCount  = (volRes.data ?? []).length;

		// Domains live per-service; sum across all services
		const domCounts = await Promise.all(
			services.map(s => api.getDomains(s.id))
		);
		domainCount = domCounts.reduce((sum, r) => sum + (r.data?.length ?? 0), 0);

		loading = false;
	});

	function formatDate(iso: string) {
		return new Date(iso).toLocaleDateString('en-US', {
			year: 'numeric', month: 'long', day: 'numeric'
		});
	}

	async function confirmDelete() {
		if (!project || confirmInput !== project.name) return;
		deleting = true;
		deleteError = '';
		const res = await api.deleteProject(orgId, projectId);
		if (res.error) {
			deleteError = res.error.message;
			deleting = false;
			return;
		}
		// Remove from store and navigate away
		projectStore.setProjects(
			$projectStore.projects.filter(p => p.id !== projectId)
		);
		await goto(`/orgs/${orgSlug}`);
	}
</script>

<div class="settings-page">
	<!-- Header -->
	<div class="page-header">
		<a class="back-link" href="/orgs/{orgSlug}/projects/{projectSlug}">
			<ArrowLeft size={15} />
			Back to project
		</a>
		<div class="header-title">
			<Settings2 size={18} />
			<h1>Project Settings</h1>
		</div>
	</div>

	{#if loading}
		<div class="state-center">
			<Loader2 size={20} class="spin" />
			<span>Loading…</span>
		</div>
	{:else if loadError}
		<div class="state-center error">
			<AlertTriangle size={18} />
			<span>{loadError}</span>
			<a class="btn btn-secondary btn-sm" href="/orgs/{orgSlug}">Go back</a>
		</div>
	{:else if project}
		<!-- Project info card -->
		<section class="card">
			<h2 class="card-title">Project information</h2>
			<div class="info-grid">
				<div class="info-row">
					<span class="info-label">Name</span>
					<span class="info-value">{project.name}</span>
				</div>
				<div class="info-row">
					<span class="info-label">Slug</span>
					<code class="info-value mono">{project.slug}</code>
				</div>
				<div class="info-row">
					<span class="info-label">Project ID</span>
					<code class="info-value mono small">{project.id}</code>
				</div>
				<div class="info-row">
					<span class="info-label">
						<FolderOpen size={13} />
						Directory
					</span>
					<code class="info-value mono small">{project.directory_path || '—'}</code>
				</div>
				<div class="info-row">
					<span class="info-label">
						<Calendar size={13} />
						Created
					</span>
					<span class="info-value">{formatDate(project.created_at)}</span>
				</div>
			</div>
		</section>

		<!-- Resources card -->
		<section class="card">
			<h2 class="card-title">Resources</h2>
			<div class="resource-grid">
				<div class="resource-item">
					<Layers size={20} />
					<span class="resource-count">{services.length}</span>
					<span class="resource-label">Service{services.length !== 1 ? 's' : ''}</span>
				</div>
				<div class="resource-item">
					<Network size={20} />
					<span class="resource-count">{networkCount}</span>
					<span class="resource-label">Network{networkCount !== 1 ? 's' : ''}</span>
				</div>
				<div class="resource-item">
					<HardDrive size={20} />
					<span class="resource-count">{volumeCount}</span>
					<span class="resource-label">Volume{volumeCount !== 1 ? 's' : ''}</span>
				</div>
				<div class="resource-item">
					<Globe size={20} />
					<span class="resource-count">{domainCount}</span>
					<span class="resource-label">Domain{domainCount !== 1 ? 's' : ''}</span>
				</div>
			</div>
			{#if services.length > 0}
				<div class="service-list">
					{#each services as svc}
						<div class="service-row">
							<span class="service-name">{svc.name}</span>
							<span class="service-type">{svc.type}</span>
							<span class="service-status" class:running={svc.status === 'running'}>{svc.status}</span>
						</div>
					{/each}
				</div>
			{/if}
		</section>

		<!-- Danger zone -->
		<section class="card danger-card">
			<h2 class="card-title danger-title">
				<AlertTriangle size={16} />
				Danger zone
			</h2>
			<div class="danger-row">
				<div class="danger-desc">
					<strong>Delete this project</strong>
					<p>Permanently removes the project and all its resources — services, networks, volumes, domains, deployments, and logs. Running containers will be stopped. This cannot be undone.</p>
				</div>
				<button class="btn btn-danger-outline" onclick={() => { showConfirm = true; confirmInput = ''; deleteError = ''; }}>
					<Trash2 size={13} />
					Delete project
				</button>
			</div>
		</section>
	{/if}
</div>

<!-- Delete confirmation modal -->
{#if showConfirm && project}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-backdrop" onclick={() => { if (!deleting) showConfirm = false; }} onkeydown={() => {}}></div>
	<div class="modal" role="dialog" aria-modal="true">
		<div class="modal-header">
			<div class="modal-title">
				<Trash2 size={15} />
				Delete <strong>{project.name}</strong>
			</div>
			<button class="close-btn" onclick={() => showConfirm = false} disabled={deleting}>
				<X size={15} />
			</button>
		</div>

		<div class="modal-body">
			<div class="danger-notice">
				<AlertTriangle size={15} />
				<div>
					<strong>All resources will be permanently deleted:</strong>
					<ul>
						<li>{services.length} service{services.length !== 1 ? 's' : ''} (containers will be stopped)</li>
						<li>{networkCount} network{networkCount !== 1 ? 's' : ''}</li>
						<li>{volumeCount} volume{volumeCount !== 1 ? 's' : ''}</li>
						<li>{domainCount} domain{domainCount !== 1 ? 's' : ''}</li>
						<li>All deployments and logs</li>
					</ul>
				</div>
			</div>

			<p class="confirm-label">Type <strong>{project.name}</strong> to confirm:</p>
			<input
				class="confirm-input"
				type="text"
				placeholder={project.name}
				bind:value={confirmInput}
				disabled={deleting}
				autocomplete="off"
				spellcheck="false"
			/>

			{#if deleteError}
				<p class="delete-error">{deleteError}</p>
			{/if}
		</div>

		<div class="modal-footer">
			<button class="btn btn-secondary" onclick={() => showConfirm = false} disabled={deleting}>
				Cancel
			</button>
			<button
				class="btn btn-danger"
				onclick={confirmDelete}
				disabled={deleting || confirmInput !== project.name}
			>
				{#if deleting}
					<Loader2 size={13} class="spin" />
					Deleting…
				{:else}
					<Trash2 size={13} />
					Delete project
				{/if}
			</button>
		</div>
	</div>
{/if}

<style>
	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.settings-page {
		max-width: 720px;
		margin: 0 auto;
		padding: 32px;
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	/* Header */
	.page-header {
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.back-link {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		color: var(--text-muted);
		text-decoration: none;
		width: fit-content;
		transition: color 0.15s;
	}
	.back-link:hover { color: var(--text-primary); }

	.header-title {
		display: flex;
		align-items: center;
		gap: 10px;
		color: var(--text-muted);
	}

	.header-title h1 {
		font-size: 22px;
		font-weight: 700;
		color: var(--text-primary);
		margin: 0;
	}

	/* Loading / error states */
	.state-center {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		height: 200px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.state-center.error { color: #dc2626; }

	/* Cards */
	.card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 24px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.card-title {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
		margin: 0;
		display: flex;
		align-items: center;
		gap: 8px;
	}

	/* Info grid */
	.info-grid {
		display: flex;
		flex-direction: column;
		gap: 0;
	}

	.info-row {
		display: grid;
		grid-template-columns: 160px 1fr;
		align-items: center;
		padding: 10px 0;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
		gap: 16px;
	}
	.info-row:last-child { border-bottom: none; }

	.info-label {
		color: var(--text-muted);
		display: flex;
		align-items: center;
		gap: 5px;
		font-weight: 500;
	}

	.info-value { color: var(--text-primary); word-break: break-all; }
	.info-value.mono { font-family: var(--font-mono, monospace); font-size: 12px; }
	.info-value.small { font-size: 11px; }

	/* Resource grid */
	.resource-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 12px;
	}

	.resource-item {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 16px 8px;
		background: var(--bg-elevated, #f9fafb);
		border: 1px solid var(--border);
		border-radius: 8px;
		color: var(--text-muted);
	}

	.resource-count {
		font-size: 24px;
		font-weight: 700;
		color: var(--text-primary);
		line-height: 1;
	}

	.resource-label {
		font-size: 11px;
		color: var(--text-muted);
		text-align: center;
	}

	/* Service list */
	.service-list {
		border: 1px solid var(--border);
		border-radius: 6px;
		overflow: hidden;
	}

	.service-row {
		display: grid;
		grid-template-columns: 1fr auto auto;
		align-items: center;
		gap: 12px;
		padding: 9px 14px;
		font-size: 13px;
		border-bottom: 1px solid var(--border);
	}
	.service-row:last-child { border-bottom: none; }

	.service-name { color: var(--text-primary); font-weight: 500; }
	.service-type { color: var(--text-muted); font-size: 11px; text-transform: capitalize; }
	.service-status {
		font-size: 11px;
		padding: 2px 7px;
		border-radius: 10px;
		background: var(--bg-elevated, #f3f4f6);
		color: var(--text-muted);
		font-weight: 500;
	}
	.service-status.running {
		background: #dcfce7;
		color: #15803d;
	}

	/* Danger zone */
	.danger-card { border-color: #fecaca; }

	.danger-title { color: #dc2626; }

	.danger-row {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 24px;
	}

	.danger-desc {
		font-size: 13px;
		color: var(--text-secondary, var(--text-muted));
	}
	.danger-desc strong { color: var(--text-primary); display: block; margin-bottom: 4px; font-size: 14px; }
	.danger-desc p { margin: 0; line-height: 1.5; }

	.btn-danger-outline {
		display: flex;
		align-items: center;
		gap: 6px;
		flex-shrink: 0;
		background: transparent;
		border: 1px solid #fca5a5;
		color: #dc2626;
		padding: 7px 14px;
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		white-space: nowrap;
		transition: all 0.15s;
	}
	.btn-danger-outline:hover { background: #fee2e2; border-color: #dc2626; }

	/* Modal */
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.45);
		z-index: 100;
	}

	.modal {
		position: fixed;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(500px, calc(100vw - 32px));
		background: var(--bg-surface, #fff);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: 0 20px 60px rgba(0, 0, 0, 0.18);
		z-index: 101;
		display: flex;
		flex-direction: column;
	}

	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px;
		border-bottom: 1px solid var(--border);
	}

	.modal-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		padding: 4px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}
	.close-btn:hover:not(:disabled) { color: var(--text-primary); }

	.modal-body {
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.danger-notice {
		display: flex;
		gap: 12px;
		padding: 14px;
		background: #fef2f2;
		border: 1px solid #fecaca;
		border-radius: 6px;
		color: #dc2626;
		font-size: 13px;
	}
	.danger-notice :global(svg) { flex-shrink: 0; margin-top: 2px; }
	.danger-notice strong { display: block; margin-bottom: 6px; }
	.danger-notice ul {
		margin: 0;
		padding-left: 18px;
		color: #7f1d1d;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.confirm-label {
		font-size: 13px;
		color: var(--text-secondary, var(--text-muted));
		margin: 0;
	}

	.confirm-input {
		width: 100%;
		padding: 8px 12px;
		font-size: 13px;
		font-family: var(--font-mono, monospace);
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg-base);
		color: var(--text-primary);
		box-sizing: border-box;
		outline: none;
	}
	.confirm-input:focus { border-color: #dc2626; box-shadow: 0 0 0 2px #fee2e2; }

	.delete-error { font-size: 13px; color: #dc2626; margin: 0; }

	.modal-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 16px 20px;
		border-top: 1px solid var(--border);
	}

	.btn-danger {
		display: flex;
		align-items: center;
		gap: 6px;
		background: #dc2626;
		border: 1px solid #dc2626;
		color: #fff;
		padding: 7px 14px;
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		transition: background 0.15s;
	}
	.btn-danger:hover:not(:disabled) { background: #b91c1c; border-color: #b91c1c; }
	.btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }

	@media (max-width: 639px) {
		.settings-page { padding: 16px; }
		.resource-grid { grid-template-columns: repeat(2, 1fr); }
		.danger-row { flex-direction: column; }
		.info-row { grid-template-columns: 120px 1fr; }
	}
</style>
