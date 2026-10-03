<script lang="ts">
	import { api } from '$lib/api/client';
	import { DataTable, StatusDot, Badge, PageHeader } from '$lib/components/ui';

	interface ProvisioningJob {
		id: string;
		org_name: string;
		org_slug: string;
		name: string;
		provider: string;
		region: string;
		status: string;
		created_at: string;
	}

	let totalJobs = $state(0);

	async function fetchProvisioningPage(params: { page: number; pageSize: number; search: string }) {
		const res = await api.get<{ items: ProvisioningJob[]; total: number }>(
			`/admin/deployments/provisioning?page=${params.page}&limit=${params.pageSize}&q=${encodeURIComponent(params.search)}`
		);
		if (!res.data) throw new Error(res.error?.message ?? 'Failed to load provisioning jobs');
		totalJobs = res.data.total;
		return { rows: res.data.items, total: res.data.total };
	}

	// Same `compute_nodes.status` -> StatusDot mapping as the Nodes page (Task 44) —
	// this endpoint reads the same table (filtered to the non-active/non-stopped
	// rows), so the same 7-state -> 5-dot collapse and label set apply. See the
	// Nodes page (src/routes/admin/nodes/+page.svelte) for the full reasoning.
	type StatusKey =
		| 'active'
		| 'degraded'
		| 'failed'
		| 'provisioning'
		| 'cloud_init_running'
		| 'wireguard_joined'
		| 'stopped';

	const STATUS_LABEL: Record<StatusKey, string> = {
		active: 'Active',
		degraded: 'Degraded',
		failed: 'Failed',
		provisioning: 'Provisioning',
		cloud_init_running: 'Init',
		wireguard_joined: 'Joining',
		stopped: 'Stopped'
	};

	const STATUS_DOT: Record<StatusKey, 'running' | 'pending' | 'deploying' | 'failed' | 'stopped'> = {
		active: 'running',
		degraded: 'failed',
		failed: 'failed',
		provisioning: 'pending',
		cloud_init_running: 'deploying',
		wireguard_joined: 'deploying',
		stopped: 'stopped'
	};

	function statusLabel(s: string): string {
		return STATUS_LABEL[s as StatusKey] ?? s;
	}
	function statusDot(s: string): 'running' | 'pending' | 'deploying' | 'failed' | 'stopped' {
		return STATUS_DOT[s as StatusKey] ?? 'stopped';
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

<PageHeader title="Tenant Provisioning" subtitle="Compute nodes being provisioned for tenant organizations.">
	{#snippet actions()}
		<Badge tone="neutral">{totalJobs} total</Badge>
	{/snippet}
</PageHeader>

<DataTable
	fetchPage={fetchProvisioningPage}
	rowKey={(j) => j.id}
	searchable={false}
	columns={[
		{ key: 'org', label: 'Organization' },
		{ key: 'node', label: 'Node' },
		{ key: 'provider', label: 'Provider' },
		{ key: 'region', label: 'Region' },
		{ key: 'status', label: 'Status' },
		{ key: 'started', label: 'Started' }
	]}
	emptyMessage="No provisioning jobs in progress."
>
	{#snippet row(j)}
		<tr>
			<td><a class="pv-link" href="/orgs/{j.org_slug}" target="_blank">{j.org_name}</a></td>
			<td class="pv-mono">{j.name}</td>
			<td>{j.provider}</td>
			<td>{j.region}</td>
			<td>
				<span class="pv-status-cell">
					<StatusDot status={statusDot(j.status)} />
					{statusLabel(j.status)}
				</span>
			</td>
			<td class="pv-dim">{relTime(j.created_at)}</td>
		</tr>
	{/snippet}
</DataTable>

<style>
	.pv-link { color: var(--text-primary); text-decoration: none; }
	.pv-link:hover { text-decoration: underline; color: var(--accent); }
	.pv-mono { font-family: var(--mono); }
	.pv-status-cell { display: inline-flex; align-items: center; gap: 6px; font-size: 12.5px; font-weight: 500; color: var(--text-secondary); }
	.pv-dim { color: var(--text-muted); font-size: 11.5px; }
</style>
