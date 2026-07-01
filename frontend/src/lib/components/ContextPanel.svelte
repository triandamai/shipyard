<script lang="ts">
	import { Plus, FolderOpen } from '@lucide/svelte';
	import { projectStore } from '$lib/stores/project.store';
	import { orgStore } from '$lib/stores/org.store';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';

	interface Props {
		orgSlug: string;
		collapsed?: boolean;
	}

	let { orgSlug, collapsed = false }: Props = $props();

	let projects = $derived($projectStore.projects);
	let activeProject = $derived($projectStore.activeProject);
	let org = $derived($orgStore.activeOrg);

	function navigateToProject(projectSlug: string) {
		goto(`/orgs/${orgSlug}/projects/${projectSlug}`);
	}

	function isProjectActive(projectSlug: string): boolean {
		return page.url.pathname.includes(`/projects/${projectSlug}`);
	}
</script>

<aside class="context-panel" class:hidden={collapsed}>
	<!-- Org header -->
	<div class="org-header">
		<div class="org-icon">
			{#if org}
				{org.name.charAt(0).toUpperCase()}
			{:else}
				O
			{/if}
		</div>
		<div class="org-info">
			<span class="org-name">{org?.name ?? 'Organization'}</span>
			<span class="org-slug">{org?.slug ?? ''}</span>
		</div>
	</div>

	<!-- Projects section -->
	<div class="section">
		<div class="section-label">Projects</div>

		{#if $projectStore.isLoading}
			<div class="loading-state">
				<span>Loading…</span>
			</div>
		{:else if projects.length === 0}
			<div class="empty-state">
				<FolderOpen size={16} />
				<span>No projects yet</span>
			</div>
		{:else}
			<ul class="project-list">
				{#each projects as project (project.id)}
					<li>
						<button
							class="project-item"
							class:active={isProjectActive(project.slug)}
							onclick={() => navigateToProject(project.slug)}
						>
							<span class="project-dot"></span>
							<span class="project-name">{project.name}</span>
						</button>
					</li>
				{/each}
			</ul>
		{/if}
	</div>

	<!-- New project button -->
	<div class="bottom-actions">
		<button class="new-project-btn">
			<Plus size={14} />
			New Project
		</button>
	</div>
</aside>

<style>
	.context-panel {
		width: var(--context-panel-width, 220px);
		flex-shrink: 0;
		background: var(--sidebar-surface);
		border-right: 1px solid var(--sidebar-border);
		display: flex;
		flex-direction: column;
		height: 100vh;
		position: fixed;
		left: var(--sidebar-width, 52px);
		top: 0;
		z-index: 40;
		overflow: hidden;
		transition: transform 0.2s ease, opacity 0.2s ease;
	}

	.context-panel.hidden {
		transform: translateX(-100%);
		opacity: 0;
		pointer-events: none;
	}

	.org-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 14px;
		height: 52px;
		border-bottom: 1px solid var(--sidebar-border);
		flex-shrink: 0;
	}

	.org-icon {
		width: 24px;
		height: 24px;
		border-radius: var(--radius-sm);
		background: rgba(59, 130, 246, 0.16);
		color: #60A5FA;
		font-size: 11px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.org-info {
		display: flex;
		flex-direction: column;
		min-width: 0;
	}

	.org-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--sidebar-text-active);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.org-slug {
		font-size: 11px;
		color: var(--sidebar-text);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.section {
		flex: 1;
		overflow-y: auto;
		padding: 8px 0;
	}

	.section::-webkit-scrollbar {
		width: 3px;
	}

	.section::-webkit-scrollbar-thumb {
		background: var(--sidebar-border);
		border-radius: 2px;
	}

	.section-label {
		font-size: 10px;
		font-weight: 700;
		color: var(--sidebar-text);
		text-transform: uppercase;
		letter-spacing: 0.10em;
		padding: 4px 14px 6px;
		opacity: 0.7;
	}

	.project-list {
		list-style: none;
		margin: 0;
		padding: 0;
	}

	.project-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 6px 14px;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		border-radius: 0;
		color: var(--sidebar-text-hover);
		font-size: 13px;
		font-family: var(--font-sans);
		transition: all var(--transition-fast);
	}

	.project-item:hover {
		background: var(--sidebar-hover-bg);
		color: var(--sidebar-text-active);
	}

	.project-item.active {
		background: var(--sidebar-active-bg);
		color: #60A5FA;
	}

	.project-dot {
		width: 5px;
		height: 5px;
		border-radius: 50%;
		background: currentColor;
		opacity: 0.7;
		flex-shrink: 0;
	}

	.project-name {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.loading-state,
	.empty-state {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 14px;
		color: var(--sidebar-text);
		font-size: 12px;
	}

	.bottom-actions {
		border-top: 1px solid var(--sidebar-border);
		padding: 10px;
		flex-shrink: 0;
	}

	.new-project-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 7px 10px;
		background: transparent;
		border: 1px dashed rgba(255, 255, 255, 0.12);
		border-radius: var(--radius-md);
		color: var(--sidebar-text);
		font-size: 12px;
		font-family: var(--font-sans);
		font-weight: 500;
		cursor: pointer;
		transition: all var(--transition-fast);
	}

	.new-project-btn:hover {
		border-color: rgba(96, 165, 250, 0.5);
		color: #60A5FA;
		background: rgba(59, 130, 246, 0.08);
	}

	@media (max-width: 639px) {
		.context-panel { display: none; }
	}
</style>
