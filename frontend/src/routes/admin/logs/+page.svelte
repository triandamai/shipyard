<script lang="ts">
	import { api } from '$lib/api/client';
	import { PageHeader } from '$lib/components/ui';
	import ObservabilityLogs from '$lib/components/ObservabilityLogs.svelte';
</script>

<svelte:head><title>Logs · Admin · Shipyard</title></svelte:head>

<div class="logs-page">
	<PageHeader eyebrow="Observability" title="Platform logs" subtitle="Traefik, Postgres, backend, frontend and every tenant container on this node." />

	<ObservabilityLogs
		scope="admin"
		fetchLogs={(params) => api.getAdminLogs(params)}
		fetchStats={(params) => api.getAdminLogStats(params)}
	/>
</div>

<style>
	/* The admin shell scrolls .main; give the log console a viewport-tall frame. */
	.logs-page {
		display: flex;
		flex-direction: column;
		height: calc(100dvh - 48px);
		min-height: 560px;
	}
	.logs-page :global(.ui-page-header) { margin-bottom: 16px; }
	@media (max-width: 639px) {
		.logs-page { height: auto; }
	}
</style>
