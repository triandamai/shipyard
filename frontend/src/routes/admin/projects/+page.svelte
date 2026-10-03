<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { DataTable, Avatar, Badge, Button, PageHeader } from '$lib/components/ui';

	interface AdminProject {
		id: string;
		name: string;
		slug: string;
		org_id: string;
		org_name: string;
		org_slug: string;
		service_count: number;
		created_at: string;
	}

	let totalProjects = $state(0);

	async function fetchProjectsPage(params: { page: number; pageSize: number; search: string }) {
		const res = await api.get<{ items: AdminProject[]; total: number }>(
			`/admin/projects?page=${params.page}&limit=${params.pageSize}&q=${encodeURIComponent(params.search)}`
		);
		if (!res.data) throw new Error(res.error?.message ?? 'Failed to load projects');
		totalProjects = res.data.total;
		return { rows: res.data.items, total: res.data.total };
	}

	// Same deterministic index pattern as the Orgs/Users pages (Task 40/41):
	// name.charCodeAt(0) % length picks one of Avatar's fixed `tone` values.
	const avaTones: Array<'blue' | 'green' | 'red' | 'yellow' | 'purple'> = ['blue', 'purple', 'green', 'yellow', 'red'];
	function avaTone(name: string): 'blue' | 'green' | 'red' | 'yellow' | 'purple' {
		return avaTones[name.charCodeAt(0) % avaTones.length];
	}
</script>

<PageHeader title="Projects">
	{#snippet actions()}
		<Badge tone="neutral">{totalProjects} total</Badge>
	{/snippet}
</PageHeader>

<DataTable
	fetchPage={fetchProjectsPage}
	rowKey={(p) => p.id}
	columns={[
		{ key: 'project', label: 'Project' },
		{ key: 'org', label: 'Organization' },
		{ key: 'services', label: 'Services' },
		{ key: 'created', label: 'Created' },
		{ key: 'actions', label: '' }
	]}
	emptyMessage="No projects found."
>
	{#snippet row(p)}
		<tr>
			<td>
				<div class="pr-stack">
					<span class="pr-name">{p.name}</span>
					<span class="pr-sub">{p.slug}</span>
				</div>
			</td>
			<td>
				<div class="pr-org">
					<Avatar initials={p.org_name[0]} tone={avaTone(p.org_name)} size={22} />
					<div class="pr-stack">
						<span class="pr-name">{p.org_name}</span>
						<span class="pr-sub">{p.org_slug}</span>
					</div>
				</div>
			</td>
			<td>{p.service_count}</td>
			<td>{new Date(p.created_at).toLocaleDateString()}</td>
			<td><Button variant="ghost" size="sm" onclick={() => goto(`/orgs/${p.org_slug}/projects/${p.slug}`)}>Open</Button></td>
		</tr>
	{/snippet}
</DataTable>

<style>
	.pr-stack { display: flex; flex-direction: column; }
	.pr-name { font-size: 12.5px; font-weight: 600; color: var(--text-primary); }
	.pr-sub { font-size: 10.5px; color: var(--text-dim); }
	.pr-org { display: flex; align-items: center; gap: 8px; }
</style>
