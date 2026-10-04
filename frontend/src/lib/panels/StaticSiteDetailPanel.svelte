<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import {
		Globe, RefreshCw, Upload, Settings2, Play,
		ChevronRight, CheckCircle2, XCircle, Clock, AlertCircle,
		Plus, Trash2, CheckCircle, AlertTriangle
	} from '@lucide/svelte';
	import {
		Button, Badge, Card, ListRow, Tabs, KeyValueList, FormField, TextField, Select,
		InlineAlert, Spinner, SectionLabel, ActivityList, StatusDot, EmptyState, ConfirmDialog
	} from '$lib/components/ui';
	import { toDotStatus } from '$lib/utils/status';
	import type { DotStatus } from '$lib/utils/status';
	import type { KeyValueItem, TabItem } from '$lib/components/ui';
	import { api } from '$lib/api/client';
	import { uiStore } from '$lib/stores/ui.store';
	import DomainAddPanel from './resources/DomainAddPanel.svelte';
	import DeploymentLogsPanel from './DeploymentLogsPanel.svelte';
	import LogViewerOverlay from '$lib/components/LogViewerOverlay.svelte';
	import MonitorViewOverlay from '$lib/components/MonitorViewOverlay.svelte';
	import EnvManagerOverlay from '$lib/components/EnvManagerOverlay.svelte';
	import GitSettingsSection from '$lib/components/GitSettingsSection.svelte';
	import type { StaticSiteConfig, Service, Deployment, Domain } from '$lib/api/types';
	import { formatDistanceToNow } from 'date-fns';

	interface Props {
		serviceId:   string;
		projectId:   string;
		orgId:       string;
		onDeployed?: () => void;
		onDeleted?:  () => void;
	}

	let { serviceId, projectId, orgId, onDeployed, onDeleted }: Props = $props();

	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return { destroy() { node.remove(); } };
	}

	// ── Core state ─────────────────────────────────────────────────────────────
	let config       = $state<StaticSiteConfig | null>(null);
	let service      = $state<Service | null>(null);
	let serviceSlug  = $state('');  // loaded for delete confirmation
	let deployments  = $state<Deployment[]>([]);
	let loading      = $state(true);
	let loadError    = $state('');
	let activeTab    = $state<'overview' | 'config' | 'git' | 'deployments' | 'domains' | 'docs'>('overview');

	// ── Visitor Logs overlay ──────────────────────────────────────────────────
	let logOverlayOpen = $state(false);

	// ── Monitor overlay ────────────────────────────────────────────────────────
	let monitorOpen = $state(false);

	// ── Env overlay ────────────────────────────────────────────────────────────
	let envOpen = $state(false);

	// ── Delete state ───────────────────────────────────────────────────────────
	let showDeleteModal  = $state(false);
	let isDeleting       = $state(false);
	let deleteError      = $state('');


	// ── Domains ────────────────────────────────────────────────────────────────
	let domains         = $state<Domain[]>([]);
	let loadingDomains  = $state(false);
	let domainError     = $state('');
	let dnsCheckState   = $state<Record<string, 'idle' | 'checking' | 'ok' | 'fail'>>({});
	let dnsCheckAddrs   = $state<Record<string, string[]>>({});

	// ── Config editing ─────────────────────────────────────────────────────────
	let editing         = $state(false);
	let saveError       = $state('');
	let saving          = $state(false);
	let editSource      = $state<'git' | 'upload'>('git');
	let editBuildCmd    = $state('');
	let editOutputDir   = $state('');
	let editNodeVersion = $state('');
	let editInstallCmd  = $state('');
	let editFramework   = $state('');

	// ── Git Config editing ─────────────────────────────────────────────────────
	let editGitDeployStrategy   = $state<'push' | 'tag' | 'pull_request'>('push');
	let editGitDeployBranch     = $state('');
	let editGitDeployTagPattern = $state('');
	let gitSaving               = $state(false);
	let gitSaveError            = $state('');
	let gitSaveSuccess          = $state('');

	// ── Webhook URL & Auto-register ──────────────────────────────────────────
	let webhookToken      = $state('');
	let webhookProvider   = $state<'github' | 'gitlab' | 'gitea'>('github');
	let gitProviderId       = $state('');
	let orgGitProviders     = $state<import('$lib/api/types').GitProvider[]>([]);
	let loadingGitProviders = $state(false);
	let gitProviderSaving   = $state(false);
	let gitProviderError    = $state('');
	let gitProviderSuccess  = $state('');

	async function loadGitProviders() {
		loadingGitProviders = true;
		const res = await api.listGitProviders(orgId);
		if (res.data) {
			orgGitProviders = res.data;
			if (service?.git_provider_id) {
				const activeProv = res.data.find(p => p.id === service?.git_provider_id);
				if (activeProv) {
					const pType = activeProv.provider_type;
					if (pType === 'github' || pType === 'gitlab' || pType === 'gitea') {
						webhookProvider = pType;
					}
				}
			}
		}
		loadingGitProviders = false;
	}

	let webhookRegStatus = $state<{ ok: boolean; message: string } | null>(null);

	async function tryAutoRegisterWebhook() {
		try {
			const res = await api.post<{ message: string }>(
				`/projects/${projectId}/services/${serviceId}/webhook/auto-register`
			);
			if (res.error) return { ok: false, message: res.error.message };
			return { ok: true, message: res.data ? res.data.message : 'Webhook registered' };
		} catch (err: any) {
			return { ok: false, message: err.message ?? 'Webhook registration failed' };
		}
	}

	async function saveGitProvider() {
		gitProviderSaving = true;
		gitProviderError = '';
		gitProviderSuccess = '';
		webhookRegStatus = null;
		const providerVal = gitProviderId === '' ? null : gitProviderId;
		const res = await api.put<Service>(`/projects/${projectId}/services/${serviceId}`, {
			git_provider_id: providerVal,
		});
		gitProviderSaving = false;
		if (res.error) {
			gitProviderError = res.error.message;
		} else if (res.data) {
			const svc = res.data;
			service = svc;
			gitProviderSuccess = 'Git provider updated successfully';
			if (svc.git_provider_id) {
				const activeProv = orgGitProviders.find(p => p.id === svc.git_provider_id);
				if (activeProv) {
					const pType = activeProv.provider_type;
					if (pType === 'github' || pType === 'gitlab' || pType === 'gitea') {
						webhookProvider = pType;
					}
					if (pType === 'github' || pType === 'gitlab') {
						webhookRegStatus = await tryAutoRegisterWebhook();
						if (webhookRegStatus.ok) {
							setTimeout(() => { webhookRegStatus = null; }, 5000);
						}
					}
				}
			}
		}
	}

	let isLoadingWebhook  = $state(false);
	let webhookCopied     = $state(false);
	let isRotatingWebhook = $state(false);
	let rotateConfirm     = $state(false);

	async function loadWebhookToken() {
		if (isLoadingWebhook || webhookToken) return;
		isLoadingWebhook = true;
		const res = await api.getWebhookToken(projectId, serviceId);
		if (res.data?.token) webhookToken = res.data.token;
		isLoadingWebhook = false;
	}

	async function rotateWebhook() {
		if (!rotateConfirm) { rotateConfirm = true; return; }
		rotateConfirm = false;
		isRotatingWebhook = true;
		const res = await api.rotateWebhookToken(projectId, serviceId);
		if (res.data?.token) webhookToken = res.data.token;
		isRotatingWebhook = false;
	}

	async function copyWebhookUrl() {
		const url = `${window.location.origin}/api/webhooks/${webhookProvider}/${serviceId}/${webhookToken}`;
		await navigator.clipboard.writeText(url);
		webhookCopied = true;
		setTimeout(() => { webhookCopied = false; }, 2000);
	}

	function resetGitEditForm(c: StaticSiteConfig) {
		editGitDeployStrategy   = c.git_deploy_strategy || 'push';
		editGitDeployBranch     = c.git_deploy_branch || '';
		editGitDeployTagPattern = c.git_deploy_tag_pattern || '';
		gitSaveError            = '';
		gitSaveSuccess          = '';
	}

	async function saveGitConfig() {
		gitSaving = true;
		gitSaveError = '';
		gitSaveSuccess = '';
		
		const branchVal = editGitDeployBranch.trim() === '' ? null : editGitDeployBranch.trim();
		const tagPatternVal = editGitDeployTagPattern.trim() === '' ? null : editGitDeployTagPattern.trim();

		const res = await api.updateStaticConfig(serviceId, {
			git_deploy_strategy:    editGitDeployStrategy,
			git_deploy_branch:      branchVal,
			git_deploy_tag_pattern: tagPatternVal,
		});
		gitSaving = false;
		if (res.error) {
			gitSaveError = res.error.message;
		} else if (res.data) {
			config = res.data;
			resetGitEditForm(res.data);
			gitSaveSuccess = 'Settings saved successfully';
		}
	}

	const FRAMEWORK_PRESETS: Record<string, { install: string; build: string; output: string; ver: string }> = {
		sveltekit: { install: 'bun install', build: 'bun run build',      output: 'build',          ver: '1' },
		nextjs:    { install: 'bun install', build: 'bun run build',      output: 'out',            ver: '1' },
		nuxt:      { install: 'bun install', build: 'bunx nuxi generate', output: '.output/public', ver: '1' },
		astro:     { install: 'bun install', build: 'bun run build',      output: 'dist',           ver: '1' },
		gatsby:    { install: 'bun install', build: 'bun run build',      output: 'public',         ver: '1' },
		vite:      { install: 'bun install', build: 'bun run build',      output: 'dist',           ver: '1' },
		bun:       { install: 'bun install', build: 'bun run build',      output: 'dist',           ver: '1' },
		hugo:      { install: '',            build: 'hugo',               output: 'public',         ver: '' },
		jekyll:    { install: 'bundle install', build: 'bundle exec jekyll build', output: '_site', ver: '' },
	};

	const EDIT_FRAMEWORKS = [
		{ value: 'auto',      label: 'auto — detect from repo' },
		{ value: 'custom',    label: 'Custom — enter commands manually' },
		{ value: 'bun',       label: 'Bun (generic JS/TS project)' },
		{ value: 'sveltekit', label: 'SvelteKit' },
		{ value: 'nextjs',    label: 'Next.js (static export)' },
		{ value: 'nuxt',      label: 'Nuxt (nuxi generate)' },
		{ value: 'astro',     label: 'Astro' },
		{ value: 'vite',      label: 'Vite' },
		{ value: 'gatsby',    label: 'Gatsby' },
		{ value: 'hugo',      label: 'Hugo' },
		{ value: 'jekyll',    label: 'Jekyll' },
	];

	function applyFrameworkPreset(f: string) {
		editFramework = f;
		const p = FRAMEWORK_PRESETS[f];
		if (p) {
			editInstallCmd  = p.install;
			editBuildCmd    = p.build;
			editOutputDir   = p.output;
			editNodeVersion = p.ver;
		}
	}

	// ── Upload ─────────────────────────────────────────────────────────────────
	let uploadFile    = $state<File | null>(null);
	let uploadMsg     = $state('');
	let uploading     = $state(false);
	let uploadError   = $state('');
	let uploadSuccess = $state('');
	let isDragOver    = $state(false);

	// ── Git deploy ─────────────────────────────────────────────────────────────
	let deploying     = $state(false);
	let deployError   = $state('');
	let deploySuccess = $state('');

	// ── Derived ────────────────────────────────────────────────────────────────
	let latestDeployment = $derived(deployments[0] ?? null);
	let isDeploymentActive = $derived(
		latestDeployment?.status === 'running' ||
		latestDeployment?.status === 'queued'  ||
		latestDeployment?.status === 'pending'
	);

	// ── Helpers ────────────────────────────────────────────────────────────────
	function formatTime(ts: string | null | undefined): string {
		if (!ts) return '–';
		try { return formatDistanceToNow(new Date(ts), { addSuffix: true }); }
		catch { return ts; }
	}

	function deployStatusIcon(status: string) {
		if (status === 'success') return CheckCircle2;
		if (status === 'failed')  return XCircle;
		if (status === 'running') return RefreshCw;
		if (status === 'queued' || status === 'pending') return Clock;
		return AlertCircle;
	}

	function deployIconTone(status: string): 'green' | 'red' | 'yellow' | 'blue' {
		if (status === 'success') return 'green';
		if (status === 'failed')  return 'red';
		if (status === 'running') return 'yellow';
		return 'blue';
	}

	// toDotStatus has no 'success' state (it would render grey), so a finished
	// deployment maps to the green dot.
	function deployDot(status: string): DotStatus {
		return status === 'success' ? 'running' : toDotStatus(status);
	}

	function deployBadgeTone(status: string): 'green' | 'red' | 'yellow' | 'blue' | 'neutral' {
		if (status === 'success') return 'green';
		if (status === 'failed')  return 'red';
		if (status === 'running') return 'yellow';
		if (status === 'queued' || status === 'pending') return 'blue';
		return 'neutral';
	}

	const SOURCE_OPTIONS = [
		{ value: 'git',    label: 'git — clone & build' },
		{ value: 'upload', label: 'upload — pre-built zip' },
	];

	let tabs = $derived<TabItem[]>([
		{ id: 'overview',    label: 'Overview' },
		{ id: 'deployments', label: 'Deployments' },
		{ id: 'config',      label: 'Build Config' },
		...(config?.source === 'git' ? [{ id: 'git', label: 'Git' }] : []),
		{ id: 'domains',     label: 'Domains' },
		{ id: 'docs',        label: 'Guide' },
	]);

	let configItems = $derived<KeyValueItem[]>(config ? [
		{ key: 'Source',          value: config.source },
		{ key: 'Framework',       value: config.framework },
		{ key: 'Node version',    value: config.node_version },
		{ key: 'Install command', value: config.install_command, mono: true },
		{ key: 'Build command',   value: config.build_command,   mono: true },
		{ key: 'Output dir',      value: config.output_dir,      mono: true },
	] : []);

	// ── Deployment log viewer ──────────────────────────────────────────────────

	function openDeploymentLogs(dep: Deployment) {
		uiStore.pushPanel({
			component: DeploymentLogsPanel,
			title: `Deployment ${dep.id.slice(0, 8)}`,
			key: `dep-logs-${dep.id}`,
			props: { orgId, projectId, serviceId, deployment: dep },
		});
	}

	// ── Domains ────────────────────────────────────────────────────────────────

	async function loadDomains() {
		loadingDomains = true;
		domainError = '';
		const res = await api.get<Domain[]>(`/services/${serviceId}/domains`);
		if (res.data) domains = res.data;
		else if (res.error) domainError = res.error.message;
		loadingDomains = false;
	}

	function openAddDomainPanel() {
		uiStore.pushPanel({
			component: DomainAddPanel,
			title: 'Add Domain',
			props: {
				serviceId,
				onCreated: (domain: Domain) => {
					domains = [...domains, domain];
					dnsCheckState = { ...dnsCheckState, [domain.id]: 'idle' };
				},
			},
		});
	}

	async function removeDomain(domainId: string) {
		const res = await api.delete(`/services/${serviceId}/domains/${domainId}`);
		if (!res.error) domains = domains.filter(d => d.id !== domainId);
	}

	async function checkDns(domainId: string) {
		dnsCheckState = { ...dnsCheckState, [domainId]: 'checking' };
		const res = await api.checkDomainDns(serviceId, domainId);
		if (res.data) {
			dnsCheckState = { ...dnsCheckState, [domainId]: res.data.resolves ? 'ok' : 'fail' };
			dnsCheckAddrs = { ...dnsCheckAddrs, [domainId]: res.data.addresses };
		} else {
			dnsCheckState = { ...dnsCheckState, [domainId]: 'fail' };
		}
	}

	// ── Config load / edit ─────────────────────────────────────────────────────

	async function loadConfig() {
		const [cfgRes, svcRes] = await Promise.all([
			api.getStaticConfig(serviceId),
			api.getService(projectId, serviceId),
		]);
		if (cfgRes.error) {
			loadError = cfgRes.error.message;
		} else if (cfgRes.data) {
			config = cfgRes.data;
			resetEditForm(cfgRes.data);
			resetGitEditForm(cfgRes.data);
		}
		if (svcRes.data) {
			service = svcRes.data;
			serviceSlug = svcRes.data.slug;
			gitProviderId = svcRes.data.git_provider_id || '';
		}
	}

	async function deleteStaticSite() {
		if (isDeleting) return;
		isDeleting = true;
		deleteError = '';
		const res = await api.deleteService(projectId, serviceId);
		if (res.error) {
			deleteError = res.error.message;
			isDeleting = false;
			return;
		}
		// Close modal and notify parent — parent should close the panel and refresh topology
		showDeleteModal = false;
		onDeleted?.();
		uiStore.clearPanels();
	}

	// ConfirmDialog owns the type-to-confirm text, so a failure returns false to
	// keep the dialog open with `deleteError`.
	async function confirmDeleteStaticSite(): Promise<boolean> {
		await deleteStaticSite();
		return !deleteError;
	}

	async function loadDeployments() {
		const res = await api.getDeployments(serviceId);
		if (res.data) deployments = res.data.slice(0, 20);
	}

	function resetEditForm(c: StaticSiteConfig) {
		editSource      = c.source;
		editBuildCmd    = c.build_command;
		editOutputDir   = c.output_dir;
		editNodeVersion = c.node_version;
		editInstallCmd  = c.install_command;
		editFramework   = c.framework;
		resetGitEditForm(c);
	}

	function startEdit() {
		if (config) resetEditForm(config);
		editing = true;
		saveError = '';
	}

	async function saveConfig() {
		saving = true;
		saveError = '';
		const res = await api.updateStaticConfig(serviceId, {
			source:          editSource,
			build_command:   editBuildCmd,
			output_dir:      editOutputDir,
			node_version:    editNodeVersion,
			install_command: editInstallCmd,
			framework:       editFramework,
		});
		saving = false;
		if (res.error) {
			saveError = res.error.message;
		} else if (res.data) {
			config = res.data;
			editing = false;
		}
	}

	// ── Actions ────────────────────────────────────────────────────────────────

	async function triggerGitDeploy() {
		deploying = true;
		deployError = '';
		deploySuccess = '';
		const res = await api.deployService(serviceId);
		deploying = false;
		if (res.error) {
			deployError = res.error.message;
		} else if (res.data) {
			const dep = res.data as unknown as Deployment;
			deployments = [dep, ...deployments];
			deploySuccess = 'Deployment queued';
			onDeployed?.();
			// Auto-open the log viewer so the user sees progress immediately
			await openDeploymentLogs(dep);
		}
	}

	async function handleUpload() {
		if (!uploadFile) return;
		uploading = true;
		uploadError = '';
		uploadSuccess = '';
		const res = await api.uploadStaticSite(serviceId, uploadFile, uploadMsg || undefined);
		uploading = false;
		if (res.error) {
			uploadError = res.error.message;
		} else {
			uploadSuccess = `Queued (${res.data?.deployment_id?.slice(0, 8)}…)`;
			uploadFile = null;
			uploadMsg  = '';
			onDeployed?.();
			await loadDeployments();
			// Open the log viewer for the new deployment
			const newDep = deployments[0];
			if (newDep) await openDeploymentLogs(newDep);
		}
	}

	function onFileChange(e: Event) {
		const target = e.target as HTMLInputElement;
		uploadFile = target.files?.[0] ?? null;
	}

	function onDragOver(e: DragEvent) {
		e.preventDefault();
		isDragOver = true;
	}

	function onDragLeave() {
		isDragOver = false;
	}

	function onDrop(e: DragEvent) {
		e.preventDefault();
		isDragOver = false;
		const file = e.dataTransfer?.files?.[0];
		if (file) uploadFile = file;
	}

	async function fetchVisitorLogs(tail: number): Promise<string[]> {
		const res = await api.getStaticLogs(serviceId, tail);
		return res.data ?? [];
	}

	async function switchTab(tab: typeof activeTab) {
		activeTab = tab;
		if (tab === 'domains' && domains.length === 0) await loadDomains();
		if (tab === 'deployments') await loadDeployments();
		if (tab === 'git') {
			void loadWebhookToken();
			void loadGitProviders();
		}
	}

	// ── Lifecycle ──────────────────────────────────────────────────────────────

	onMount(async () => {
		await Promise.all([loadConfig(), loadDeployments()]);
		loading = false;
	});

	onDestroy(() => {});
