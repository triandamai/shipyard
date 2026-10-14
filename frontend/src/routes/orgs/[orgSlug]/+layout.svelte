<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import { subscribeToOrgEvents } from '$lib/mqtt/subscriptions';
	import { versionStore } from '$lib/stores/version.store';
	import AppNav from '$lib/components/AppNav.svelte';
	import ProjectDrawer from '$lib/components/ProjectDrawer.svelte';
	import PanelContainer from '$lib/components/PanelContainer.svelte';

	let { children } = $props();

	let orgSlug = $derived(page.params.orgSlug ?? '');

	let unsubscribeOrgEvents: (() => void) | null = null;
	let lastLoadedSlug = '';

	$effect(() => {
		const currentSlug = orgSlug;
		if (!currentSlug || currentSlug === lastLoadedSlug) return;
		lastLoadedSlug = currentSlug;

		let isCancelled = false;

		(async () => {
			unsubscribeOrgEvents?.();
			unsubscribeOrgEvents = null;

			const orgsRes = await api.getOrgs();
			if (isCancelled) return;

			let found = orgsRes.data?.find((o) => o.slug === currentSlug || o.id === currentSlug);

			if (!found) {
				const singleRes = await api.getOrg(currentSlug);
				if (isCancelled) return;
				if (singleRes.data) {
					found = singleRes.data;
				}
			}

			if (!found) {
				// User no longer has access to this org (e.g. membership revoked).
				// Clear stale state and send them to the org picker.
				orgStore.setActiveOrg(null);
				projectStore.setProjects([]);
				goto('/orgs');
				return;
			}

			if (orgsRes.data) {
				if (!orgsRes.data.some((o) => o.id === found!.id)) {
					orgStore.setOrganizations([...orgsRes.data, found]);
				} else {
					orgStore.setOrganizations(orgsRes.data);
				}
			}

			orgStore.setActiveOrg(found);
			projectStore.setLoading(true);

			const [projectsRes, membershipRes] = await Promise.all([
				api.getProjects(found.id),
				api.getMyMembership(found.id)
			]);
			if (isCancelled) return;

			if (projectsRes.data) projectStore.setProjects(projectsRes.data);
			// Always call setMyMembership (even on error) to mark membershipLoaded = true
			// so permission guards in child layouts don't block indefinitely.
			orgStore.setMyMembership(membershipRes.data ?? null);
			projectStore.setLoading(false);

			unsubscribeOrgEvents = subscribeToOrgEvents(found.id);

			// Check for updates once per org load (non-blocking).
			api.checkVersion().then((res) => {
				if (!isCancelled && res.data) versionStore.setInfo(res.data);
			});
		})();

		return () => {
			isCancelled = true;
			unsubscribeOrgEvents?.();
			unsubscribeOrgEvents = null;
		};
	});
</script>

<div class="app-shell">
	<AppNav {orgSlug} />
	<ProjectDrawer {orgSlug} />
	<main class="main-content">
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
		min-width: 0;
		height: 100vh;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}
	@media (max-width: 639px) {
		.main-content { padding-bottom: 56px; }
	}
</style>
