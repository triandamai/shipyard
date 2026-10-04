<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import {
		ArrowLeft, Trash2, AlertTriangle, Save, Check,
		Layers, Network, HardDrive, Globe, ChevronDown, ChevronRight
	} from '@lucide/svelte';
	import {
		PageHeader, Card, FormField, TextField, Button, InlineAlert,
		ConfirmDialog, KeyValueList, StatCard, Badge, Spinner
	} from '$lib/components/ui';
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

	// Rename
	let renameValue  = $state('');
	let renaming     = $state(false);
	let renameError  = $state('');
	let renameSaved  = $state(false);

	$effect(() => { if (project) renameValue = project.name; });

	async function saveRename() {
		if (!project || !renameValue.trim() || renameValue.trim() === project.name) return;
		renaming = true; renameError = ''; renameSaved = false;
		const res = await api.updateProject(orgId, projectId, renameValue.trim());
		if (res.error) {
			renameError = res.error.message;
		} else if (res.data) {
			project = res.data;
			renameValue = res.data.name;
			// sync the store so the sidebar label updates
			projectStore.setProjects(
				$projectStore.projects.map(p => p.id === projectId ? { ...p, name: res.data!.name } : p)
			);
			renameSaved = true;
			setTimeout(() => (renameSaved = false), 3000);
		}
		renaming = false;
	}

	// Confirmation dialog
	let deleteError  = $state('');
	let showConfirm  = $state(false);

	// Expandable service rows
	let expandedServices = $state(new Set<string>());

	function toggleService(id: string) {
		const next = new Set(expandedServices);
		if (next.has(id)) next.delete(id); else next.add(id);
		expandedServices = next;
	}

	onMount(async () => {
		if (!orgId || !projectId) { loadError = 'Project not found.'; loading = false; return; }

		const [projRes, svcRes, netRes] = await Promise.all([
			api.getProject(orgId, projectId),
			api.getServices(projectId),
			api.getNetworks(projectId),
		]);

		if (projRes.error) { loadError = projRes.error.message; loading = false; return; }
		project  = projRes.data ?? null;
		services = svcRes.data ?? [];
		networkCount = (netRes.data ?? []).length;

		// Volumes and domains are per-service; sum across all
		const [volResults, domResults] = await Promise.all([
			Promise.all(services.map(s => api.getVolumes(s.id))),
			Promise.all(services.map(s => api.getDomains(s.id))),
		]);
		volumeCount = volResults.reduce((sum, r) => sum + (r.data?.length ?? 0), 0);
		domainCount = domResults.reduce((sum, r) => sum + (r.data?.length ?? 0), 0);

		loading = false;
	});

	function formatDate(iso: string) {
		return new Date(iso).toLocaleDateString('en-US', {
			year: 'numeric', month: 'long', day: 'numeric'
		});
	}

	async function confirmDelete(): Promise<boolean | void> {
		if (!project) return;
		deleteError = '';
		const res = await api.deleteProject(orgId, projectId);
		if (res.error) {
			deleteError = res.error.message;
			return false;
		}
		projectStore.setProjects(
			$projectStore.projects.filter(p => p.id !== projectId)
		);
		await goto(`/orgs/${orgSlug}`);
	}
</script>

