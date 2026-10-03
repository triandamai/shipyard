<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { DataTable, StatusDot, Badge, Button, PageHeader, Select, TextField } from '$lib/components/ui';

	interface AppDeployment {
		id: string;
		org_name: string;
		org_slug: string;
		service_name: string;
		status: string;
		triggered_by: string | null;
		created_at: string;
		finished_at: string | null;
	}

	let totalDeployments = $state(0);

	// Page-local filters. These are NOT part of DataTable's own `search` concept —
	// DataTable's server-mode fetchPage signature only threads {page, pageSize,
	// search} through, so these live here and are read directly inside
	// fetchDeploymentsPage's closure (see below) in addition to those params.
	let orgFilter = $state('');
	let statusFilter = $state('');
	let refreshNonce = $state(0);

	const STATUS_OPTIONS = [
		{ value: '', label: 'All Statuses' },
		{ value: 'running', label: 'Running' },
		{ value: 'success', label: 'Success' },
		{ value: 'failed', label: 'Failed' },
		{ value: 'queued', label: 'Queued' },
		{ value: 'cancelled', label: 'Cancelled' }
	];

	async function fetchDeploymentsPage(params: { page: number; pageSize: number; search: string }) {
		const qs = new URLSearchParams({
			page: String(params.page),
			limit: String(params.pageSize),
			...(params.search ? { q: params.search } : {}),
			...(orgFilter ? { org: orgFilter } : {}),
			...(statusFilter ? { status: statusFilter } : {})
		});
		const res = await api.get<{ items: AppDeployment[]; total: number }>(`/admin/deployments/app?${qs}`);
		if (!res.data) throw new Error(res.error?.message ?? 'Failed to load deployments');
		totalDeployments = res.data.total;
		return { rows: res.data.items, total: res.data.total };
	}

	// Translates this page's pre-existing status grouping (previously used only
	// to pick a CSS color via a hand-rolled dot: success/done -> var(--ok) green,
	// failed/error -> var(--danger) red, running -> var(--accent) blue, anything
	// else -> var(--text-3) neutral grey) onto StatusDot's fixed 5-state enum,
	// preserving the same groupings and color intent. `running` maps to the
	// pulsing `deploying` dot rather than the static `running` one since it's
	// the one state here that's still in flight (same reasoning as the Nodes
	// page: pulsing is reserved for transitional states).
	function statusDot(s: string): 'running' | 'pending' | 'deploying' | 'failed' | 'stopped' {
		if (s === 'success' || s === 'done') return 'running';
		if (s === 'failed' || s === 'error') return 'failed';
		if (s === 'running') return 'deploying';
		return 'stopped';
	}

	// Unchanged from the pre-migration version of this page.
	function relTime(iso: string): string {
		const diff = Date.now() - new Date(iso).getTime();
		const m = Math.floor(diff / 60000);
		if (m < 1) return 'just now';
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		return `${Math.floor(h / 24)}d ago`;
	}
</script>

<PageHeader title="App Deployments" subtitle="All service deployments across organizations.">
	{#snippet actions()}
		<Badge tone="neutral">{totalDeployments} total</Badge>
		<Button variant="ghost" size="sm" onclick={() => goto('/admin/deployments/provisioning')}>
			Tenant Provisioning &rarr;
		</Button>
	{/snippet}
</PageHeader>

<div class="dp-toolbar">
	<div class="dp-org">
		<TextField placeholder="Filter by org…" bind:value={orgFilter} />
	</div>
	<div class="dp-status">
		<Select bind:value={statusFilter} options={STATUS_OPTIONS} />
	</div>
	<Button variant="secondary" size="sm" onclick={() => refreshNonce++}>
		<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13">
			<path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/>
		</svg>
		Refresh
	</Button>
</div>

{#key orgFilter + statusFilter + refreshNonce}
	<DataTable
		fetchPage={fetchDeploymentsPage}
		rowKey={(d) => d.id}
		columns={[
			{ key: 'org', label: 'Org' },
			{ key: 'service', label: 'Service' },
			{ key: 'status', label: 'Status' },
			{ key: 'triggered', label: 'Triggered' },
			{ key: 'started', label: 'Started' }
		]}
		emptyMessage="No app deployments found."
	>
		{#snippet row(d)}
			<tr>
				<td><a class="dp-link" href="/orgs/{d.org_slug}" target="_blank">{d.org_name}</a></td>
				<td>{d.service_name}</td>
				<td>
					<span class="dp-status-cell">
						<StatusDot status={statusDot(d.status)} />
						{d.status}
					</span>
				</td>
				<td class="dp-dim">{d.triggered_by ?? '—'}</td>
				<td class="dp-dim">{relTime(d.created_at)}</td>
			</tr>
		{/snippet}
	</DataTable>
{/key}

<style>
	.dp-toolbar {
		display: flex;
		align-items: center;
		gap: 8px;
		margin-bottom: 14px;
		flex-wrap: wrap;
	}
	.dp-org { width: 200px; }
	.dp-status { width: 160px; }
	.dp-link { color: var(--text-primary); text-decoration: none; }
	.dp-link:hover { text-decoration: underline; color: var(--accent); }
	.dp-status-cell { display: inline-flex; align-items: center; gap: 6px; font-size: 12.5px; font-weight: 500; color: var(--text-secondary); }
	.dp-dim { color: var(--text-muted); font-size: 11.5px; }

	@media (max-width: 640px) {
		.dp-org, .dp-status { width: 100%; }
	}
</style>
