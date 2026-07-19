<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import {
		Zap, Globe, Plus, Trash2, RefreshCw, CheckCircle, XCircle,
		GitBranch, AlertTriangle, ExternalLink, Code2, FileText,
		Terminal, Copy, ChevronRight, X, RotateCcw, BookOpen, Settings, Key
	} from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { uiStore } from '$lib/stores/ui.store';
	import { formatDistanceToNow } from 'date-fns';
	import EdgeFnDomainAddPanel from './resources/EdgeFnDomainAddPanel.svelte';
	import LogViewerOverlay from '$lib/components/LogViewerOverlay.svelte';
	import type { LogColumn } from '$lib/components/LogViewerOverlay.svelte';
	import MonitorViewOverlay from '$lib/components/MonitorViewOverlay.svelte';
	import EnvManagerOverlay from '$lib/components/EnvManagerOverlay.svelte';
	import { EditorView, basicSetup } from 'codemirror';
	import { javascript } from '@codemirror/lang-javascript';
	import { oneDark } from '@codemirror/theme-one-dark';
	import { EditorState } from '@codemirror/state';
	import type { GitProvider } from '$lib/api/types';
	import GitSettingsSection from '$lib/components/GitSettingsSection.svelte';

	interface Props {
		groupId:    string;
		orgId:      string;
		projectId:  string;
		serviceId?: string;
		onDeleted?: () => void;
	}

	let { groupId, orgId, projectId, serviceId, onDeleted }: Props = $props();

	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return { destroy() { node.remove(); } };
	}

	// ── Types ──────────────────────────────────────────────────────────────────

	type Tab = 'overview' | 'functions' | 'git' | 'domains' | 'danger';

	interface Group {
		id: string; org_id: string; project_id: string | null;
		provider: string; repo_url: string; branch: string;
		webhook_secret: string;
		auto_deploy: boolean; deploy_strategy: string; deploy_tag_pattern: string | null;
		git_provider_id: string | null;
		last_deployed_sha: string | null; created_at: string;
	}
	interface EFn {
		id: string; name: string; runtime: string; status: string;
		last_deployed_at: string | null; public_url: string;
	}
	interface EFnDetail extends EFn {
		env_vars: Record<string, string>;
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

	let canDelete = $state<boolean>(true);

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

	// Logs overlay (powered by LogViewerOverlay)
	let logsOverlayOpen = $state(false);
	let logsOverlayFn   = $state<EFn | null>(null);

	// Monitor overlay
	let monitorOpen = $state(false);

	// Env overlay
	let envOpen = $state(false);

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

	// Git tab
	let autoDeployEnabled  = $state(true);
	let deployStrategy     = $state('push');
	let deployTagPattern   = $state('');
	let deployBranch       = $state('');
	let gitSaving          = $state(false);
	let gitSaveOk          = $state(false);
	let gitSaveError       = $state('');
	let webhookCopied      = $state(false);

	// Git account (provider) linking
	let orgGitProviders     = $state<GitProvider[]>([]);
	let loadingGitProviders = $state(false);
	let gitProviderId       = $state<string>('');
	let gitProviderSaving   = $state(false);
	let gitProviderError    = $state('');
	let gitProviderSuccess  = $state('');

	// Per-function env vars
	let fnDetails    = $state<Record<string, EFnDetail>>({});
	let fnEnvLoading = $state<Record<string, boolean>>({});
	let fnEnvEditing = $state<Record<string, Record<string, string>>>({});
	let fnEnvSaving  = $state<Record<string, boolean>>({});
	let fnEnvOk      = $state<Record<string, boolean>>({});
	let fnEnvError   = $state<Record<string, string>>({});

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
			for (const fn of functions) {
				if (!fnDeployments[fn.id]) loadFnDeployments(fn.id);
			}
			if (functions.length > 0 && !expandedFnId) {
				expandedFnId = functions[0].id;
			}
		}
		if (tab === 'git' && group) {
			autoDeployEnabled = group.auto_deploy;
			deployStrategy    = group.deploy_strategy;
			deployTagPattern  = group.deploy_tag_pattern ?? '';
			deployBranch      = group.branch;
			gitProviderId     = group.git_provider_id ?? '';
			gitSaveOk = false; gitSaveError = '';
			gitProviderError = ''; gitProviderSuccess = '';
			loadGitProviders();
		}
	}

	// ── Git tab ────────────────────────────────────────────────────────────────

	async function loadGitProviders() {
		if (orgGitProviders.length > 0) return;
		loadingGitProviders = true;
		const res = await api.listGitProviders(orgId);
		if (res.data) orgGitProviders = res.data;
		loadingGitProviders = false;
	}

	async function saveGitProvider() {
		gitProviderSaving = true; gitProviderError = ''; gitProviderSuccess = '';
		const providerVal = gitProviderId === '' ? null : gitProviderId;
		const res = await api.put<{ updated: boolean; group: Group }>(`/orgs/${orgId}/edge-functions/groups/${groupId}`, {
			git_provider_id: providerVal,
		});
		gitProviderSaving = false;
		if (res.error) { gitProviderError = res.error.message; return; }
		// Use the server-echoed group to confirm exactly what was saved.
		if (res.data?.group) {
			group = res.data.group;
			gitProviderId = group.git_provider_id ?? '';
		} else if (group) {
			group = { ...group, git_provider_id: providerVal };
		}
		gitProviderSuccess = 'Git account linked.';
		setTimeout(() => { gitProviderSuccess = ''; }, 3000);
	}

	async function saveGitSettings() {
		gitSaving = true; gitSaveOk = false; gitSaveError = '';
		const res = await api.put(`/orgs/${orgId}/edge-functions/groups/${groupId}`, {
			auto_deploy:        autoDeployEnabled,
			deploy_strategy:    deployStrategy,
			deploy_tag_pattern: deployStrategy === 'tag' ? (deployTagPattern || null) : null,
			branch:             deployBranch || undefined,
		});
		gitSaving = false;
		if (res.error) { gitSaveError = res.error.message; return; }
		gitSaveOk = true;
		setTimeout(() => { gitSaveOk = false; }, 3000);
		await load();
	}

	async function copyWebhookUrl() {
		if (!group) return;
		const origin = window.location.origin;
		const url = `${origin}/api/webhooks/${group.provider}/fn/${group.id}/${group.webhook_secret}`;
		await navigator.clipboard.writeText(url);
		webhookCopied = true;
		setTimeout(() => { webhookCopied = false; }, 1500);
	}

	// ── Per-function env vars ─────────────────────────────────────────────────

	async function loadFnDetail(fnId: string) {
		if (fnDetails[fnId]) return;
		fnEnvLoading = { ...fnEnvLoading, [fnId]: true };
		const res = await api.get<EFnDetail>(`/orgs/${orgId}/edge-functions/${fnId}`);
		fnEnvLoading = { ...fnEnvLoading, [fnId]: false };
		if (res.data) {
			fnDetails = { ...fnDetails, [fnId]: res.data };
			fnEnvEditing = { ...fnEnvEditing, [fnId]: { ...res.data.env_vars } };
		}
	}

	function addEnvVar(fnId: string) {
		const env = { ...(fnEnvEditing[fnId] ?? {}) };
		env[''] = '';
		fnEnvEditing = { ...fnEnvEditing, [fnId]: env };
	}

	function removeEnvVar(fnId: string, key: string) {
		const env = { ...(fnEnvEditing[fnId] ?? {}) };
		delete env[key];
		fnEnvEditing = { ...fnEnvEditing, [fnId]: env };
	}

	function updateEnvKey(fnId: string, oldKey: string, newKey: string) {
		const env = { ...(fnEnvEditing[fnId] ?? {}) };
		const val = env[oldKey] ?? '';
		delete env[oldKey];
		env[newKey] = val;
		fnEnvEditing = { ...fnEnvEditing, [fnId]: env };
	}

	function updateEnvVal(fnId: string, key: string, val: string) {
		fnEnvEditing = { ...fnEnvEditing, [fnId]: { ...(fnEnvEditing[fnId] ?? {}), [key]: val } };
	}

	async function saveFnEnvVars(fnId: string) {
		fnEnvSaving = { ...fnEnvSaving, [fnId]: true };
		fnEnvError  = { ...fnEnvError,  [fnId]: '' };
		const res = await api.put(`/orgs/${orgId}/edge-functions/${fnId}`, {
			env_vars: fnEnvEditing[fnId] ?? {},
		});
		fnEnvSaving = { ...fnEnvSaving, [fnId]: false };
		if (res.error) { fnEnvError = { ...fnEnvError, [fnId]: res.error.message }; return; }
		fnEnvOk = { ...fnEnvOk, [fnId]: true };
		const updated = { ...fnDetails };
		delete updated[fnId];
		fnDetails = updated;
		setTimeout(() => { fnEnvOk = { ...fnEnvOk, [fnId]: false }; }, 2500);
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
			loadFnDetail(fnId);
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

	function openLogs(fn: EFn) {
		logsOverlayFn   = fn;
		logsOverlayOpen = true;
	}

	function closeLogs() {
		logsOverlayOpen = false;
	}

	// Invocation log table columns
	const invocationLogColumns: LogColumn[] = [
		{ key: 'logged_at',   label: 'Time',     width: '130px', format: (v) => formatTime(v) },
		{ key: 'method',      label: 'Method',   width: '70px',  mono: true },
		{ key: 'path',        label: 'Path',     mono: true },
		{ key: 'status_code', label: 'Status',   width: '65px',  mono: true,
			color: (row) => row.status_code < 300 ? '#22c55e' : row.status_code < 400 ? '#f59e0b' : '#ef4444' },
		{ key: 'duration_ms', label: 'Duration', width: '80px',  format: (v) => `${v}ms` },
		{ key: 'error',       label: 'Error',    format: (v) => v ?? '' },
	];

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

<!-- ── Invocation logs overlay ───────────────────────────────────────────────── -->
<div use:portal>
	<LogViewerOverlay
		open={logsOverlayOpen}
		title={logsOverlayFn?.name ?? 'Invocation Logs'}
		subtitle="invocation logs"
		columns={invocationLogColumns}
		fetchFn={async (_tail) => {
			if (!logsOverlayFn) return [];
			const res = await api.get<{ items: InvocationLog[] }>(
				`/orgs/${orgId}/edge-functions/${logsOverlayFn.id}/logs`
			);
			if (res.error) throw new Error(res.error.message);
			return res.data?.items ?? [];
		}}
		resetKey={logsOverlayFn?.id ?? ''}
		emptyMessage="No invocations recorded yet."
		onClose={closeLogs}
	/>
</div>

<!-- ── Monitor overlay ────────────────────────────────────────────────────────── -->
{#if serviceId}
	<div use:portal>
		<MonitorViewOverlay
			open={monitorOpen}
			onClose={() => { monitorOpen = false; }}
			{serviceId}
		/>
	</div>
{/if}

<!-- ── Env overlay ───────────────────────────────────────────────────────────── -->
{#if serviceId}
	<div use:portal>
		<EnvManagerOverlay
			open={envOpen}
			onClose={() => { envOpen = false; }}
			{serviceId}
			{projectId}
		/>
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
						Type the branch name <code class="modal-confirm-code">{group?.branch}</code> to confirm
					</label>
					<input class="modal-confirm-input" type="text" placeholder={group?.branch ?? ''}
						bind:value={deleteInput} autocomplete="off" />
				</div>
				{#if deleteError}<div class="form-error">{deleteError}</div>{/if}
			</div>
			<div class="modal-footer">
				<button class="btn btn-ghost" onclick={() => { showDeleteModal = false; deleteInput = ''; }}
					disabled={isDeleting}>Cancel</button>
				<button class="btn btn-danger" disabled={!deleteValid || isDeleting} onclick={deleteGroup}>
					{#if isDeleting}<div class="spinner-xs"></div> Deleting…{:else}<Trash2 size={13} /> Delete{/if}
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
		<div class="form-error">{loadError}</div>
	{:else if group}

		<div class="tabs">
			{#each [['overview','Overview'],['functions','Functions'],['git','Git'],['domains','Domains']] as [id, label]}
				<button class="tab" class:active={activeTab === id}
					class:tab-danger={id === 'danger'}
					onclick={() => switchTab(id as Tab)}>{label}
					{#if id === 'functions' && functions.length > 0}
						<span class="tab-count">{functions.length}</span>
					{/if}
				</button>
			{/each}
		</div>

		<div class="tab-content">
		<!-- ── Overview ── -->
		{#if activeTab === 'overview'}
			<section class="section">
				<div class="info-card">
					<div class="info-card-header">
						<Zap size={13} />
						<span>Edge Function Group</span>
					</div>
					<div class="info-card-body">
						<div class="info-row">
							<span class="info-key">Repository</span>
							<code class="info-val mono">{group.repo_url}</code>
						</div>
						<div class="info-row">
							<span class="info-key">Branch</span>
							<code class="info-val mono">{group.branch}</code>
						</div>
						<div class="info-row">
							<span class="info-key">Provider</span>
							<span class="info-val">{group.provider}</span>
						</div>
						<div class="info-row">
							<span class="info-key">Last SHA</span>
							<code class="info-val mono">{group.last_deployed_sha ? group.last_deployed_sha.slice(0, 7) : '—'}</code>
						</div>
						<div class="info-row">
							<span class="info-key">Functions</span>
							<span class="info-val">{functions.length}</span>
						</div>
						<div class="info-row">
							<span class="info-key">Created</span>
							<span class="info-val">{formatTime(group.created_at)}</span>
						</div>
					</div>
				</div>
			</section>

			<section class="section">
				{#if redeployError}<div class="form-error">{redeployError}</div>{/if}
				{#if redeployOk}
					{#if lastReport}
						{#if lastReport.deployed.length > 0}
							<div class="form-success">
								✓ {lastReport.deployed.length} function{lastReport.deployed.length !== 1 ? 's' : ''} deployed
								{#if lastReport.failed.length > 0} · {lastReport.failed.length} failed{/if}
							</div>
						{:else if lastReport.failed.length > 0}
							<div class="form-error">
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
						<div class="form-success">Redeploy triggered.</div>
					{/if}
				{/if}
				<div class="overview-actions">
					<button class="btn btn-primary" onclick={redeploy} disabled={redeploying}>
						{#if redeploying}<div class="spinner-xs"></div> Redeploying…
						{:else}<RefreshCw size={13} /> Redeploy Now{/if}
					</button>
					{#if serviceId}
						<button class="btn btn-secondary" onclick={() => { monitorOpen = true; }}>
							Monitor
						</button>
						<button class="btn btn-secondary" onclick={() => { envOpen = true; }}>
							Env Vars
						</button>
					{/if}
					<a class="btn btn-secondary" href="/docs/edge-functions" target="_blank" rel="noopener">
						<BookOpen size={12} /> Docs
					</a>
				</div>
			</section>
			<section class="section">
				<!-- Danger zone -->
				{#if canDelete}
					<div class="danger-zone">
						<div class="danger-header">
							<AlertTriangle size={13} />
							<span>Danger Zone</span>
						</div>
						<div class="danger-body">
							<div class="danger-row">
								<div class="danger-info">
									<span class="danger-title">Delete this service</span>
									<span class="danger-desc">Stops the service and permanently removes all data.</span>
								</div>
								<button
										class="btn btn-danger-outline btn-sm"
										onclick={() => {  }}
								>
									<Trash2 size={12} />
									Delete
								</button>
							</div>
						</div>
					</div>
				{/if}
			</section>
		{/if}

		<!-- ── Functions ── -->
		{#if activeTab === 'functions'}
			{#if functions.length === 0}
				<div class="empty-state">
					No functions deployed yet.
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
							<button class="fn-card-header" onclick={() => toggleFn(fn.id)}>
								<div class="fn-dot" class:active={fn.status === 'active'}></div>
								<span class="fn-name mono">{fn.name}</span>
								<span class="fn-rt">{fn.runtime}</span>
								<span class="fn-deployed">{formatTime(fn.last_deployed_at)}</span>
								<ChevronRight size={13} class={expanded ? 'fn-chevron rotated' : 'fn-chevron'} />
							</button>

							{#if expanded}
								<div class="fn-detail">
									<!-- Status + URL -->
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
										<button class="btn btn-secondary btn-sm" onclick={() => openCode(fn)}>
											<Code2 size={12} /> View Code
										</button>
										<button class="btn btn-secondary btn-sm" onclick={() => openLogs(fn)}>
											<FileText size={12} /> Invocation Logs
										</button>
									</div>

									<!-- Deployment History -->
									<div class="dep-history">
										<div class="dep-history-title">
											<GitBranch size={11} /> Deployment History
										</div>
										{#if fnDeploymentsLoading[fn.id]}
											<div class="dep-loading"><div class="spinner-xs-inline"></div> Loading…</div>
										{:else if !fnDeployments[fn.id] || fnDeployments[fn.id].length === 0}
											<div class="dep-empty">No deployments recorded.</div>
										{:else}
											{#if rollbackError[fn.id]}
												<div class="dep-error">{rollbackError[fn.id]}</div>
											{/if}
											<div class="dep-list">
												{#each fnDeployments[fn.id] as dep (dep.id)}
													<div class="ver-block" class:ver-live={dep.status === 'live'}>
														<div class="ver-header">
															<div class="ver-label-row">
																<span class="ver-dot" class:ver-dot-live={dep.status === 'live'}></span>
																<span class="ver-label mono">{dep.version}</span>
																{#if dep.status === 'live'}<span class="ver-latest">latest</span>{/if}
																{#if dep.commit_sha}
																	<span class="ver-sha mono">{dep.commit_sha.slice(0, 7)}</span>
																{/if}
																<span class="ver-time">{formatTime(dep.created_at)}</span>
															</div>
															<div class="dep-btns">
																<button class="dep-btn" onclick={() => openCode(fn, dep)}>
																	<Code2 size={10} /> Code
																</button>
																{#if dep.status !== 'live'}
																	<button
																		class="dep-btn dep-btn-restore"
																		disabled={rollingBackId === dep.id}
																		onclick={() => rollbackDeployment(fn, dep)}
																	>
																		{#if rollingBackId === dep.id}
																			<div class="spinner-xs-inline"></div>
																		{:else}
																			<RotateCcw size={10} /> Restore
																		{/if}
																	</button>
																{/if}
															</div>
														</div>
														{#if dep.files && dep.files.length > 0}
															<div class="ver-files">
																{#each dep.files as file}
																	<div class="ver-file">
																		<FileText size={9} />
																		<span class="mono">{file}</span>
																	</div>
																{/each}
															</div>
														{/if}
													</div>
												{/each}
											</div>
										{/if}
									</div>

									<!-- Env Vars -->
									<div class="dep-history">
										<div class="dep-history-title">
											<Key size={11} /> Environment Variables
											<button class="dep-btn" style="margin-left:auto" onclick={() => addEnvVar(fn.id)}>
												<Plus size={10} /> Add
											</button>
										</div>
										{#if fnEnvLoading[fn.id]}
											<div class="dep-loading"><div class="spinner-xs-inline"></div> Loading…</div>
										{:else}
											{@const envEntries = Object.entries(fnEnvEditing[fn.id] ?? {})}
											{#if envEntries.length === 0}
												<div class="dep-empty">No env vars set.</div>
											{:else}
												<div class="env-rows">
													{#each envEntries as [k, v] (k)}
														<div class="env-row">
															<input class="env-input env-key mono" placeholder="KEY"
																value={k}
																onchange={(e) => updateEnvKey(fn.id, k, (e.target as HTMLInputElement).value)} />
															<span class="env-eq">=</span>
															<input class="env-input env-val mono" placeholder="value"
																value={v}
																oninput={(e) => updateEnvVal(fn.id, k, (e.target as HTMLInputElement).value)} />
															<button class="env-rm" onclick={() => removeEnvVar(fn.id, k)}>
																<X size={10} />
															</button>
														</div>
													{/each}
												</div>
											{/if}
											{#if fnEnvError[fn.id]}<div class="form-error" style="margin:6px 12px">{fnEnvError[fn.id]}</div>{/if}
											{#if fnEnvOk[fn.id]}<div class="form-success" style="margin:6px 12px">Saved.</div>{/if}
											<div style="display:flex;justify-content:flex-end;padding:8px 12px">
												<button class="btn btn-primary btn-sm" disabled={fnEnvSaving[fn.id]} onclick={() => saveFnEnvVars(fn.id)}>
													{#if fnEnvSaving[fn.id]}<div class="spinner-xs"></div> Saving…{:else}<CheckCircle size={11} /> Save{/if}
												</button>
											</div>
										{/if}
									</div>

									<!-- How to invoke -->
									<div class="dep-history">
										<div class="dep-history-title"><Terminal size={11} /> How to invoke</div>
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
												<div class="invoke-label">Custom path</div>
												<pre class="invoke-code">{`curl "${fn.public_url}/your-path?param=value"`}</pre>
											</div>
										{:else}
											<div class="dep-empty">Deploy this function first to get a public URL.</div>
										{/if}
									</div>
								</div>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		{/if}

		<!-- ── Git ── -->
		{#if activeTab === 'git'}
			<GitSettingsSection
				providers={orgGitProviders}
				loadingProviders={loadingGitProviders}
				bind:providerId={gitProviderId}
				providerDefaultLabel="No account linked"
				onSaveProvider={saveGitProvider}
				providerSaving={gitProviderSaving}
				providerError={gitProviderError}
				providerSuccess={gitProviderSuccess}
				showAutoDeployToggle={true}
				bind:autoDeploy={autoDeployEnabled}
				bind:strategy={deployStrategy}
				bind:branch={deployBranch}
				bind:tagPattern={deployTagPattern}
				deployDisabled={!autoDeployEnabled}
				onSave={saveGitSettings}
				saving={gitSaving}
				saveOk={gitSaveOk}
				saveError={gitSaveError}
				webhookUrl="{window.location.origin}/api/webhooks/{group.provider}/fn/{group.id}/{group.webhook_secret}"
				showProviderTabs={false}
				webhookCopied={webhookCopied}
				onCopyWebhook={copyWebhookUrl}
			/>

			<!-- Repo info (read-only) — more detailed than generic card -->
			<div class="git-card" style="margin-top:12px">
				<div class="git-card-title">Repository</div>
				<div class="git-repo-info">
					<div class="git-repo-row">
						<span class="git-repo-label">Provider</span>
						<span class="git-repo-val">{group.provider}</span>
					</div>
					<div class="git-repo-row">
						<span class="git-repo-label">URL</span>
						<code class="git-repo-val mono">{group.repo_url}</code>
					</div>
					{#if group.last_deployed_sha}
						<div class="git-repo-row">
							<span class="git-repo-label">Last SHA</span>
							<code class="git-repo-val mono">{group.last_deployed_sha.slice(0, 7)}</code>
						</div>
					{/if}
				</div>
			</div>
		{/if}

		<!-- ── Domains ── -->
		{#if activeTab === 'domains'}
			<section class="section">
				<div class="section-head">
					<span class="section-title">Custom Domains</span>
					<button class="btn btn-secondary btn-sm" onclick={openAddDomainPanel}>
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
									<button class="btn btn-ghost btn-xs" onclick={() => checkDns(domain)}
										disabled={dnsState[domain.id] === 'checking'}>
										{dnsState[domain.id] === 'checking' ? '…' : 'DNS'}
									</button>
									<button class="btn btn-ghost btn-xs danger-ghost" onclick={() => removeDomain(domain.id)}>
										<Trash2 size={11} />
									</button>
								</div>
							</div>
						{/each}
					</div>
				{/if}

				<div class="dns-hint">
					<strong>DNS:</strong> Point your domain's A record to the Shipyard server IP,
					or CNAME to your Shipyard hostname.
				</div>
			</section>
		{/if}
		</div><!-- .tab-content -->

	{/if}
</div>

<style>
	/* ── Panel body ── */
	.panel-body {
		padding: 16px 16px 0;
		display: flex;
		flex-direction: column;
		gap: 0;
		height: 100%;
		overflow: hidden;
	}

	/* ── Tabs ── */
	.tabs {
		display: flex;
		flex-shrink: 0;
		gap: 2px;
		border-bottom: 1px solid var(--border);
		margin-bottom: 0;
		overflow-x: auto;
		scrollbar-width: none;
	}
	.tabs::-webkit-scrollbar { display: none; }

	.tab-content {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 16px 0;
	}

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

	/* ── Info card (overview) ── */
	.info-card {
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		overflow: hidden;
	}
	.info-card-header {
		display: flex; align-items: center; gap: 7px;
		padding: 9px 12px;
		font-size: 11px; font-weight: 700; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.07em;
		border-bottom: 1px solid var(--border);
		background: var(--bg-base);
	}
	.info-card-body { padding: 4px 0; }
	.info-row {
		display: flex; align-items: center; padding: 6px 12px;
		font-size: 12px; border-bottom: 1px solid var(--border);
	}
	.info-row:last-child { border-bottom: none; }
	.info-key { color: var(--text-muted); width: 100px; flex-shrink: 0; }
	.info-val { color: var(--text-primary); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }

	/* ── Git repo card (remaining after GitSettingsSection refactor) ── */
	.git-card {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 14px 16px;
		display: flex; flex-direction: column; gap: 10px;
	}
	.git-card-title { font-size: 13px; font-weight: 700; color: var(--text-primary); }
	.git-repo-info { display: flex; flex-direction: column; gap: 0; }
	.git-repo-row {
		display: flex; align-items: center; gap: 10px;
		padding: 5px 0; border-bottom: 1px solid var(--border); font-size: 12px;
	}
	.git-repo-row:last-child { border-bottom: none; }
	.git-repo-label { color: var(--text-muted); width: 72px; flex-shrink: 0; font-size: 11px; }
	.git-repo-val { color: var(--text-primary); font-weight: 500; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }

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

	.fn-detail {
		border-top: 1px solid var(--border);
		padding: 12px;
		display: flex; flex-direction: column; gap: 10px;
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

	/* ── Deployment history / shared section card ── */
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

	.ver-block {
		border-bottom: 1px solid var(--border);
		padding: 8px 12px;
	}
	.ver-block:last-child { border-bottom: none; }
	.ver-block.ver-live {
		border-left: 2px solid var(--accent);
		padding-left: 10px;
		background: color-mix(in srgb, var(--accent) 3%, transparent);
	}

	.ver-header {
		display: flex; align-items: center; justify-content: space-between;
		gap: 8px;
	}
	.ver-label-row {
		display: flex; align-items: center; gap: 7px; flex: 1; min-width: 0;
	}
	.ver-dot {
		width: 6px; height: 6px; border-radius: 50%; flex-shrink: 0;
		background: var(--text-dim);
	}
	.ver-dot-live { background: #22c55e; box-shadow: 0 0 4px #22c55e66; }
	.ver-label { font-size: 12px; font-weight: 700; color: var(--text-primary); flex-shrink: 0; }
	.ver-latest {
		font-size: 9px; font-weight: 700; padding: 1px 6px; border-radius: 99px;
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		color: var(--accent); text-transform: uppercase; letter-spacing: 0.06em; flex-shrink: 0;
	}
	.ver-sha { font-size: 10px; color: var(--text-dim); flex-shrink: 0; }
	.ver-time { font-size: 10px; color: var(--text-dim); }

	.ver-files {
		display: flex; flex-direction: column; gap: 2px;
		margin-top: 6px; padding-left: 14px;
	}
	.ver-file {
		display: flex; align-items: center; gap: 5px;
		font-size: 11px; color: var(--text-muted);
	}
	:global(.ver-file svg) { color: var(--text-dim); flex-shrink: 0; }

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

	/* ── Env vars ── */
	.env-rows { display: flex; flex-direction: column; }
	.env-row {
		display: flex; align-items: center; gap: 4px; padding: 5px 10px;
		border-bottom: 1px solid var(--border);
	}
	.env-row:last-child { border-bottom: none; }
	.env-input {
		padding: 4px 7px; font-size: 11px;
		border: 1px solid var(--border); border-radius: 4px;
		background: var(--bg-surface); color: var(--text-primary); outline: none;
		transition: border-color var(--transition-fast);
	}
	.env-input:focus { border-color: var(--accent); }
	.env-key { width: 120px; flex-shrink: 0; }
	.env-val { flex: 1; min-width: 0; }
	.env-eq { font-size: 12px; color: var(--text-dim); font-family: var(--font-mono); flex-shrink: 0; }
	.env-rm {
		display: flex; align-items: center; justify-content: center;
		width: 22px; height: 22px; border-radius: 4px;
		background: none; border: none; cursor: pointer;
		color: var(--text-dim); flex-shrink: 0;
		transition: all var(--transition-fast);
	}
	.env-rm:hover { color: #ef4444; background: color-mix(in srgb, #ef4444 10%, transparent); }

	/* ── Invoke section (inside dep-history) ── */
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

	/* ── Danger zone ── */
	.danger-zone {
		border: 1px solid color-mix(in srgb, #ef4444 40%, var(--border));
		border-radius: var(--radius-sm); overflow: hidden;
	}
	.danger-header {
		display: flex; align-items: center; gap: 6px;
		padding: 8px 12px;
		background: color-mix(in srgb, #ef4444 8%, transparent);
		color: #ef4444;
		font-size: 11px; font-weight: 700;
		text-transform: uppercase; letter-spacing: 0.05em;
	}
	.danger-body { padding: 12px; }
	.danger-row {
		display: flex; align-items: center; gap: 12px; justify-content: space-between;
	}
	.danger-info { display: flex; flex-direction: column; gap: 3px; }
	.danger-title { font-size: 12px; font-weight: 600; color: var(--text-primary); }
	.danger-desc { font-size: 11px; color: var(--text-muted); line-height: 1.5; }

	/* ── Overlays ── */
	.overlay-backdrop {
		position: fixed; inset: 0;
		background: rgba(0,0,0,0.7);
		display: flex; align-items: stretch; justify-content: center;
		z-index: 200;
		padding: 24px;
	}

	.code-overlay {
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

	.editor-wrap { flex: 1; overflow: auto; height: 100%; }

	:global(.editor-wrap .cm-editor) {
		height: 100%; font-size: 13px; font-family: var(--font-mono);
	}
	:global(.editor-wrap .cm-scroller) { overflow: auto; }

	.overlay-loading {
		display: flex; align-items: center; gap: 10px;
		padding: 32px; color: var(--text-muted); font-size: 13px;
	}

	/* ── Buttons ── */
	.overview-actions { display: flex; gap: 8px; align-items: center; }

	.btn {
		display: inline-flex; align-items: center; gap: 6px;
		font-size: 12px; font-weight: 600; font-family: var(--font-sans);
		border-radius: var(--radius-sm); cursor: pointer;
		transition: all var(--transition-fast); border: none;
		padding: 7px 14px;
	}
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-primary {
		background: var(--accent); color: white;
	}
	.btn-primary:hover:not(:disabled) { opacity: 0.88; }

	.btn-secondary {
		background: var(--bg-elevated); color: var(--text-primary);
		border: 1px solid var(--border);
	}
	.btn-secondary:hover:not(:disabled) { border-color: var(--border-hover); }

	.btn-ghost {
		background: none; color: var(--text-muted); border: none;
		padding: 5px 10px; font-weight: 500;
	}
	.btn-ghost:hover:not(:disabled) { background: var(--bg-elevated); color: var(--text-primary); }

	.btn-danger {
		background: #ef4444; color: white;
	}
	.btn-danger:hover:not(:disabled) { opacity: 0.88; }

	.btn-danger-outline {
		background: transparent; color: #ef4444;
		border: 1px solid color-mix(in srgb, #ef4444 50%, transparent);
	}
	.btn-danger-outline:hover:not(:disabled) {
		background: color-mix(in srgb, #ef4444 10%, transparent);
		border-color: #ef4444;
	}

	.btn-sm { padding: 5px 10px; font-size: 11px; }
	.btn-xs { padding: 3px 7px; font-size: 11px; }
	.danger-ghost:hover { color: #ef4444 !important; }

	/* ── Misc ── */
	.mono { font-family: var(--font-mono); font-size: 11px; }
	.loading-row { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 13px; padding: 24px 0; }

	.form-error {
		font-size: 12px; color: #ef4444; padding: 8px 10px;
		background: color-mix(in srgb, #ef4444 8%, transparent);
		border: 1px solid color-mix(in srgb, #ef4444 25%, transparent);
		border-radius: var(--radius-sm);
	}
	.form-success {
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
	.spinner-xs {
		display: inline-block; width: 12px; height: 12px;
		border: 2px solid rgba(255,255,255,0.4); border-top-color: white;
		border-radius: 50%; animation: spin 0.7s linear infinite;
	}
	.spinner-xs-inline {
		display: inline-block; width: 10px; height: 10px;
		border: 1.5px solid var(--border); border-top-color: var(--accent);
		border-radius: 50%; animation: spin 0.7s linear infinite;
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
	.modal-confirm-code {
		font-family: var(--font-mono); font-size: 11px;
		background: var(--bg-elevated); padding: 1px 5px; border-radius: 3px;
		border: 1px solid var(--border);
	}
	.modal-confirm-input {
		padding: 7px 9px; font-size: 12px; font-family: var(--font-mono);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		background: var(--bg-elevated); color: var(--text-primary); outline: none;
	}
	.modal-confirm-input:focus { border-color: #ef4444; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
