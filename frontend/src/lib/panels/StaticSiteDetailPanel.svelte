<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import {
		Globe, RefreshCw, Upload, Settings2, Play,
		ChevronRight, CheckCircle2, XCircle, Clock, AlertCircle,
		Plus, Trash2, X, CheckCircle, Loader2, AlertTriangle, Copy
	} from '@lucide/svelte';
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
	let deleteSlugInput  = $state('');
	let isDeleting       = $state(false);
	let deleteError      = $state('');
	let deleteSlugValid  = $derived(deleteSlugInput === serviceSlug && serviceSlug !== '');


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

	function deployStatusClass(status: string): string {
		if (status === 'success') return 'status-ok';
		if (status === 'failed')  return 'status-err';
		if (status === 'running') return 'status-run';
		if (status === 'queued' || status === 'pending') return 'status-queue';
		return 'status-dim';
	}

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
		if (!deleteSlugValid || isDeleting) return;
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

<!-- ─── Delete confirmation modal ────────────────────────────────────────────── -->
{#if showDeleteModal}
	<div class="modal-backdrop" role="dialog" aria-modal="true">
		<div class="modal-card">
			<div class="modal-header">
				<AlertTriangle size={18} style="color:#EF4444;flex-shrink:0" />
				<span>Delete Static Site</span>
			</div>
			<div class="modal-body">
				<p class="modal-warning">
					This will permanently delete <strong>{serviceSlug}</strong> and remove:
				</p>
				<ul class="modal-list">
					<li>All deployed file versions on disk</li>
					<li>The nginx server block (site goes offline immediately)</li>
					<li>All custom domains attached to this service</li>
					<li>All deployment history and logs</li>
					<li>The service record and build configuration</li>
				</ul>
				<p class="modal-warning"><strong>This cannot be undone.</strong></p>
				<div class="modal-confirm-field">
					<label class="modal-confirm-label">
						Type <code class="modal-confirm-code">{serviceSlug}</code> to confirm
					</label>
					<input
						class="modal-confirm-input"
						type="text"
						placeholder={serviceSlug}
						bind:value={deleteSlugInput}
						autocomplete="off"
						spellcheck="false"
					/>
				</div>
				{#if deleteError}
					<div class="form-error">{deleteError}</div>
				{/if}
			</div>
			<div class="modal-footer">
				<button
					class="btn-ghost"
					onclick={() => { showDeleteModal = false; deleteSlugInput = ''; deleteError = ''; }}
					disabled={isDeleting}
				>Cancel</button>
				<button
					class="btn-danger"
					disabled={!deleteSlugValid || isDeleting}
					onclick={deleteStaticSite}
				>
					{#if isDeleting}
						<div class="spinner-xs-dark"></div> Deleting…
					{:else}
						<Trash2 size={13} /> Delete Site
					{/if}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- ─── Main panel ──────────────────────────────────────────────────────────── -->
<div class="panel-body">
	{#if loading}
		<div class="loading-row"><div class="spinner-sm"></div> Loading…</div>
	{:else if loadError}
		<div class="error-msg">{loadError}</div>
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
		<div class="tabs">
			{#each [
				['overview','Overview'],
				['deployments','Deployments'],
				['config','Build Config'],
				...(config.source === 'git' ? [['git', 'Git']] as const : []),
				['domains','Domains'],
				['docs','Guide'],
			] as [tab, label] (tab)}
				<button
					class="tab"
					class:active={activeTab === tab}
					onclick={() => switchTab(tab as typeof activeTab)}
				>{label}</button>
			{/each}
		</div>

		<!-- ── Overview tab ── -->
		{#if activeTab === 'overview'}
			<section class="section">
				<div class="hero-row">
					<div class="hero-icon"><Globe size={20} /></div>
					<div class="hero-info">
						<div class="hero-label">Static Site</div>
						<div class="hero-sub">Source: <strong>{config.source}</strong>
							{#if latestDeployment}
								&nbsp;·&nbsp;
								<span class="dep-status-inline {deployStatusClass(latestDeployment.status)}">
									{latestDeployment.status}
								</span>
							{/if}
						</div>
					</div>
					{#if isDeploymentActive}
						<button class="btn-view-progress" onclick={() => latestDeployment && openDeploymentLogs(latestDeployment)}>
							<Loader2 size={12} class="spin-icon" /> View progress
						</button>
					{/if}
					<button class="btn-visitor-logs" onclick={() => { logOverlayOpen = true; }}>
						<AlertCircle size={12} /> Visitor Logs
					</button>
					<button class="btn-visitor-logs" onclick={() => { monitorOpen = true; }}>
						Monitor
					</button>
					<button class="btn-visitor-logs" onclick={() => { envOpen = true; }}>
						Env Vars
					</button>
				</div>
			</section>

			{#if config.source === 'git'}
				<section class="section">
					<div class="section-title">Deploy from Git</div>
					<p class="section-desc">Clones the repository, runs your build command, and publishes the output directory.</p>
					{#if deployError}<div class="form-error">{deployError}</div>{/if}
					{#if deploySuccess}<div class="form-success">{deploySuccess}</div>{/if}
					<button class="btn-primary" onclick={triggerGitDeploy} disabled={deploying || isDeploymentActive}>
						{#if deploying || isDeploymentActive}
							<div class="spinner-xs"></div>
							{isDeploymentActive && !deploying ? 'Running…' : 'Deploying…'}
						{:else}
							<Play size={13} /> Deploy Now
						{/if}
					</button>
				</section>
			{:else}
				<section class="section">
					<div class="section-title">Upload Pre-built Site</div>
					<p class="section-desc">Upload a <code>.zip</code> or <code>.tar.gz</code> of your built static files.</p>
					<div class="form-group">
						<label class="form-label">Archive file</label>
						<label
							class="file-drop"
							class:has-file={!!uploadFile}
							class:drag-over={isDragOver}
							ondragover={onDragOver}
							ondragleave={onDragLeave}
							ondrop={onDrop}
						>
							<input
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
					</div>
					<div class="form-group">
						<label class="form-label">Deploy message <span class="optional">(optional)</span></label>
						<input type="text" class="form-input" placeholder="e.g. Release v1.2.0" bind:value={uploadMsg} disabled={uploading || isDeploymentActive} />
					</div>
					{#if uploadError}<div class="form-error">{uploadError}</div>{/if}
					{#if uploadSuccess}<div class="form-success">{uploadSuccess}</div>{/if}
					<button class="btn-primary" onclick={handleUpload} disabled={!uploadFile || uploading || isDeploymentActive}>
						{#if uploading || isDeploymentActive}
							<div class="spinner-xs"></div>
							{isDeploymentActive && !uploading ? 'Running…' : 'Uploading…'}
						{:else}
							<Upload size={13} /> Upload & Deploy
						{/if}
					</button>
				</section>
			{/if}

			{#if deployments.length > 0}
				<section class="section">
					<div class="section-title">Recent Deployments</div>
					<div class="deploy-list">
						{#each deployments.slice(0, 5) as dep (dep.id)}
							{@const Icon = deployStatusIcon(dep.status)}
							<button class="deploy-row" onclick={() => openDeploymentLogs(dep)}>
								<div class="deploy-icon {deployStatusClass(dep.status)}">
									<Icon size={13} class={dep.status === 'running' ? 'spin-icon' : ''} />
								</div>
								<div class="deploy-info">
									<span class="deploy-id">{dep.id.slice(0, 8)}</span>
									<span class="deploy-ref">{dep.source_ref ?? '—'}</span>
								</div>
								<span class="deploy-status {deployStatusClass(dep.status)}">{dep.status}</span>
								<ChevronRight size={12} class="deploy-arrow" />
							</button>
						{/each}
					</div>
				</section>
			{/if}

			<!-- Danger zone -->
			<div class="danger-zone">
				<div class="danger-header">
					<AlertTriangle size={13} />
					<span>Danger Zone</span>
				</div>
				<div class="danger-body">
					<div class="danger-row">
						<div class="danger-info">
							<span class="danger-title">Delete this static site</span>
							<span class="danger-desc">
								Permanently removes all deployed files, nginx config, domains, and service records.
								This cannot be undone.
							</span>
						</div>
						<button
							class="btn-danger-outline"
							onclick={() => { showDeleteModal = true; deleteSlugInput = ''; deleteError = ''; }}
						>
							<Trash2 size={12} /> Delete
						</button>
					</div>
				</div>
			</div>
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
					<div class="empty-state">No deployments yet.</div>
				{:else}
					<div class="deploy-list-full">
						{#each deployments as dep (dep.id)}
							{@const Icon = deployStatusIcon(dep.status)}
							<button class="deploy-full-row" onclick={() => openDeploymentLogs(dep)}>
								<div class="deploy-icon-lg {deployStatusClass(dep.status)}">
									<Icon size={14} class={dep.status === 'running' ? 'spin-icon' : ''} />
								</div>
								<div class="deploy-full-info">
									<div class="deploy-full-id">{dep.id.slice(0, 8)}…</div>
									<div class="deploy-full-meta">
										<span>{dep.source_ref ?? '—'}</span>
										<span>{dep.triggered_by ?? '—'}</span>
										<span>{formatTime(dep.created_at)}</span>
									</div>
								</div>
								<span class="deploy-status-pill {deployStatusClass(dep.status)}">{dep.status}</span>
								<ChevronRight size={13} class="deploy-arrow" />
							</button>
						{/each}
					</div>
				{/if}
			</section>
		{/if}

		<!-- ── Build Config tab ── -->
		{#if activeTab === 'config'}
			<section class="section">
				{#if !editing}
					<div class="config-grid">
						<div class="config-row"><span class="config-key">Source</span><span class="config-val">{config.source}</span></div>
						<div class="config-row"><span class="config-key">Framework</span><span class="config-val">{config.framework}</span></div>
						<div class="config-row"><span class="config-key">Node version</span><span class="config-val">{config.node_version || '—'}</span></div>
						<div class="config-row"><span class="config-key">Install command</span><span class="config-val mono">{config.install_command || '—'}</span></div>
						<div class="config-row"><span class="config-key">Build command</span><span class="config-val mono">{config.build_command || '—'}</span></div>
						<div class="config-row"><span class="config-key">Output dir</span><span class="config-val mono">{config.output_dir || '—'}</span></div>
					</div>
					<button class="btn-secondary" onclick={startEdit}>
						<Settings2 size={13} /> Edit Config
					</button>
				{:else}
					<div class="form-grid">
						<div class="form-group">
							<label class="form-label">Source</label>
							<select class="form-select" bind:value={editSource}>
								<option value="git">git — clone & build</option>
								<option value="upload">upload — pre-built zip</option>
							</select>
						</div>
						{#if editSource === 'git'}
							<div class="form-group">
								<label class="form-label">Framework</label>
								<select
									class="form-select"
									value={editFramework}
									onchange={(e) => applyFrameworkPreset((e.target as HTMLSelectElement).value)}
								>
									{#each EDIT_FRAMEWORKS as f (f.value)}
										<option value={f.value}>{f.label}</option>
									{/each}
								</select>
								<span class="field-hint">Selecting a framework fills in the commands below.</span>
							</div>
							<div class="form-group">
								<label class="form-label">Bun / Node version</label>
								<input class="form-input" bind:value={editNodeVersion} placeholder="1" />
							</div>
							<div class="form-group">
								<label class="form-label">Install command</label>
								<input class="form-input mono" bind:value={editInstallCmd} placeholder="bun install" />
							</div>
							<div class="form-group">
								<label class="form-label">Build command</label>
								<input class="form-input mono" bind:value={editBuildCmd} placeholder="bun run build" />
							</div>
							<div class="form-group">
								<label class="form-label">Output directory</label>
								<input class="form-input mono" bind:value={editOutputDir} placeholder="dist" />
							</div>
						{/if}
					</div>
					{#if saveError}<div class="form-error">{saveError}</div>{/if}
					<div class="action-row">
						<button class="btn-primary" onclick={saveConfig} disabled={saving}>
							{saving ? 'Saving…' : 'Save'}
						</button>
						<button class="btn-ghost" onclick={() => editing = false}>Cancel</button>
					</div>
				{/if}
			</section>
		{/if}

		<!-- ── Domains tab ── -->
		{#if activeTab === 'domains'}
			<section class="section">
				<div class="domains-header">
					<div class="section-title">Custom Domains</div>
					<button class="btn-secondary btn-sm" onclick={openAddDomainPanel}>
						<Plus size={12} /> Add Domain
					</button>
				</div>

				{#if loadingDomains}
					<div class="loading-row"><div class="spinner-sm"></div> Loading…</div>
				{:else if domainError}
					<div class="form-error">{domainError}</div>
				{:else if domains.length === 0}
					<div class="empty-state">
						No domains configured.<br />
						<span class="empty-sub">Add a custom domain to serve this site on your own hostname.</span>
					</div>
				{:else}
					<div class="domain-list">
						{#each domains as domain (domain.id)}
							<div class="domain-row">
								<div class="domain-info">
									<span class="domain-hostname">{domain.hostname}</span>
									{#if dnsCheckState[domain.id] === 'ok'}
										<span class="dns-badge dns-ok"><CheckCircle size={10} /> DNS OK</span>
									{:else if dnsCheckState[domain.id] === 'fail'}
										<span class="dns-badge dns-fail"><XCircle size={10} /> DNS fail</span>
										{#if dnsCheckAddrs[domain.id]?.length}
											<span class="dns-addrs">resolves to: {dnsCheckAddrs[domain.id].join(', ')}</span>
										{/if}
									{:else if dnsCheckState[domain.id] === 'checking'}
										<span class="dns-badge dns-checking"><div class="spinner-xs-inline"></div> Checking…</span>
									{/if}
								</div>
								<div class="domain-actions">
									<button
										class="btn-ghost btn-xs"
										onclick={() => checkDns(domain.id)}
										disabled={dnsCheckState[domain.id] === 'checking'}
									>Check DNS</button>
									<button class="btn-ghost btn-xs btn-danger-ghost" onclick={() => removeDomain(domain.id)}>
										<Trash2 size={11} />
									</button>
								</div>
							</div>
						{/each}
					</div>
				{/if}

				<div class="dns-hint">
					<strong>DNS setup:</strong> Point your domain's A record to the Shipyard server IP, or add a CNAME to your Shipyard hostname.
				</div>
			</section>
		{/if}

		<!-- ── Guide tab ── -->
		{#if activeTab === 'docs'}
			<section class="section">
				<div class="section-title">What makes a valid static build?</div>
				<div class="doc-block">
					<p>Shipyard validates your output directory after every build. It will fail if:</p>
					<ul>
						<li>The output directory is empty or doesn't exist</li>
						<li>A <code>server/</code> subdirectory is present (SSR build, not static)</li>
						<li>A <code>node_modules/</code> directory is present (server bundle)</li>
						<li>No <code>.html</code> files are found anywhere in the output</li>
					</ul>
				</div>
			</section>

			<section class="section">
				<div class="section-title">Auto-detection — no config needed</div>
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
			</section>

			<section class="section">
				<div class="section-title">SvelteKit — static adapter</div>
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
			</section>

			<section class="section">
				<div class="section-title">shipyard.json — optional overrides</div>
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
			</section>
		{/if}


	{/if}
</div>

<style>
	.panel-body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 0;
		height: 100%;
		overflow-y: auto;
	}

	.loading-row {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-muted);
		font-size: 13px;
		padding: 24px 0;
	}

	.error-msg {
		color: var(--status-failed, #ef4444);
		font-size: 13px;
		padding: 12px;
		background: color-mix(in srgb, #ef4444 8%, transparent);
		border-radius: var(--radius-sm);
	}

	/* ── Tabs ── */
	.tabs {
		display: flex;
		gap: 2px;
		margin-bottom: 16px;
		border-bottom: 1px solid var(--border);
		flex-wrap: nowrap;
		overflow-x: auto;
		scrollbar-width: none;
		-ms-overflow-style: none;
	}
	.tabs::-webkit-scrollbar {
		display: none;
	}

	.tab {
		padding: 7px 12px;
		font-size: 12px;
		font-weight: 500;
		color: var(--text-muted);
		background: none;
		border: none;
		border-bottom: 2px solid transparent;
		cursor: pointer;
		margin-bottom: -1px;
		transition: all var(--transition-fast);
		white-space: nowrap;
		flex-shrink: 0;
	}
	.tab:hover { color: var(--text-primary); }
	.tab.active { color: var(--accent); border-bottom-color: var(--accent); }

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

	/* ── Hero row ── */
	.hero-row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px;
		background: var(--bg-elevated);
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
	}

	.hero-icon {
		width: 36px;
		height: 36px;
		border-radius: var(--radius-sm);
		background: color-mix(in srgb, #22c55e 12%, transparent);
		color: #22c55e;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.hero-info { flex: 1; }
	.hero-label { font-size: 13px; font-weight: 600; color: var(--text-primary); }
	.hero-sub { font-size: 11px; color: var(--text-muted); }

	.dep-status-inline { font-weight: 600; }
	.dep-status-inline.status-ok  { color: #22c55e; }
	.dep-status-inline.status-err { color: #ef4444; }
	.dep-status-inline.status-run { color: #f59e0b; }
	.dep-status-inline.status-queue { color: #6366f1; }

	.btn-view-progress {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 11px;
		font-weight: 600;
		color: var(--accent);
		background: color-mix(in srgb, var(--accent) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
		border-radius: var(--radius-sm);
		padding: 4px 10px;
		cursor: pointer;
		white-space: nowrap;
		transition: opacity var(--transition-fast);
	}
	.btn-view-progress:hover { opacity: 0.8; }

	/* ── Buttons ── */
	.btn-primary {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		font-size: 12px;
		font-weight: 600;
		background: var(--accent);
		color: white;
		border: none;
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: opacity var(--transition-fast);
	}
	.btn-primary:hover:not(:disabled) { opacity: 0.88; }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	.btn-secondary {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		font-size: 12px;
		font-weight: 500;
		background: var(--bg-elevated);
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.btn-secondary:hover { border-color: var(--border-hover); }
	.btn-secondary.btn-sm { padding: 5px 10px; font-size: 11px; }

	.btn-ghost {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 12px;
		font-size: 12px;
		color: var(--text-muted);
		background: none;
		border: none;
		border-radius: var(--radius-sm);
		cursor: pointer;
	}
	.btn-ghost:hover { color: var(--text-primary); background: var(--bg-elevated); }
	.btn-ghost.btn-xs { padding: 4px 8px; font-size: 11px; }
	.btn-ghost.btn-danger-ghost:hover { color: #ef4444; }

	.action-row { display: flex; align-items: center; gap: 8px; }

	/* ── Form elements ── */
	.form-grid { display: flex; flex-direction: column; gap: 12px; }
	.form-group { display: flex; flex-direction: column; gap: 4px; }

	.form-label {
		font-size: 11px;
		font-weight: 500;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.form-input, .form-select {
		padding: 7px 10px;
		font-size: 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-input, var(--bg-surface));
		color: var(--text-primary);
		outline: none;
		font-family: var(--font-sans);
	}
	.form-input:focus, .form-select:focus { border-color: var(--accent); }
	.form-input.mono { font-family: var(--font-mono); }
	.mono { font-family: var(--font-mono); }
	.field-hint { font-size: 10px; color: var(--text-dim); margin-top: 2px; }

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
	.optional { color: var(--text-dim); font-weight: 400; }

	.form-error {
		font-size: 12px;
		color: var(--status-failed, #ef4444);
		padding: 8px 10px;
		background: color-mix(in srgb, #ef4444 8%, transparent);
		border-radius: var(--radius-sm);
	}

	.form-success {
		font-size: 12px;
		color: #22c55e;
		padding: 8px 10px;
		background: color-mix(in srgb, #22c55e 8%, transparent);
		border-radius: var(--radius-sm);
	}

	/* ── Config grid ── */
	.config-grid {
		display: flex;
		flex-direction: column;
		gap: 0;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		overflow: hidden;
	}

	.config-row {
		display: flex;
		align-items: center;
		padding: 8px 12px;
		font-size: 12px;
		border-bottom: 1px solid var(--border);
	}
	.config-row:last-child { border-bottom: none; }
	.config-key { color: var(--text-muted); width: 120px; flex-shrink: 0; }
	.config-val { color: var(--text-primary); font-weight: 500; }

	/* ── Deploy lists ── */
	.deploy-list { display: flex; flex-direction: column; gap: 4px; }

	.deploy-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		background: var(--bg-elevated);
		cursor: pointer;
		width: 100%;
		text-align: left;
		transition: border-color var(--transition-fast);
	}
	.deploy-row:hover { border-color: var(--accent); }

	.deploy-icon {
		width: 22px;
		height: 22px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.deploy-info {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
	}

	.deploy-id { font-size: 11px; font-family: var(--font-mono); color: var(--text-primary); font-weight: 600; }
	.deploy-ref { font-size: 11px; color: var(--text-dim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
	.deploy-status { font-size: 10px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.05em; }
	:global(.deploy-arrow) { color: var(--text-dim); flex-shrink: 0; }

	.deploy-list-full { display: flex; flex-direction: column; gap: 6px; }

	.deploy-full-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-surface);
		cursor: pointer;
		width: 100%;
		text-align: left;
		transition: border-color var(--transition-fast);
	}
	.deploy-full-row:hover { border-color: var(--accent); }

	.deploy-icon-lg {
		width: 28px;
		height: 28px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}

	.deploy-full-info { flex: 1; min-width: 0; }
	.deploy-full-id { font-size: 12px; font-family: var(--font-mono); font-weight: 600; color: var(--text-primary); }
	.deploy-full-meta { display: flex; gap: 8px; font-size: 11px; color: var(--text-dim); flex-wrap: wrap; }

	.deploy-status-pill {
		font-size: 10px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		padding: 2px 8px;
		border-radius: 100px;
	}

	/* Status colours shared */
	.status-ok   { color: #22c55e; background: color-mix(in srgb, #22c55e 12%, transparent); }
	.status-err  { color: #ef4444; background: color-mix(in srgb, #ef4444 12%, transparent); }
	.status-run  { color: #f59e0b; background: color-mix(in srgb, #f59e0b 12%, transparent); }
	.status-queue { color: #6366f1; background: color-mix(in srgb, #6366f1 12%, transparent); }
	.status-dim  { color: var(--text-dim); background: var(--bg-elevated); }

	/* ── Domains ── */
	.domains-header { display: flex; align-items: center; justify-content: space-between; }

	.domain-list { display: flex; flex-direction: column; gap: 6px; }

	.domain-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
	}

	.domain-info { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
	.domain-hostname { font-size: 13px; font-family: var(--font-mono); font-weight: 600; color: var(--text-primary); }

	.dns-badge {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 10px;
		font-weight: 600;
		padding: 2px 7px;
		border-radius: 100px;
		text-transform: uppercase;
	}
	.dns-ok      { background: color-mix(in srgb, #22c55e 12%, transparent); color: #22c55e; }
	.dns-fail    { background: color-mix(in srgb, #ef4444 12%, transparent); color: #ef4444; }
	.dns-checking { background: var(--bg-surface); color: var(--text-muted); }

	.dns-addrs { font-size: 10px; color: var(--text-dim); font-family: var(--font-mono); }

	.domain-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }

	.dns-hint {
		font-size: 11px;
		color: var(--text-muted);
		padding: 8px 10px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		line-height: 1.5;
	}

	/* ── Empty state ── */
	.empty-state {
		font-size: 13px;
		color: var(--text-dim);
		text-align: center;
		padding: 24px;
		line-height: 1.6;
	}
	.empty-sub { font-size: 11px; }

	/* ── Docs ── */
	.doc-block {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 12px 14px;
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

	/* ── Spinners ── */
	.spinner-sm {
		width: 16px; height: 16px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	.spinner-xs {
		width: 12px; height: 12px;
		border: 2px solid rgba(255,255,255,0.4);
		border-top-color: white;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	.spinner-xs-inline {
		width: 10px; height: 10px;
		border: 1.5px solid var(--border);
		border-top-color: var(--text-muted);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
		display: inline-block;
	}

	:global(.spin-icon) { animation: spin 0.7s linear infinite; }

	@keyframes spin { to { transform: rotate(360deg); } }
	@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }

	/* ── Danger zone ── */
	.danger-zone {
		border: 1px solid color-mix(in srgb, #ef4444 40%, var(--border));
		border-radius: var(--radius-sm);
		overflow: hidden;
		margin-top: 8px;
	}

	.danger-header {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 12px;
		background: color-mix(in srgb, #ef4444 8%, transparent);
		color: #ef4444;
		font-size: 11px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	.danger-body { padding: 12px; }

	.danger-row {
		display: flex;
		align-items: center;
		gap: 12px;
		justify-content: space-between;
	}

	.danger-info { display: flex; flex-direction: column; gap: 3px; }
	.danger-title { font-size: 12px; font-weight: 600; color: var(--text-primary); }
	.danger-desc { font-size: 11px; color: var(--text-muted); line-height: 1.5; }

	.btn-danger-outline {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 6px 12px;
		font-size: 12px;
		font-weight: 600;
		color: #ef4444;
		background: transparent;
		border: 1px solid color-mix(in srgb, #ef4444 50%, transparent);
		border-radius: var(--radius-sm);
		cursor: pointer;
		white-space: nowrap;
		transition: all var(--transition-fast);
		flex-shrink: 0;
	}
	.btn-danger-outline:hover {
		background: color-mix(in srgb, #ef4444 10%, transparent);
		border-color: #ef4444;
	}

	/* ── Delete modal ── */
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0,0,0,0.6);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
		padding: 16px;
	}

	.modal-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		width: 100%;
		max-width: 420px;
		display: flex;
		flex-direction: column;
		gap: 0;
		box-shadow: 0 8px 32px rgba(0,0,0,0.4);
	}

	.modal-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
		font-size: 14px;
		font-weight: 700;
		color: var(--text-primary);
	}

	.modal-body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.modal-warning {
		font-size: 12px;
		color: var(--text-primary);
		line-height: 1.5;
		margin: 0;
	}

	.modal-list {
		margin: 0;
		padding-left: 18px;
		display: flex;
		flex-direction: column;
		gap: 3px;
	}

	.modal-list li {
		font-size: 12px;
		color: var(--text-muted);
	}

	.modal-confirm-field { display: flex; flex-direction: column; gap: 5px; }

	.modal-confirm-label {
		font-size: 11px;
		color: var(--text-muted);
		font-weight: 500;
	}

	.modal-confirm-code {
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-elevated);
		padding: 1px 5px;
		border-radius: 3px;
		border: 1px solid var(--border);
		color: var(--text-primary);
	}

	.modal-confirm-input {
		padding: 8px 10px;
		font-size: 12px;
		font-family: var(--font-mono);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		color: var(--text-primary);
		outline: none;
	}
	.modal-confirm-input:focus { border-color: #ef4444; }

	.modal-footer {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: 8px;
		padding: 12px 16px;
		border-top: 1px solid var(--border);
	}

	.btn-danger {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		font-size: 12px;
		font-weight: 600;
		background: #ef4444;
		color: white;
		border: none;
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: opacity var(--transition-fast);
	}
	.btn-danger:hover:not(:disabled) { opacity: 0.88; }
	.btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }

	.spinner-xs-dark {
		width: 12px; height: 12px;
		border: 2px solid rgba(255,255,255,0.3);
		border-top-color: white;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	/* ── Visitor Logs button ── */
	.btn-visitor-logs {
		display: inline-flex; align-items: center; gap: 5px;
		font-size: 11px; font-weight: 600;
		color: var(--text-muted);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 4px 10px; cursor: pointer;
		transition: all var(--transition-fast);
		white-space: nowrap; flex-shrink: 0;
	}
	.btn-visitor-logs:hover { border-color: var(--accent); color: var(--accent); }
	.webhook-provider-tabs {
		display: inline-flex;
		gap: 3px;
		background: var(--bg-elevated);
		padding: 2px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
	}
	.webhook-provider-tabs button {
		background: transparent;
		border: none;
		outline: none;
		font-size: 10px;
		font-weight: 600;
		padding: 2px 8px;
		border-radius: calc(var(--radius-sm) - 1px);
		color: var(--text-muted);
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.webhook-provider-tabs button.active {
		border: 1px solid var(--border);
		color: var(--accent);
		background: rgba(37,99,235,0.07);
	}
	.webhook-url-row { display: flex; gap: 6px; align-items: center; }
	.webhook-url-input {
		flex: 1;
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-secondary);
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 5px 8px;
		outline: none;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.webhook-copy-btn {
		display: flex; align-items: center; gap: 4px;
		font-size: 11px; font-weight: 500; font-family: var(--font-sans);
		padding: 5px 10px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		background: var(--bg-elevated);
		color: var(--text-secondary);
		cursor: pointer;
		white-space: nowrap;
		flex-shrink: 0;
		transition: all var(--transition-fast);
	}
	.webhook-copy-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.webhook-copy-btn:disabled { opacity: 0.5; cursor: default; }
	.webhook-loading { display: flex; align-items: center; gap: 6px; padding: 10px 14px; font-size: 12px; color: var(--text-dim); }
	.webhook-actions {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 0 10px;
		flex-wrap: wrap;
	}
	.webhook-rotate-confirm-text { font-size: 11px; color: var(--text-muted); flex: 1; }
	.webhook-rotate-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-muted);
		font-size: 11px;
		font-family: var(--font-sans);
		padding: 3px 9px;
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.webhook-rotate-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.webhook-rotate-btn.danger { border-color: rgba(239,68,68,0.5); color: #EF4444; }
	.webhook-rotate-btn.danger:hover:not(:disabled) { background: rgba(239,68,68,0.08); }
	.webhook-rotate-btn:disabled { opacity: 0.5; cursor: default; }

	.webhook-status {
		margin-top: 6px;
		font-size: 11px;
		padding: 6px 8px;
		border-radius: var(--radius-sm);
	}
	.webhook-status.success {
		background: rgba(16,185,129,0.08);
		color: #10B981;
		border: 1px solid rgba(16,185,129,0.15);
	}
	.webhook-status.error {
		background: rgba(239,68,68,0.08);
		color: #EF4444;
		border: 1px solid rgba(239,68,68,0.15);
	}
	.webhook-status.info {
		background: color-mix(in srgb, var(--accent) 6%, transparent);
		color: var(--text-muted);
		border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
	}

	/* ── Git config cards (shared pattern with ServiceDetailPanel) ── */
	.git-config-section { display: flex; flex-direction: column; gap: 12px; }

	.git-card {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 14px 16px;
		display: flex; flex-direction: column; gap: 10px;
	}

	.git-card-header {
		display: flex; align-items: center; justify-content: space-between;
	}

	.git-card-title {
		font-size: 13px; font-weight: 700; color: var(--text-primary);
	}

	.git-card-desc {
		font-size: 12px; color: var(--text-muted); line-height: 1.5; margin: 0;
	}

	.git-field { display: flex; flex-direction: column; gap: 5px; }

	.git-label {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
	}

	.git-select {
		width: 100%; padding: 7px 10px; border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		background: var(--bg-surface); color: var(--text-primary);
		font-size: 12px; outline: none; cursor: pointer;
		transition: border-color var(--transition-fast);
	}
	.git-select:focus { border-color: var(--accent); }

	.git-branch-row {
		display: flex; align-items: center;
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		background: var(--bg-surface); overflow: hidden;
		transition: border-color var(--transition-fast);
	}
	.git-branch-row:focus-within { border-color: var(--accent); }

	.git-branch-icon {
		padding: 0 8px; font-size: 13px; color: var(--text-dim);
		background: var(--bg-elevated); border-right: 1px solid var(--border);
		display: flex; align-items: center; height: 32px; flex-shrink: 0;
	}

	.git-branch-input {
		flex: 1; padding: 6px 10px; border: none; outline: none;
		background: transparent; color: var(--text-primary);
		font-size: 12px; font-family: var(--font-mono);
	}

	.git-hint { font-size: 11px; color: var(--text-dim); margin: 0; line-height: 1.4; }
	.git-hint code {
		font-family: var(--font-mono); font-size: 10px;
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
		border: 1px solid var(--border);
	}

	.git-save-row { display: flex; align-items: center; gap: 8px; padding-top: 2px; }
	.git-error { font-size: 11px; color: #ef4444; margin: 0; }
	.git-save-success { font-size: 11px; color: #22c55e; margin: 0; }

	.btn-sm { padding: 5px 10px; font-size: 11px; }
</style>
