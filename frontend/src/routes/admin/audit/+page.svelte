<script lang="ts">
	import { api } from '$lib/api/client';
	import type { AuditLogEntry } from '$lib/api/types';
	import { ChevronRight } from '@lucide/svelte';
	import { PageHeader, FormField, TextField, DataTable } from '$lib/components/ui';

	const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

	let orgIdFilter = $state('');
	// The backend rejects anything that isn't a full UUID, so the filter only
	// applies once the field is empty or holds a complete one.
	let appliedOrg = $state('');
	let orgInvalid = $derived(orgIdFilter.trim() !== '' && !UUID_RE.test(orgIdFilter.trim()));

	let orgDebounce: ReturnType<typeof setTimeout> | undefined;
	$effect(() => {
		const v = orgIdFilter.trim();
		clearTimeout(orgDebounce);
		if (v !== '' && !UUID_RE.test(v)) return;
		orgDebounce = setTimeout(() => (appliedOrg = v), 300);
		return () => clearTimeout(orgDebounce);
	});

	let expanded = $state(new Set<string>());
	function toggleExpand(id: string) {
		const next = new Set(expanded);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		expanded = next;
	}

	async function fetchAuditPage(params: { page: number; pageSize: number; search: string }) {
		const res = await api.getAdminAuditLogs({
			page: params.page,
			limit: params.pageSize,
			org_id: appliedOrg || undefined,
		});
		if (!res.data) throw new Error(res.error?.message ?? 'Failed to load audit logs');
		return { rows: res.data.items, total: res.data.total ?? res.data.items.length };
	}
</script>

<PageHeader title="Audit Log" subtitle="Organization activity history." />

<FormField label="Filter by organization ID" hint={orgInvalid ? 'Enter a full organization UUID to filter.' : undefined}>
	<TextField bind:value={orgIdFilter} placeholder="org UUID (optional)" />
</FormField>

{#key appliedOrg}
	<DataTable
		fetchPage={fetchAuditPage}
		rowKey={(log: AuditLogEntry) => log.id}
		searchable={false}
		columns={[
			{ key: 'action', label: 'Action', width: '22%' },
			{ key: 'resource_type', label: 'Resource', width: '14%' },
			{ key: 'user_id', label: 'User', width: '30%' },
			{ key: 'ip_address', label: 'IP', width: '14%' },
			{ key: 'created_at', label: 'Time', width: '20%' }
		]}
		emptyMessage="No audit entries found."
	>
		{#snippet row(log: AuditLogEntry)}
			<!-- svelte-ignore a11y_click_events_have_key_events -->
			<!-- svelte-ignore a11y_no_static_element_interactions -->
			<tr class="audit-row" onclick={() => toggleExpand(log.id)}>
				<td class="audit-trunc">
					<span class="audit-expand-cell">
						<ChevronRight size={12} class={expanded.has(log.id) ? 'audit-chevron audit-chevron-expanded' : 'audit-chevron'} />
						{log.action}
					</span>
				</td>
				<td class="audit-trunc">{log.resource_type ?? '—'}</td>
				<td class="audit-mono audit-trunc">{log.user_id ?? 'system'}</td>
				<td class="audit-mono audit-trunc">{log.ip_address ?? '—'}</td>
				<td class="audit-time">{new Date(log.created_at).toLocaleString()}</td>
			</tr>
			{#if expanded.has(log.id)}
				<tr class="audit-detail-row">
					<td colspan="5">
						<pre class="audit-detail">{JSON.stringify(log.metadata, null, 2)}</pre>
					</td>
				</tr>
			{/if}
		{/snippet}
	</DataTable>
{/key}

<style>
	.audit-row { cursor: pointer; }
	.audit-mono { font-family: var(--font-mono); font-size: 11.5px; }
	.audit-trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; max-width: 0; }
	.audit-expand-cell { display: flex; align-items: center; gap: 6px; min-width: 0; }
	:global(.audit-chevron) { color: var(--text-dim); transition: transform 0.2s; flex-shrink: 0; }
	:global(.audit-chevron-expanded) { transform: rotate(90deg); }
	.audit-time { font-variant-numeric: tabular-nums; white-space: nowrap; color: var(--text-dim); font-size: 11.5px; }
	.audit-detail-row td { background: var(--bg-elevated); border-top: 1px solid var(--border); padding: 12px 16px; }
	.audit-detail {
		margin: 0;
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-secondary);
		white-space: pre-wrap;
		max-height: 240px;
		overflow-y: auto;
	}
</style>
