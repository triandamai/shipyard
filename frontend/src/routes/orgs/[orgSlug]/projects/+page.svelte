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
	<div style="max-width: 900px; margin: 0 auto;">

		<!-- Header -->
		<div
			style="
				display: flex;
				align-items: center;
				justify-content: space-between;
				margin-bottom: 28px;
			"
		>
			<div>
				<h1 style="font-size: 20px; font-weight: 700; margin-bottom: 4px;">Projects</h1>
				{#if currentOrg}
					<p style="color: var(--text-muted); font-size: 13px;">{currentOrg.name}</p>
				{/if}
			</div>
			<button class="btn btn-primary" onclick={openModal}>
				+ New Project
			</button>
		</div>

		<!-- Error -->
		{#if fetchError}
			<div
				style="
					padding: 12px 16px;
					background: var(--accent-red-muted);
					border: 1px solid var(--accent-red);
					border-radius: var(--radius-md);
					color: var(--accent-red);
					font-size: 13px;
					margin-bottom: 20px;
				"
			>
				{fetchError}
			</div>
		{/if}

		<!-- Loading -->
		{#if loading}
			<div
				style="
					display: flex;
					align-items: center;
					justify-content: center;
					padding: 80px 0;
					gap: 12px;
					color: var(--text-muted);
					font-size: 14px;
				"
			>
				<span
					style="
						width: 20px;
						height: 20px;
						border: 2px solid var(--border);
						border-top-color: var(--accent);
						border-radius: 50%;
						display: inline-block;
						animation: spin 0.7s linear infinite;
					"
				></span>
				Loading projects…
			</div>

		<!-- Empty state -->
		{:else if projects.length === 0}
			<div
				style="
					display: flex;
					flex-direction: column;
					align-items: center;
					justify-content: center;
					padding: 80px 0;
					gap: 16px;
					text-align: center;
				"
			>
				<div
					style="
						width: 56px;
						height: 56px;
						border-radius: var(--radius-lg);
						background: var(--bg-elevated);
						border: 1px solid var(--border);
						display: flex;
						align-items: center;
						justify-content: center;
						font-size: 24px;
					"
				>
					📦
				</div>
				<div>
					<p style="color: var(--text-primary); font-weight: 500; margin-bottom: 4px;">
						No projects yet
					</p>
					<p style="color: var(--text-muted); font-size: 13px;">
						Create your first project to start deploying services
					</p>
				</div>
				<button class="btn btn-primary" onclick={openModal}>
					+ New Project
				</button>
			</div>

		<!-- Project grid -->
		{:else}
			<div
				style="
					display: grid;
					grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
					gap: 16px;
				"
			>
				{#each projects as project (project.id)}
					<button
						class="card card-interactive"
						style="
							text-align: left;
							cursor: pointer;
							display: flex;
							flex-direction: column;
							gap: 12px;
						"
						onclick={() => goToProject(project)}
					>
						<div
							style="
								width: 40px;
								height: 40px;
								border-radius: var(--radius-md);
								background: var(--accent-blue-muted);
								border: 1px solid rgba(59, 130, 246, 0.3);
								display: flex;
								align-items: center;
								justify-content: center;
								font-size: 18px;
								font-weight: 700;
								color: var(--accent-blue);
							"
						>
							{project.name[0]?.toUpperCase() ?? '?'}
						</div>
						<div>
							<div
								style="
									font-size: 14px;
									font-weight: 600;
									color: var(--text-primary);
									margin-bottom: 4px;
								"
							>
								{project.name}
							</div>
							<div
								style="
									font-size: 12px;
									color: var(--text-muted);
									font-family: var(--font-mono);
								"
							>
								{project.slug}
							</div>
						</div>
					</button>
				{/each}
			</div>
		{/if}

	</div>
</div>

<!-- New Project Modal -->
{#if showModal}
	<div
		style="
			position: fixed;
			inset: 0;
			background: rgba(0,0,0,0.6);
			z-index: 50;
			display: flex;
			align-items: center;
			justify-content: center;
			padding: 24px;
		"
		onclick={(e) => { if (e.target === e.currentTarget) closeModal(); }}
		onkeydown={(e) => { if (e.key === 'Escape') closeModal(); }}
		role="dialog"
		aria-modal="true"
		aria-label="Create project"
		tabindex="-1"
	>
		<div
			style="
				background: var(--bg-elevated);
				border: 1px solid var(--border);
				border-radius: var(--radius-xl);
				padding: 28px;
				width: 100%;
				max-width: 420px;
				box-shadow: var(--shadow-lg);
			"
		>
			<h2 style="font-size: 16px; font-weight: 600; margin-bottom: 20px;">
				New Project
			</h2>

			{#if createError}
				<div
					style="
						padding: 10px 14px;
						background: var(--accent-red-muted);
						border: 1px solid var(--accent-red);
						border-radius: var(--radius-md);
						color: var(--accent-red);
						font-size: 13px;
						margin-bottom: 16px;
					"
				>
					{createError}
				</div>
			{/if}

			<form
				onsubmit={handleCreateProject}
				style="display: flex; flex-direction: column; gap: 16px;"
			>
				<div style="display: flex; flex-direction: column; gap: 6px;">
					<label
						for="project-name"
						style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
					>
						Project Name
					</label>
					<input
						id="project-name"
						type="text"
						class="input"
						placeholder="My Project"
						bind:value={newProjectName}
						required
					/>
				</div>

				<div style="display: flex; flex-direction: column; gap: 6px;">
					<label
						for="project-slug"
						style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
					>
						Slug
					</label>
					<input
						id="project-slug"
						type="text"
						class="input"
						placeholder="my-project"
						bind:value={newProjectSlug}
						required
						pattern="[a-z0-9-]+"
					/>
					<span style="font-size: 11px; color: var(--text-dim);">
						Lowercase letters, numbers, and hyphens only
					</span>
				</div>

				<div
					style="
						display: flex;
						justify-content: flex-end;
						gap: 10px;
						margin-top: 4px;
					"
				>
					<button
						type="button"
						class="btn btn-secondary"
						onclick={closeModal}
						disabled={creating}
					>
						Cancel
					</button>
					<button
						type="submit"
						class="btn btn-primary"
						disabled={creating || !newProjectName.trim()}
					>
						{#if creating}
							<span
								style="
									width: 14px;
									height: 14px;
									border: 2px solid rgba(255,255,255,0.3);
									border-top-color: white;
									border-radius: 50%;
									display: inline-block;
									animation: spin 0.7s linear infinite;
								"
							></span>
							Creating…
						{:else}
							Create Project
						{/if}
					</button>
				</div>
			</form>
		</div>
	</div>
{/if}

<style>
	.projects-scroll {
		padding: 32px;
		overflow-y: auto;
		height: 100%;
	}

	@media (max-width: 639px) {
		.projects-scroll { padding: 16px 16px 72px; }
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
