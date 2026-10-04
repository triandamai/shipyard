<script lang="ts">
	import { page } from '$app/state';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Tabs from '$lib/components/ui/Tabs.svelte';

	let { children } = $props();

	let orgSlug     = $derived(page.params.orgSlug ?? '');
	let currentPath = $derived(page.url.pathname);

	let tabs = $derived([
		{ id: 'browse',   label: 'Browse',   href: `/orgs/${orgSlug}/registry`          },
		{ id: 'settings', label: 'Settings', href: `/orgs/${orgSlug}/registry/settings` },
	]);

	function isActive(href: string) {
		if (href === `/orgs/${orgSlug}/registry`) {
			return currentPath === href || (
				currentPath.startsWith(href + '/') && !currentPath.startsWith(href + '/settings')
			);
		}
		return currentPath === href || currentPath.startsWith(href + '/');
	}

	let activeTab = $derived(tabs.find((t) => isActive(t.href))?.id ?? '');
</script>

<div class="registry-shell">
	<div class="registry-header">
		<PageHeader
			title="Registry"
			subtitle="Artifact storage for images, static bundles, and edge functions"
		/>
		<Tabs {tabs} value={activeTab} ariaLabel="Registry sections" />
	</div>

	<div class="registry-content">
		{@render children()}
	</div>
</div>

<style>
	.registry-shell {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow: hidden;
	}

	.registry-header {
		flex-shrink: 0;
		padding: 24px 32px 0;
		display: flex;
		flex-direction: column;
		gap: 16px;
		border-bottom: 1px solid var(--border);
	}

	.registry-header :global(.ui-page-header) { margin-bottom: 0; }

	.registry-content {
		flex: 1;
		overflow-y: auto;
	}
</style>
