<script lang="ts">
	import { api } from '$lib/api/client';
	import { PageHeader } from '$lib/components/ui';
	import ObservabilityLogs from '$lib/components/ObservabilityLogs.svelte';
</script>

<PageHeader
	eyebrow="OBSERVABILITY"
	title="Platform Logs"
	subtitle="Durable masked logs across Traefik, Database, Backend, Frontend, and tenant containers."
/>

<div class="logs-page-container">
	<ObservabilityLogs
		scope="admin"
		allowStackTabs={true}
		fetchLogs={(params) => api.getAdminLogs(params)}
		fetchStats={(params) => api.getAdminLogStats(params)}
	/>
</div>

<style>
	.logs-page-container {
		display: flex;
		flex-direction: column;
		min-height: 0;
		flex: 1;
		height: calc(100vh - 170px);
	}
</style>
