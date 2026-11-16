<script lang="ts">
	import { Plus, ChevronsUpDown, ArrowLeftRight, Package, Command, ShieldAlert } from '@lucide/svelte';
	import { page } from '$app/state';
	import { NavDrawer, Button } from '$lib/components/ui';
	import { projectStore } from '$lib/stores/project.store';
	import { orgStore } from '$lib/stores/org.store';
	import { uiStore } from '$lib/stores/ui.store';
	import { authStore } from '$lib/stores/auth.store';

	interface Props {
		orgSlug: string;
	}

	let { orgSlug }: Props = $props();

	let org = $derived($orgStore.activeOrg);
	let open = $derived(!$uiStore.sidebarCollapsed);
	let isSuperadmin = $derived($authStore.user?.is_superadmin === true);

	function isActive(href: string): boolean {
		return page.url.pathname === href || page.url.pathname.startsWith(href + '/');
	}

	function closeOnMobile() {
		if (typeof window !== 'undefined' && window.matchMedia('(max-width: 639px)').matches) {
			uiStore.setSidebarCollapsed(true);
		}
	}

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
	title="Projects"
	{items}
	loading={$projectStore.isLoading}
	emptyText="No projects yet"
	onNavigate={closeOnMobile}
	onClose={() => uiStore.setSidebarCollapsed(true)}
>
	{#snippet header()}
		<a href="/orgs" class="pd-org" title="Switch organization" onclick={closeOnMobile}>
			<span class="pd-org-icon">{org ? org.name.charAt(0).toUpperCase() : 'O'}</span>
			<span class="pd-org-info">
				<span class="pd-org-name" title={org?.name ?? 'Organization'}>{org?.name ?? 'Organization'}</span>
				<span class="pd-org-slug">{org?.slug ?? ''}</span>
			</span>
			<ChevronsUpDown size={14} class="pd-org-chevron" />
		</a>
	{/snippet}
	{#snippet extra()}
		<nav class="pd-mobile-nav" aria-label="More navigation">
			<a
				href="/orgs/{orgSlug}/transfers"
				class="pd-mobile-item"
				class:pd-mobile-item--active={isActive(`/orgs/${orgSlug}/transfers`)}
				aria-current={isActive(`/orgs/${orgSlug}/transfers`) ? 'page' : undefined}
				onclick={closeOnMobile}
			>
				<span class="pd-mobile-icon"><ArrowLeftRight size={16} /></span>
				<span class="pd-mobile-label">Transfers</span>
			</a>
			<a
				href="/orgs/{orgSlug}/registry"
				class="pd-mobile-item"
				class:pd-mobile-item--active={isActive(`/orgs/${orgSlug}/registry`)}
				aria-current={isActive(`/orgs/${orgSlug}/registry`) ? 'page' : undefined}
				onclick={closeOnMobile}
			>
				<span class="pd-mobile-icon"><Package size={16} /></span>
				<span class="pd-mobile-label">Registry</span>
			</a>
			{#if isSuperadmin}
				<a
					href="/admin"
					class="pd-mobile-item"
					class:pd-mobile-item--active={isActive('/admin')}
					aria-current={isActive('/admin') ? 'page' : undefined}
					onclick={closeOnMobile}
				>
					<span class="pd-mobile-icon"><ShieldAlert size={16} /></span>
					<span class="pd-mobile-label">Admin Panel</span>
				</a>
			{/if}
			<button
				type="button"
				class="pd-mobile-item"
				onclick={() => {
					closeOnMobile();
					uiStore.openCommandPalette();
				}}
			>
				<span class="pd-mobile-icon"><Command size={16} /></span>
				<span class="pd-mobile-label">Command Palette</span>
			</button>
		</nav>
	{/snippet}
	{#snippet footer()}
		<Button variant="secondary" size="sm" href="/orgs/{orgSlug}/projects" onclick={closeOnMobile}>
			<Plus size={14} /> New Project
		</Button>
	{/snippet}
</NavDrawer>

<style>
	.pd-org {
		display: flex;
		align-items: center;
		gap: 10px;
		min-width: 0;
		max-width: 100%;
		padding: 4px;
		margin: -4px;
		border-radius: var(--radius-md);
		text-decoration: none;
		color: inherit;
		transition: background var(--transition-fast);
	}
	.pd-org:hover { background: var(--bg-hover); }
	.pd-org:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
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
	.pd-org-info { display: flex; flex-direction: column; min-width: 0; flex: 1; max-width: 100%; }
	.pd-org-name { font-size: 13px; font-weight: 700; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.pd-org-slug { font-size: 11px; color: var(--text-dim); font-family: var(--font-mono); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.pd-org :global(.pd-org-chevron) { color: var(--text-dim); flex-shrink: 0; }

	.pd-mobile-nav { display: none; }

	@media (max-width: 639px) {
		.pd-mobile-nav {
			display: flex;
			flex-direction: column;
			gap: 2px;
		}
		.pd-mobile-item {
			display: flex;
			align-items: center;
			gap: 10px;
			width: 100%;
			min-height: 40px;
			padding: 9px 10px;
			border: none;
			border-radius: var(--radius-md);
			background: transparent;
			font-family: var(--font-sans);
			font-size: 13.5px;
			font-weight: 500;
			color: var(--text-secondary);
			text-decoration: none;
			text-align: left;
			cursor: pointer;
			min-width: 0;
		}
		.pd-mobile-item:hover { background: var(--bg-hover); color: var(--text-primary); }
		.pd-mobile-item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
		.pd-mobile-item--active { background: var(--accent-muted); color: var(--accent); font-weight: 600; }
		.pd-mobile-icon {
			width: 16px;
			display: flex;
			justify-content: center;
			opacity: 0.85;
			flex-shrink: 0;
		}
		.pd-mobile-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
	}
</style>
