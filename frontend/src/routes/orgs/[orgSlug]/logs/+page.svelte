<script lang="ts">
	import { page } from '$app/state';
	import { replaceState } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { projectStore } from '$lib/stores/project.store';
	import { PageHeader, Spinner } from '$lib/components/ui';
	import ObservabilityLogs, { type LogServiceOption } from '$lib/components/ObservabilityLogs.svelte';

	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let projects = $derived($projectStore.projects);

	// Read once: the logs view owns the filters after mount and mirrors them back.
	const initialService = page.url.searchParams.get('service') ?? '';
	const initialProject = page.url.searchParams.get('project') ?? '';

	let services = $state<LogServiceOption[] | null>(null);
	let loadedFor = '';

	$effect(() => {
		const key = `${orgId}:${projects.map((p) => p.id).join(',')}`;
		if (!orgId || $projectStore.isLoading || key === loadedFor) return;
		loadedFor = key;
		(async () => {
			const lists = await Promise.all(
				projects.map(async (p) => {
					const res = await api.getServices(p.id);
					return (res.data ?? []).map((s) => ({ id: s.id, name: s.name, project_id: p.id, project_name: p.name }));
				})
			);
			services = lists.flat();
		})();
	});

	function mirrorFilters({ projectId, serviceId }: { projectId: string; serviceId: string }) {
		const url = new URL(page.url);
		if (serviceId) url.searchParams.set('service', serviceId); else url.searchParams.delete('service');
		if (projectId && !serviceId) url.searchParams.set('project', projectId); else url.searchParams.delete('project');
		replaceState(url, page.state);
	}
</script>

<svelte:head><title>Logs · Shipyard</title></svelte:head>

<div class="logs-page">
	<PageHeader eyebrow="Observability" title="Logs" subtitle="Runtime output from every service in this organization, masked and kept for 90 days." />

	{#if !orgId || services === null}
		<div class="logs-loading"><Spinner size={16} /> Loading services…</div>
	{:else}
		<ObservabilityLogs
			scope="org"
			{services}
			projectId={initialProject}
			serviceId={initialService}
			onFilterChange={mirrorFilters}
			fetchLogs={(params) => api.getOrgLogs(orgId, params)}
			fetchStats={(params) => api.getOrgLogStats(orgId, params)}
		/>
	{/if}
</div>

<style>
	.logs-page {
		flex: 1;
		min-height: 0;
		display: flex;
		flex-direction: column;
		padding: 20px 28px 24px;
		overflow-y: auto;
	}
	.logs-page :global(.ui-page-header) { margin-bottom: 16px; }
	.logs-loading {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: var(--text-muted);
		padding: 24px 0;
	}
	@media (max-width: 639px) {
		.logs-page { padding: 16px 14px; }
	}
</style>
