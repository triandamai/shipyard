<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import {
		Rocket, RefreshCw, CheckCircle2, XCircle, Clock, Loader2,
		GitBranch, User, Zap, Save, Check
	} from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import StatusDot from '$lib/components/ui/StatusDot.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import { toDotStatus, type DotStatus } from '$lib/utils/status';
	import api from '$lib/api/client';
	import type { AdminDeploymentsResponse, AdminDeploymentRow } from '$lib/api/types';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let canDeploymentsRead  = $derived(
		can(myRole, myPerms, perm(orgId, 'deployments', 'read')) ||
		can(myRole, myPerms, perm(orgId, 'settings', 'read'))
	);
	let canDeploymentsWrite = $derived(can(myRole, myPerms, perm(orgId, 'deployments', 'write')));
	let canDeploymentsAny   = $derived(canDeploymentsRead || canDeploymentsWrite);

	// ─── State ────────────────────────────────────────────────────────────────
	let response   = $state<AdminDeploymentsResponse | null>(null);
	let refreshing = $state(false);
	// Bumped to make the table re-fetch its current page (manual + auto refresh).
	let refreshTick = $state(0);

	// Filters
	let statusFilter = $state('');
	const PER_PAGE   = 50;

	// Parallelism setting
	let maxParallel     = $state<number | undefined>(undefined);
	let parallelInput   = $state('');
	let savingParallel  = $state(false);
	let savedParallel   = $state(false);
	let parallelError   = $state('');

	let orgSlug = $derived($page.params.orgSlug);

	// ─── Load deployments ─────────────────────────────────────────────────────
	// DataTable (server mode) calls this for the current page / page size.
	async function fetchRows(params: { page: number; pageSize: number; search: string }) {
		refreshing = true;
		try {
			const res = await api.listAllDeployments(orgId, {
				status:   statusFilter || undefined,
				page:     params.page + 1,
				per_page: params.pageSize,
			});
			if (res.data) {
				response = res.data;
				return { rows: res.data.data, total: res.data.total };
			}
			throw new Error(res.error?.message ?? 'Failed to load deployments');
		} finally {
			refreshing = false;
		}
	}

	// A new function identity makes DataTable re-fetch the page it is on.
	let tableFetch = $derived.by(() => {
		refreshTick;
		return (p: { page: number; pageSize: number; search: string }) => fetchRows(p);
	});

	function load(_silent = false) {
		refreshTick++;
	}

	// ─── Load parallelism setting ──────────────────────────────────────────────
	async function loadParallelism() {
		if (!orgId) return;
		const res = await api.get<{ max_parallel_deployments?: number }>(`/settings/deployments?org_id=${orgId}`);
		if (res.data) {
			maxParallel = res.data.max_parallel_deployments ?? 0;
			parallelInput = String(maxParallel);
		}
	}

	async function saveParallelism() {
		if (!canDeploymentsWrite || !orgId) return;
		savingParallel = true;
		parallelError = '';
		try {
			const res = await api.put(`/settings/deployments?org_id=${orgId}`, { max_parallel_deployments: parallelInput === '' ? 0 : Number(parallelInput) });
			if (res.error) parallelError = res.error.message;
			else { savedParallel = true; setTimeout(() => (savedParallel = false), 3000); }
		} finally {
			savingParallel = false;
		}
	}

	let lastLoadedOrgId = $state<string | null>(null);

	$effect(() => {
		if (canDeploymentsAny && orgId && orgId !== lastLoadedOrgId) {
			lastLoadedOrgId = orgId;
			void loadParallelism();
		}
	});

	// Auto-refresh every 10s when there are running/queued deployments
	let interval: ReturnType<typeof setInterval> | null = null;
	$effect(() => {
		const hasActive = (response?.stats.running ?? 0) + (response?.stats.queued ?? 0) > 0;
		if (hasActive && !interval) {
			interval = setInterval(() => load(true), 10000);
		} else if (!hasActive && interval) {
			clearInterval(interval);
			interval = null;
		}
	});
	onDestroy(() => { if (interval) clearInterval(interval); });

	// ─── Filter / page changes ─────────────────────────────────────────────────
	function applyFilter(status: string) {
		statusFilter = status;
	}

	// ─── Helpers ──────────────────────────────────────────────────────────────
	function duration(row: AdminDeploymentRow): string {
		const end = row.finished_at ? new Date(row.finished_at) : new Date();
		const ms = end.getTime() - new Date(row.created_at).getTime();
		const s = Math.floor(ms / 1000);
		if (s < 60) return `${s}s`;
		const m = Math.floor(s / 60);
		return `${m}m ${s % 60}s`;
	}

	function relativeTime(iso: string): string {
		const diff = Date.now() - new Date(iso).getTime();
		const m = Math.floor(diff / 60000);
		if (m < 1) return 'just now';
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		return `${Math.floor(h / 24)}d ago`;
	}

	function navToDeployment(row: AdminDeploymentRow) {
		goto(`/orgs/${orgSlug}/settings/deployments/${row.id}`);
	}

	// Deployment statuses are not the same set as service statuses: keep the
	// pre-migration colours (success green, running blue + pulsing).
	function deployDot(status: string): DotStatus {
		if (status === 'success') return 'running';
		if (status === 'running') return 'deploying';
		return toDotStatus(status);
	}
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canDeploymentsAny}
	message="You need the 'View deployments' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if canDeploymentsAny}
