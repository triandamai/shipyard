<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import {
		Zap, Globe, Plus, Trash2, RefreshCw, CheckCircle, XCircle,
		GitBranch, AlertTriangle, ExternalLink, Code2, FileText,
		Terminal, Copy, ChevronRight, X, RotateCcw, BookOpen
	} from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { uiStore } from '$lib/stores/ui.store';
	import { formatDistanceToNow } from 'date-fns';
	import EdgeFnDomainAddPanel from './resources/EdgeFnDomainAddPanel.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { javascript } from '@codemirror/lang-javascript';
	import { oneDark } from '@codemirror/theme-one-dark';
	import { EditorState } from '@codemirror/state';

	interface Props {
		groupId:   string;
		orgId:     string;
		projectId: string;
		onDeleted?: () => void;
	}

	let { groupId, orgId, projectId, onDeleted }: Props = $props();

	// ── Types ──────────────────────────────────────────────────────────────────

	type Tab = 'overview' | 'functions' | 'domains' | 'danger';

	interface Group {
		id: string; org_id: string; project_id: string | null;
		provider: string; repo_url: string; branch: string;
		last_deployed_sha: string | null; created_at: string;
	}
	interface EFn {
		id: string; name: string; runtime: string; status: string;
		last_deployed_at: string | null; public_url: string;
	}
	interface EFnDomain {
		id: string; service_id: string; hostname: string;
		tls_enabled: boolean; cert_provider: string; port: number | null;
		traefik_router_name: string; created_at: string;
	}
	interface InvocationLog {
		id: string; request_id: string; method: string; path: string;
		status_code: number; duration_ms: number; error: string | null; logged_at: string;
	}
	interface Deployment {
		id: string;
		version: string;
		commit_sha: string | null;
		deployed_by: string | null;
		status: string;
		error: string | null;
		artifact_path: string | null;
		files: string[];
		created_at: string;
	}
	interface DeployReport {
		deployed: string[];
		skipped: string[];
		failed: [string, string][];
		deleted: string[];
	}

	// ── State ──────────────────────────────────────────────────────────────────

	let activeTab   = $state<Tab>('overview');
	let loading     = $state(true);
	let loadError   = $state('');

	let group     = $state<Group | null>(null);
	let functions = $state<EFn[]>([]);
	let domains   = $state<EFnDomain[]>([]);

	// Per-function expanded state in functions tab
	let expandedFnId = $state<string | null>(null);

	// Per-function deployment history
	let fnDeployments        = $state<Record<string, Deployment[]>>({});
	let fnDeploymentsLoading = $state<Record<string, boolean>>({});
	let rollingBackId        = $state<string | null>(null);
	let rollbackError        = $state<Record<string, string>>({});

	// Code overlay
	let codeOverlay     = $state(false);
	let codeLoading     = $state(false);
	let codeContent     = $state('');
	let codeFnName      = $state('');
	let codeSubtitle    = $state('live deployment');
	let codeEditorEl    = $state<HTMLElement | null>(null);
	let codeEditorView: EditorView | null = null;
	let copied          = $state(false);

	// Logs overlay
	let logsOverlay   = $state(false);
	let logsLoading   = $state(false);
	let logsItems     = $state<InvocationLog[]>([]);
	let logsFnName    = $state('');
	let logsFnId      = $state('');
	let logsError     = $state('');

	// Domain DNS check
	let dnsState = $state<Record<string, 'idle' | 'checking' | 'ok' | 'fail'>>({});
	let dnsAddrs = $state<Record<string, string[]>>({});

	// Redeploy
	let redeploying    = $state(false);
	let redeployError  = $state('');
	let redeployOk     = $state(false);
	let lastReport     = $state<DeployReport | null>(null);

	// Delete
	let showDeleteModal = $state(false);
	let deleteInput     = $state('');
	let isDeleting      = $state(false);
	let deleteError     = $state('');
	let deleteValid     = $derived(deleteInput.trim() === (group?.branch ?? '') && deleteInput !== '');

	// ── Helpers ────────────────────────────────────────────────────────────────

	function formatTime(ts: string | null | undefined): string {
		if (!ts) return '—';
		try { return formatDistanceToNow(new Date(ts), { addSuffix: true }); }
		catch { return ts!; }
	}

	function statusColor(code: number): string {
		if (code < 300) return '#22c55e';
		if (code < 400) return '#f59e0b';
		return '#ef4444';
	}

	function repoName(url: string) {
		return url.trim().replace(/\.git$/, '').split('/').pop() ?? url;
	}

	// ── Load ───────────────────────────────────────────────────────────────────

	async function load() {
		loading = true; loadError = '';
		const [grpRes, fnRes] = await Promise.all([
			api.get<Group[]>(`/orgs/${orgId}/edge-functions/groups`),
			api.get<EFn[]>(`/orgs/${orgId}/edge-functions?group_id=${groupId}`),
		]);
		if (grpRes.error) { loadError = grpRes.error.message; loading = false; return; }
		if (fnRes.error)  { loadError = fnRes.error.message;  loading = false; return; }
		group = grpRes.data?.find(g => g.id === groupId) ?? null;
		if (!group) { loadError = 'Group not found.'; loading = false; return; }
		functions = fnRes.data ?? [];
		loading = false;
	}

	async function loadDomains() {
		const res = await api.get<EFnDomain[]>(`/orgs/${orgId}/edge-functions/groups/${groupId}/domains`);
		if (res.data) domains = res.data;
	}

	onMount(load);

	async function switchTab(tab: Tab) {
		activeTab = tab;
		if (tab === 'domains' && domains.length === 0) await loadDomains();
		if (tab === 'functions') {
			// Preload deployment history for all functions in parallel
			for (const fn of functions) {
				if (!fnDeployments[fn.id]) loadFnDeployments(fn.id);
			}
			// Auto-expand the first function so history is immediately visible
			if (functions.length > 0 && !expandedFnId) {
				expandedFnId = functions[0].id;
			}
		}
	}

	// ── Deployment history ─────────────────────────────────────────────────────

	async function loadFnDeployments(fnId: string) {
		fnDeploymentsLoading = { ...fnDeploymentsLoading, [fnId]: true };
		const res = await api.get<{ items: Deployment[] }>(`/orgs/${orgId}/edge-functions/${fnId}/deployments`);
		fnDeploymentsLoading = { ...fnDeploymentsLoading, [fnId]: false };
		if (res.data?.items) fnDeployments = { ...fnDeployments, [fnId]: res.data.items };
	}

	function toggleFn(fnId: string) {
		if (expandedFnId === fnId) {
			expandedFnId = null;
		} else {
			expandedFnId = fnId;
			if (!fnDeployments[fnId]) loadFnDeployments(fnId);
		}
	}

	async function rollbackDeployment(fn: EFn, dep: Deployment) {
		rollingBackId = dep.id;
		rollbackError = { ...rollbackError, [fn.id]: '' };
		const res = await api.post(`/orgs/${orgId}/edge-functions/${fn.id}/rollback/${dep.id}`, {});
		rollingBackId = null;
		if (res.error) {
			rollbackError = { ...rollbackError, [fn.id]: res.error.message };
			return;
		}
		await Promise.all([loadFnDeployments(fn.id), load()]);
	}

	// ── Code overlay ───────────────────────────────────────────────────────────

	async function openCode(fn: EFn, dep?: Deployment) {
		codeFnName  = fn.name;
		codeSubtitle = dep
			? `snapshot · ${dep.commit_sha?.slice(0, 7) ?? 'manual upload'}`
			: 'live deployment';
		codeContent = '';
		codeOverlay = true;
		codeLoading = true;
		const url = dep
			? `/orgs/${orgId}/edge-functions/${fn.id}/code?deployment_id=${dep.id}`
			: `/orgs/${orgId}/edge-functions/${fn.id}/code`;
		const res = await api.get<{ code: string | null }>(url);
		codeLoading = false;
		codeContent = res.data?.code ?? '// No deployed code found.';
	}

	$effect(() => {
		if (codeOverlay && codeEditorEl && !codeLoading) {
			codeEditorView?.destroy();
			codeEditorView = new EditorView({
				state: EditorState.create({
					doc: codeContent,
					extensions: [
						basicSetup,
						javascript({ typescript: true }),
						oneDark,
						EditorView.editable.of(false),
						EditorView.lineWrapping,
					],
				}),
				parent: codeEditorEl,
			});
		}
		if (!codeOverlay) {
			codeEditorView?.destroy();
			codeEditorView = null;
		}
	});

	async function copyCode() {
		await navigator.clipboard.writeText(codeContent);
		copied = true;
		setTimeout(() => { copied = false; }, 1500);
	}

	function closeCode() {
		codeOverlay = false;
	}

	// ── Logs overlay ───────────────────────────────────────────────────────────

	async function openLogs(fn: EFn) {
		logsFnName  = fn.name;
		logsFnId    = fn.id;
		logsItems   = [];
		logsError   = '';
		logsOverlay = true;
		logsLoading = true;
		const res = await api.get<{ items: InvocationLog[] }>(`/orgs/${orgId}/edge-functions/${fn.id}/logs`);
		logsLoading = false;
		if (res.error) { logsError = res.error.message; return; }
		logsItems = res.data?.items ?? [];
	}

	async function refreshLogs() {
		logsLoading = true; logsError = '';
		const res = await api.get<{ items: InvocationLog[] }>(`/orgs/${orgId}/edge-functions/${logsFnId}/logs`);
		logsLoading = false;
		if (res.error) { logsError = res.error.message; return; }
		logsItems = res.data?.items ?? [];
	}

	function closeLogs() {
		logsOverlay = false;
	}

	// ── Actions ────────────────────────────────────────────────────────────────

	async function redeploy() {
		redeploying = true; redeployError = ''; redeployOk = false; lastReport = null;
		const res = await api.post<DeployReport>(`/orgs/${orgId}/edge-functions/groups/${groupId}/deploy`, {});
		redeploying = false;
		if (res.error) { redeployError = res.error.message; return; }
		lastReport = res.data ?? null;
		redeployOk = true;
		setTimeout(() => { redeployOk = false; lastReport = null; }, 8000);
		await load();
	}

	function openAddDomainPanel() {
		uiStore.pushPanel({
			component: EdgeFnDomainAddPanel,
			title: 'Add Domain',
			props: {
				orgId, groupId,
				onCreated: (d: EFnDomain) => { domains = [...domains, d]; },
			},
		});
	}

	async function removeDomain(domainId: string) {
		await api.delete(`/orgs/${orgId}/edge-functions/groups/${groupId}/domains/${domainId}`);
		domains = domains.filter(d => d.id !== domainId);
		delete dnsState[domainId];
	}

	async function checkDns(domain: EFnDomain) {
		dnsState = { ...dnsState, [domain.id]: 'checking' };
		try {
			const res  = await fetch(`https://dns.google/resolve?name=${encodeURIComponent(domain.hostname)}&type=A`);
			const json = await res.json();
			const addrs: string[] = (json.Answer ?? []).map((a: any) => a.data).filter(Boolean);
			dnsState = { ...dnsState, [domain.id]: addrs.length > 0 ? 'ok' : 'fail' };
			dnsAddrs  = { ...dnsAddrs,  [domain.id]: addrs };
		} catch {
			dnsState = { ...dnsState, [domain.id]: 'fail' };
		}
	}

	async function deleteGroup() {
		if (!deleteValid || isDeleting) return;
		isDeleting = true; deleteError = '';
		const res = await api.delete(`/orgs/${orgId}/edge-functions/groups/${groupId}`);
		if (res.error) { deleteError = res.error.message; isDeleting = false; return; }
		showDeleteModal = false;
		onDeleted?.();
		uiStore.clearPanels();
	}

	onDestroy(() => { codeEditorView?.destroy(); });
