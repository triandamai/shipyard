<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AdminUser } from '$lib/api/types';
	import { DataTable, Avatar, Badge, Button, PageHeader, InlineAlert } from '$lib/components/ui';

	let users = $state<AdminUser[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let patchingId = $state<string | null>(null);

	onMount(() => load());

	async function load() {
		loading = true;
		const res = await api.getAdminUsers();
		if (res.data) users = res.data;
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	}

	async function toggleAdmin(user: AdminUser) {
		patchingId = user.id;
		await api.patchAdminUser(user.id, { is_superadmin: !user.is_superadmin });
		await load();
		patchingId = null;
	}

	async function toggleSuspend(user: AdminUser) {
		patchingId = user.id;
		await api.patchAdminUser(user.id, { is_suspended: !user.is_suspended });
		await load();
		patchingId = null;
	}

	let adminCount = $derived(users.filter(u => u.is_superadmin).length);

	// Same deterministic index (email.charCodeAt(0) % length) the page always used
	// for its avatar styling — only the output array changed, from raw CSS color
	// pairs to the design-system Avatar `tone` names, since Avatar takes a tone
	// enum rather than arbitrary colors.
	const avaTones: Array<'blue' | 'green' | 'red' | 'yellow' | 'purple'> = ['blue', 'purple', 'green', 'yellow', 'red'];
	function avaTone(email: string): 'blue' | 'green' | 'red' | 'yellow' | 'purple' {
		return avaTones[email.charCodeAt(0) % avaTones.length];
	}
</script>

<PageHeader title="Users">
	{#snippet actions()}
		<Badge tone="neutral">{users.length} total</Badge>
		{#if adminCount > 0}
			<Badge tone="red">{adminCount} admin{adminCount !== 1 ? 's' : ''}</Badge>
		{/if}
	{/snippet}
</PageHeader>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
<DataTable
	items={users}
	rowKey={(u) => u.id}
	searchFields={['email']}
	columns={[
		{ key: 'user', label: 'User', width: '32%' },
		{ key: 'role', label: 'Role', width: '14%' },
		{ key: 'orgs', label: 'Orgs', width: '10%' },
		{ key: 'joined', label: 'Joined', width: '16%' },
		{ key: 'actions', label: 'Actions', width: '28%' }
	]}
	emptyMessage="No users found."
>
	{#snippet row(user)}
		<tr>
			<td>
				<div class="us-user-cell">
					<Avatar initials={user.email[0]} tone={avaTone(user.email)} size={32} />
					<div class="us-user-info">
						<span class="us-email">{user.email}</span>
						<span class="us-id">{user.id.slice(0, 8)}…</span>
					</div>
				</div>
			</td>
			<td>
				{#if user.is_superadmin}
					<Badge tone="blue">Superadmin</Badge>
				{:else}
					<Badge tone="neutral">User</Badge>
				{/if}
			</td>
			<td class="us-num">{user.org_count}</td>
			<td class="us-date">{new Date(user.created_at).toLocaleDateString()}</td>
			<td>
				<div class="us-actions">
					<Button
						variant="secondary"
						size="sm"
						disabled={patchingId === user.id}
						onclick={() => toggleSuspend(user)}
					>
						{patchingId === user.id ? '…' : user.is_suspended ? 'Unsuspend' : 'Suspend'}
					</Button>
					{#if user.is_superadmin}
						<Button
							variant="danger-outline"
							size="sm"
							disabled={patchingId === user.id}
							onclick={() => toggleAdmin(user)}
						>
							{patchingId === user.id ? '…' : 'Revoke admin'}
						</Button>
					{/if}
				</div>
			</td>
		</tr>
	{/snippet}
</DataTable>
{/if}

<style>
	.us-user-cell { display: flex; align-items: center; gap: 9px; min-width: 0; }
	.us-user-info { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
	.us-email { font-size: 12.5px; font-weight: 500; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
	.us-id { font-size: 10px; color: var(--text-dim); font-family: var(--font-mono); }
	.us-num { font-variant-numeric: tabular-nums; color: var(--text-secondary); }
	.us-date { font-size: 11.5px; color: var(--text-dim); white-space: nowrap; }
	.us-actions { display: flex; justify-content: flex-end; gap: 6px; }
</style>
