<script lang="ts">
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { ShieldCheck, RefreshCw, ChevronLeft, ChevronRight } from '@lucide/svelte';
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
	let canViewAudit = $derived(can(myRole, myPerms, perm(orgId, 'audit', 'read')));
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

	function actionColor(action: string): string {
		if (action.includes('delete') || action.includes('revoke') || action.includes('remove')) return 'danger';
		if (action.includes('create') || action.includes('invite') || action.includes('deploy')) return 'success';
		if (action.includes('update') || action.includes('login') || action.includes('rollback')) return 'info';
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
		<button class="refresh-btn" onclick={refresh} disabled={loading}>
			<RefreshCw size={12} class={loading ? 'spin' : ''} />Refresh
		</button>
	</div>

	{#if error}
		<div class="error-msg">{error}</div>
	{:else if loading && logs.length === 0}
		<div class="loading-row"><div class="spinner"></div>Loading…</div>
	{:else if logs.length === 0}
		<div class="empty">No audit events recorded yet.</div>
	{:else}
		<div class="table-wrap">
			<table class="audit-table">
				<thead>
					<tr>
						<th>Time</th>
						<th>Action</th>
						<th>Resource</th>
						<th>User</th>
						<th>IP</th>
					</tr>
				</thead>
				<tbody>
					{#each logs as entry (entry.id)}
						<tr class="audit-row">
							<td class="col-time font-mono">{formatTime(entry.created_at)}</td>
							<td class="col-action">
								<span class="action-badge {actionColor(entry.action)}">{actionLabel(entry.action)}</span>
							</td>
							<td class="col-resource">
								{#if entry.resource_type}
									<span class="resource-type">{entry.resource_type}</span>
									{#if entry.resource_id}
										<span class="resource-id font-mono">{entry.resource_id.slice(0, 8)}…</span>
									{/if}
								{:else}
									<span class="text-dim">—</span>
								{/if}
							</td>
							<td class="col-user font-mono">
								{entry.user_id ? entry.user_id.slice(0, 8) + '…' : '—'}
							</td>
							<td class="col-ip font-mono">{entry.ip_address ?? '—'}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<div class="pagination">
			<button class="page-btn" onclick={prev} disabled={!hasPrev || loading}>
				<ChevronLeft size={13} />Prev
			</button>
			<span class="page-info">Page {pageNum}</span>
			<button class="page-btn" onclick={next} disabled={!hasNext || loading}>
				Next<ChevronRight size={13} />
			</button>
		</div>
	{/if}
</div>
{/if}

<style>
	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

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

	.refresh-btn {
		display: inline-flex; align-items: center; gap: 5px;
		background: transparent; border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-muted);
		font-size: 12px; font-family: var(--font-sans); padding: 5px 11px;
		cursor: pointer; transition: all var(--transition-fast);
	}
	.refresh-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.refresh-btn:disabled { opacity: 0.5; cursor: default; }

	.loading-row { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 13px; padding: 32px 0; }
	.spinner { width: 16px; height: 16px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }
	.empty { color: var(--text-dim); font-size: 13px; padding: 32px 0; text-align: center; }
	.error-msg { color: #EF4444; font-size: 13px; padding: 12px 16px; background: rgba(239,68,68,0.08); border-radius: var(--radius-md); border: 1px solid rgba(239,68,68,0.2); }

	.table-wrap {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
		overflow-x: auto;
	}

	.audit-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
	}
	.audit-table th {
		text-align: left;
		padding: 10px 14px;
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-dim);
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		white-space: nowrap;
	}
	.audit-row { border-bottom: 1px solid var(--border); transition: background var(--transition-fast); }
	.audit-row:last-child { border-bottom: none; }
	.audit-row:hover { background: var(--bg-elevated); }
	.audit-row td { padding: 10px 14px; color: var(--text-secondary); vertical-align: middle; }

	.col-time { color: var(--text-dim); white-space: nowrap; }
	.col-action { white-space: nowrap; }
	.col-resource { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
	.col-user { color: var(--text-dim); }
	.col-ip { color: var(--text-dim); }

	.action-badge {
		display: inline-block;
		padding: 2px 8px;
		border-radius: 99px;
		font-size: 11px;
		font-weight: 500;
		text-transform: capitalize;
	}
	.action-badge.success { background: rgba(16,185,129,0.12); color: #10B981; }
	.action-badge.danger  { background: rgba(239,68,68,0.10);  color: #EF4444; }
	.action-badge.info    { background: rgba(59,130,246,0.12); color: #3B82F6; }
	.action-badge.neutral { background: var(--bg-elevated); color: var(--text-muted); border: 1px solid var(--border); }

	.resource-type { color: var(--text-secondary); }
	.resource-id { color: var(--text-dim); font-size: 11px; }
	.text-dim { color: var(--text-dim); }

	.font-mono { font-family: var(--font-mono); }

	.pagination {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 12px;
	}
	.page-btn {
		display: inline-flex; align-items: center; gap: 4px;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-muted);
		font-size: 12px; font-family: var(--font-sans); padding: 5px 11px;
		cursor: pointer; transition: all var(--transition-fast);
	}
	.page-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.page-btn:disabled { opacity: 0.4; cursor: default; }
	.page-info { font-size: 12px; color: var(--text-dim); }

	@media (max-width: 639px) {
		.col-ip, .col-user { display: none; }
	}
</style>