</script>

<!-- ── Code overlay ─────────────────────────────────────────────────────────── -->
{#if codeOverlay}
	<div class="overlay-backdrop" role="dialog" aria-modal="true">
		<div class="code-overlay">
			<div class="overlay-header">
				<div class="overlay-title">
					<Code2 size={14} />
					<span>{codeFnName}</span>
					<span class="overlay-subtitle">{codeSubtitle}</span>
				</div>
				<div class="overlay-actions">
					<button class="btn-icon" onclick={copyCode} title="Copy code">
						{#if copied}<CheckCircle size={14} style="color:#22c55e" />{:else}<Copy size={14} />{/if}
					</button>
					<button class="btn-icon" onclick={closeCode} title="Close"><X size={14} /></button>
				</div>
			</div>
			<div class="code-body">
				{#if codeLoading}
					<div class="overlay-loading"><div class="spinner-sm"></div> Loading code…</div>
				{:else}
					<div class="editor-wrap" bind:this={codeEditorEl}></div>
				{/if}
			</div>
		</div>
	</div>
{/if}

<!-- ── Logs overlay ─────────────────────────────────────────────────────────── -->
{#if logsOverlay}
	<div class="overlay-backdrop" role="dialog" aria-modal="true">
		<div class="logs-overlay">
			<div class="overlay-header">
				<div class="overlay-title">
					<FileText size={14} />
					<span>{logsFnName}</span>
					<span class="overlay-subtitle">invocation logs · last 100</span>
				</div>
				<div class="overlay-actions">
					<button class="btn-icon" onclick={refreshLogs} title="Refresh">
						<RotateCcw size={13} />
					</button>
					<button class="btn-icon" onclick={closeLogs} title="Close"><X size={14} /></button>
				</div>
			</div>
			<div class="logs-body">
				{#if logsLoading}
					<div class="overlay-loading"><div class="spinner-sm"></div> Loading logs…</div>
				{:else if logsError}
					<div class="err-msg">{logsError}</div>
				{:else if logsItems.length === 0}
					<div class="overlay-empty">No invocations recorded yet.</div>
				{:else}
					<table class="logs-table">
						<thead>
							<tr>
								<th>Time</th>
								<th>Method</th>
								<th>Path</th>
								<th>Status</th>
								<th>Duration</th>
								<th>Error</th>
							</tr>
						</thead>
						<tbody>
							{#each logsItems as log (log.id)}
								<tr class:log-error={!!log.error}>
									<td class="log-time">{formatTime(log.logged_at)}</td>
									<td><span class="method-badge">{log.method}</span></td>
									<td class="log-path mono">{log.path}</td>
									<td>
										<span class="status-badge" style="color:{statusColor(log.status_code)}">
											{log.status_code}
										</span>
									</td>
									<td class="log-dur">{log.duration_ms}ms</td>
									<td class="log-err-msg">{log.error ?? ''}</td>
								</tr>
							{/each}
						</tbody>
					</table>
				{/if}
			</div>
		</div>
	</div>
{/if}

<!-- ── Delete modal ──────────────────────────────────────────────────────────── -->
{#if showDeleteModal}
	<div class="modal-backdrop" role="dialog" aria-modal="true">
		<div class="modal-card">
			<div class="modal-header">
				<AlertTriangle size={16} style="color:#ef4444;flex-shrink:0" />
				<span>Delete Edge Function Group</span>
			</div>
			<div class="modal-body">
				<p class="modal-warning">
					Permanently removes the group, all deployed functions, code, invocation logs, and custom domains.
					<strong>This cannot be undone.</strong>
				</p>
				<div class="modal-confirm-field">
					<label class="modal-confirm-label">
						Type the branch name <code class="code">{group?.branch}</code> to confirm
					</label>
					<input class="modal-input" type="text" placeholder={group?.branch ?? ''}
						bind:value={deleteInput} autocomplete="off" />
				</div>
				{#if deleteError}<div class="err-msg">{deleteError}</div>{/if}
			</div>
			<div class="modal-footer">
				<button class="btn-ghost" onclick={() => { showDeleteModal = false; deleteInput = ''; }}
					disabled={isDeleting}>Cancel</button>
				<button class="btn-danger" disabled={!deleteValid || isDeleting} onclick={deleteGroup}>
					{#if isDeleting}<div class="spin-xs"></div> Deleting…{:else}<Trash2 size={13} /> Delete{/if}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- ── Panel body ────────────────────────────────────────────────────────────── -->
<div class="panel-body">
	{#if loading}
		<div class="loading-row"><div class="spinner-sm"></div> Loading…</div>
	{:else if loadError}
		<div class="err-msg">{loadError}</div>
	{:else if group}

		<div class="tabs">
			{#each [['overview','Overview'],['functions','Functions'],['domains','Domains'],['danger','Danger']] as [id, label]}
				<button class="tab" class:active={activeTab === id}
					class:tab-danger={id === 'danger'}
					onclick={() => switchTab(id as Tab)}>{label}
					{#if id === 'functions' && functions.length > 0}
						<span class="tab-count">{functions.length}</span>
					{/if}
				</button>
			{/each}
		</div>

		<!-- ── Overview ── -->
		{#if activeTab === 'overview'}
			<section class="section">
				<div class="hero-row">
					<div class="hero-icon"><Zap size={18} /></div>
					<div class="hero-info">
						<div class="hero-name">{repoName(group.repo_url)}</div>
						<div class="hero-sub">
							<GitBranch size={10} />
							<span class="mono">{group.branch}</span>
							{#if group.provider}<span class="sep">·</span><span>{group.provider}</span>{/if}
						</div>
					</div>
				</div>
			</section>

			<section class="section">
				<div class="kv-grid">
					<div class="kv-row"><span class="kv-k">Repo</span><span class="kv-v mono">{group.repo_url}</span></div>
					<div class="kv-row"><span class="kv-k">Branch</span><span class="kv-v mono">{group.branch}</span></div>
					<div class="kv-row"><span class="kv-k">Last SHA</span>
						<span class="kv-v mono">{group.last_deployed_sha ? group.last_deployed_sha.slice(0, 7) : '—'}</span>
					</div>
					<div class="kv-row"><span class="kv-k">Created</span><span class="kv-v">{formatTime(group.created_at)}</span></div>
					<div class="kv-row"><span class="kv-k">Functions</span><span class="kv-v">{functions.length}</span></div>
				</div>
			</section>

			<section class="section">
				{#if redeployError}<div class="err-msg">{redeployError}</div>{/if}
				{#if redeployOk}
					{#if lastReport}
						{#if lastReport.deployed.length > 0}
							<div class="ok-msg">
								✓ {lastReport.deployed.length} function{lastReport.deployed.length !== 1 ? 's' : ''} deployed
								{#if lastReport.failed.length > 0} · {lastReport.failed.length} failed{/if}
							</div>
						{:else if lastReport.failed.length > 0}
							<div class="err-msg">
								Deploy ran but {lastReport.failed.length} function{lastReport.failed.length !== 1 ? 's' : ''} failed:
								{lastReport.failed.map(([name, err]) => `${name}: ${err}`).join(', ')}
							</div>
						{:else}
							<div class="warn-msg">
								No functions detected. Ensure your repo has a <code class="mono-inline">functions/</code> directory
								with <code class="mono-inline">.ts</code> or <code class="mono-inline">.js</code> files
								containing <code class="mono-inline">export default</code>.
							</div>
						{/if}
					{:else}
						<div class="ok-msg">Redeploy triggered.</div>
					{/if}
				{/if}
				<div class="overview-actions">
					<button class="btn-primary" onclick={redeploy} disabled={redeploying}>
						{#if redeploying}<div class="spin-xs-w"></div> Redeploying…
						{:else}<RefreshCw size={13} /> Redeploy Now{/if}
					</button>
					<a class="btn-docs" href="/docs/edge-functions" target="_blank" rel="noopener">
						<BookOpen size={12} /> Docs
					</a>
				</div>
			</section>
		{/if}

		<!-- ── Functions ── -->
		{#if activeTab === 'functions'}
			{#if functions.length === 0}
				<div class="empty-state">No functions deployed yet.
					<span class="empty-sub">
						Click <strong>Redeploy Now</strong> on the Overview tab, or push to
						<code class="mono-inline">{group?.branch ?? 'main'}</code>.
					</span>
					<span class="empty-sub">
						Repo must have a <code class="mono-inline">functions/</code> directory with
						<code class="mono-inline">.ts</code>/<code class="mono-inline">.js</code> files
						that contain <code class="mono-inline">export default</code>.
					</span>
				</div>
			{:else}
				<div class="fn-cards">
					{#each functions as fn (fn.id)}
						{@const expanded = expandedFnId === fn.id}
						<div class="fn-card" class:expanded>
							<!-- card header — always visible -->
							<button class="fn-card-header" onclick={() => toggleFn(fn.id)}>
								<div class="fn-dot" class:active={fn.status === 'active'}></div>
								<span class="fn-name mono">{fn.name}</span>
								<span class="fn-rt">{fn.runtime}</span>
								<span class="fn-deployed">{formatTime(fn.last_deployed_at)}</span>
								<ChevronRight size={13} class={expanded ? 'fn-chevron rotated' : 'fn-chevron'} />
							</button>

							<!-- expanded content -->
							{#if expanded}
								<div class="fn-detail">
									<!-- Status row -->
									<div class="fn-status-row">
										<span class="fn-status-badge" class:status-active={fn.status === 'active'}>
											{fn.status}
										</span>
										{#if fn.public_url}
											<a class="fn-url mono" href={fn.public_url} target="_blank" rel="noopener">
												{fn.public_url}<ExternalLink size={10} style="margin-left:4px;flex-shrink:0" />
											</a>
										{/if}
									</div>

									<!-- Action buttons -->
									<div class="fn-actions">
										<button class="btn-action" onclick={() => openCode(fn)}>
											<Code2 size={12} /> View Live Code
										</button>
										<button class="btn-action" onclick={() => openLogs(fn)}>
											<FileText size={12} /> Invocation Logs
										</button>
									</div>

									<!-- Deployment History -->
									<div class="dep-history">
										<div class="dep-history-title">
											<GitBranch size={11} /> Deployment History
										</div>
										{#if fnDeploymentsLoading[fn.id]}
											<div class="dep-loading"><div class="spin-xs-inline"></div> Loading…</div>
										{:else if !fnDeployments[fn.id] || fnDeployments[fn.id].length === 0}
											<div class="dep-empty">No deployments recorded.</div>
										{:else}
											{#if rollbackError[fn.id]}
												<div class="dep-error">{rollbackError[fn.id]}</div>
											{/if}
											<div class="dep-list">
												{#each fnDeployments[fn.id] as dep (dep.id)}
													<div class="dep-row">
														<div class="dep-meta">
															<span class="dep-dot" class:dep-dot-live={dep.status === 'live'}></span>
															<span class="dep-version mono">{dep.version}</span>
															<span class="dep-status-label" class:dep-live-label={dep.status === 'live'}>
																{dep.status}
															</span>
															{#if dep.commit_sha}
																<span class="dep-sha mono">{dep.commit_sha.slice(0, 7)}</span>
															{/if}
															<span class="dep-time">{formatTime(dep.created_at)}</span>
														</div>
														<div class="dep-btns">
															<button class="dep-btn" onclick={() => openCode(fn, dep)}>
																<Code2 size={10} /> View
															</button>
															{#if dep.status !== 'live'}
																<button
																	class="dep-btn dep-btn-restore"
																	disabled={rollingBackId === dep.id}
																	onclick={() => rollbackDeployment(fn, dep)}
																>
																	{#if rollingBackId === dep.id}
																		<div class="spin-xs-inline"></div>
																	{:else}
																		<RotateCcw size={10} /> Restore
																	{/if}
																</button>
															{/if}
														</div>
													</div>
													{#if dep.files && dep.files.length > 0}
														<div class="dep-files">
															{#each dep.files as file}
																<span class="dep-file mono">{file}</span>
															{/each}
														</div>
													{/if}
												{/each}
											</div>
										{/if}
									</div>

									<!-- How to invoke -->
									<div class="invoke-section">
										<div class="invoke-title"><Terminal size={11} /> How to invoke</div>

										{#if fn.public_url}
											<div class="invoke-block">
												<div class="invoke-label">GET request</div>
												<pre class="invoke-code">{`curl -X GET "${fn.public_url}"`}</pre>
											</div>
											<div class="invoke-block">
												<div class="invoke-label">POST with JSON body</div>
												<pre class="invoke-code">{`curl -X POST "${fn.public_url}" \\
  -H "Content-Type: application/json" \\
  -d '{"key": "value"}'`}</pre>
											</div>
											<div class="invoke-block">
												<div class="invoke-label">With custom path</div>
												<pre class="invoke-code">{`curl "${fn.public_url}/your-path?param=value"`}</pre>
											</div>
										{:else}
											<div class="invoke-note">Deploy this function first to get a public URL.</div>
										{/if}
									</div>
								</div>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		{/if}

		<!-- ── Domains ── -->
		{#if activeTab === 'domains'}
			<section class="section">
				<div class="section-head">
					<span class="section-title">Custom Domains</span>
					<button class="btn-secondary btn-sm" onclick={openAddDomainPanel}>
						<Plus size={12} /> Add Domain
					</button>
				</div>

				{#if domains.length === 0}
					<div class="empty-state">
						No domains configured.<br />
						<span class="empty-sub">Add a custom domain to route traffic to your edge functions.</span>
					</div>
				{:else}
					<div class="domain-list">
						{#each domains as domain (domain.id)}
							<div class="domain-row">
								<div class="domain-info">
									<Globe size={12} class="domain-globe" />
									<span class="domain-hostname mono">{domain.hostname}</span>
									{#if domain.tls_enabled}<span class="tls-badge">HTTPS</span>{/if}
									{#if dnsState[domain.id] === 'ok'}
										<span class="dns-ok"><CheckCircle size={10} /> DNS OK</span>
									{:else if dnsState[domain.id] === 'fail'}
										<span class="dns-fail"><XCircle size={10} /> No DNS</span>
									{/if}
								</div>
								<div class="domain-actions">
									<button class="btn-ghost btn-xs" onclick={() => checkDns(domain)}
										disabled={dnsState[domain.id] === 'checking'}>
										{dnsState[domain.id] === 'checking' ? '…' : 'DNS'}
									</button>
									<button class="btn-ghost btn-xs danger-ghost" onclick={() => removeDomain(domain.id)}>
										<Trash2 size={11} />
									</button>
								</div>
							</div>
						{/each}
					</div>
				{/if}

				<div class="dns-hint">
					<strong>DNS:</strong> Point your domain's A record to the Shipyard server IP,
					or CNAME to your Shipyard hostname. Traffic routes to the edge runtime on port 8000.
				</div>
			</section>
		{/if}

		<!-- ── Danger ── -->
		{#if activeTab === 'danger'}
			<section class="section danger-section">
				<div class="section-title danger-title">Delete this group</div>
				<p class="section-desc">
					Permanently deletes all functions, code bundles, invocation logs, and custom domains.
					Traefik routes are removed immediately. This cannot be undone.
				</p>
				<button class="btn-danger-outline" onclick={() => { showDeleteModal = true; deleteInput = ''; }}>
					<Trash2 size={12} /> Delete Group
				</button>
			</section>
		{/if}

	{/if}
</div>

<style>
	/* ── Panel body ── */
	.panel-body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 0;
		height: 100%;
		overflow-y: auto;
	}

	/* ── Tabs ── */
	.tabs {
		display: flex;
		gap: 2px;
		border-bottom: 1px solid var(--border);
		margin-bottom: 16px;
		overflow-x: auto;
		scrollbar-width: none;
	}
	.tabs::-webkit-scrollbar { display: none; }

	.tab {
		display: flex; align-items: center; gap: 5px;
		padding: 7px 12px;
		font-size: 12px; font-weight: 500;
		color: var(--text-muted);
		background: none; border: none;
		border-bottom: 2px solid transparent;
		cursor: pointer; margin-bottom: -1px;
		transition: all var(--transition-fast);
		white-space: nowrap;
	}
	.tab:hover { color: var(--text-primary); }
	.tab.active { color: var(--accent); border-bottom-color: var(--accent); }
	.tab.tab-danger:hover { color: #ef4444; }
	.tab.tab-danger.active { color: #ef4444; border-bottom-color: #ef4444; }

	.tab-count {
		font-size: 10px; font-weight: 700;
		padding: 1px 5px; border-radius: 99px;
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		color: var(--accent);
	}

	/* ── Sections ── */
	.section {
		display: flex; flex-direction: column; gap: 10px;
		margin-bottom: 20px;
	}
	.section-head {
		display: flex; align-items: center; justify-content: space-between;
	}
	.section-title {
		font-size: 11px; font-weight: 600; color: var(--text-muted);
		text-transform: uppercase; letter-spacing: 0.05em;
	}
	.section-desc { font-size: 12px; color: var(--text-muted); line-height: 1.5; }

	/* ── Hero ── */
	.hero-row {
		display: flex; align-items: center; gap: 12px; padding: 12px;
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm);
	}
	.hero-icon {
		width: 36px; height: 36px; border-radius: var(--radius-sm);
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent); display: flex; align-items: center; justify-content: center;
		flex-shrink: 0;
	}
	.hero-info { flex: 1; min-width: 0; }
	.hero-name { font-size: 14px; font-weight: 700; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.hero-sub { display: flex; align-items: center; gap: 5px; font-size: 11px; color: var(--text-muted); margin-top: 2px; }
	.sep { color: var(--border); }

	/* ── KV grid ── */
	.kv-grid {
		display: flex; flex-direction: column; gap: 0;
		border: 1px solid var(--border); border-radius: var(--radius-sm); overflow: hidden;
	}
	.kv-row {
		display: flex; align-items: center; padding: 7px 12px;
		font-size: 12px; border-bottom: 1px solid var(--border);
	}
	.kv-row:last-child { border-bottom: none; }
	.kv-k { color: var(--text-muted); width: 100px; flex-shrink: 0; }
	.kv-v { color: var(--text-primary); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }

	/* ── Function cards ── */
	.fn-cards { display: flex; flex-direction: column; gap: 6px; }

	.fn-card {
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		overflow: hidden;
		transition: border-color var(--transition-fast);
	}
	.fn-card:hover { border-color: var(--border-hover); }
	.fn-card.expanded { border-color: var(--accent); }

	.fn-card-header {
		display: flex; align-items: center; gap: 8px;
		padding: 9px 12px; width: 100%;
		background: none; border: none; cursor: pointer;
		font-family: var(--font-sans); text-align: left;
		transition: background var(--transition-fast);
	}
	.fn-card-header:hover { background: var(--bg-surface); }

	.fn-dot {
		width: 7px; height: 7px; border-radius: 50%; flex-shrink: 0;
		background: var(--text-dim);
	}
	.fn-dot.active { background: #22c55e; box-shadow: 0 0 4px #22c55e66; }

	.fn-name { flex: 1; font-size: 13px; font-weight: 600; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.fn-rt   { font-size: 10px; color: var(--text-dim); flex-shrink: 0; }
	.fn-deployed { font-size: 10px; color: var(--text-dim); flex-shrink: 0; }

	:global(.fn-chevron) { color: var(--text-dim); flex-shrink: 0; transition: transform var(--transition-fast); }
	:global(.fn-chevron.rotated) { transform: rotate(90deg); }

	/* ── Function detail (expanded) ── */
	.fn-detail {
		border-top: 1px solid var(--border);
		padding: 12px;
		display: flex; flex-direction: column; gap: 12px;
	}

	.fn-status-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
	.fn-status-badge {
		font-size: 10px; font-weight: 700; padding: 2px 8px; border-radius: 99px;
		background: color-mix(in srgb, var(--text-dim) 15%, transparent);
		color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.05em;
	}
	.fn-status-badge.status-active {
		background: color-mix(in srgb, #22c55e 15%, transparent);
		color: #22c55e;
	}
	.fn-url {
		font-size: 11px; color: var(--text-muted); text-decoration: none;
		display: inline-flex; align-items: center; gap: 3px;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0;
	}
	.fn-url:hover { color: var(--accent); }

	.fn-actions { display: flex; gap: 6px; }
	.btn-action {
		display: inline-flex; align-items: center; gap: 5px;
		padding: 5px 11px; font-size: 11px; font-weight: 600;
		background: var(--bg-surface); color: var(--text-primary);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		cursor: pointer; transition: all var(--transition-fast);
		font-family: var(--font-sans);
	}
	.btn-action:hover { border-color: var(--accent); color: var(--accent); }

	/* ── Deployment history ── */
	.dep-history {
		background: var(--bg-base); border: 1px solid var(--border);
		border-radius: var(--radius-sm); overflow: hidden;
	}
	.dep-history-title {
		display: flex; align-items: center; gap: 6px;
		padding: 7px 12px;
		font-size: 10px; font-weight: 700; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em;
		border-bottom: 1px solid var(--border); background: var(--bg-elevated);
	}
	.dep-loading, .dep-empty {
		padding: 10px 12px; font-size: 11px; color: var(--text-dim);
		display: flex; align-items: center; gap: 6px;
	}
	.dep-error {
		margin: 6px 12px 0; padding: 6px 8px; font-size: 11px; color: #ef4444;
		background: color-mix(in srgb, #ef4444 8%, transparent);
		border: 1px solid color-mix(in srgb, #ef4444 25%, transparent);
		border-radius: 4px;
	}
	.dep-list { display: flex; flex-direction: column; }
	.dep-row {
		display: flex; align-items: center; justify-content: space-between;
		padding: 7px 12px; border-bottom: 1px solid var(--border);
		gap: 8px;
	}
	.dep-row:last-child { border-bottom: none; }
	.dep-meta { display: flex; align-items: center; gap: 7px; flex: 1; min-width: 0; }
	.dep-dot {
		width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0;
		background: var(--text-dim);
	}
	.dep-dot-live { background: #22c55e; }
	.dep-status-label {
		font-size: 10px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.05em; flex-shrink: 0;
	}
	.dep-live-label { color: #22c55e; }
	.dep-version { font-size: 10px; font-weight: 700; color: var(--accent); flex-shrink: 0; }
	.dep-sha { font-size: 11px; color: var(--text-primary); flex-shrink: 0; }
	.dep-time { font-size: 10px; color: var(--text-dim); }
	.dep-files {
		display: flex; flex-wrap: wrap; gap: 4px;
		padding: 4px 12px 8px 26px;
		border-bottom: 1px solid var(--border);
	}
	.dep-files:last-child { border-bottom: none; }
	.dep-file {
		font-size: 10px; color: var(--text-dim);
		background: var(--bg-elevated); border: 1px solid var(--border);
		padding: 1px 6px; border-radius: 3px;
	}
	.dep-btns { display: flex; gap: 4px; flex-shrink: 0; }
	.dep-btn {
		display: inline-flex; align-items: center; gap: 4px;
		padding: 3px 8px; font-size: 10px; font-weight: 600;
		background: var(--bg-elevated); color: var(--text-muted);
		border: 1px solid var(--border); border-radius: 4px;
		cursor: pointer; transition: all var(--transition-fast);
		font-family: var(--font-sans);
	}
	.dep-btn:hover { border-color: var(--accent); color: var(--accent); }
	.dep-btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.dep-btn-restore:hover { border-color: #f59e0b; color: #f59e0b; }
	.spin-xs-inline {
		display: inline-block; width: 10px; height: 10px;
		border: 1.5px solid var(--border); border-top-color: var(--accent);
		border-radius: 50%; animation: spin 0.7s linear infinite;
	}

	/* ── Invoke section ── */
	.invoke-section {
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		overflow: hidden;
	}
	.invoke-title {
		display: flex; align-items: center; gap: 6px;
		padding: 7px 12px;
		font-size: 10px; font-weight: 700; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.invoke-block { border-bottom: 1px solid var(--border); }
	.invoke-block:last-child { border-bottom: none; }
	.invoke-label {
		padding: 5px 12px 0;
		font-size: 10px; color: var(--text-dim); font-weight: 500;
	}
	.invoke-code {
		margin: 0; padding: 6px 12px 10px;
		font-family: var(--font-mono); font-size: 11px;
		color: var(--text-primary); white-space: pre-wrap; word-break: break-all;
		line-height: 1.7;
	}
	.invoke-note {
		padding: 10px 12px; font-size: 11px; color: var(--text-dim); font-style: italic;
	}

	/* ── Domain list ── */
	.domain-list { display: flex; flex-direction: column; gap: 6px; }
	.domain-row {
		display: flex; align-items: center; justify-content: space-between;
		padding: 9px 12px; border: 1px solid var(--border);
		border-radius: var(--radius-sm); background: var(--bg-elevated);
	}
	.domain-info { display: flex; align-items: center; gap: 7px; flex-wrap: wrap; flex: 1; min-width: 0; }
	:global(.domain-globe) { color: var(--text-dim); flex-shrink: 0; }
	.domain-hostname { font-size: 13px; font-weight: 600; color: var(--text-primary); }
	.tls-badge {
		font-size: 10px; font-weight: 600; padding: 1px 6px; border-radius: 100px;
		background: color-mix(in srgb, #22c55e 12%, transparent); color: #22c55e;
	}
	.dns-ok  { display: inline-flex; align-items: center; gap: 3px; font-size: 10px; font-weight: 600; color: #22c55e; }
	.dns-fail{ display: inline-flex; align-items: center; gap: 3px; font-size: 10px; font-weight: 600; color: #ef4444; }
	.domain-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
	.dns-hint {
		font-size: 11px; color: var(--text-muted);
		padding: 8px 10px; background: var(--bg-surface);
		border: 1px solid var(--border); border-radius: var(--radius-sm); line-height: 1.5;
	}

	/* ── Danger ── */
	.danger-section {
		border: 1px solid color-mix(in srgb, #ef4444 35%, transparent);
		border-radius: var(--radius-sm); padding: 14px;
		background: color-mix(in srgb, #ef4444 4%, transparent);
	}
	.danger-title { color: #ef4444; }

	/* ── Overlays ── */
	.overlay-backdrop {
		position: fixed; inset: 0;
		background: rgba(0,0,0,0.7);
		display: flex; align-items: stretch; justify-content: center;
		z-index: 200;
		padding: 24px;
	}

	.code-overlay, .logs-overlay {
		display: flex; flex-direction: column;
		width: 100%; max-width: 900px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
		box-shadow: 0 16px 48px rgba(0,0,0,0.5);
	}

	.overlay-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 11px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		flex-shrink: 0;
	}
	.overlay-title {
		display: flex; align-items: center; gap: 8px;
		font-size: 13px; font-weight: 700; color: var(--text-primary);
	}
	.overlay-subtitle { font-size: 11px; font-weight: 400; color: var(--text-dim); }
	.overlay-actions { display: flex; align-items: center; gap: 4px; }

	.btn-icon {
		display: flex; align-items: center; justify-content: center;
		width: 28px; height: 28px; border-radius: var(--radius-sm);
		background: none; border: none; cursor: pointer;
		color: var(--text-muted); transition: all var(--transition-fast);
	}
	.btn-icon:hover { background: var(--bg-surface); color: var(--text-primary); }

	.code-body {
		flex: 1; overflow: hidden; display: flex; flex-direction: column;
		min-height: 0;
	}

	.editor-wrap {
		flex: 1; overflow: auto; height: 100%;
	}

	/* Override CodeMirror to fill the container */
	:global(.editor-wrap .cm-editor) {
		height: 100%;
		font-size: 13px;
		font-family: var(--font-mono);
	}
	:global(.editor-wrap .cm-scroller) { overflow: auto; }

	.overlay-loading {
		display: flex; align-items: center; gap: 10px;
		padding: 32px; color: var(--text-muted); font-size: 13px;
	}
	.overlay-empty {
		padding: 32px; text-align: center; color: var(--text-dim); font-size: 13px;
	}

	/* ── Logs table ── */
	.logs-body {
		flex: 1; overflow: auto; min-height: 0;
	}
	.logs-table {
		width: 100%; border-collapse: collapse; font-size: 12px;
	}
	.logs-table thead {
		position: sticky; top: 0; z-index: 1;
		background: var(--bg-elevated);
	}
	.logs-table th {
		padding: 8px 12px; text-align: left;
		font-size: 10px; font-weight: 700; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
		border-bottom: 1px solid var(--border);
	}
	.logs-table td {
		padding: 7px 12px; border-bottom: 1px solid var(--border);
		color: var(--text-primary); vertical-align: middle;
	}
	.logs-table tr:last-child td { border-bottom: none; }
	.logs-table tr:hover td { background: var(--bg-elevated); }
	.logs-table tr.log-error td { background: color-mix(in srgb, #ef4444 4%, transparent); }

	.log-time  { color: var(--text-dim); white-space: nowrap; font-size: 11px; }
	.log-path  { font-size: 11px; max-width: 200px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.log-dur   { color: var(--text-muted); white-space: nowrap; }
	.log-err-msg { color: #ef4444; font-size: 11px; max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.method-badge {
		font-size: 10px; font-weight: 700; padding: 1px 5px; border-radius: 3px;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent); font-family: var(--font-mono);
	}
	.status-badge { font-size: 12px; font-weight: 700; font-family: var(--font-mono); }

	/* ── Buttons ── */
	.overview-actions { display: flex; gap: 8px; align-items: center; }
	.btn-primary {
		display: inline-flex; align-items: center; gap: 6px;
		padding: 7px 14px; font-size: 12px; font-weight: 600;
		background: var(--accent); color: white; border: none;
		border-radius: var(--radius-sm); cursor: pointer;
		transition: opacity var(--transition-fast); flex: 1; justify-content: center;
	}
	.btn-primary:hover:not(:disabled) { opacity: 0.88; }
	.btn-docs {
		display: inline-flex; align-items: center; gap: 5px;
		padding: 7px 12px; border-radius: var(--radius-sm);
		background: var(--bg-elevated); border: 1px solid var(--border);
		color: var(--text-muted); font-size: 12px; font-weight: 600;
		text-decoration: none; font-family: var(--font-sans);
		transition: all var(--transition-fast); white-space: nowrap; cursor: pointer;
	}
	.btn-docs:hover { border-color: var(--accent); color: var(--accent); }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-secondary {
		display: inline-flex; align-items: center; gap: 6px;
		padding: 7px 12px; font-size: 12px; font-weight: 500;
		background: var(--bg-elevated); color: var(--text-primary);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		cursor: pointer; transition: border-color var(--transition-fast);
	}
	.btn-secondary:hover { border-color: var(--border-hover); }
	.btn-secondary.btn-sm { padding: 4px 9px; font-size: 11px; }

	.btn-ghost {
		display: inline-flex; align-items: center; gap: 5px;
		padding: 5px 10px; font-size: 11px; color: var(--text-muted);
		background: none; border: none; border-radius: var(--radius-sm); cursor: pointer;
	}
	.btn-ghost:hover:not(:disabled) { background: var(--bg-elevated); color: var(--text-primary); }
	.btn-ghost.btn-xs { padding: 3px 7px; }
	.btn-ghost.danger-ghost:hover { color: #ef4444; }

	.btn-danger {
		display: inline-flex; align-items: center; gap: 6px;
		padding: 7px 14px; font-size: 12px; font-weight: 600;
		background: #ef4444; color: white; border: none;
		border-radius: var(--radius-sm); cursor: pointer;
	}
	.btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-danger-outline {
		display: inline-flex; align-items: center; gap: 5px;
		padding: 6px 12px; font-size: 12px; font-weight: 600; color: #ef4444;
		background: transparent; border: 1px solid color-mix(in srgb, #ef4444 50%, transparent);
		border-radius: var(--radius-sm); cursor: pointer; transition: all var(--transition-fast);
	}
	.btn-danger-outline:hover { background: color-mix(in srgb, #ef4444 10%, transparent); border-color: #ef4444; }

	/* ── Misc ── */
	.mono { font-family: var(--font-mono); font-size: 11px; }
	.loading-row { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 13px; padding: 24px 0; }

	.err-msg {
		font-size: 12px; color: #ef4444; padding: 8px 10px;
		background: color-mix(in srgb, #ef4444 8%, transparent);
		border: 1px solid color-mix(in srgb, #ef4444 25%, transparent);
		border-radius: var(--radius-sm);
	}
	.ok-msg {
		font-size: 12px; color: #22c55e; padding: 8px 10px;
		background: color-mix(in srgb, #22c55e 8%, transparent);
		border: 1px solid color-mix(in srgb, #22c55e 25%, transparent);
		border-radius: var(--radius-sm);
	}
	.warn-msg {
		font-size: 12px; color: #f59e0b; padding: 8px 10px;
		background: color-mix(in srgb, #f59e0b 8%, transparent);
		border: 1px solid color-mix(in srgb, #f59e0b 25%, transparent);
		border-radius: var(--radius-sm); line-height: 1.6;
	}
	.mono-inline {
		font-family: var(--font-mono); font-size: 11px;
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
	}
	.empty-state {
		font-size: 13px; color: var(--text-dim); text-align: center; padding: 24px;
		display: flex; flex-direction: column; gap: 8px; line-height: 1.7;
	}
	.empty-sub { font-size: 11px; display: block; }

	/* ── Spinners ── */
	.spinner-sm {
		width: 16px; height: 16px; border: 2px solid var(--border);
		border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite;
	}
	.spin-xs-w {
		width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3);
		border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite;
	}
	.spin-xs {
		width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3);
		border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite;
	}

	/* ── Delete modal ── */
	.modal-backdrop {
		position: absolute; inset: 0; background: rgba(0,0,0,0.55);
		display: flex; align-items: center; justify-content: center; z-index: 30; padding: 16px;
	}
	.modal-card {
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius-md); width: 100%; max-width: 400px;
		box-shadow: 0 8px 32px rgba(0,0,0,0.4);
	}
	.modal-header {
		display: flex; align-items: center; gap: 8px;
		padding: 13px 16px; border-bottom: 1px solid var(--border);
		font-size: 13px; font-weight: 700; color: var(--text-primary);
	}
	.modal-body { padding: 16px; display: flex; flex-direction: column; gap: 10px; }
	.modal-footer {
		display: flex; justify-content: flex-end; gap: 8px;
		padding: 12px 16px; border-top: 1px solid var(--border);
	}
	.modal-warning { font-size: 12px; color: var(--text-primary); line-height: 1.5; margin: 0; }
	.modal-confirm-field { display: flex; flex-direction: column; gap: 5px; }
	.modal-confirm-label { font-size: 11px; color: var(--text-muted); }
	.code {
		font-family: var(--font-mono); font-size: 11px;
		background: var(--bg-elevated); padding: 1px 5px; border-radius: 3px;
	}
	.modal-input {
		padding: 7px 9px; font-size: 12px; font-family: var(--font-mono);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		background: var(--bg-elevated); color: var(--text-primary); outline: none;
	}
	.modal-input:focus { border-color: #ef4444; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
