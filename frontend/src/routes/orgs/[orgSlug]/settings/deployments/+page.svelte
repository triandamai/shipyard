<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import {
		Rocket, RefreshCw, CheckCircle2, XCircle, Clock, Loader2,
		GitBranch, User, Zap, Save, Check, ChevronLeft, ChevronRight
	} from '@lucide/svelte';
	import api from '$lib/api/client';
	import type { AdminDeploymentsResponse, AdminDeploymentRow, AdminDeploymentStats } from '$lib/api/types';
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
	let loading    = $state(true);
	let error      = $state('');
	let refreshing = $state(false);

	// Filters
	let statusFilter = $state('');
	let currentPage  = $state(1);
	const PER_PAGE   = 50;

	// Parallelism setting
	let maxParallel     = $state<number | undefined>(undefined);
	let savingParallel  = $state(false);
	let savedParallel   = $state(false);
	let parallelError   = $state('');

	let orgSlug = $derived($page.params.orgSlug);

	// ─── Load deployments ─────────────────────────────────────────────────────
	async function load(silent = false) {
		if (!silent) loading = true;
		else refreshing = true;
		error = '';
		try {
			const res = await api.listAllDeployments(orgId, {
				status:   statusFilter || undefined,
				page:     currentPage,
				per_page: PER_PAGE,
			});
			if (res.data) response = res.data;
			else error = res.error?.message ?? 'Failed to load deployments';
		} catch {
			error = 'Failed to load deployments';
		} finally {
			loading = false;
			refreshing = false;
		}
	}

	// ─── Load parallelism setting ──────────────────────────────────────────────
	async function loadParallelism() {
		if (!orgId) return;
		const res = await api.get<{ max_parallel_deployments?: number }>(`/settings/deployments?org_id=${orgId}`);
		if (res.data) maxParallel = res.data.max_parallel_deployments ?? 0;
	}

	async function saveParallelism() {
		if (!canDeploymentsWrite || !orgId) return;
		savingParallel = true;
		parallelError = '';
		try {
			const res = await api.put(`/settings/deployments?org_id=${orgId}`, { max_parallel_deployments: maxParallel ?? 0 });
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
			currentPage = 1;
			void load();
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
		currentPage = 1;
		load();
	}

	function goPage(p: number) {
		currentPage = p;
		load();
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

	const STATUS_COLORS: Record<string, string> = {
		success:  'status-success',
		failed:   'status-failed',
		running:  'status-running',
		queued:   'status-queued',
		pending:  'status-queued',
		cancelled:'status-cancelled',
	};

	let totalPages = $derived(Math.ceil((response?.total ?? 0) / PER_PAGE));
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
		<button class="icon-btn" onclick={() => load(true)} disabled={refreshing} aria-label="Refresh">
			<RefreshCw size={15} class={refreshing ? 'spin' : ''} />
		</button>
	</div>

	<!-- ── Parallelism setting ── -->
	<div class="parallelism-card" class:parallelism-locked={maxParallel === -1}>
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
			<input
				type="number"
				min="0"
				max="20"
				bind:value={maxParallel}
				placeholder="2 (default)"
				class="parallel-input"
			/>
			<button class="btn-save" onclick={saveParallelism} disabled={savingParallel}>
				{#if savedParallel}<Check size={13} /> Saved{:else if savingParallel}<Loader2 size={13} class="spin" /> Saving…{:else}<Save size={13} /> Save{/if}
			</button>
		</div>
		{:else}
		<span class="locked-badge">Plan Locked</span>
		{/if}
		{#if parallelError}<p class="inline-error">{parallelError}</p>{/if}
	</div>

	<!-- ── Stats bar ── -->
	{#if response}
		<div class="stats-bar">
			<button class="stat-chip" class:active={statusFilter === ''} onclick={() => applyFilter('')}>
				<Rocket size={13} />
				<span class="stat-num">{response.stats.total}</span>
				<span>Total</span>
			</button>
			<button class="stat-chip running" class:active={statusFilter === 'running'} onclick={() => applyFilter('running')}>
				<Loader2 size={13} class={(response.stats.running > 0) ? 'spin' : ''} />
				<span class="stat-num">{response.stats.running}</span>
				<span>Running</span>
			</button>
			<button class="stat-chip queued" class:active={statusFilter === 'queued'} onclick={() => applyFilter('queued')}>
				<Clock size={13} />
				<span class="stat-num">{response.stats.queued}</span>
				<span>Queued</span>
			</button>
			<button class="stat-chip success" class:active={statusFilter === 'success'} onclick={() => applyFilter('success')}>
				<CheckCircle2 size={13} />
				<span class="stat-num">{response.stats.success}</span>
				<span>Success</span>
			</button>
			<button class="stat-chip failed" class:active={statusFilter === 'failed'} onclick={() => applyFilter('failed')}>
				<XCircle size={13} />
				<span class="stat-num">{response.stats.failed}</span>
				<span>Failed</span>
			</button>
		</div>
	{/if}

	<!-- ── Content ── -->
	{#if loading}
		<div class="empty-state"><Loader2 size={24} class="spin" /><span>Loading…</span></div>
	{:else if error}
		<div class="error-state">{error}</div>
	{:else if !response || response.data.length === 0}
		<div class="empty-state">
			<Rocket size={32} />
			<p>{statusFilter ? `No ${statusFilter} deployments.` : 'No deployments yet.'}</p>
		</div>
	{:else}
		<!-- Desktop table -->
		<div class="table-wrap">
			<table>
				<thead>
					<tr>
						<th>Status</th>
						<th>Project / Service</th>
						<th>Ref</th>
						<th>Triggered by</th>
						<th>Duration</th>
						<th>Started</th>
					</tr>
				</thead>
				<tbody>
					{#each response.data as row (row.id)}
						<tr class="row-link" onclick={() => navToDeployment(row)} role="button" tabindex="0"
							onkeydown={(e) => e.key === 'Enter' && navToDeployment(row)}>
							<td><span class="status-dot {STATUS_COLORS[row.status] ?? ''}"></span><span class="status-text">{row.status}</span></td>
							<td>
								<div class="service-cell">
									<span class="project-name">{row.project_name}</span>
									<span class="service-name">{row.service_name}</span>
								</div>
							</td>
							<td><span class="ref-badge"><GitBranch size={11} />{row.source_ref}</span></td>
							<td><span class="triggered"><User size={11} />{row.triggered_by}</span></td>
							<td class="muted">{duration(row)}</td>
							<td class="muted">{relativeTime(row.created_at)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<!-- Mobile cards -->
		<div class="mobile-cards">
			{#each response.data as row (row.id)}
				<div class="card" onclick={() => navToDeployment(row)} role="button" tabindex="0"
					onkeydown={(e) => e.key === 'Enter' && navToDeployment(row)}>
					<div class="card-header">
						<div class="card-title">
							<span class="status-dot {STATUS_COLORS[row.status] ?? ''}"></span>
							<span class="service-name">{row.service_name}</span>
						</div>
						<span class="muted">{relativeTime(row.created_at)}</span>
					</div>
					<span class="project-name">{row.project_name}</span>
					<div class="card-chips">
						<span class="ref-badge"><GitBranch size={11} />{row.source_ref}</span>
						<span class="triggered"><User size={11} />{row.triggered_by}</span>
						<span class="muted">{duration(row)}</span>
					</div>
				</div>
			{/each}
		</div>

		<!-- Pagination -->
		{#if totalPages > 1}
			<div class="pagination">
				<button class="page-btn" onclick={() => goPage(currentPage - 1)} disabled={currentPage <= 1}>
					<ChevronLeft size={14} />
				</button>
				<span class="page-info">Page {currentPage} of {totalPages}</span>
				<button class="page-btn" onclick={() => goPage(currentPage + 1)} disabled={currentPage >= totalPages}>
					<ChevronRight size={14} />
				</button>
			</div>
		{/if}
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
		padding: 14px 16px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		flex-wrap: wrap;
	}
	.parallelism-info {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		color: var(--text-muted);
	}
	.parallelism-label { display: block; font-size: 13px; font-weight: 500; color: var(--text-primary); }
	.parallelism-hint  { display: block; font-size: 12px; color: var(--text-muted); margin-top: 2px; }
	.parallelism-hint.plan-locked { color: var(--warn, #b45309); }
	.parallelism-locked { background: var(--bg-elevated); border-color: rgba(180,83,9,0.2); }
	.locked-badge { display:inline-flex; align-items:center; padding:3px 10px; border-radius:999px; font-size:11px; font-weight:600; background:rgba(180,83,9,0.08); color:#b45309; border:1px solid rgba(180,83,9,0.2); white-space:nowrap; }
	.parallelism-controls { display: flex; align-items: center; gap: 8px; }
	.parallel-input {
		width: 100px;
		padding: 6px 10px;
		background: var(--bg-muted);
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 13px;
		color: var(--text-primary);
		outline: none;
	}
	.parallel-input:focus { border-color: var(--accent); }
	.btn-save {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 6px 14px;
		background: var(--accent);
		color: #fff;
		border: none;
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		white-space: nowrap;
	}
	.btn-save:disabled { opacity: 0.6; cursor: not-allowed; }
	.inline-error { font-size: 12px; color: #ef4444; margin: 4px 0 0; width: 100%; }

	/* ── Stats bar ── */
	.stats-bar {
		display: flex;
		gap: 8px;
		flex-wrap: wrap;
	}
	.stat-chip {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 12px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 20px;
		font-size: 12px;
		color: var(--text-muted);
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
	}
	.stat-chip:hover,
	.stat-chip.active { border-color: var(--accent); color: var(--accent); background: rgba(var(--accent-rgb,99,102,241),.06); }
	.stat-chip.running.active { border-color: #2563eb; color: #2563eb; background: rgba(37,99,235,.08); }
	.stat-chip.queued.active  { border-color: #d97706; color: #d97706; background: rgba(217,119,6,.08); }
	.stat-chip.success.active { border-color: #16a34a; color: #16a34a; background: rgba(22,163,74,.08); }
	.stat-chip.failed.active  { border-color: #dc2626; color: #dc2626; background: rgba(220,38,38,.08); }
	.stat-num { font-weight: 600; font-size: 13px; color: inherit; }

	/* ── Table ── */
	.table-wrap {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	table { width: 100%; border-collapse: collapse; font-size: 13px; }
	thead { background: var(--bg-muted); }
	th {
		padding: 9px 14px;
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	td { padding: 10px 14px; border-top: 1px solid var(--border); vertical-align: middle; }
	.row-link { cursor: pointer; transition: background 0.12s; }
	.row-link:hover td { background: var(--bg-muted); }
	.muted { color: var(--text-muted); font-size: 12px; }

	/* ── Status dot ── */
	.status-dot {
		display: inline-block;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		margin-right: 7px;
		background: var(--border);
		vertical-align: middle;
	}
	.status-success  .status-dot,  td .status-success  { background: #16a34a; }
	.status-failed   .status-dot,  td .status-failed   { background: #dc2626; }
	.status-running  .status-dot,  td .status-running  { background: #2563eb; }
	.status-queued   .status-dot,  td .status-queued   { background: #d97706; }
	.status-cancelled .status-dot, td .status-cancelled { background: var(--text-muted); }

	/* status-dot standalone (used as: <span class="status-dot status-success">) */
	.status-dot.status-success  { background: #16a34a; }
	.status-dot.status-failed   { background: #dc2626; }
	.status-dot.status-running  { background: #2563eb; animation: blink 1.5s ease-in-out infinite; }
	.status-dot.status-queued   { background: #d97706; }
	.status-dot.status-cancelled { background: var(--text-muted); }
	@keyframes blink { 0%,100%{opacity:1} 50%{opacity:.35} }

	.status-text { text-transform: capitalize; }

	.service-cell { display: flex; flex-direction: column; gap: 2px; }
	.project-name { font-size: 11px; color: var(--text-muted); }
	.service-name { font-weight: 500; color: var(--text-primary); }

	.ref-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 7px;
		background: var(--bg-muted);
		border: 1px solid var(--border);
		border-radius: 10px;
		font-size: 11px;
		font-family: var(--font-mono);
		color: var(--text-muted);
		max-width: 160px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.triggered {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 12px;
		color: var(--text-muted);
	}

	/* ── Pagination ── */
	.pagination {
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 12px;
	}
	.page-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border: 1px solid var(--border);
		background: var(--bg-surface);
		border-radius: 6px;
		cursor: pointer;
		color: var(--text-muted);
	}
	.page-btn:disabled { opacity: 0.4; cursor: not-allowed; }
	.page-info { font-size: 13px; color: var(--text-muted); }

	/* ── Empty / error ── */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 48px 16px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.error-state {
		padding: 14px 16px;
		background: rgba(239,68,68,.08);
		border: 1px solid rgba(239,68,68,.2);
		border-radius: 8px;
		color: #ef4444;
		font-size: 13px;
	}

	/* ── Icon buttons ── */
	.icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border: 1px solid var(--border);
		background: var(--bg-surface);
		border-radius: 6px;
		color: var(--text-muted);
		cursor: pointer;
	}
	.icon-btn:hover { color: var(--text-primary); background: var(--bg-muted); }
	:global(.spin) { animation: spin 1s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ── Mobile cards ── */
	.mobile-cards { display: none; flex-direction: column; gap: 8px; }
	.card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 12px 14px;
		display: flex;
		flex-direction: column;
		gap: 8px;
		cursor: pointer;
	}
	.card:hover { background: var(--bg-muted); }
	.card-header { display: flex; align-items: center; justify-content: space-between; }
	.card-title  { display: flex; align-items: center; gap: 8px; }
	.card-chips  { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

	@media (max-width: 639px) {
		.table-wrap { display: none; }
		.mobile-cards { display: flex; }
		.parallelism-card { flex-direction: column; align-items: flex-start; }
		.parallelism-controls { width: 100%; }
		.parallel-input { flex: 1; }
	}
</style>
