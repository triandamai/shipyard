<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import {
		Zap, Globe, Plus, Trash2, RefreshCw, CheckCircle, XCircle,
		GitBranch, AlertTriangle, ExternalLink, Code2, FileText,
		Terminal, Copy, ChevronRight, X, RotateCcw, BookOpen, Settings, Key
	} from '@lucide/svelte';
	import {
		Button, Badge, Card, ListRow, Tabs, KeyValueList, TextField, SectionLabel, InlineAlert, Spinner, ActivityList, EmptyState, ConfirmDialog
	} from '$lib/components/ui';
	import type { KeyValueItem, TabItem } from '$lib/components/ui';
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
	let deleteError     = $state('');

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

	const tabs = $derived<TabItem[]>([
		{ id: 'overview',  label: 'Overview' },
		{ id: 'functions', label: 'Functions', badge: functions.length > 0 ? String(functions.length) : undefined },
		{ id: 'git',       label: 'Git' },
		{ id: 'domains',   label: 'Domains' },
	]);

	const overviewItems = $derived<KeyValueItem[]>(group ? [
		{ key: 'Repository', value: group.repo_url, mono: true },
		{ key: 'Branch',     value: group.branch, mono: true },
		{ key: 'Provider',   value: group.provider },
		{ key: 'Last SHA',   value: group.last_deployed_sha ? group.last_deployed_sha.slice(0, 7) : null, mono: true },
		{ key: 'Functions',  value: functions.length },
		{ key: 'Created',    value: formatTime(group.created_at) },
	] : []);

	const repoItems = $derived<KeyValueItem[]>(group ? [
		{ key: 'Provider', value: group.provider },
		{ key: 'URL',      value: group.repo_url, mono: true },
		...(group.last_deployed_sha ? [{ key: 'Last SHA', value: group.last_deployed_sha.slice(0, 7), mono: true }] : []),
	] : []);

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

	// ConfirmDialog owns the typed text and the in-flight state; returning false
	// keeps it open with `deleteError` shown.
	async function deleteGroup(): Promise<boolean> {
		deleteError = '';
		const res = await api.delete(`/orgs/${orgId}/edge-functions/groups/${groupId}`);
		if (res.error) { deleteError = res.error.message; return false; }
		showDeleteModal = false;
		onDeleted?.();
		uiStore.clearPanels();
		return true;
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
					<Button variant="ghost" size="icon" onclick={copyCode} title="Copy code" aria-label="Copy code">
						{#if copied}<CheckCircle size={14} style="color:var(--accent-green)" />{:else}<Copy size={14} />{/if}
					</Button>
					<Button variant="ghost" size="icon" onclick={closeCode} title="Close" aria-label="Close"><X size={14} /></Button>
				</div>
			</div>
			<div class="code-body">
				{#if codeLoading}
					<div class="overlay-loading"><Spinner size={16} /> Loading code…</div>
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

<!-- ── Delete confirmation (portalled to body) ───────────────────────────────── -->
{#if showDeleteModal}
	<div use:portal>
		<ConfirmDialog
			bind:open={showDeleteModal}
			title="Delete Edge Function Group"
			message="Permanently removes the group, all deployed functions, code, invocation logs, and custom domains. This cannot be undone."
			confirmLabel="Delete"
			confirmText={group?.branch ?? ''}
			error={deleteError}
			onConfirm={deleteGroup}
		/>
	</div>
{/if}

<!-- ── Panel body ────────────────────────────────────────────────────────────── -->
<div class="panel-body">
	{#if loading}
		<div class="loading-row"><Spinner size={16} /> Loading…</div>
	{:else if loadError}
		<div role="alert"><InlineAlert tone="error">{loadError}</InlineAlert></div>
	{:else if group}

		<div class="tabs-wrap">
			<Tabs {tabs} value={activeTab} onChange={(id) => switchTab(id as Tab)} ariaLabel="Edge function sections" />
		</div>

		<div class="tab-content">
		<!-- ── Overview ── -->
		{#if activeTab === 'overview'}
			<section class="section">
				<Card padding="4px 14px 12px">
					<ListRow title="Edge Function Group" meta={repoName(group.repo_url)}>
						{#snippet icon()}<Zap size={16} />{/snippet}
					</ListRow>
					<KeyValueList items={overviewItems} keyWidth="100px" />
				</Card>
			</section>

			<section class="section">
				{#if redeployError}<div role="alert"><InlineAlert tone="error">{redeployError}</InlineAlert></div>{/if}
				{#if redeployOk}
					{#if lastReport}
						{#if lastReport.deployed.length > 0}
							<div role="status">
								<InlineAlert tone="success">
									✓ {lastReport.deployed.length} function{lastReport.deployed.length !== 1 ? 's' : ''} deployed
									{#if lastReport.failed.length > 0} · {lastReport.failed.length} failed{/if}
								</InlineAlert>
							</div>
						{:else if lastReport.failed.length > 0}
							<div role="alert">
								<InlineAlert tone="error">
									Deploy ran but {lastReport.failed.length} function{lastReport.failed.length !== 1 ? 's' : ''} failed:
									{lastReport.failed.map(([name, err]) => `${name}: ${err}`).join(', ')}
								</InlineAlert>
							</div>
						{:else}
							<div role="status">
								<InlineAlert tone="warning">
									No functions detected. Ensure your repo has a <code class="mono-inline">functions/</code> directory
									with <code class="mono-inline">.ts</code> or <code class="mono-inline">.js</code> files
									containing <code class="mono-inline">export default</code>.
								</InlineAlert>
							</div>
						{/if}
					{:else}
						<div role="status"><InlineAlert tone="success">Redeploy triggered.</InlineAlert></div>
					{/if}
				{/if}
				<div class="overview-actions">
					<Button size="sm" onclick={redeploy} disabled={redeploying}>
						{#if redeploying}<Spinner size={12} tone="current" /> Redeploying…
						{:else}<RefreshCw size={13} /> Redeploy Now{/if}
					</Button>
					{#if serviceId}
						<Button variant="secondary" size="sm" onclick={() => { monitorOpen = true; }}>Monitor</Button>
						<Button variant="secondary" size="sm" onclick={() => { envOpen = true; }}>Env Vars</Button>
					{/if}
					<Button variant="secondary" size="sm" href="/docs/edge-functions" target="_blank" rel="noopener">
						<BookOpen size={12} /> Docs
					</Button>
				</div>
			</section>
			<section class="section">
				<!-- Danger zone -->
				{#if canDelete}
					<Card tone="danger" padding="12px">
						<div class="danger-header">
							<AlertTriangle size={13} />
							<span>Danger Zone</span>
						</div>
						<div class="danger-row">
							<div class="danger-info">
								<span class="danger-title">Delete this service</span>
								<span class="danger-desc">Stops the service and permanently removes all data.</span>
							</div>
							<Button variant="danger-outline" size="sm" onclick={() => {  }}>
								<Trash2 size={12} />
								Delete
							</Button>
						</div>
					</Card>
				{/if}
			</section>
		{/if}

		<!-- ── Functions ── -->
		{#if activeTab === 'functions'}
			{#if functions.length === 0}
				<EmptyState
					message="No functions deployed yet."
					sub={`Click Redeploy Now on the Overview tab, or push to ${group?.branch ?? 'main'}.`}
				/>
				<p class="empty-note">
					Repo must have a <code class="mono-inline">functions/</code> directory with
					<code class="mono-inline">.ts</code>/<code class="mono-inline">.js</code> files
					that contain <code class="mono-inline">export default</code>.
				</p>
			{:else}
				<ActivityList>
					{#each functions as fn (fn.id)}
						{@const expanded = expandedFnId === fn.id}
						<div class="fn-item">
							<ListRow
									ariaExpanded={expanded}
									onclick={() => toggleFn(fn.id)}
									title={fn.name}
									meta={`${fn.runtime} · ${formatTime(fn.last_deployed_at)}`}
									iconTone={fn.status === 'active' ? 'green' : 'blue'}
								>
									{#snippet icon()}<Zap size={14} />{/snippet}
									{#snippet trailing()}
										<ChevronRight size={13} class={expanded ? 'fn-chevron rotated' : 'fn-chevron'} />
									{/snippet}
								</ListRow>

							{#if expanded}
								<div class="fn-detail">
									<!-- Status + URL -->
									<div class="fn-status-row">
										<Badge tone={fn.status === 'active' ? 'green' : 'neutral'}>{fn.status}</Badge>
										{#if fn.public_url}
											<a class="fn-url mono" href={fn.public_url} target="_blank" rel="noopener">
												{fn.public_url}<ExternalLink size={10} style="margin-left:4px;flex-shrink:0" />
											</a>
										{/if}
									</div>

									<!-- Action buttons -->
									<div class="fn-actions">
										<Button variant="secondary" size="sm" onclick={() => openCode(fn)}>
											<Code2 size={12} /> View Code
										</Button>
										<Button variant="secondary" size="sm" onclick={() => openLogs(fn)}>
											<FileText size={12} /> Invocation Logs
										</Button>
									</div>

									<!-- Deployment History -->
									<div class="dep-history">
										<div class="dep-history-title">
											<GitBranch size={11} /> Deployment History
										</div>
										{#if fnDeploymentsLoading[fn.id]}
											<div class="dep-loading"><Spinner size={10} /> Loading…</div>
										{:else if !fnDeployments[fn.id] || fnDeployments[fn.id].length === 0}
											<div class="dep-empty">No deployments recorded.</div>
										{:else}
											{#if rollbackError[fn.id]}
												<div class="dep-alert" role="alert"><InlineAlert tone="error">{rollbackError[fn.id]}</InlineAlert></div>
											{/if}
											<div class="dep-list">
												{#each fnDeployments[fn.id] as dep (dep.id)}
													<div class="ver-block" class:ver-live={dep.status === 'live'}>
														<div class="ver-header">
															<div class="ver-label-row">
																<span class="ver-dot" class:ver-dot-live={dep.status === 'live'}></span>
																<span class="ver-label mono">{dep.version}</span>
																{#if dep.status === 'live'}<Badge tone="blue">latest</Badge>{/if}
																{#if dep.commit_sha}
																	<span class="ver-sha mono">{dep.commit_sha.slice(0, 7)}</span>
																{/if}
																<span class="ver-time">{formatTime(dep.created_at)}</span>
															</div>
															<div class="dep-btns">
																<Button variant="secondary" size="sm" onclick={() => openCode(fn, dep)}>
																	<Code2 size={10} /> Code
																</Button>
																{#if dep.status !== 'live'}
																	<Button
																		variant="secondary"
																		size="sm"
																		disabled={rollingBackId === dep.id}
																		onclick={() => rollbackDeployment(fn, dep)}
																	>
																		{#if rollingBackId === dep.id}
																			<Spinner size={10} tone="current" />
																		{:else}
																			<RotateCcw size={10} /> Restore
																		{/if}
																	</Button>
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
											<span class="dep-title-action">
												<Button variant="secondary" size="sm" onclick={() => addEnvVar(fn.id)}>
													<Plus size={10} /> Add
												</Button>
											</span>
										</div>
										{#if fnEnvLoading[fn.id]}
											<div class="dep-loading"><Spinner size={10} /> Loading…</div>
										{:else}
											{@const envEntries = Object.entries(fnEnvEditing[fn.id] ?? {})}
											{#if envEntries.length === 0}
												<div class="dep-empty">No env vars set.</div>
											{:else}
												<div class="env-rows">
													{#each envEntries as [k, v] (k)}
														<div class="env-row">
															<div class="env-key">
																<TextField placeholder="KEY" aria-label="Variable name"
																	value={k}
																	onchange={(e) => updateEnvKey(fn.id, k, (e.target as HTMLInputElement).value)} />
															</div>
															<span class="env-eq">=</span>
															<div class="env-val">
																<TextField placeholder="value" aria-label="Variable value"
																	value={v}
																	oninput={(e) => updateEnvVal(fn.id, k, (e.target as HTMLInputElement).value)} />
															</div>
															<Button variant="ghost" size="icon" aria-label="Remove variable" title="Remove variable" onclick={() => removeEnvVar(fn.id, k)}>
																<X size={12} />
															</Button>
														</div>
													{/each}
												</div>
											{/if}
											{#if fnEnvError[fn.id]}<div class="dep-alert" role="alert"><InlineAlert tone="error">{fnEnvError[fn.id]}</InlineAlert></div>{/if}
											{#if fnEnvOk[fn.id]}<div class="dep-alert" role="status"><InlineAlert tone="success">Saved.</InlineAlert></div>{/if}
											<div class="env-save">
												<Button size="sm" disabled={fnEnvSaving[fn.id]} onclick={() => saveFnEnvVars(fn.id)}>
													{#if fnEnvSaving[fn.id]}<Spinner size={12} tone="current" /> Saving…{:else}<CheckCircle size={11} /> Save{/if}
												</Button>
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
				</ActivityList>
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
			<div class="git-repo">
				<SectionLabel>Repository</SectionLabel>
				<Card padding="4px 14px">
					<KeyValueList items={repoItems} keyWidth="72px" />
				</Card>
			</div>
		{/if}

		<!-- ── Domains ── -->
		{#if activeTab === 'domains'}
			<section class="section">
				<div class="section-head">
					<span class="section-title">Custom Domains</span>
					<Button variant="secondary" size="sm" onclick={openAddDomainPanel}>
						<Plus size={12} /> Add Domain
					</Button>
				</div>

				{#if domains.length === 0}
					<EmptyState
						message="No domains configured."
						sub="Add a custom domain to route traffic to your edge functions."
					/>
				{:else}
					<ActivityList>
						{#each domains as domain (domain.id)}
							<div class="domain-item">
								<ListRow title={domain.hostname}>
									{#snippet icon()}<Globe size={14} />{/snippet}
									{#snippet trailing()}
										<div class="domain-actions">
											<Button variant="secondary" size="sm" onclick={() => checkDns(domain)}
												disabled={dnsState[domain.id] === 'checking'}>
												{#if dnsState[domain.id] === 'checking'}<Spinner size={12} tone="current" />{:else}Check DNS{/if}
											</Button>
											<Button variant="ghost" size="icon" aria-label="Remove domain" title="Remove domain" onclick={() => removeDomain(domain.id)}>
												<Trash2 size={13} />
											</Button>
										</div>
									{/snippet}
								</ListRow>
								{#if domain.tls_enabled || dnsState[domain.id] === 'ok' || dnsState[domain.id] === 'fail'}
									<div class="domain-badges">
										{#if domain.tls_enabled}<Badge tone="green">HTTPS</Badge>{/if}
										{#if dnsState[domain.id] === 'ok'}
											<Badge tone="green"><CheckCircle size={10} /> DNS OK</Badge>
										{:else if dnsState[domain.id] === 'fail'}
											<Badge tone="red"><XCircle size={10} /> No DNS</Badge>
										{/if}
									</div>
								{/if}
							</div>
						{/each}
					</ActivityList>
				{/if}

				<InlineAlert tone="info">
					<strong>DNS:</strong> Point your domain's A record to the Shipyard server IP,
					or CNAME to your Shipyard hostname.
				</InlineAlert>
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

	.tab-content {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 16px 0;
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

	.git-repo { margin-top: 12px; }

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
	.ver-dot-live { background: var(--accent-green); box-shadow: 0 0 4px color-mix(in srgb, var(--accent-green) 40%, transparent); }
	.ver-label { font-size: 12px; font-weight: 700; color: var(--text-primary); flex-shrink: 0; }
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

	/* ── Env vars ── */
	.env-rows { display: flex; flex-direction: column; }
	.env-row {
		display: flex; align-items: center; gap: 4px; padding: 5px 10px;
		border-bottom: 1px solid var(--border);
	}
	.env-row:last-child { border-bottom: none; }
	.env-eq { font-size: 12px; color: var(--text-dim); font-family: var(--font-mono); flex-shrink: 0; }
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

	/* ── Domains ── */
	.domain-item + .domain-item { border-top: 1px solid var(--border); }
	.domain-item :global(.ui-list-row) { border-bottom: none; }
	.domain-item :global(.ui-list-row-title) { font-family: var(--font-mono); font-size: 12px; }
	.domain-badges { padding: 0 0 10px 42px; display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
	.domain-actions { display: flex; align-items: center; gap: 4px; }

	/* ── Danger zone ── */
	.danger-header {
		display: flex; align-items: center; gap: 6px;
		color: var(--accent-red); margin-bottom: 10px;
		font-size: 11px; font-weight: 700;
		text-transform: uppercase; letter-spacing: 0.05em;
	}
	.danger-row { display: flex; align-items: center; gap: 12px; justify-content: space-between; }
	.danger-info { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
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
		box-shadow: var(--shadow-lg);
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

	.overview-actions { display: flex; flex-wrap: wrap; gap: 8px; align-items: center; }

	/* ── Tabs / functions (shared components own the chrome) ── */
	.tabs-wrap { flex-shrink: 0; }
	.empty-note { font-size: 11px; color: var(--text-dim); text-align: center; line-height: 1.7; margin: 0; }

	.fn-item + .fn-item { border-top: 1px solid var(--border); }
	.fn-item :global(.ui-list-row) { border-bottom: none; }
	:global(.fn-chevron) { color: var(--text-dim); flex-shrink: 0; transition: transform var(--transition-fast); }
	:global(.fn-chevron.rotated) { transform: rotate(90deg); }

	.fn-detail {
		border-top: 1px solid var(--border);
		padding: 12px 0 6px;
		display: flex; flex-direction: column; gap: 10px;
	}
	.fn-status-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
	.fn-url {
		font-size: 11px; color: var(--text-muted); text-decoration: none;
		display: inline-flex; align-items: center; gap: 3px;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0;
	}
	.fn-url:hover { color: var(--accent); }
	.fn-actions { display: flex; flex-wrap: wrap; gap: 6px; }

	.dep-title-action { margin-left: auto; }
	.dep-alert { margin: 6px 12px 0; }
	.ver-header { flex-wrap: wrap; }
	.dep-btns { display: flex; gap: 4px; flex-shrink: 0; }
	.env-key { width: 120px; flex-shrink: 0; }
	.env-val { flex: 1; min-width: 0; }
	.env-row :global(.ui-textfield) { height: 30px; font-size: 11px; font-family: var(--font-mono); padding: 0 8px; }
	.env-save { display: flex; justify-content: flex-end; padding: 8px 12px; }

	/* ── Misc ── */
	.mono { font-family: var(--font-mono); font-size: 11px; }
	.loading-row { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 13px; padding: 24px 0; }

	.mono-inline {
		font-family: var(--font-mono); font-size: 11px;
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
	}
</style>