<div class="page">
	<!-- ── Header ── -->
	<div class="page-header">
		<div class="header-text">
			<h2>Deployments</h2>
			<p>All deployment activity across every project and service.</p>
		</div>
		<Button variant="secondary" size="icon" onclick={() => load(true)} disabled={refreshing} aria-label="Refresh">
			{#if refreshing}<Spinner size={15} tone="current" />{:else}<RefreshCw size={15} />{/if}
		</Button>
	</div>

	<!-- ── Parallelism setting ── -->
	<Card>
		<div class="parallelism-card">
			<div class="parallelism-info">
				<Zap size={15} />
				<div>
					<span class="parallelism-label">Max parallel deployments</span>
					{#if maxParallel === -1}
						<span class="parallelism-hint plan-locked">Fixed to <strong>1</strong> by your plan — upgrade to change this limit.</span>
					{:else}
						<span class="parallelism-hint">Deployments beyond this limit are queued. Default is <code>2</code>. Set to <code>0</code> for unlimited.</span>
					{/if}
				</div>
			</div>
			{#if maxParallel !== -1}
			<div class="parallelism-controls">
				<div class="parallel-input">
					<TextField
						type="number"
						min="0"
						max="20"
						bind:value={parallelInput}
						placeholder="2 (default)"
						aria-label="Max parallel deployments"
					/>
				</div>
				<Button onclick={saveParallelism} disabled={savingParallel}>
					{#if savedParallel}<Check size={13} /> Saved{:else if savingParallel}<Spinner size={13} tone="current" /> Saving…{:else}<Save size={13} /> Save{/if}
				</Button>
			</div>
			{:else}
			<Badge tone="yellow">Plan Locked</Badge>
			{/if}
			{#if parallelError}<div class="inline-error" role="alert"><InlineAlert tone="error">{parallelError}</InlineAlert></div>{/if}
		</div>
	</Card>

	<!-- ── Stats bar ── -->
	{#if response}
		<div class="stats-bar">
			<Button variant={statusFilter === '' ? 'secondary' : 'ghost'} size="sm" onclick={() => applyFilter('')}>
				<Rocket size={13} />
				<span class="stat-num">{response.stats.total}</span>
				<span>Total</span>
			</Button>
			<Button variant={statusFilter === 'running' ? 'secondary' : 'ghost'} size="sm" onclick={() => applyFilter('running')}>
				{#if response.stats.running > 0}<Spinner size={13} tone="current" />{:else}<Loader2 size={13} />{/if}
				<span class="stat-num">{response.stats.running}</span>
				<span>Running</span>
			</Button>
			<Button variant={statusFilter === 'queued' ? 'secondary' : 'ghost'} size="sm" onclick={() => applyFilter('queued')}>
				<Clock size={13} />
				<span class="stat-num">{response.stats.queued}</span>
				<span>Queued</span>
			</Button>
			<Button variant={statusFilter === 'success' ? 'secondary' : 'ghost'} size="sm" onclick={() => applyFilter('success')}>
				<CheckCircle2 size={13} />
				<span class="stat-num">{response.stats.success}</span>
				<span>Success</span>
			</Button>
			<Button variant={statusFilter === 'failed' ? 'secondary' : 'ghost'} size="sm" onclick={() => applyFilter('failed')}>
				<XCircle size={13} />
				<span class="stat-num">{response.stats.failed}</span>
				<span>Failed</span>
			</Button>
		</div>
	{/if}

	<!-- ── Content ── -->
	{#if orgId}
		{#key `${orgId}|${statusFilter}`}
			<DataTable
				fetchPage={tableFetch}
				rowKey={(r: AdminDeploymentRow) => r.id}
				pageSize={PER_PAGE}
				searchable={false}
				emptyMessage={statusFilter ? `No ${statusFilter} deployments.` : 'No deployments yet.'}
				columns={[
					{ key: 'status', label: 'Status' },
					{ key: 'service', label: 'Project / Service' },
					{ key: 'ref', label: 'Ref' },
					{ key: 'by', label: 'Triggered by' },
					{ key: 'duration', label: 'Duration' },
					{ key: 'started', label: 'Started' }
				]}
			>
				{#snippet row(row: AdminDeploymentRow)}
					<tr class="row-link" onclick={() => navToDeployment(row)} role="button" tabindex="0"
						onkeydown={(e) => e.key === 'Enter' && navToDeployment(row)}>
						<td><span class="status-cell"><StatusDot status={deployDot(row.status)} /><span class="status-text">{row.status}</span></span></td>
						<td>
							<div class="service-cell">
								<span class="project-name">{row.project_name}</span>
								<span class="service-name">{row.service_name}</span>
							</div>
						</td>
						<td><span class="ref-badge"><Badge tone="neutral"><GitBranch size={11} />{row.source_ref}</Badge></span></td>
						<td><span class="triggered"><User size={11} />{row.triggered_by}</span></td>
						<td class="muted">{duration(row)}</td>
						<td class="muted">{relativeTime(row.created_at)}</td>
					</tr>
				{/snippet}
			</DataTable>
		{/key}

		<!-- Phone card list: same page of data the table shows -->
		<div class="mobile-cards">
			{#each response?.data ?? [] as row (row.id)}
				<div class="dep-card" onclick={() => navToDeployment(row)} role="button" tabindex="0"
					onkeydown={(e) => e.key === 'Enter' && navToDeployment(row)}>
					<Card padding="12px 14px">
						<div class="card-inner">
							<div class="card-header">
								<div class="card-title">
									<StatusDot status={deployDot(row.status)} />
									<span class="service-name">{row.service_name}</span>
								</div>
								<span class="muted">{relativeTime(row.created_at)}</span>
							</div>
							<span class="project-name">{row.project_name}</span>
							<div class="card-chips">
								<span class="ref-badge"><Badge tone="neutral"><GitBranch size={11} />{row.source_ref}</Badge></span>
								<span class="triggered"><User size={11} />{row.triggered_by}</span>
								<span class="muted">{duration(row)}</span>
							</div>
						</div>
					</Card>
				</div>
			{/each}
		</div>
	{/if}
</div>
{/if}

<style>
	.page { display: flex; flex-direction: column; gap: 16px; }

	/* ── Header ── */
	.page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
	}
	.header-text h2 { font-size: 16px; font-weight: 600; color: var(--text-primary); margin: 0 0 4px; }
	.header-text p  { font-size: 13px; color: var(--text-muted); margin: 0; }

	/* ── Parallelism card ── */
	.parallelism-card {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
		flex-wrap: wrap;
	}
	.parallelism-info { display: flex; align-items: flex-start; gap: 10px; color: var(--text-muted); }
	.parallelism-label { display: block; font-size: 13px; font-weight: 500; color: var(--text-primary); }
	.parallelism-hint  { display: block; font-size: 12px; color: var(--text-muted); margin-top: 2px; }
	.parallelism-hint.plan-locked { color: var(--accent-yellow); }
	.parallelism-controls { display: flex; align-items: center; gap: 8px; }
	.parallel-input { width: 120px; }
	.inline-error { width: 100%; }

	/* ── Stats bar ── */
	.stats-bar { display: flex; gap: 8px; flex-wrap: wrap; }
	.stat-num { font-weight: 600; font-size: 13px; color: inherit; }

	/* ── Table ── */
	:global(tr.row-link) { cursor: pointer; }
	.muted { color: var(--text-muted); font-size: 12px; }
	.status-cell { display: inline-flex; align-items: center; gap: 7px; }
	.status-text { text-transform: capitalize; }
	.service-cell { display: flex; flex-direction: column; gap: 2px; }
	.project-name { font-size: 11px; color: var(--text-muted); }
	.service-name { font-weight: 500; color: var(--text-primary); }
	.ref-badge { font-family: var(--font-mono); }
	.ref-badge :global(.ui-badge) { gap: 4px; max-width: 160px; overflow: hidden; text-overflow: ellipsis; }
	.triggered { display: inline-flex; align-items: center; gap: 4px; font-size: 12px; color: var(--text-muted); }

	/* ── Mobile cards ── */
	.mobile-cards { display: none; flex-direction: column; gap: 8px; }
	.dep-card { cursor: pointer; }
	.card-inner { display: flex; flex-direction: column; gap: 8px; }
	.card-header { display: flex; align-items: center; justify-content: space-between; }
	.card-title { display: flex; align-items: center; gap: 8px; }
	.card-chips { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

	@media (max-width: 639px) {
		.mobile-cards { display: flex; }
		.page :global(.ui-data-table-scroll) { display: none; }
		.parallelism-card { flex-direction: column; align-items: flex-start; }
		.parallelism-controls { width: 100%; }
		.parallel-input { flex: 1; }
	}
</style>
