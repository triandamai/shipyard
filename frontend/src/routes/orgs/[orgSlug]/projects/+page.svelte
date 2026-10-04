<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import { get } from 'svelte/store';
	import type { Project, Organization } from '$lib/api/types';
	import { Plus, Package } from '@lucide/svelte';
	import {
		PageHeader, Card, Button, Modal, FormField, TextField, InlineAlert,
		EmptyState, Spinner, Avatar
	} from '$lib/components/ui';

	let orgSlug = $derived(page.params.orgSlug ?? '');

	// Read from stores — parent layout already loads these
	let storeState = $state(get(projectStore));
	let orgState = $state(get(orgStore));

	$effect(() => {
		const unsub = projectStore.subscribe((s) => (storeState = s));
		return unsub;
	});

	$effect(() => {
		const unsub = orgStore.subscribe((s) => (orgState = s));
		return unsub;
	});

	// Resolve org UUID from slug for API calls
	let currentOrg = $derived(
		orgState.activeOrg ??
			orgState.organizations.find((o) => o.slug === orgSlug || o.id === orgSlug) ??
			null
	);

	let projects = $derived(storeState.projects);
	let loading = $derived(storeState.isLoading);

	let fetchError = $state('');

	// New project modal
	let showModal = $state(false);
	let newProjectName = $state('');
	let newProjectSlug = $state('');
	let creating = $state(false);
	let createError = $state('');
	let createForm = $state<HTMLFormElement>();

	// Auto-generate slug from name
	$effect(() => {
		newProjectSlug = newProjectName
			.toLowerCase()
			.replace(/\s+/g, '-')
			.replace(/[^a-z0-9-]/g, '');
	});

	onMount(() => {
		const auth = get(authStore);
		if (!auth.token) {
			goto('/login');
		}
	});

	async function handleCreateProject(e: SubmitEvent) {
		e.preventDefault();
		createError = '';
		creating = true;

		try {
			const res = await api.createProject(currentOrg?.id ?? orgSlug, newProjectName, newProjectSlug);

			if (res.error || !res.data) {
				createError = res.error?.message ?? 'Failed to create project.';
				return;
			}

			projectStore.addProject(res.data);
			closeModal();
		} finally {
			creating = false;
		}
	}

	function openModal() {
		newProjectName = '';
		newProjectSlug = '';
		createError = '';
		showModal = true;
	}

	function closeModal() {
		showModal = false;
		newProjectName = '';
		newProjectSlug = '';
		createError = '';
	}

	function goToProject(project: Project) {
		projectStore.setActiveProject(project);
		goto(`/orgs/${orgSlug}/projects/${project.slug}`);
	}
</script>

<div class="projects-scroll">
	<div class="projects-inner">

		<PageHeader title="Projects" subtitle={currentOrg?.name}>
			{#snippet actions()}
				<Button onclick={openModal}>
					<Plus size={14} />
					New Project
				</Button>
			{/snippet}
		</PageHeader>

		<!-- Error -->
		{#if fetchError}
			<div role="alert"><InlineAlert tone="error">{fetchError}</InlineAlert></div>
		{/if}

		<!-- Loading -->
		{#if loading}
			<div class="loading-wrap">
				<Spinner size={20} />
				Loading projects…
			</div>

		<!-- Empty state -->
		{:else if projects.length === 0}
			<div class="empty-wrap">
				<EmptyState message="No projects yet" sub="Create your first project to start deploying services">
					{#snippet icon()}<Package size={28} />{/snippet}
				</EmptyState>
				<Button onclick={openModal}>
					<Plus size={14} />
					New Project
				</Button>
			</div>

		<!-- Project grid -->
		{:else}
			<div class="project-grid">
				{#each projects as project (project.id)}
					<div class="project-tile">
						<Card padding="0">
							<button class="project-card" onclick={() => goToProject(project)}>
								<Avatar initials={project.name[0] ?? '?'} size={40} />
								<div class="project-text">
									<div class="project-name">{project.name}</div>
									<div class="project-slug">{project.slug}</div>
								</div>
							</button>
						</Card>
					</div>
				{/each}
			</div>
		{/if}

	</div>
</div>

<!-- New Project Modal -->
<Modal bind:open={showModal} title="New Project">
	{#if createError}
		<div role="alert"><InlineAlert tone="error">{createError}</InlineAlert></div>
	{/if}

	<form class="create-form" bind:this={createForm} onsubmit={handleCreateProject}>
		<FormField label="Project Name" for="project-name">
			<TextField
				id="project-name"
				type="text"
				placeholder="My Project"
				bind:value={newProjectName}
				required
			/>
		</FormField>

		<FormField label="Slug" for="project-slug" hint="Lowercase letters, numbers, and hyphens only">
			<TextField
				id="project-slug"
				type="text"
				placeholder="my-project"
				bind:value={newProjectSlug}
				required
				pattern="[a-z0-9-]+"
			/>
		</FormField>
	</form>

	{#snippet footer()}
		<Button variant="secondary" onclick={closeModal} disabled={creating}>Cancel</Button>
		<Button onclick={() => createForm?.requestSubmit()} disabled={creating || !newProjectName.trim()}>
			{#if creating}
				<Spinner size={14} tone="current" />
				Creating…
			{:else}
				Create Project
			{/if}
		</Button>
	{/snippet}
</Modal>

<style>
	.projects-scroll {
		padding: 32px;
		overflow-y: auto;
		height: 100%;
	}
	.projects-inner {
		max-width: 900px;
		margin: 0 auto;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	@media (max-width: 639px) {
		.projects-scroll { padding: 16px 16px 72px; }
	}

	.loading-wrap {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 80px 0;
		gap: 12px;
		color: var(--text-muted);
		font-size: 14px;
	}
	.empty-wrap {
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 40px 0;
		gap: 8px;
	}

	.project-grid {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
		gap: 16px;
	}
	.project-tile :global(.ui-card) { transition: border-color var(--transition-fast); }
	.project-tile:hover :global(.ui-card) { border-color: var(--border-hover); }
	.project-card {
		width: 100%;
		text-align: left;
		cursor: pointer;
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
		background: transparent;
		border: none;
		border-radius: var(--radius-lg);
		font-family: inherit;
		color: inherit;
	}
	.project-text { min-width: 0; }
	.project-name {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
		margin-bottom: 4px;
		overflow-wrap: anywhere;
	}
	.project-slug {
		font-size: 12px;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}
	.create-form { display: flex; flex-direction: column; gap: 16px; }
</style>
