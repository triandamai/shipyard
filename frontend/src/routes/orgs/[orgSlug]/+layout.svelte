<script lang="ts">
	import { onMount } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import { uiStore } from '$lib/stores/ui.store';
	import { subscribeToOrgEvents } from '$lib/mqtt/subscriptions';
	import { versionStore } from '$lib/stores/version.store';
	import IconSidebar from '$lib/components/IconSidebar.svelte';
	import ContextPanel from '$lib/components/ContextPanel.svelte';
	import PanelContainer from '$lib/components/PanelContainer.svelte';

	let { children } = $props();

	let orgSlug = $derived(page.params.orgSlug ?? '');
	let collapsed = $derived($uiStore.sidebarCollapsed);

	let unsubscribeOrgEvents: (() => void) | null = null;

	onMount(async () => {
		if (!orgSlug) return;

		const orgsRes = await api.getOrgs();
		if (orgsRes.data) {
			orgStore.setOrganizations(orgsRes.data);
			const found = orgsRes.data.find((o) => o.slug === orgSlug || o.id === orgSlug);

			if (!found) {
				// User no longer has access to this org (e.g. membership revoked).
				// Clear stale state and send them to the org picker.
				orgStore.setActiveOrg(null);
				projectStore.setProjects([]);
				goto('/orgs');
				return;
			}

			orgStore.setActiveOrg(found);
			projectStore.setLoading(true);

			const [projectsRes, membershipRes] = await Promise.all([
				api.getProjects(found.id),
				api.getMyMembership(found.id)
			]);
			if (projectsRes.data) projectStore.setProjects(projectsRes.data);
			// Always call setMyMembership (even on error) to mark membershipLoaded = true
			// so permission guards in child layouts don't block indefinitely.
			orgStore.setMyMembership(membershipRes.data ?? null);
			projectStore.setLoading(false);

			unsubscribeOrgEvents = subscribeToOrgEvents(found.id);

			// Check for updates once per org load (non-blocking).
			api.checkVersion().then((res) => {
				if (res.data) versionStore.setInfo(res.data);
			});
		}

		return () => {
			unsubscribeOrgEvents?.();
		};
	});
</script>

<div class="app-shell">
	<IconSidebar {orgSlug} />
	<ContextPanel {orgSlug} {collapsed} />

	<main class="main-content" class:panel-hidden={collapsed}>
		{@render children()}
	</main>

	<PanelContainer />
</div>

<style>
	.app-shell {
		display: flex;
		height: 100vh;
		overflow: hidden;
		background: var(--bg-base);
	}

	.main-content {
		flex: 1;
		/* icon sidebar (52px) + context panel (220px) */
		margin-left: calc(52px + 220px);
		height: 100vh;
		overflow: hidden;
		display: flex;
		flex-direction: column;
		transition: margin-left 0.2s ease;
	}

	.main-content.panel-hidden {
		/* icon sidebar only */
		margin-left: 52px;
	}

	@media (max-width: 639px) {
		.main-content,
		.main-content.panel-hidden {
			margin-left: 0;
			padding-bottom: 56px;
		}
	}
</style>
