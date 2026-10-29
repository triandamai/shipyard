<script lang="ts">
	import { api } from '$lib/api/client';
	import { PageHeader } from '$lib/components/ui';
	import ObservabilityLogs from '$lib/components/ObservabilityLogs.svelte';
</script>

<PageHeader
	title="Platform Logs"
	subtitle="Full-stack observability across Traefik, Database, Backend, Frontend, and App containers."
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
		margin-top: 16px;
		height: calc(100vh - 180px);
	}
</style>
