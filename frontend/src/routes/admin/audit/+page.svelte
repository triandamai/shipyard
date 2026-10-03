<script lang="ts">
	import { api } from '$lib/api/client';
	import type { AuditLogEntry } from '$lib/api/types';
	import { Activity, ChevronDown } from '@lucide/svelte';
	import { PageHeader, FormField, TextField, ActivityList, ListRow, Button, InlineAlert, EmptyState, Spinner } from '$lib/components/ui';

	let logs        = $state<AuditLogEntry[]>([]);
	let nextCursor  = $state<string | null>(null);
	let loading     = $state(true);
	let loadingMore = $state(false);
	let error       = $state('');
	let orgIdFilter = $state('');

	let hasMore = $derived(nextCursor !== null);

	let expanded = $state(new Set<string>());
	function toggleExpand(id: string) {
		if (expanded.has(id)) expanded.delete(id);
		else expanded.add(id);
		expanded = new Set(expanded);
	}

	// Monotonic guard so an out-of-order response from an earlier request
	// (e.g. a stale keystroke's fetch resolving after a newer one) can never
	// clobber the result of a request started later.
	let requestSeq = 0;

	async function load(cursor?: string) {
		const seq = ++requestSeq;
		if (cursor) loadingMore = true;
		else { loading = true; logs = []; nextCursor = null; }
		error = '';
		const res = await api.getAdminAuditLogs({
			cursor,
			limit: 50,
			org_id: orgIdFilter.trim() || undefined,
		});
		if (seq !== requestSeq) return; // superseded by a newer request; discard
		if (res.data) {
			logs = cursor ? [...logs, ...res.data.items] : res.data.items;
			nextCursor = res.data.next_cursor;
		} else {
			error = res.error?.message ?? 'Failed to load audit logs';
		}
		loading = false;
		loadingMore = false;
	}

	function loadMore() {
		if (nextCursor) load(nextCursor);
	}

	// Refetch from scratch whenever the org-ID filter changes (also fires the
	// initial load). Replaces the old page's explicit "Apply" button now that
	// the new template only exposes a bound TextField. Debounced so typing a
	// UUID doesn't fire a request per keystroke.
	let orgFilterDebounce: ReturnType<typeof setTimeout> | undefined;
	let firstLoad = true;
	$effect(() => {
		orgIdFilter;
		if (firstLoad) {
			firstLoad = false;
			load();
			return;
		}
		clearTimeout(orgFilterDebounce);
		orgFilterDebounce = setTimeout(() => load(), 300);
		return () => clearTimeout(orgFilterDebounce);
	});
</script>

<PageHeader title="Audit Log" subtitle="Organization activity history." />

<FormField label="Filter by organization ID">
	<TextField bind:value={orgIdFilter} placeholder="org UUID (optional)" />
</FormField>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else if loading}
	<div class="audit-loading"><Spinner size={18} /></div>
{:else if logs.length === 0}
	<EmptyState message="No audit entries found." />
{:else}
	<ActivityList>
		{#each logs as log (log.id)}
			<div class="audit-row-wrap">
				<button type="button" class="audit-row-toggle" onclick={() => toggleExpand(log.id)}>
					<ListRow
						iconTone="blue"
						title={log.action}
						meta="{log.resource_type ?? ''} · {log.user_id ?? 'system'} · {log.ip_address ?? ''}"
					>
						{#snippet icon()}<Activity size={13} />{/snippet}
						{#snippet trailing()}<span class="audit-time">{new Date(log.created_at).toLocaleString()}</span>{/snippet}
					</ListRow>
				</button>
				{#if expanded.has(log.id)}
					<pre class="audit-detail">{JSON.stringify(log.metadata, null, 2)}</pre>
				{/if}
			</div>
		{/each}
	</ActivityList>

	{#if hasMore}
		<div class="audit-load-more">
			<Button variant="secondary" onclick={loadMore} disabled={loadingMore}>
				<ChevronDown size={13} />
				{loadingMore ? 'Loading…' : 'Load more'}
			</Button>
		</div>
	{/if}
{/if}

<style>
	.audit-row-toggle { display: block; width: 100%; background: none; border: none; padding: 0; cursor: pointer; text-align: left; }
	.audit-time { font-size: 10.5px; color: var(--text-dim); font-variant-numeric: tabular-nums; white-space: nowrap; }
	.audit-detail {
		margin: 0 0 8px 42px;
		padding: 10px 12px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-secondary);
		overflow-x: auto;
		max-height: 240px;
	}
	.audit-load-more { display: flex; justify-content: center; margin-top: 16px; }
	.audit-loading { display: flex; justify-content: center; padding: 40px 0; }
</style>