</script>

<!-- ─── Delete confirmation (portalled to body) ──────────────────────────────── -->
{#if showDeleteModal}
	<div use:portal>
		<ConfirmDialog
			bind:open={showDeleteModal}
			title="Delete Static Site"
			message={`This will permanently delete ${serviceSlug} and remove all deployed file versions on disk, the nginx server block (the site goes offline immediately), all custom domains attached to this service, all deployment history and logs, and the service record and build configuration. This cannot be undone.`}
			confirmLabel="Delete Site"
			confirmText={serviceSlug}
			error={deleteError}
			onConfirm={confirmDeleteStaticSite}
		/>
	</div>
{/if}

<!-- ─── Deployment row (Recent Deployments + Deployments tab) ─────────────────── -->
{#snippet deployRow(dep: Deployment, full: boolean)}
	{@const Icon = deployStatusIcon(dep.status)}
	<button type="button" class="dep-row" onclick={() => openDeploymentLogs(dep)}>
		<ListRow
			title={full ? `${dep.id.slice(0, 8)}…` : dep.id.slice(0, 8)}
			meta={full
				? `${dep.source_ref ?? '—'} · ${dep.triggered_by ?? '—'} · ${formatTime(dep.created_at)}`
				: (dep.source_ref ?? '—')}
			iconTone={deployIconTone(dep.status)}
		>
			{#snippet icon()}
				<Icon size={14} class={dep.status === 'running' ? 'spin-icon' : ''} />
			{/snippet}
			{#snippet trailing()}
				<span class="dep-status">
					<StatusDot status={deployDot(dep.status)} />
					<span>{dep.status}</span>
					<ChevronRight size={13} />
				</span>
			{/snippet}
		</ListRow>
	</button>
{/snippet}

<!-- ─── Main panel ──────────────────────────────────────────────────────────── -->
<div class="panel-body">
	{#if loading}
		<div class="loading-row"><Spinner size={16} /> Loading…</div>
	{:else if loadError}
		<div role="alert"><InlineAlert tone="error">{loadError}</InlineAlert></div>
	{:else if config}

		<!-- Visitor Logs overlay -->
		<div use:portal>
			<LogViewerOverlay
				open={logOverlayOpen}
				title="Visitor Logs"
				subtitle={service?.slug ?? serviceId}
				streamUrl="/api/services/{serviceId}/static/logs/stream"
				fetchFn={fetchVisitorLogs}
				tailOptions={[100, 200, 500, 1000]}
				initialTail={200}
				emptyMessage="No visitor traffic recorded yet."
				onClose={() => { logOverlayOpen = false; }}
			/>
		</div>

		<!-- Monitor overlay -->
		<div use:portal>
			<MonitorViewOverlay
				open={monitorOpen}
				onClose={() => { monitorOpen = false; }}
				{serviceId}
			/>
		</div>

		<!-- Env overlay -->
		<div use:portal>
			<EnvManagerOverlay
				open={envOpen}
				onClose={() => { envOpen = false; }}
				{serviceId}
				{projectId}
				serviceName={service?.name ?? serviceId}
			/>
		</div>

		<!-- Tabs -->
		<div class="tabs-wrap">
			<Tabs {tabs} value={activeTab} onChange={(id) => switchTab(id as typeof activeTab)} ariaLabel="Static site sections" />
		</div>

		<div class="tab-content">
		<!-- ── Overview tab ── -->
		{#if activeTab === 'overview'}
				<section class="section">
					<Card padding="4px 14px 12px">
						<ListRow title="Static Site" meta={`Source: ${config.source}`}>
							{#snippet icon()}<Globe size={16} />{/snippet}
							{#snippet trailing()}
								{#if latestDeployment}
									<Badge tone={deployBadgeTone(latestDeployment.status)}>{latestDeployment.status}</Badge>
								{/if}
							{/snippet}
						</ListRow>
						<div class="header-actions">
							{#if isDeploymentActive}
								<Button variant="primary" size="sm" onclick={() => latestDeployment && openDeploymentLogs(latestDeployment)}>
									<Spinner size={12} tone="current" /> View progress
								</Button>
							{/if}
							<Button variant="secondary" size="sm" onclick={() => { logOverlayOpen = true; }}>
								<AlertCircle size={12} /> Visitor Logs
							</Button>
							<Button variant="secondary" size="sm" onclick={() => { monitorOpen = true; }}>Monitor</Button>
							<Button variant="secondary" size="sm" onclick={() => { envOpen = true; }}>Env Vars</Button>
						</div>
					</Card>
				</section>

			{#if config.source === 'git'}
				<section class="section">
					<Card>
						<SectionLabel>Deploy from Git</SectionLabel>
						<div class="stack">
							<p class="section-desc">Clones the repository, runs your build command, and publishes the output directory.</p>
							{#if deployError}<div role="alert"><InlineAlert tone="error">{deployError}</InlineAlert></div>{/if}
							{#if deploySuccess}<div role="status"><InlineAlert tone="success">{deploySuccess}</InlineAlert></div>{/if}
							<div class="action-row">
								<Button variant="primary" onclick={triggerGitDeploy} disabled={deploying || isDeploymentActive}>
									{#if deploying || isDeploymentActive}
										<Spinner size={12} tone="current" />
										{isDeploymentActive && !deploying ? 'Running…' : 'Deploying…'}
									{:else}
										<Play size={13} /> Deploy Now
									{/if}
								</Button>
							</div>
						</div>
					</Card>
				</section>
			{:else}
				<section class="section">
					<Card>
						<SectionLabel>Upload Pre-built Site</SectionLabel>
						<div class="stack">
							<p class="section-desc">Upload a <code>.zip</code> or <code>.tar.gz</code> of your built static files.</p>
							<FormField label="Archive file" for="ss-archive">
								<label
									class="file-drop"
									class:has-file={!!uploadFile}
									class:drag-over={isDragOver}
									ondragover={onDragOver}
									ondragleave={onDragLeave}
									ondrop={onDrop}
								>
									<input
										id="ss-archive"
										type="file"
										accept=".zip,.tar.gz,.tgz"
										onchange={onFileChange}
										class="file-input-hidden"
										disabled={uploading || isDeploymentActive}
									/>
									{#if uploadFile}
										<span class="file-name">{uploadFile.name}</span>
										<span class="file-size">({(uploadFile.size / 1024 / 1024).toFixed(1)} MB)</span>
									{:else}
										<span class="file-placeholder">Drop here or click to select a <code>.zip</code> or <code>.tar.gz</code></span>
									{/if}
								</label>
							</FormField>
							<FormField label="Deploy message (optional)" for="ss-deploy-msg">
								<TextField id="ss-deploy-msg" placeholder="e.g. Release v1.2.0" bind:value={uploadMsg} disabled={uploading || isDeploymentActive} />
							</FormField>
							{#if uploadError}<div role="alert"><InlineAlert tone="error">{uploadError}</InlineAlert></div>{/if}
							{#if uploadSuccess}<div role="status"><InlineAlert tone="success">{uploadSuccess}</InlineAlert></div>{/if}
							<div class="action-row">
								<Button variant="primary" onclick={handleUpload} disabled={!uploadFile || uploading || isDeploymentActive}>
									{#if uploading || isDeploymentActive}
										<Spinner size={12} tone="current" />
										{isDeploymentActive && !uploading ? 'Running…' : 'Uploading…'}
									{:else}
										<Upload size={13} /> Upload & Deploy
									{/if}
								</Button>
							</div>
						</div>
					</Card>
				</section>
			{/if}
			{#if deployments.length > 0}
				<section class="section">
					<div class="section-title">Recent Deployments</div>
					<ActivityList>
						{#each deployments.slice(0, 5) as dep (dep.id)}
							{@render deployRow(dep, false)}
						{/each}
					</ActivityList>
				</section>
			{/if}

				<!-- Danger zone -->
				<Card tone="danger" padding="12px">
					<div class="danger-header">
						<AlertTriangle size={13} />
						<span>Danger Zone</span>
					</div>
					<div class="danger-row">
						<div class="danger-info">
							<span class="danger-title">Delete this static site</span>
							<span class="danger-desc">
								Permanently removes all deployed files, nginx config, domains, and service records.
								This cannot be undone.
							</span>
						</div>
						<Button
							variant="danger-outline"
							size="sm"
							onclick={() => { showDeleteModal = true; deleteError = ''; }}
						>
							<Trash2 size={12} /> Delete
						</Button>
					</div>
				</Card>
		{/if}

		<!-- ── Git tab ── -->
		{#if activeTab === 'git'}
			<GitSettingsSection
				providers={orgGitProviders}
				loadingProviders={loadingGitProviders}
				bind:providerId={gitProviderId}
				providerDefaultLabel="No provider linked"
				onSaveProvider={saveGitProvider}
				providerSaving={gitProviderSaving}
				providerError={gitProviderError}
				providerSuccess={gitProviderSuccess}
				providerWebhookStatus={webhookRegStatus}
				showAutoDeployToggle={false}
				bind:strategy={editGitDeployStrategy}
				bind:branch={editGitDeployBranch}
				bind:tagPattern={editGitDeployTagPattern}
				onSave={saveGitConfig}
				saving={gitSaving}
				saveError={gitSaveError}
				saveSuccess={gitSaveSuccess}
				webhookUrl={webhookToken
					? `${window.location.origin}/api/webhooks/${webhookProvider}/${serviceId}/${webhookToken}`
					: `${window.location.origin}/api/webhooks/${webhookProvider}/${serviceId}/…`}
				webhookLoading={isLoadingWebhook}
				showProviderTabs={true}
				bind:webhookProvider={webhookProvider}
				webhookCopied={webhookCopied}
				onCopyWebhook={copyWebhookUrl}
				onRotateWebhook={rotateWebhook}
				bind:webhookRotateConfirm={rotateConfirm}
				isRotatingWebhook={isRotatingWebhook}
				autoWebhookInfo={service?.git_provider_id && (webhookProvider === 'github' || webhookProvider === 'gitlab')
					? `Webhook is auto-registered on ${webhookProvider === 'github' ? 'GitHub' : 'GitLab'} when auto deploy is enabled.`
					: undefined}
			/>
		{/if}

		<!-- ── Deployments tab ── -->
		{#if activeTab === 'deployments'}
			<section class="section">
				{#if deployments.length === 0}
					<EmptyState message="No deployments yet." />
				{:else}
					<ActivityList>
						{#each deployments as dep (dep.id)}
							{@render deployRow(dep, true)}
						{/each}
					</ActivityList>
				{/if}
			</section>
		{/if}

		<!-- ── Build Config tab ── -->
		{#if activeTab === 'config'}
			<section class="section">
				{#if !editing}
					<Card padding="4px 14px">
						<KeyValueList items={configItems} keyWidth="120px" />
					</Card>
					<div class="action-row">
						<Button variant="secondary" size="sm" onclick={startEdit}>
							<Settings2 size={13} /> Edit Config
						</Button>
					</div>
				{:else}
					<Card>
						<div class="form-grid">
							<FormField label="Source" for="ss-source">
								<Select
									id="ss-source"
									value={editSource}
									onchange={(e) => (editSource = (e.target as HTMLSelectElement).value as 'git' | 'upload')}
									options={SOURCE_OPTIONS}
								/>
							</FormField>
							{#if editSource === 'git'}
								<FormField label="Framework" for="ss-framework" hint="Selecting a framework fills in the commands below.">
									<Select
										id="ss-framework"
										value={editFramework}
										onchange={(e) => applyFrameworkPreset((e.target as HTMLSelectElement).value)}
										options={EDIT_FRAMEWORKS}
									/>
								</FormField>
								<FormField label="Bun / Node version" for="ss-node">
									<TextField id="ss-node" bind:value={editNodeVersion} placeholder="1" />
								</FormField>
								<FormField label="Install command" for="ss-install">
									<div class="mono-field"><TextField id="ss-install" bind:value={editInstallCmd} placeholder="bun install" /></div>
								</FormField>
								<FormField label="Build command" for="ss-build">
									<div class="mono-field"><TextField id="ss-build" bind:value={editBuildCmd} placeholder="bun run build" /></div>
								</FormField>
								<FormField label="Output directory" for="ss-output">
									<div class="mono-field"><TextField id="ss-output" bind:value={editOutputDir} placeholder="dist" /></div>
								</FormField>
							{/if}
							{#if saveError}<div role="alert"><InlineAlert tone="error">{saveError}</InlineAlert></div>{/if}
							<div class="action-row">
								<Button variant="primary" onclick={saveConfig} disabled={saving}>
									{saving ? 'Saving…' : 'Save'}
								</Button>
								<Button variant="ghost" onclick={() => editing = false}>Cancel</Button>
							</div>
						</div>
					</Card>
				{/if}
			</section>
		{/if}

		<!-- ── Domains tab ── -->
		{#if activeTab === 'domains'}
			<section class="section">
				<div class="domains-header">
					<div class="section-title">Custom Domains</div>
					<Button variant="secondary" size="sm" onclick={openAddDomainPanel}>
						<Plus size={12} /> Add Domain
					</Button>
				</div>

				{#if loadingDomains}
					<div class="loading-row"><Spinner size={16} /> Loading…</div>
				{:else if domainError}
					<div role="alert"><InlineAlert tone="error">{domainError}</InlineAlert></div>
				{:else if domains.length === 0}
					<EmptyState
						message="No domains configured."
						sub="Add a custom domain to serve this site on your own hostname."
					/>
				{:else}
					<ActivityList>
						{#each domains as domain (domain.id)}
							<div class="domain-item">
							<ListRow title={domain.hostname}>
								{#snippet icon()}<Globe size={14} />{/snippet}
								{#snippet trailing()}
									<div class="domain-actions">
										<Button
											variant="secondary"
											size="sm"
											onclick={() => checkDns(domain.id)}
											disabled={dnsCheckState[domain.id] === 'checking'}
										>Check DNS</Button>
										<Button variant="ghost" size="icon" aria-label="Remove domain" title="Remove domain" onclick={() => removeDomain(domain.id)}>
											<Trash2 size={13} />
										</Button>
									</div>
								{/snippet}
							</ListRow>
							{#if dnsCheckState[domain.id] === 'ok'}
								<div class="domain-dns"><Badge tone="green"><CheckCircle size={10} /> DNS OK</Badge></div>
							{:else if dnsCheckState[domain.id] === 'fail'}
								<div class="domain-dns">
									<Badge tone="red"><XCircle size={10} /> DNS fail</Badge>
									{#if dnsCheckAddrs[domain.id]?.length}
										<span class="dns-addrs">resolves to: {dnsCheckAddrs[domain.id].join(', ')}</span>
									{/if}
								</div>
							{:else if dnsCheckState[domain.id] === 'checking'}
								<div class="domain-dns"><Badge tone="neutral"><Spinner size={10} tone="current" /> Checking…</Badge></div>
							{/if}
							</div>
						{/each}
					</ActivityList>
				{/if}

				<InlineAlert tone="info">
					<strong>DNS setup:</strong> Point your domain's A record to the Shipyard server IP, or add a CNAME to your Shipyard hostname.
				</InlineAlert>
			</section>
		{/if}

		<!-- ── Guide tab ── -->
		{#if activeTab === 'docs'}
			<section class="section">
				<SectionLabel>What makes a valid static build?</SectionLabel>
				<Card padding="12px 14px">
					<div class="doc-block">
					<p>Shipyard validates your output directory after every build. It will fail if:</p>
					<ul>
						<li>The output directory is empty or doesn't exist</li>
						<li>A <code>server/</code> subdirectory is present (SSR build, not static)</li>
						<li>A <code>node_modules/</code> directory is present (server bundle)</li>
						<li>No <code>.html</code> files are found anywhere in the output</li>
					</ul>
					</div>
				</Card>
			</section>

			<section class="section">
				<SectionLabel>Auto-detection — no config needed</SectionLabel>
				<Card padding="12px 14px">
					<div class="doc-block">
					<p>If you don't add a <code>shipyard.json</code>, Shipyard auto-detects your framework from files in the repo root:</p>
					<div class="detect-table">
						<div class="detect-row header">
							<span>Detected file</span><span>Framework</span><span>Output dir</span>
						</div>
						<div class="detect-row"><span><code>svelte.config.js/ts</code></span><span>SvelteKit</span><span><code>build</code></span></div>
						<div class="detect-row"><span><code>next.config.js/ts/mjs</code></span><span>Next.js</span><span><code>out</code></span></div>
						<div class="detect-row"><span><code>nuxt.config.js/ts</code></span><span>Nuxt</span><span><code>.output/public</code></span></div>
						<div class="detect-row"><span><code>astro.config.*</code></span><span>Astro</span><span><code>dist</code></span></div>
						<div class="detect-row"><span><code>gatsby-config.js/ts</code></span><span>Gatsby</span><span><code>public</code></span></div>
						<div class="detect-row"><span><code>hugo.toml</code></span><span>Hugo</span><span><code>public</code></span></div>
						<div class="detect-row"><span><code>_config.yml</code></span><span>Jekyll</span><span><code>_site</code></span></div>
						<div class="detect-row"><span><code>vite.config.*</code></span><span>Vite</span><span><code>dist</code></span></div>
						<div class="detect-row"><span><code>package.json</code></span><span>npm (generic)</span><span><code>dist</code></span></div>
					</div>
					<p>Add a <code>shipyard.json</code> with a <code>"build"</code> section to override any of these defaults.</p>
					</div>
				</Card>
			</section>

			<section class="section">
				<SectionLabel>SvelteKit — static adapter</SectionLabel>
				<Card padding="12px 14px">
					<div class="doc-block">
					<p>SvelteKit defaults to SSR. You must switch to the static adapter:</p>
					<pre class="code-block">npm install -D @sveltejs/adapter-static</pre>
					<p>In <code>svelte.config.js</code>:</p>
					<pre class="code-block">import adapter from '@sveltejs/adapter-static';
export default &#123;
  kit: &#123;
    adapter: adapter(&#123;
      pages: 'build',
      assets: 'build',
      fallback: 'index.html',
    &#125;)
  &#125;
&#125;;</pre>
					</div>
				</Card>
			</section>

			<section class="section">
				<SectionLabel>shipyard.json — optional overrides</SectionLabel>
				<Card padding="12px 14px">
					<div class="doc-block">
					<p>Add a <code>shipyard.json</code> to your repo root to override build settings or configure runtime behaviour. The file is entirely optional.</p>
					<pre class="code-block">&#123;
  "build": &#123;
    "command": "npm run build",
    "output": "build",
    "node_version": "20",
    "install_command": "npm ci"
  &#125;,
  "spa": true,
  "redirects": [&#123; "src": "/old", "dest": "/new", "status": 301 &#125;],
  "error_pages": &#123; "404": "404.html" &#125;
&#125;</pre>
					<p><strong>Priority:</strong> <code>shipyard.json</code> → auto-detect → saved UI config</p>
					</div>
				</Card>
			</section>
		{/if}
		</div><!-- .tab-content -->

	{/if}
</div>

<style>
	.panel-body {
		padding: 16px 16px 0;
		display: flex;
		flex-direction: column;
		gap: 0;
		height: 100%;
		overflow: hidden;
	}

	.tabs-wrap { flex-shrink: 0; }
	.header-actions { display: flex; flex-wrap: wrap; gap: 8px; padding-top: 12px; border-top: 1px solid var(--border); }
	.stack { display: flex; flex-direction: column; gap: 10px; }
	.mono-field :global(input) { font-family: var(--font-mono); }

	.dep-row { display: block; width: 100%; padding: 0; background: none; border: none; text-align: left; cursor: pointer; color: inherit; font: inherit; }
	.dep-row + .dep-row, .domain-item + .domain-item { border-top: 1px solid var(--border); }
	.dep-row :global(.ui-list-row), .domain-item :global(.ui-list-row) { border-bottom: none; }
	.dep-row:hover :global(.ui-list-row-title) { color: var(--accent); }
	.dep-status { display: inline-flex; align-items: center; gap: 6px; font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; color: var(--text-muted); }
	.domain-dns { padding: 0 0 10px 42px; display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

	.loading-row {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-muted);
		font-size: 13px;
		padding: 24px 0;
	}

	.tab-content {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
		padding: 16px 0;
	}


	/* ── Section ── */
	.section {
		display: flex;
		flex-direction: column;
		gap: 10px;
		margin-bottom: 20px;
	}

	.section-title {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.section-desc {
		font-size: 12px;
		color: var(--text-muted);
		line-height: 1.5;
	}

	.action-row { display: flex; align-items: center; gap: 8px; }

	/* ── Form elements ── */
	.form-grid { display: flex; flex-direction: column; gap: 12px; }

	.file-drop {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 14px 12px;
		border: 1px dashed var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		cursor: pointer;
		transition: all var(--transition-fast);
		min-height: 48px;
	}
	.file-drop:hover { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 4%, transparent); }
	.file-drop.has-file { border-color: var(--accent); border-style: solid; }
	.file-drop.drag-over { border-color: var(--accent); background: color-mix(in srgb, var(--accent) 8%, transparent); border-style: solid; }

	.file-input-hidden { display: none; }

	.file-placeholder {
		font-size: 12px;
		color: var(--text-dim);
	}
	.file-placeholder code {
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-base);
		padding: 1px 4px;
		border-radius: 3px;
		border: 1px solid var(--border);
	}
	.file-name {
		font-size: 12px;
		font-family: var(--font-mono);
		color: var(--text-primary);
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		flex: 1;
	}
	.file-size {
		font-size: 11px;
		color: var(--text-dim);
		flex-shrink: 0;
	}

	/* ── Domains ── */
	.domains-header { display: flex; align-items: center; justify-content: space-between; }

	.dns-addrs { font-size: 10px; color: var(--text-dim); font-family: var(--font-mono); }

	.domain-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }

	/* ── Docs ── */
	.doc-block {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.doc-block p { font-size: 12px; color: var(--text-primary); line-height: 1.6; margin: 0; }
	.doc-block ul { margin: 0; padding-left: 18px; display: flex; flex-direction: column; gap: 3px; }
	.doc-block li { font-size: 12px; color: var(--text-primary); line-height: 1.5; }
	.doc-block code {
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-base);
		padding: 1px 4px;
		border-radius: 3px;
		border: 1px solid var(--border);
	}

	.detect-table { display: flex; flex-direction: column; gap: 0; border: 1px solid var(--border); border-radius: var(--radius-sm); overflow: hidden; font-size: 11px; }
	.detect-row { display: grid; grid-template-columns: 2fr 1.2fr 1fr; gap: 8px; padding: 6px 10px; border-bottom: 1px solid var(--border); color: var(--text-primary); align-items: center; }
	.detect-row:last-child { border-bottom: none; }
	.detect-row.header { font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em; color: var(--text-muted); background: var(--bg-base); }

	.code-block {
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 10px 12px;
		overflow-x: auto;
		white-space: pre;
		color: var(--text-primary);
		line-height: 1.6;
		margin: 0;
	}

	:global(.spin-icon) { animation: spin 0.7s linear infinite; }

	@keyframes spin { to { transform: rotate(360deg); } }

	.danger-header {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--accent-red);
		font-size: 11px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-bottom: 10px;
	}

	.danger-row {
		display: flex;
		align-items: center;
		gap: 12px;
		justify-content: space-between;
	}

	.danger-info { display: flex; flex-direction: column; gap: 3px; }
	.danger-title { font-size: 12px; font-weight: 600; color: var(--text-primary); }
	.danger-desc { font-size: 11px; color: var(--text-muted); line-height: 1.5; }
</style>