<div class="settings-scroll">
<div class="settings-page">
	<!-- Header -->
	<div class="page-header">
		<a class="back-link" href="/orgs/{orgSlug}/projects/{projectSlug}">
			<ArrowLeft size={15} />
			Back to project
		</a>
		<PageHeader title="Project Settings" />
	</div>

	{#if loading}
		<div class="state-center">
			<Spinner size={20} />
			<span>Loading…</span>
		</div>
	{:else if loadError}
		<div class="state-center">
			<InlineAlert tone="error">
				<span class="load-error"><AlertTriangle size={14} /> {loadError}</span>
			</InlineAlert>
			<Button variant="secondary" size="sm" href="/orgs/{orgSlug}">Go back</Button>
		</div>
	{:else if project}
		<!-- Project info card -->
		<Card padding="24px">
			<div class="card-body">
				<h2 class="card-title">Project information</h2>
				<KeyValueList
					keyWidth="120px"
					items={[
						{ key: 'Name', value: project.name },
						{ key: 'Slug', value: project.slug, mono: true },
						{ key: 'Project ID', value: project.id, mono: true },
						{ key: 'Directory', value: project.directory_path || '—', mono: true },
						{ key: 'Created', value: formatDate(project.created_at) }
					]}
				/>
			</div>
		</Card>

		<!-- Rename card -->
		<Card padding="24px">
			<div class="card-body">
				<h2 class="card-title">Rename project</h2>
				<p class="card-desc">Change the display name. The URL slug stays the same.</p>
				<form class="rename-form" onsubmit={(e) => { e.preventDefault(); saveRename(); }}>
					<FormField label="Project name" for="project-rename">
						<TextField
							id="project-rename"
							type="text"
							bind:value={renameValue}
							placeholder="Project name"
							disabled={renaming}
							maxlength={80}
							autocomplete="off"
						/>
					</FormField>
					{#if renameError}
						<div role="alert"><InlineAlert tone="error">{renameError}</InlineAlert></div>
					{/if}
					<div class="rename-actions">
						<Button
							type="submit"
							disabled={renaming || !renameValue.trim() || renameValue.trim() === project.name}
						>
							{#if renaming}
								<Spinner size={13} tone="current" /> Saving…
							{:else if renameSaved}
								<Check size={13} /> Saved
							{:else}
								<Save size={13} /> Save name
							{/if}
						</Button>
					</div>
				</form>
			</div>
		</Card>

		<!-- Resources card -->
		<Card padding="24px">
			<div class="card-body">
				<h2 class="card-title">Resources</h2>
				<div class="resource-grid">
					<StatCard value={services.length} label={`Service${services.length !== 1 ? 's' : ''}`}>
						{#snippet icon()}<Layers size={16} />{/snippet}
					</StatCard>
					<StatCard value={networkCount} label={`Network${networkCount !== 1 ? 's' : ''}`}>
						{#snippet icon()}<Network size={16} />{/snippet}
					</StatCard>
					<StatCard value={volumeCount} label={`Volume${volumeCount !== 1 ? 's' : ''}`}>
						{#snippet icon()}<HardDrive size={16} />{/snippet}
					</StatCard>
					<StatCard value={domainCount} label={`Domain${domainCount !== 1 ? 's' : ''}`}>
						{#snippet icon()}<Globe size={16} />{/snippet}
					</StatCard>
				</div>

				{#if services.length > 0}
					<div class="service-list">
						{#each services as svc}
							{@const expanded = expandedServices.has(svc.id)}
							<div class="service-item" class:expanded>
								<button class="service-header" onclick={() => toggleService(svc.id)} aria-expanded={expanded}>
									<span class="chevron-wrap">
										{#if expanded}
											<ChevronDown size={12} />
										{:else}
											<ChevronRight size={12} />
										{/if}
									</span>
									<span class="service-name">{svc.name}</span>
									<span class="service-type">{svc.type}</span>
									<Badge tone={svc.status === 'running' ? 'green' : 'neutral'}>{svc.status}</Badge>
								</button>

								{#if expanded}
									<div class="service-details">
										{#if svc.image}
											<div class="detail-row">
												<span class="detail-key">Image</span>
												<code class="detail-val">{svc.image}</code>
											</div>
										{/if}
										{#if svc.git_repo_url}
											<div class="detail-row">
												<span class="detail-key">Repository</span>
												<code class="detail-val">{svc.git_repo_url}</code>
											</div>
											<div class="detail-row">
												<span class="detail-key">Branch</span>
												<code class="detail-val">{svc.git_branch || 'main'}</code>
											</div>
										{/if}
										{#if svc.ports?.length > 0}
											<div class="detail-row">
												<span class="detail-key">Ports</span>
												<span class="detail-val">{svc.ports.join(', ')}</span>
											</div>
										{/if}
										{#if svc.directory_path}
											<div class="detail-row">
												<span class="detail-key">Directory</span>
												<code class="detail-val">{svc.directory_path}</code>
											</div>
										{/if}
										<div class="detail-row">
											<span class="detail-key">Service ID</span>
											<code class="detail-val dim">{svc.id}</code>
										</div>
									</div>
								{/if}
							</div>
						{/each}
					</div>
				{/if}
			</div>
		</Card>

		<!-- Danger zone -->
		<Card tone="danger" padding="24px">
			<div class="card-body">
				<h2 class="card-title danger-title">
					<AlertTriangle size={16} />
					Danger zone
				</h2>
				<div class="danger-row">
					<div class="danger-desc">
						<strong>Delete this project</strong>
						<p>Permanently removes the project and all its resources — services, networks, volumes, domains, deployments, and logs. Running containers will be stopped. This cannot be undone.</p>
					</div>
					<Button variant="danger-outline" onclick={() => { showConfirm = true; deleteError = ''; }}>
						<Trash2 size={13} />
						Delete project
					</Button>
				</div>
			</div>
		</Card>
	{/if}
</div>
</div>

<!-- Delete confirmation -->
{#if project}
	<ConfirmDialog
		bind:open={showConfirm}
		title={`Delete ${project.name}`}
		message={`All resources will be permanently deleted: ${services.length} service${services.length !== 1 ? 's' : ''} (containers will be stopped), ${networkCount} network${networkCount !== 1 ? 's' : ''}, ${volumeCount} volume${volumeCount !== 1 ? 's' : ''}, ${domainCount} domain${domainCount !== 1 ? 's' : ''}, and all deployments and logs.`}
		confirmLabel="Delete project"
		confirmText={project.name}
		error={deleteError}
		onConfirm={confirmDelete}
	/>
{/if}

<style>
	/* Scroll container — fills main-content (overflow: hidden) and scrolls internally */
	.settings-scroll {
		height: 100%;
		overflow-y: auto;
	}

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
	.page-header :global(.ui-page-header) { margin-bottom: 0; }

	.back-link {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		color: var(--text-muted);
		text-decoration: none;
		width: fit-content;
		transition: color var(--transition-fast);
	}
	.back-link:hover { color: var(--text-primary); }

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
	.load-error { display: inline-flex; align-items: center; gap: 8px; }

	.card-body {
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

	/* Resource grid */
	.resource-grid {
		display: grid;
		grid-template-columns: repeat(4, 1fr);
		gap: 12px;
	}

	/* Service list */
	.service-list {
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.service-item {
		border-bottom: 1px solid var(--border);
	}
	.service-item:last-child { border-bottom: none; }

	.service-header {
		width: 100%;
		display: grid;
		grid-template-columns: 20px 1fr auto auto;
		align-items: center;
		gap: 10px;
		padding: 9px 14px;
		font-size: 13px;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		transition: background var(--transition-fast);
		font-family: var(--font-sans);
	}
	.service-header:hover { background: var(--bg-elevated); }
	.service-item.expanded > .service-header { background: var(--bg-elevated); }

	.chevron-wrap {
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-dim);
		flex-shrink: 0;
	}

	.service-name { color: var(--text-primary); font-weight: 500; }
	.service-type { color: var(--text-muted); font-size: 11px; text-transform: capitalize; }

	/* Expanded detail panel */
	.service-details {
		border-top: 1px solid var(--border);
		background: var(--bg-base);
		padding: 10px 14px 10px 44px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.detail-row {
		display: grid;
		grid-template-columns: 100px 1fr;
		gap: 12px;
		align-items: baseline;
		font-size: 12px;
	}

	.detail-key {
		color: var(--text-dim);
		font-weight: 500;
		flex-shrink: 0;
	}

	.detail-val {
		color: var(--text-secondary);
		font-family: var(--font-mono);
		word-break: break-all;
	}
	.detail-val.dim { color: var(--text-dim); font-size: 11px; }

	/* Rename */
	.card-desc { font-size: 13px; color: var(--text-muted); margin: -8px 0 0; line-height: 1.5; }
	.rename-form { display: flex; flex-direction: column; gap: 10px; }
	.rename-actions { display: flex; justify-content: flex-end; }

	/* Danger zone */
	.danger-title { color: var(--accent-red); }

	.danger-row {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 24px;
	}

	.danger-desc {
		font-size: 13px;
		color: var(--text-secondary);
	}
	.danger-desc strong { color: var(--text-primary); display: block; margin-bottom: 4px; font-size: 14px; }
	.danger-desc p { margin: 0; line-height: 1.5; }

	@media (max-width: 639px) {
		.settings-page { padding: 16px; }
		.resource-grid { grid-template-columns: repeat(2, 1fr); }
		.danger-row { flex-direction: column; }
		.detail-row { grid-template-columns: 80px 1fr; }
	}
</style>
