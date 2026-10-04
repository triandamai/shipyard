<script lang="ts">
	import { Plus } from '@lucide/svelte';
	import { page } from '$app/state';
	import { NavDrawer, Button } from '$lib/components/ui';
	import { projectStore } from '$lib/stores/project.store';
	import { orgStore } from '$lib/stores/org.store';
	import { uiStore } from '$lib/stores/ui.store';

	interface Props {
		orgSlug: string;
	}

	let { orgSlug }: Props = $props();

	let org = $derived($orgStore.activeOrg);
	let open = $derived(!$uiStore.sidebarCollapsed);

	let items = $derived(
		$projectStore.projects.map((p) => ({
			href: `/orgs/${orgSlug}/projects/${p.slug}`,
			label: p.name,
			active: page.url.pathname.includes(`/projects/${p.slug}`)
		}))
	);
</script>

<NavDrawer
	{open}
	persistent
	hideOnPhone
	title="Projects"
	{items}
	loading={$projectStore.isLoading}
	emptyText="No projects yet"
	onNavigate={() => {}}
>
	{#snippet header()}
		<div class="pd-org">
			<span class="pd-org-icon">{org ? org.name.charAt(0).toUpperCase() : 'O'}</span>
			<span class="pd-org-info">
				<span class="pd-org-name" title={org?.name ?? 'Organization'}>{org?.name ?? 'Organization'}</span>
				<span class="pd-org-slug">{org?.slug ?? ''}</span>
			</span>
		</div>
	{/snippet}
	{#snippet footer()}
		<!-- Kept handler-less: the pre-migration button had no click handler (see plan ruling). -->
		<Button variant="secondary" size="sm">
			<Plus size={14} /> New Project
		</Button>
	{/snippet}
</NavDrawer>

<style>
	.pd-org { display: flex; align-items: center; gap: 10px; min-width: 0; }
	.pd-org-icon {
		width: 30px;
		height: 30px;
		border-radius: var(--radius-md);
		background: var(--accent-muted);
		color: var(--accent);
		font-size: 13px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.pd-org-info { display: flex; flex-direction: column; min-width: 0; }
	.pd-org-name { font-size: 13px; font-weight: 700; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.pd-org-slug { font-size: 11px; color: var(--text-dim); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.pd-org-info, .pd-org { max-width: 100%; }
</style>
