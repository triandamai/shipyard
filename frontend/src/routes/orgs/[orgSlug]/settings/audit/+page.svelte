<script lang="ts">
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { ShieldCheck, RefreshCw, ChevronLeft, ChevronRight } from '@lucide/svelte';
	import { MediaQuery } from 'svelte/reactivity';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import type { AuditLogEntry } from '$lib/api/types';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { page } from '$app/state';

	const LIMIT = 50;

	let orgSlug = $derived(page.params.orgSlug ?? '');
	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let myRole    = $derived($orgStore.myMembership?.role ?? null);
	let myPerms   = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let canViewAudit = $derived(
		can(myRole, myPerms, perm(orgId, 'audit', 'read')) ||
		can(myRole, myPerms, perm(orgId, 'settings', 'read'))
	);
	let logs       = $state<AuditLogEntry[]>([]);
	let loading    = $state(true);
	let error      = $state('');
	let nextCursor = $state<string | null>(null);
	// Stack of cursors for previous pages — entry i is the cursor used to load page i+1
	let cursorStack = $state<string[]>([]);

	function formatTime(iso: string) {
		try {
			return new Date(iso).toLocaleString('en-US', {
				month: 'short', day: 'numeric',
				hour: '2-digit', minute: '2-digit', second: '2-digit'
			});
		} catch { return iso; }
	}

	function actionLabel(action: string) { return action.replace(/_/g, ' '); }

	function actionColor(action: string): 'red' | 'green' | 'blue' | 'neutral' {
		if (action.includes('delete') || action.includes('revoke') || action.includes('remove')) return 'red';
		if (action.includes('create') || action.includes('invite') || action.includes('deploy')) return 'green';
		if (action.includes('update') || action.includes('login') || action.includes('rollback')) return 'blue';
		return 'neutral';
	}

	async function loadPage(cursor?: string) {
		if (!orgId) return;
		loading = true;
		error = '';
		const res = await api.getAuditLogs(orgId, cursor, LIMIT);
		if (res.error) { error = res.error.message; loading = false; return; }
		logs       = res.data?.items ?? [];
		nextCursor = res.data?.next_cursor ?? null;
		loading    = false;
	}

	$effect(() => { if (orgId && canViewAudit) { cursorStack = []; loadPage(); } });

	function refresh() { cursorStack = []; loadPage(); }

	function next() {
		if (!nextCursor) return;
		cursorStack = [...cursorStack, nextCursor];
		loadPage(nextCursor);
	}

	function prev() {
		if (cursorStack.length === 0) return;
		const stack = [...cursorStack];
		stack.pop(); // remove the cursor we used for the current page
		const prevCursor = stack[stack.length - 1]; // cursor for the page before current
		cursorStack = stack;
		loadPage(prevCursor);
	}

	// The old page hid the User and IP cells at <=639px; the table now drops those columns entirely.
	const narrow = new MediaQuery('max-width: 639px');
	let columns = $derived(
		narrow.current
			? [
					{ key: 'created_at', label: 'Time' },
					{ key: 'action', label: 'Action' },
					{ key: 'resource_type', label: 'Resource' }
				]
			: [
					{ key: 'created_at', label: 'Time' },
					{ key: 'action', label: 'Action' },
					{ key: 'resource_type', label: 'Resource' },
					{ key: 'user_id', label: 'User' },
					{ key: 'ip_address', label: 'IP' }
				]
	);

	let pageNum = $derived(cursorStack.length + 1);
	let hasPrev = $derived(cursorStack.length > 0);
	let hasNext = $derived(nextCursor !== null);
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canViewAudit}
	message="You need the 'View audit logs' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if canViewAudit}
<div class="audit-page">
	<div class="audit-header">
		<div class="audit-title-row">
			<ShieldCheck size={16} />
			<h2 class="audit-title">Audit Log</h2>
		</div>
		<Button variant="secondary" size="sm" onclick={refresh} disabled={loading}>
			{#if loading}<Spinner size={12} tone="current" />{:else}<RefreshCw size={12} />{/if}Refresh
		</Button>
	</div>

	{#if error}
		<div role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{:else if loading && logs.length === 0}
		<div class="loading-row"><Spinner size={16} />Loading…</div>
	{:else if logs.length === 0}
		<EmptyState message="No audit events recorded yet." />
	{:else}
		<DataTable
			items={logs}
			rowKey={(entry: AuditLogEntry) => entry.id}
			searchable={false}
			pageSize={LIMIT}
			{columns}
		>
			{#snippet row(entry: AuditLogEntry)}
				<tr>
					<td class="col-time font-mono">{formatTime(entry.created_at)}</td>
					<td class="col-action">
						<Badge tone={actionColor(entry.action)}><span class="action-label">{actionLabel(entry.action)}</span></Badge>
					</td>
					<td>
						{#if entry.resource_type}
							<span class="resource-type">{entry.resource_type}</span>
							{#if entry.resource_id}
								<span class="resource-id font-mono">{entry.resource_id.slice(0, 8)}…</span>
							{/if}
						{:else}
							<span class="text-dim">—</span>
						{/if}
					</td>
					{#if !narrow.current}
						<td class="col-user font-mono">{entry.user_id ? entry.user_id.slice(0, 8) + '…' : '—'}</td>
						<td class="col-ip font-mono">{entry.ip_address ?? '—'}</td>
					{/if}
				</tr>
			{/snippet}
		</DataTable>

		<div class="pagination">
			<Button variant="secondary" size="sm" onclick={prev} disabled={!hasPrev || loading}>
				<ChevronLeft size={13} />Prev
			</Button>
			<span class="page-info">Page {pageNum}</span>
			<Button variant="secondary" size="sm" onclick={next} disabled={!hasNext || loading}>
				Next<ChevronRight size={13} />
			</Button>
		</div>
	{/if}
</div>
{/if}

<style>
	.audit-page { display: flex; flex-direction: column; gap: 16px; }

	.audit-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
	}
	.audit-title-row { display: flex; align-items: center; gap: 8px; color: var(--accent); }
	.audit-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0; }

	.loading-row { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 13px; padding: 32px 0; }

	.col-time { color: var(--text-dim); white-space: nowrap; }
	.col-action { white-space: nowrap; }
	.col-user, .col-ip { color: var(--text-dim); }
	.action-label { text-transform: capitalize; }
	.resource-type { color: var(--text-secondary); margin-right: 6px; }
	.resource-id { color: var(--text-dim); font-size: 11px; }
	.text-dim { color: var(--text-dim); }
	.font-mono { font-family: var(--font-mono); }

	.pagination {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 12px;
	}
	.page-info { font-size: 12px; color: var(--text-dim); }

	@media (max-width: 639px) {
		.col-time { white-space: normal; min-width: 70px; }
	}
</style>
