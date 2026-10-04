<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import BrandLogo from '$lib/components/BrandLogo.svelte';
	import {
		Play, Square, RefreshCw, Trash2, AlertTriangle,
		GitBranch, Box, FileCode, Terminal, Settings, X,
		ChevronRight, CheckCircle, XCircle, Clock, Loader,
		Eye, EyeOff, Copy, Globe, Plus, Shield, ShieldOff, FileText,
		CheckCircle2, AlertCircle, Loader2, Network, HardDrive, Database, Activity,
		Check, Minus, Circle, Undo2
	} from '@lucide/svelte';
	import DbClientModal from '$lib/components/DbClientModal.svelte';
	import DomainAddPanel from './resources/DomainAddPanel.svelte';
	import NetworkPickerPanel from './resources/NetworkPickerPanel.svelte';
	import VolumeMountList from '$lib/components/VolumeMountList.svelte';
	import type { VolumeMount } from '$lib/components/VolumeMountList.svelte';
	import { formatDistanceToNow } from 'date-fns';
	import {
		Button, Card, Badge, StatusDot, Tabs, KeyValueList, ListRow,
		Spinner, EmptyState, InlineAlert, ConfirmDialog,
		ActivityList, FormField, TextField, Checkbox
	} from '$lib/components/ui';
	import type { KeyValueItem } from '$lib/components/ui';
	import { toDotStatus } from '$lib/utils/status';

	import { api } from '$lib/api/client';
	import { serviceStore } from '$lib/stores/service.store';
	import { containerStore } from '$lib/stores/container.store';
	import { deploymentStore } from '$lib/stores/deployment.store';
	import { uiStore } from '$lib/stores/ui.store';
	import { orgStore } from '$lib/stores/org.store';
	import { topologyStore } from '$lib/stores/topology.store';
	import { can, permProject } from '$lib/auth/permissions';
	import { subscribeToService, subscribeToDeployment } from '$lib/mqtt/subscriptions';
	import { eventBus } from '$lib/mqtt/eventBus';
	import ExecPanel from './ExecPanel.svelte';
	import DeploymentLogsPanel from './DeploymentLogsPanel.svelte';
	import LogViewerOverlay from '$lib/components/LogViewerOverlay.svelte';
	import MonitorViewOverlay from '$lib/components/MonitorViewOverlay.svelte';
	import EnvManagerOverlay from '$lib/components/EnvManagerOverlay.svelte';
	import GitSettingsSection from '$lib/components/GitSettingsSection.svelte';
	import type {
		Service, Container, Deployment, DeploymentStep,
		DeploymentLog, MqttPayload, ContainerStatus, Domain,
		Network as NetworkType, SwarmNode, ConnectionInfo, ArtifactSource, VolumeAdvice, Volume
	} from '$lib/api/types';

	// Portal action — moves the node to document.body so position:fixed works
	// correctly even when a parent has CSS transform applied.
	function portal(node: HTMLElement) {
		document.body.appendChild(node);
		return {
			destroy() { node.remove(); }
		};
	}

	interface Props {
		serviceId: string;
		projectId: string;
		orgId: string;
		onDeleted?: () => void;
		initialTab?: Tab;
	}

	let { serviceId, projectId, orgId, onDeleted, initialTab }: Props = $props();

	// ── Permission gates ─────────────────────────────────────────────
	let myRole  = $derived($orgStore.myMembership?.role ?? null);
	let myPerms = $derived($orgStore.myMembership?.permissions ?? []);
	let canDeploy = $derived(can(myRole, myPerms, permProject(orgId, projectId, 'service', 'deploy')));
	let canWrite  = $derived(can(myRole, myPerms, permProject(orgId, projectId, 'service', 'write')));
	let canDelete = $derived(can(myRole, myPerms, permProject(orgId, projectId, 'service', 'delete')));

	// ── Tabs ─────────────────────────────────────────────────────────
	type Tab = 'overview' | 'deploy' | 'logs' | 'git' | 'replicas' | 'volumes' | 'domains' | 'settings';
	let activeTab = $state<Tab>(initialTab ?? 'overview');

	// ── Core state ───────────────────────────────────────────────────
	let service = $state<Service | null>(null);
	let containers = $state<Container[]>([]);
	let deployments = $state<Deployment[]>([]);
	let steps = $state<DeploymentStep[]>([]);

	// node_id → hostname map, loaded once for the replica tab
	let nodeMap = $state<Map<string, SwarmNode>>(new Map());
	async function ensureNodes() {
		if (nodeMap.size > 0) return;
		try {
			const res = await api.getSwarmNodes(orgId);
			if (!res.error && res.data) {
				nodeMap = new Map(res.data.map(n => [n.id, n]));
			}
		} catch { /* single-node setup — no swarm, ignore */ }
	}

	let isLoadingService = $state(true);
	let isLoadingContainers = $state(false);
	let isDeploying = $state(false);
	let isStopping = $state(false);
	let isRestarting = $state(false);
	let isRollingBack = $state<string | null>(null); // deployment id being rolled back
	let serviceError = $state<string | null>(null);

	// ── Domains ──────────────────────────────────────────────────────
	let domains = $state<Domain[]>([]);
	let isLoadingDomains = $state(false);
	let domainError = $state('');
	let dnsCheckState = $state<Record<string, 'idle' | 'checking' | 'ok' | 'fail'>>({});
	let dnsCheckAddresses = $state<Record<string, string[]>>({});

	// ── Volumes ──────────────────────────────────────────────────────
	let volumes = $state<Volume[]>([]);
	let isLoadingVolumes = $state(false);
	let volumeError = $state('');

	// ── Container logs ────────────────────────────────────────────────
	let containerLogsTarget = $state<Container | null>(null);
	let containerLogs      = $state<string[]>([]);        // one-shot initial fetch
	let isLoadingContainerLogs = $state(false);

	// real-time stream
	type LogStatus = 'idle' | 'connecting' | 'connected' | 'error';
	let clogSource: EventSource | null = null;
	let clogStatus  = $state<LogStatus>('idle');
	let clogLines   = $state<string[]>([]);
	let clogError   = $state('');
	let clogEl      = $state<HTMLDivElement | null>(null);
	let clogTail    = $state(200);
	let clogSearch  = $state('');

	const CLOG_TAIL_OPTIONS = [50, 100, 200, 500, 1000] as const;

	let filteredContainerLogs = $derived(
		clogSearch
			? containerLogs.filter(l => l.toLowerCase().includes(clogSearch.toLowerCase()))
			: containerLogs
	);
	let filteredClogLines = $derived(
		clogSearch
			? clogLines.filter(l => l.toLowerCase().includes(clogSearch.toLowerCase()))
			: clogLines
	);

	// ── Env panel ────────────────────────────────────────────────────
	let showEnvPanel    = $state(false);
	let showExecPanel   = $state(false);
	let showDbClient    = $state(false);
	let showMonitor     = $state(false);

	// ── Settings edit state ──────────────────────────────────────────
	let editReplicas = $state<number | null>(1);
	let editPorts = $state<string[]>([]);
	let editImage = $state('');
	let editCpuLimit = $state<number | null>(null);
	let editMemLimit = $state<number | null>(null);
	let editRegistryUrl = $state('');
	let editRegistryUser = $state('');
	let editRegistryPass = $state('');
	let registryPassIsSet = $state(false);
	let editVolumeMounts = $state<VolumeMount[]>([]);
	let isLoadingSettingsEnvs = $state(false);
	let editNetworks = $state<NetworkType[]>([]);
	let isLoadingSettingsNetworks = $state(false);
	let isSavingSettings = $state(false);
	let settingsSaveError = $state('');
	let settingsSaveSuccess = $state(false);

	// Artifact source (Shipyard registry binding) — only set for services deployed
	// from the internal registry. `null` hides the settings section entirely.
	let artifactSource = $state<ArtifactSource | null>(null);
	let editAutoDeployOnPush = $state(false);
	let isLoadingArtifactSource = $state(false);

	// Anonymous-volume advice — image-declared VOLUME paths with no configured mount.
	let volumeAdvice = $state<VolumeAdvice | null>(null);

	// ── Danger zone / Delete ─────────────────────────────────────────
	let showDeleteConfirm = $state(false);
	let isDeleting = $state(false);
	let deleteError = $state('');

	// ── Connection info ───────────────────────────────────────────────
	let connInfo        = $state<ConnectionInfo | null>(null);
	let connInfoLoading = $state(false);
	let connInfoCopied  = $state(false);
	let connHostRevealed = $state(false);
	let connUrlRevealed  = $state(false);

	async function loadConnectionInfo() {
		if (!service) return;
		connInfoLoading = true;
		try {
			const res = await api.getConnectionInfo(projectId, service.id);
			if (!res.error && res.data) connInfo = res.data;
		} catch { /* ignore */ } finally {
			connInfoLoading = false;
		}
	}

	// ── Webhook / Git deploy config ───────────────────────────────────
	let webhookToken      = $state('');
	let webhookProvider   = $state<'github' | 'gitlab' | 'gitea'>('github');
	let isLoadingWebhook  = $state(false);
	let webhookCopied     = $state(false);
	let isRotatingWebhook = $state(false);
	let rotateConfirm     = $state(false);

	// Git deploy config — local editable copies
	let gitAutoDeploy   = $state(true);
	let gitBranch       = $state('main');
	let gitDeployStrategy   = $state<'push' | 'tag' | 'pull_request'>('push');
	let gitDeployBranch     = $state('');
	let gitDeployTagPattern = $state('');
	let gitSaving       = $state(false);
	let gitSaveOk       = $state(false);
	let gitSaveError    = $state('');

	let gitProviderId       = $state('');
	let orgGitProviders     = $state<import('$lib/api/types').GitProvider[]>([]);
	let loadingGitProviders = $state(false);

	// ── MQTT cleanup ─────────────────────────────────────────────────
	let unsubscribeService: (() => void) | null = null;
	let unsubscribeDeployment: (() => void) | null = null;

	// ── Derived ──────────────────────────────────────────────────────
	let latestDeployment = $derived(deployments[0] ?? null);
	let runningContainers = $derived(containers.filter(c => c.status === 'running'));
	// True when a deployment pipeline is actively running — disables deploy/redeploy/restart.
	let isDeploymentRunning = $derived(
		latestDeployment?.status === 'running' ||
		latestDeployment?.status === 'queued'  ||
		latestDeployment?.status === 'pending'
	);

	// ── Status helpers ───────────────────────────────────────────────

	function statusLabel(status: string): string {
		const map: Record<string, string> = {
			running: 'Running', stopping: 'Stopping', deploying: 'Deploying',
			need_attention: 'Need attention', queued: 'Queued',
			pending: 'Pending', preparing: 'Preparing',
			failed: 'Failed', rejected: 'Rejected', stopped: 'Stopped',
			shutdown: 'Shutdown', orphan: 'Orphan', complete: 'Complete'
		};
		return map[status] ?? status ?? 'Unknown';
	}

	function deployStatusClass(s: string) {
		switch (s) {
			case 'success': return 'running';
			case 'running': return 'pending';
			case 'queued':  return 'queued';
			case 'failed':  return 'failed';
			default:        return 'stopped';
		}
	}

	function deployBadgeTone(s: string): 'green' | 'blue' | 'yellow' | 'red' | 'neutral' {
		switch (s) {
			case 'success': return 'green';
			case 'running': return 'blue';
			case 'queued':  return 'yellow';
			case 'failed':  return 'red';
			default:        return 'neutral';
		}
	}

	function stepTone(s: string): 'blue' | 'green' | 'red' | 'yellow' {
		switch (s) {
			case 'success': return 'green';
			case 'failed':  return 'red';
			case 'skipped': return 'yellow';
			default:        return 'blue';
		}
	}

	const webhookProviderTabs = (['github', 'gitlab', 'gitea'] as const).map((p) => ({
		id: p,
		label: p.charAt(0).toUpperCase() + p.slice(1)
	}));

	let webhookItems = $derived<KeyValueItem[]>([
		{
			key: 'URL',
			mono: true,
			value: webhookToken
				? `${window.location.origin}/api/webhooks/${webhookProvider}/${serviceId}/${webhookToken}`
				: `${window.location.origin}/api/webhooks/${webhookProvider}/${serviceId}/…`
		}
	]);

	function formatTime(ts: string | null | undefined): string {
		if (!ts) return '–';
		try { return formatDistanceToNow(new Date(ts), { addSuffix: true }); }
		catch { return ts; }
	}

	function logLevelClass(level: string): string {
		switch (level) {
			case 'error': return 'log-error';
			case 'warn':  return 'log-warn';
			case 'debug': return 'log-debug';
			default:      return 'log-info';
		}
	}

	function typeLabel(t: string): string {
		const map: Record<string, string> = {
			docker: 'Docker', git: 'Git', docker_compose: 'Compose',
			database: 'Database', static: 'Static', manual: 'Manual'
		};
		return map[t] ?? t;
	}

	// ── Data loading ─────────────────────────────────────────────────
	async function loadService() {
		isLoadingService = true;
		serviceError = null;
		const res = await api.get<Service>(`/projects/${projectId}/services/${serviceId}`);
		if (res.error) serviceError = res.error.message;
		else if (res.data) { service = res.data; serviceStore.setActiveService(res.data); }
		isLoadingService = false;
	}

	async function loadContainers() {
		isLoadingContainers = true;
		const res = await api.get<Container[]>(`/services/${serviceId}/containers`);
		if (res.data) {
			containers = res.data.sort((a, b) => (a.replica_index ?? 0) - (b.replica_index ?? 0));
			containerStore.loadForService(serviceId, res.data);
		}
		isLoadingContainers = false;
	}

	async function loadDeployments() {
		const res = await api.get<Deployment[]>(`/services/${serviceId}/deployments`);
		if (res.data) {
			deployments = res.data.sort(
				(a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime()
			);
			deploymentStore.setDeployments(res.data);
		}
	}

	function openDeploymentLogs(dep: Deployment) {
		uiStore.pushPanel({
			component: DeploymentLogsPanel,
			title: `Deployment ${dep.id.slice(0, 8)}`,
			key: `dep-logs-${dep.id}`,
			props: { orgId, projectId, serviceId, deployment: dep },
		});
	}

	async function loadDomains() {
		isLoadingDomains = true;
		domainError = '';
		const res = await api.get<Domain[]>(`/services/${serviceId}/domains`);
		if (res.data) domains = res.data;
		else if (res.error) domainError = res.error.message;
		isLoadingDomains = false;
	}

	async function loadVolumes() {
		isLoadingVolumes = true;
		volumeError = '';
		const res = await api.getVolumes(serviceId);
		if (res.data) volumes = res.data;
		else if (res.error) volumeError = res.error.message;
		isLoadingVolumes = false;
	}

	function fmtVolumeSize(mb: number): string {
		return mb >= 1024 ? `${(mb / 1024).toFixed(1)} GB` : `${mb} MB`;
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

	// ── Container record delete ───────────────────────────────────────
	const TERMINAL_STATUSES = new Set(['shutdown', 'failed', 'orphan', 'complete', 'rejected']);

	function isTerminal(status: string) { return TERMINAL_STATUSES.has(status); }

	let replicaDeleteError = $state<string | null>(null);
	let deletingContainerId = $state<string | null>(null);

	async function deleteContainerRecord(containerId: string) {
		replicaDeleteError = null;
		deletingContainerId = containerId;
		const res = await api.deleteContainer(serviceId, containerId);
		deletingContainerId = null;
		if (res.error) {
			replicaDeleteError = res.error.message ?? 'Failed to delete container record';
		} else {
			containers = containers.filter(c => c.id !== containerId);
		}
	}

	async function removeDomain(domainId: string) {
		const res = await api.delete(`/services/${serviceId}/domains/${domainId}`);
		if (!res.error) domains = domains.filter(d => d.id !== domainId);
	}

	async function validateDns(domainId: string) {
		dnsCheckState = { ...dnsCheckState, [domainId]: 'checking' };
		const res = await api.checkDomainDns(serviceId, domainId);
		if (res.data) {
			dnsCheckState = { ...dnsCheckState, [domainId]: res.data.resolves ? 'ok' : 'fail' };
			dnsCheckAddresses = { ...dnsCheckAddresses, [domainId]: res.data.addresses };
		} else {
			dnsCheckState = { ...dnsCheckState, [domainId]: 'fail' };
		}
	}

	// ── Log line parsing ──────────────────────────────────────────────────────

	type LogLevel = 'error' | 'warn' | 'debug' | 'info' | 'trace';

	interface ParsedLog {
		ts: string;       // time portion only, e.g. "20:42:40.282"
		level: LogLevel;
		target: string;   // module/crate name if present
		content: string;  // the actual message
	}

	function stripAnsi(s: string): string {
		// Strips ESC[ sequences and bare [ sequences (ESC char invisible in SSE stream)
		return s.replace(/(\x1b\[|(?<!\w)\[)[0-9;]*[A-Za-z]/g, '');
	}

	const ISO_RE = /^(\d{4}-\d{2}-\d{2}T(\d{2}:\d{2}:\d{2})(?:\.(\d+))?Z?)\s*/;
	const LVL_RE = /^\s*(ERROR|FATAL|CRITICAL|WARN(?:ING)?|INFO|DEBUG|TRACE)\s*/i;
	// Rust tracing/log target: "some_crate::module " or "some_crate " followed by content
	const TARGET_RE = /^([a-zA-Z_][a-zA-Z0-9_:]*)\s+/;

	function parseLine(raw: string): ParsedLog {
		const clean = stripAnsi(raw);

		let rest = clean.trimStart();
		let ts = '';

		// First ISO timestamp (Docker --timestamps prefix)
		let m = ISO_RE.exec(rest);
		if (m) {
			const ms = m[3] ? m[3].slice(0, 3) : '000';
			ts = `${m[2]}.${ms}`;
			rest = rest.slice(m[0].length);
		}

		// Optional second ISO timestamp (from app's own logger)
		m = ISO_RE.exec(rest);
		if (m) {
			if (!ts) { const ms = m[3] ? m[3].slice(0, 3) : '000'; ts = `${m[2]}.${ms}`; }
			rest = rest.slice(m[0].length);
		}

		// Level keyword
		let level: LogLevel = 'info';
		const lvlM = LVL_RE.exec(rest);
		if (lvlM) {
			const raw_lvl = lvlM[1].toUpperCase();
			if (raw_lvl === 'ERROR' || raw_lvl === 'FATAL' || raw_lvl === 'CRITICAL') level = 'error';
			else if (raw_lvl === 'WARN' || raw_lvl === 'WARNING') level = 'warn';
			else if (raw_lvl === 'DEBUG') level = 'debug';
			else if (raw_lvl === 'TRACE') level = 'trace';
			else level = 'info';
			rest = rest.slice(lvlM[0].length);
		} else {
			// Fallback: guess from keywords
			const lo = clean.toLowerCase();
			if (/\b(error|fatal|critical)\b/.test(lo)) level = 'error';
			else if (/\bwarn(ing)?\b/.test(lo)) level = 'warn';
			else if (/\bdebug\b/.test(lo)) level = 'debug';
			else if (/\btrace\b/.test(lo)) level = 'trace';
		}

		// Optional rust-style target (module::path or crate_name)
		let target = '';
		const tgtM = TARGET_RE.exec(rest);
		// Only treat it as a target if it looks like a module path (contains _ or ::)
		if (tgtM && (tgtM[1].includes('_') || tgtM[1].includes('::'))) {
			target = tgtM[1];
			rest = rest.slice(tgtM[0].length);
			// Strip a leading colon/space separator if present
			rest = rest.replace(/^:\s*/, '');
		}

		return { ts, level, target, content: rest.trim() };
	}

	async function openContainerLogs(c: Container) {
		// Close any previous stream
		clogSource?.close();
		clogSource = null;
		clogStatus = 'idle';
		clogLines = [];
		clogError = '';
		clogSearch = '';

		containerLogsTarget = c;
		isLoadingContainerLogs = true;
		containerLogs = [];
		const res = await api.get<string[]>(
			`/services/${serviceId}/containers/${c.docker_container_id}/logs?tail=${clogTail}&timestamps=true`
		);
		if (res.data) containerLogs = res.data;
		isLoadingContainerLogs = false;
	}

	let showLogCloseConfirm = $state(false);

	function requestCloseContainerLogs() {
		showLogCloseConfirm = true;
	}

	function closeContainerLogs() {
		showLogCloseConfirm = false;
		clogSource?.close();
		clogSource = null;
		clogStatus = 'idle';
		containerLogsTarget = null;
		containerLogs = [];
		clogLines = [];
	}

	function connectContainerLogs() {
		if (!containerLogsTarget || clogSource) return;
		clogStatus = 'connecting';
		clogError = '';
		clogLines = [];

		const cid = containerLogsTarget.docker_container_id;
		const es = new EventSource(`/api/services/${serviceId}/containers/${cid}/logs/stream?tail=${clogTail}`);
		clogSource = es;

		es.onopen = () => { clogStatus = 'connected'; };

		es.onmessage = (e) => {
			if (!e.data?.trim()) return;
			clogLines = [...clogLines, e.data];
			// auto-scroll
			if (clogEl) requestAnimationFrame(() => {
				if (clogEl) clogEl.scrollTop = clogEl.scrollHeight;
			});
		};

		es.addEventListener('error', (e: MessageEvent) => {
			clogError = e.data ?? 'Stream error';
			clogStatus = 'error';
		});

		es.onerror = () => {
			if (clogStatus === 'connecting') {
				clogError = 'Could not connect';
				clogStatus = 'error';
				es.close();
				clogSource = null;
			}
		};
	}

	function disconnectContainerLogs() {
		clogSource?.close();
		clogSource = null;
		clogStatus = 'idle';
	}

	async function loadStepsForLatest() {
		if (!latestDeployment) return;
		const res = await api.get<DeploymentStep[]>(`/deployments/${latestDeployment.id}/steps`);
		if (res.data) {
			steps = res.data.sort((a, b) => a.order_index - b.order_index);
			deploymentStore.setSteps(res.data);
		}
	}

	// ── Actions ──────────────────────────────────────────────────────
	async function triggerDeploy() {
		if (!service || isDeploying) return;
		isDeploying = true;

		// Optimistically push "deploying" to canvas node immediately.
		topologyStore.refreshNode(`svc_${serviceId}`, { data: { status: 'deploying' } });

		const res = await api.post<Deployment>(`/services/${serviceId}/deploy`);
		if (res.data) {
			const depId = res.data.id;
			deployments = [res.data, ...deployments];
			deploymentStore.setActiveDeployment(res.data);
			unsubscribeDeployment?.();
			unsubscribeDeployment = subscribeToDeployment(orgId, projectId, serviceId, depId);

			// Track all status changes so banner stays accurate.
			const depStatusTopic = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}/deployments/${depId}/status`;
			const onDepStatus = (payload: MqttPayload) => {
				const evt = payload.event ?? '';
				const mqttStatus = (payload.meta as any)?.status as string | undefined;
				const newStatus: Deployment['status'] | null =
					(evt.includes('success') || mqttStatus === 'success') ? 'success' :
					(evt.includes('failed')  || mqttStatus === 'failed')  ? 'failed'  :
					(evt.includes('cancel')  || mqttStatus === 'cancelled') ? 'cancelled' :
					(mqttStatus === 'running') ? 'running' : null;

				if (newStatus) {
					deployments = deployments.map(d => d.id === depId ? { ...d, status: newStatus! } : d);
				}
				// Unsubscribe on terminal states only.
				if (newStatus === 'success' || newStatus === 'failed' || newStatus === 'cancelled') {
					eventBus.off(depStatusTopic, onDepStatus);
				}
			};
			eventBus.on(depStatusTopic, onDepStatus);

			await loadStepsForLatest();
		} else {
			// API call failed — revert canvas optimistic update.
			topologyStore.refreshNode(`svc_${serviceId}`, { data: { status: service?.status ?? 'stopped' } });
		}
		isDeploying = false;
	}

	let _stopTimeoutId: ReturnType<typeof setTimeout> | null = null;

	async function triggerStop() {
		if (!service || isStopping) return;
		isStopping = true;

		// Optimistic UI — show "stopping" immediately on panel + canvas node
		service = { ...service, status: 'stopping' };
		topologyStore.refreshNode(`svc_${serviceId}`, { data: { status: 'stopping' } });

		const res = await api.post(`/services/${serviceId}/stop`);
		if (res.error) {
			// Revert on failure
			await loadService();
			isStopping = false;
			return;
		}

		// Keep isStopping = true — MQTT will clear it via handleServiceStatus.
		// Safety fallback: clear after 30 s in case the MQTT event never arrives.
		if (_stopTimeoutId) clearTimeout(_stopTimeoutId);
		_stopTimeoutId = setTimeout(() => {
			isStopping = false;
			_stopTimeoutId = null;
		}, 30_000);
	}

	async function triggerRestart() {
		if (!service || isRestarting) return;
		isRestarting = true;
		await api.restartService(serviceId);
		isRestarting = false;
	}

	async function triggerRedeploy() {
		if (!service || isDeploying) return;
		isDeploying = true;
		const res = await api.post<Deployment>(`/services/${serviceId}/redeploy`);
		if (res.data) deployments = [res.data, ...deployments];
		isDeploying = false;
	}

	async function triggerRollback(dep: Deployment, e: MouseEvent) {
		e.stopPropagation();
		if (isRollingBack) return;
		isRollingBack = dep.id;
		const res = await api.rollbackDeployment(serviceId, dep.id);
		if (res.data) await loadDeployments();
		isRollingBack = null;
	}

	async function deleteService() {
		if (!service || isDeleting) return;
		isDeleting = true;
		deleteError = '';

		// Stop the service first if it's running (swarm cleanup)
		if (service.status === 'running') {
			await api.post(`/services/${serviceId}/stop`);
		}

		const res = await api.deleteService(projectId, serviceId);
		if (res.error) {
			deleteError = res.error.message;
			isDeleting = false;
			return;
		}

		// Notify parent to close panel and refresh topology
		onDeleted?.();
	}

	function initSettingsFromService() {
		if (!service) return;
		editReplicas = service.replicas;
		editPorts = [...(service.ports ?? [])];
		editImage = service.image ?? '';
		editCpuLimit = (service as any).cpu_limit ?? null;
		editMemLimit = (service as any).memory_limit_mb ?? null;
		settingsSaveError = '';
		settingsSaveSuccess = false;
	}

	async function loadSettingsEnvs() {
		isLoadingSettingsEnvs = true;
		const res = await api.getServiceEnvs(serviceId);
		if (res.data) {
			for (const env of res.data) {
				if (env.key === 'DOCKER_REGISTRY')  editRegistryUrl  = env.value_encrypted ?? '';
				if (env.key === 'DOCKER_USERNAME')  editRegistryUser = env.value_encrypted ?? '';
				if (env.key === 'DOCKER_PASSWORD') {
					editRegistryPass = '';
					registryPassIsSet = true;
				}
				if (env.key === '__VOLUME_MOUNTS__') {
					try { editVolumeMounts = JSON.parse(env.value_encrypted ?? '[]'); } catch { editVolumeMounts = []; }
				}
			}
		}
		isLoadingSettingsEnvs = false;
	}

	async function loadSettingsNetworks() {
		isLoadingSettingsNetworks = true;
		const res = await api.getServiceNetworks(serviceId);
		if (res.data) editNetworks = res.data;
		isLoadingSettingsNetworks = false;
	}

	async function loadArtifactSource() {
		isLoadingArtifactSource = true;
		const res = await api.getArtifactSource(serviceId);
		// 404 (no binding) is expected for git / external-registry services.
		artifactSource = res.data ?? null;
		editAutoDeployOnPush = artifactSource?.auto_deploy_on_push ?? false;
		isLoadingArtifactSource = false;
	}

	async function loadVolumeAdvice() {
		const res = await api.getVolumeAdvice(serviceId);
		volumeAdvice = res.data ?? null;
	}

	function openNetworkPickerForSettings() {
		uiStore.pushPanel({
			component: NetworkPickerPanel,
			title: 'Add Network',
			props: {
				projectId,
				initialSelected: editNetworks.map(n => n.id),
				onConfirm: async (_ids: string[], items: NetworkType[]) => {
					for (const net of items) {
						if (!editNetworks.find(n => n.id === net.id)) {
							await api.attachNetwork(projectId, net.id, serviceId);
							editNetworks = [...editNetworks, net];
						}
					}
				},
			},
		});
	}

	async function removeSettingsNetwork(networkId: string) {
		await api.detachNetwork(projectId, networkId, serviceId);
		editNetworks = editNetworks.filter(n => n.id !== networkId);
	}

	async function saveSettings() {
		if (!service || isSavingSettings) return;
		isSavingSettings = true;
		settingsSaveError = '';
		settingsSaveSuccess = false;
		const ports = editPorts.map(p => p.trim()).filter(Boolean);
		const image = editImage.trim();
		const res = await api.updateService(projectId, serviceId, {
			replicas: editReplicas as number, // null when the field is cleared (pre-migration behavior)
			ports,
			...(image ? { image } : {}),
			...(editCpuLimit !== null ? { cpu_limit: editCpuLimit } : {}),
			...(editMemLimit !== null ? { memory_limit_mb: editMemLimit } : {}),
		});
		if (res.error) {
			settingsSaveError = res.error.message;
			isSavingSettings = false;
			return;
		}
		if (res.data) {
			service = res.data;
			editPorts = [...(res.data.ports ?? [])];
			editReplicas = res.data.replicas;
			editImage = res.data.image ?? '';
			editCpuLimit = (res.data as any).cpu_limit ?? null;
			editMemLimit = (res.data as any).memory_limit_mb ?? null;
		}

		// Save registry credentials as env vars (only if non-empty)
		const registryEnvs: Array<{ key: string; value: string; is_secret: boolean }> = [];
		if (editRegistryUrl.trim())  registryEnvs.push({ key: 'DOCKER_REGISTRY', value: editRegistryUrl.trim(), is_secret: false });
		if (editRegistryUser.trim()) registryEnvs.push({ key: 'DOCKER_USERNAME', value: editRegistryUser.trim(), is_secret: false });
		if (editRegistryPass.trim()) registryEnvs.push({ key: 'DOCKER_PASSWORD', value: editRegistryPass.trim(), is_secret: true });
		for (const env of registryEnvs) {
			await api.upsertEnv(serviceId, env);
		}
		if (editRegistryPass.trim()) { editRegistryPass = ''; registryPassIsSet = true; }

		// Save volume mounts as __VOLUME_MOUNTS__ env var
		const validMounts = editVolumeMounts.filter(m => m.source.trim() && m.target.trim());
		await api.upsertEnv(serviceId, {
			key: '__VOLUME_MOUNTS__',
			value: JSON.stringify(validMounts),
			is_secret: false,
		});

		// Persist the auto-deploy-on-push flag for artifact-source services,
		// re-sending the unchanged namespace/repo/tag binding.
		if (artifactSource && editAutoDeployOnPush !== artifactSource.auto_deploy_on_push) {
			const res = await api.putArtifactSource(serviceId, {
				namespace_id: artifactSource.namespace_id,
				repo: artifactSource.repo,
				tag: artifactSource.tag,
				auto_deploy_on_push: editAutoDeployOnPush,
			});
			if (res.data) artifactSource = res.data;
		}

		settingsSaveSuccess = true;
		setTimeout(() => { settingsSaveSuccess = false; }, 2500);
		isSavingSettings = false;
	}

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

	async function loadGitProviders() {
		loadingGitProviders = true;
		const res = await api.listGitProviders(orgId);
		if (res.data) {
			orgGitProviders = res.data;
		}
		loadingGitProviders = false;
	}

	function initGitConfig() {
		if (!service) return;
		gitAutoDeploy = service.auto_deploy;
		gitBranch     = service.git_branch ?? 'main';
		gitDeployStrategy = service.git_deploy_strategy || 'push';
		gitDeployBranch   = service.git_deploy_branch || '';
		gitDeployTagPattern = service.git_deploy_tag_pattern || '';
		gitProviderId = service.git_provider_id || '';
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

	async function saveGitConfig() {
		gitSaving = true;
		gitSaveOk = false;
		gitSaveError = '';
		webhookRegStatus = null;

		const branchVal = gitDeployBranch.trim() === '' ? null : gitDeployBranch.trim();
		const tagPatternVal = gitDeployTagPattern.trim() === '' ? null : gitDeployTagPattern.trim();
		const providerVal = gitProviderId === '' ? null : gitProviderId;

		const res = await api.updateService(projectId, serviceId, {
			git_branch: gitBranch.trim() || 'main',
			auto_deploy: gitAutoDeploy,
			git_deploy_strategy: gitDeployStrategy,
			git_deploy_branch: branchVal,
			git_deploy_tag_pattern: tagPatternVal,
			git_provider_id: providerVal,
		});
		if (res.error) {
			gitSaveError = res.error.message;
		} else {
			if (res.data) {
				service = res.data;
				initGitConfig();
			}
			gitSaveOk = true;
			setTimeout(() => { gitSaveOk = false; }, 2500);

			// Auto-register webhook when auto deploy is on and provider is GitHub/GitLab
			const selectedProvider = orgGitProviders.find(p => p.id === providerVal);
			if (gitAutoDeploy && selectedProvider && (selectedProvider.provider_type === 'github' || selectedProvider.provider_type === 'gitlab')) {
				webhookRegStatus = await tryAutoRegisterWebhook();
				if (webhookRegStatus.ok) {
					setTimeout(() => { webhookRegStatus = null; }, 5000);
				}
			}
		}
		gitSaving = false;
	}

	function addPort() { editPorts = [...editPorts, '']; }
	function removePort(i: number) { editPorts = editPorts.filter((_, idx) => idx !== i); }
	function updatePort(i: number, val: string) {
		editPorts = editPorts.map((p, idx) => idx === i ? val : p);
	}

	// ── Tab change ───────────────────────────────────────────────────
	async function switchTab(tab: Tab) {
		activeTab = tab;
		if (tab === 'overview' && !connInfo && !connInfoLoading) void loadConnectionInfo();
		if (tab === 'replicas') { if (containers.length === 0) await loadContainers(); await ensureNodes(); }
		if (tab === 'deploy' || tab === 'logs') await loadDeployments();
		if (tab === 'logs') void loadWebhookToken();
		if (tab === 'git') { initGitConfig(); void loadWebhookToken(); void loadGitProviders(); }
		if (tab === 'deploy' && latestDeployment && steps.length === 0) await loadStepsForLatest();
		if (tab === 'volumes' && volumes.length === 0) await loadVolumes();
		if (tab === 'domains' && domains.length === 0) await loadDomains();
		if (tab === 'settings') {
			initSettingsFromService();
			editRegistryUrl = '';
			editRegistryUser = '';
			editRegistryPass = '';
			registryPassIsSet = false;
			editVolumeMounts = [];
			editNetworks = [];
			await Promise.all([
				loadSettingsEnvs(),
				loadSettingsNetworks(),
				loadArtifactSource(),
				loadVolumeAdvice(),
			]);
		}
	}

	// ── MQTT handlers ────────────────────────────────────────────────
	function handleServiceStatus(payload: MqttPayload) {
		const meta = payload.meta as any;
		if (meta?.status && service) {
			service = { ...service, status: meta.status, replicas: meta.replicas ?? service.replicas };
			// MQTT confirmed the new state — clear the stopping overlay and safety timer
			if (isStopping) {
				isStopping = false;
				if (_stopTimeoutId) { clearTimeout(_stopTimeoutId); _stopTimeoutId = null; }
			}
		}
	}

	function handleContainers(payload: MqttPayload) {
		const meta = payload.meta as any;
		if (Array.isArray(meta?.containers)) {
			containers = (meta.containers as Container[]).sort(
				(a, b) => (a.replica_index ?? 0) - (b.replica_index ?? 0)
			);
			containerStore.handleMqttFullUpdate(serviceId, containers);
		} else if (meta?.container_id && meta?.status) {
			containers = containers.map((c) =>
				c.id === meta.container_id
					? { ...c, status: meta.status as ContainerStatus, status_message: meta.message ?? c.status_message }
					: c
			);
			containerStore.handleMqttStatusUpdate(serviceId, meta.container_id, {
				status: meta.status, status_message: meta.message
			});
		}
	}

	// ── Lifecycle ────────────────────────────────────────────────────
	let serviceStatusTopic = '';
	let serviceContainersTopic = '';

	onMount(async () => {
		serviceStatusTopic = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}/status`;
		serviceContainersTopic = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}/containers`;
		await loadService();
		await Promise.all([loadDeployments(), loadContainers()]);
		if (activeTab === 'replicas') await ensureNodes();
		if (activeTab === 'volumes') await loadVolumes();
		if (activeTab === 'domains') await loadDomains();
		if (latestDeployment) await loadStepsForLatest();
		void loadConnectionInfo();
		unsubscribeService = subscribeToService(orgId, projectId, serviceId);
		eventBus.on(serviceStatusTopic, handleServiceStatus);
		eventBus.on(serviceContainersTopic, handleContainers);
	});

	onDestroy(() => {
		unsubscribeService?.();
		unsubscribeDeployment?.();
		clogSource?.close();
		if (_stopTimeoutId) { clearTimeout(_stopTimeoutId); _stopTimeoutId = null; }
		if (serviceStatusTopic) eventBus.off(serviceStatusTopic, handleServiceStatus);
		if (serviceContainersTopic) eventBus.off(serviceContainersTopic, handleContainers);
		serviceStore.setActiveService(null);
	});

	const baseTabs: { id: Tab; label: string }[] = [
		{ id: 'overview',  label: 'Overview'  },
		{ id: 'deploy',    label: 'Deploy'    },
		{ id: 'logs',      label: 'Logs'      },
		{ id: 'replicas',  label: 'Replicas'  },
		{ id: 'volumes',   label: 'Volumes'   },
		{ id: 'domains',   label: 'Domains'   },
		{ id: 'settings',  label: 'Settings'  },
	];
	let tabs = $derived(
		service?.type === 'git'
			? [
				{ id: 'overview' as Tab, label: 'Overview' },
				{ id: 'deploy'   as Tab, label: 'Deploy'   },
				{ id: 'logs'     as Tab, label: 'Logs'     },
				{ id: 'git'      as Tab, label: 'Git'      },
				{ id: 'replicas' as Tab, label: 'Replicas' },
				{ id: 'volumes'  as Tab, label: 'Volumes'  },
				{ id: 'domains'  as Tab, label: 'Domains'  },
				{ id: 'settings' as Tab, label: 'Settings' },
			]
			: baseTabs
	);

	// ── Overview (KeyValueList data + snippets) ──────────────────────
	let overviewItems = $derived<KeyValueItem[]>(
		service
			? [
				{ key: 'Name', value: service.name },
				{ key: 'Slug', value: service.slug, mono: true },
				{ key: 'Hostname', value: service.slug, mono: true },
				{ key: 'Status', value: statusLabel(service.status) },
				{ key: 'Replicas', value: service.replicas },
				{ key: 'Created', value: formatTime(service.created_at) },
				{ key: 'Updated', value: formatTime(service.updated_at) },
			]
			: []
	);

	// Secret rows deliberately carry no `value` so KeyValueList never exposes
	// the (blurred) text through a hover tooltip; the snippet renders it instead.
	let connItems = $derived<KeyValueItem[]>([
		...(connInfo && connInfo.driver !== 'TCP' ? [{ key: 'Driver' }] : []),
		{ key: 'Host' },
		...(connInfo ? [{ key: 'URL' }] : []),
	]);

	async function copyConnUrl() {
		if (!connInfo) return;
		await navigator.clipboard.writeText(connInfo.url_template);
		connInfoCopied = true;
		setTimeout(() => connInfoCopied = false, 1500);
	}

	// Delete confirmation: ConfirmDialog owns the type-to-confirm text, so the
	// failure path returns false to keep the dialog open with `deleteError`.
	async function confirmDeleteService(): Promise<boolean> {
		await deleteService();
		return !deleteError;
	}
</script>

<!-- ─── Overview snippets (KeyValueList cells) ───────────────────────── -->
{#snippet ovValue(item: KeyValueItem)}
	{#if item.key === 'Status' && service}
		<StatusDot status={toDotStatus(service.status)} />
		<span class="kv-text">{item.value}</span>
	{:else}
		<span class="kv-text">{item.value === null || item.value === undefined || item.value === '' ? '—' : item.value}</span>
	{/if}
{/snippet}

{#snippet ovAction(item: KeyValueItem)}
	{#if item.key === 'Hostname'}
		<Button
			variant="ghost"
			size="icon"
			title="Copy hostname — use this to connect from other containers"
			aria-label="Copy hostname"
			onclick={() => navigator.clipboard.writeText(service?.slug || '')}
		><Copy size={12} /></Button>
	{/if}
{/snippet}

{#snippet connValue(item: KeyValueItem)}
	{#if item.key === 'Driver'}
		<Badge tone="blue">{connInfo?.driver}</Badge>
	{:else if item.key === 'Host'}
		<code class="conn-val" class:conn-masked={!connHostRevealed}>
			{#if connInfo}
				{connInfo.host}:{connInfo.port}
			{:else}
				{service?.slug}
			{/if}
		</code>
	{:else if item.key === 'URL'}
		<code class="conn-url" class:conn-masked={!connUrlRevealed}>
			{connInfo?.url_template}
		</code>
	{/if}
{/snippet}

{#snippet connAction(item: KeyValueItem)}
	{#if item.key === 'Host'}
		<Button
			variant="ghost"
			size="icon"
			title={connHostRevealed ? 'Hide' : 'Reveal'}
			aria-label={connHostRevealed ? 'Hide host' : 'Reveal host'}
			onclick={() => connHostRevealed = !connHostRevealed}
		>
			{#if connHostRevealed}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
		</Button>
		<Button
			variant="ghost"
			size="icon"
			title="Copy"
			aria-label="Copy host"
			onclick={() => navigator.clipboard.writeText(connInfo ? `${connInfo.host}:${connInfo.port}` : service!.slug)}
		>
			<Copy size={12} />
		</Button>
	{:else if item.key === 'URL'}
		<Button
			variant="ghost"
			size="icon"
			title={connUrlRevealed ? 'Hide' : 'Reveal'}
			aria-label={connUrlRevealed ? 'Hide URL' : 'Reveal URL'}
			onclick={() => connUrlRevealed = !connUrlRevealed}
		>
			{#if connUrlRevealed}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
		</Button>
		<Button variant="ghost" size="icon" title="Copy" aria-label="Copy URL" onclick={copyConnUrl}>
			{#if connInfoCopied}<CheckCircle2 size={12} />{:else}<Copy size={12} />{/if}
		</Button>
	{/if}
{/snippet}

<!-- ─── DB Client Modal ───────────────────────────────────────────────── -->
{#if showDbClient && service}
	<div use:portal>
		<DbClientModal
			serviceId={serviceId}
			onClose={() => showDbClient = false}
		/>
	</div>
{/if}

<!-- ─── Exec Terminal ─────────────────────────────────────────────────── -->
{#if showExecPanel && service}
	<ExecPanel
		projectId={projectId}
		serviceId={serviceId}
		serviceName={service.name}
		onClose={() => showExecPanel = false}
	/>
{/if}

<!-- ─── Container Monitor Overlay ───────────────────────────────────────── -->
{#if showMonitor}
	<div use:portal>
		<MonitorViewOverlay
			open={showMonitor}
			onClose={() => showMonitor = false}
			{serviceId}
		/>
	</div>
{/if}

<!-- ─── Env Manager Overlay ───────────────────────────────────────────── -->
<div use:portal>
	<EnvManagerOverlay
		open={showEnvPanel}
		onClose={() => showEnvPanel = false}
		{serviceId}
		{projectId}
		serviceName={service?.name ?? ''}
	/>
</div>

<!-- ─── Container Logs Overlay ─────────────────────────────────────────── -->
{#if containerLogsTarget}
	{#snippet replicaSelector()}
		<select
			class="clog-replica-select"
			value={containerLogsTarget?.id}
			onchange={async (e) => {
				const t = e.currentTarget as HTMLSelectElement;
				const c = containers.find(ct => ct.id === t.value);
				if (c) await openContainerLogs(c);
			}}
		>
			{#each containers.filter(c => c.docker_container_id).sort((a, b) => (a.replica_index ?? 0) - (b.replica_index ?? 0)) as c}
				<option value={c.id}>replica-{c.replica_index ?? '?'}</option>
			{/each}
		</select>
	{/snippet}
	<div use:portal>
		<LogViewerOverlay
			open={!!containerLogsTarget}
			title={service?.name ?? 'Container Logs'}
			subtitle={containerLogsTarget.docker_container_id.slice(0, 12)}
			fetchFn={async (tail) => {
				if (!containerLogsTarget) return [];
				const cid = containerLogsTarget.docker_container_id;
				const res = await api.get<string[]>(
					`/services/${serviceId}/containers/${cid}/logs?tail=${tail}&timestamps=true`
				);
				if (res.error) throw new Error(res.error.message);
				return res.data ?? [];
			}}
			streamUrl="/api/services/{serviceId}/containers/{containerLogsTarget.docker_container_id}/logs/stream"
			parseLine={parseLine}
			resetKey={containerLogsTarget.id}
			tailOptions={CLOG_TAIL_OPTIONS as unknown as number[]}
			initialTail={clogTail}
			headerControls={replicaSelector}
			onClose={closeContainerLogs}
		/>
	</div>
{/if}

<!-- ─── Delete Confirmation (portalled to body) ──────────────────────── -->
{#if showDeleteConfirm && service}
	<div use:portal>
		<ConfirmDialog
			bind:open={showDeleteConfirm}
			title="Delete Service"
			message={`This will permanently delete ${service.name} and all its deployments, env vars, and configuration. If it's currently running on swarm it will be stopped first. This cannot be undone.`}
			confirmLabel="Delete Service"
			confirmText={service.slug}
			error={deleteError}
			onConfirm={confirmDeleteService}
		/>
	</div>
{/if}

<!-- ─── Main Panel ────────────────────────────────────────────────────── -->
<div class="panel-content">
	{#if isLoadingService}
		<div class="loading-state">
			<Spinner size={28} />
			<span>Loading service…</span>
		</div>
	{:else if serviceError}
		<div class="error-state">
			<span>{serviceError}</span>
			<Button variant="secondary" size="sm" onclick={loadService}>Retry</Button>
		</div>
	{:else if service}
		{@const svc = service}
		<!-- Header -->
		<div class="svc-header">
			<Card padding="4px 14px 12px">
				<ListRow
					title={service.name}
					meta={`${statusLabel(service.status)} · ${typeLabel(service.type)} · ${service.replicas} replica${service.replicas === 1 ? '' : 's'}`}
				>
					{#snippet icon()}
						<BrandLogo icon={svc.icon} type={svc.type} size={22} iconSize={13} />
					{/snippet}
					{#snippet trailing()}
						<StatusDot status={toDotStatus(svc.status)} />
					{/snippet}
				</ListRow>
				<div class="header-actions">
					{#if service.status === 'running'}
						<Button variant="secondary" size="sm" onclick={() => showExecPanel = true} title="Open a shell in this container">
							<Terminal size={12} />
							Terminal
						</Button>
						<Button variant="secondary" size="sm" onclick={() => showMonitor = true} title="View container metrics">
							<Activity size={12} />
							Monitor
						</Button>
					{/if}
					<Button variant="secondary" size="sm" onclick={() => showDbClient = true} title="Open database client">
						<Database size={12} />
						DB Client
					</Button>
				</div>
			</Card>
		</div>

		<!-- Tabs -->
		<div class="tabs-wrap">
			<Tabs {tabs} value={activeTab} onChange={(id) => switchTab(id as Tab)} ariaLabel="Service sections" />
		</div>

		<!-- Deploying banner -->
		{#if isDeploying || isDeploymentRunning}
			<div class="deploying-banner">
				<Loader2 size={13} class="spin-icon" />
				<span>Deploying{isDeploymentRunning && !isDeploying ? ' — pipeline running' : '…'}</span>
			</div>
		{/if}

		<!-- Stopping banner -->
		{#if isStopping}
			<div class="stopping-banner">
				<Loader2 size={13} class="spin-icon" />
				<span>Stopping service… please wait</span>
			</div>
		{/if}

		<!-- Tab content -->
		<div class="tab-content">

			<!-- ── Overview ── -->
			{#if activeTab === 'overview'}
<div class="overview-wrap">
					<!-- Metadata -->
					<div class="ov-block">
						<KeyValueList items={overviewItems} keyWidth="90px" value={ovValue} action={ovAction} />
					</div>

					<!-- Manage Env / logs -->
					<div class="section-action">
						<Button variant="secondary" size="sm" onclick={() => showEnvPanel = true}>
							<Settings size={13} />
							Manage Environment Variables
						</Button>
						<Button
							variant="secondary"
							size="sm"
							onclick={() => {
								const c = containers.find(ct => ct.docker_container_id);
								if (c) openContainerLogs(c);
							}}
							disabled={!containers.some(ct => ct.docker_container_id)}
						>
							<FileText size={13} />
							View Logs
						</Button>
					</div>

					<!-- Type-specific info -->
					<div class="ov-block">
						{#if service.type === 'docker' || service.type === 'database'}
							<div class="ov-block-head"><Box size={13} /><span>Docker Image</span></div>
							<Card padding="10px 12px">
								<code class="image-tag">{service.image || '—'}</code>
								{#if service.ports?.length}
									<div class="ports-row">
										{#each service.ports as port}
											<Badge tone="blue">{port}</Badge>
										{/each}
									</div>
								{/if}
							</Card>

						{:else if service.type === 'git'}
							<div class="ov-block-head"><GitBranch size={13} /><span>Git Source</span></div>
							<KeyValueList
								keyWidth="90px"
								items={[
									{ key: 'Source path', value: service.directory_path || '—', mono: true },
									{ key: 'Build type', value: 'Auto-detect (Dockerfile / Nixpacks)', mono: true }
								]}
							/>

						{:else if service.type === 'docker_compose'}
							<div class="ov-block-head"><FileCode size={13} /><span>Docker Compose</span></div>
							<KeyValueList
								keyWidth="90px"
								items={[{ key: 'Compose file', value: service.directory_path || 'docker-compose.yml', mono: true }]}
							/>
						{:else if service.type === 'static'}
							<div class="ov-block-head"><Globe size={13} /><span>Static Site</span></div>
							<Card padding="10px 12px">
								<div class="info-hint">
									Served by the shared Shipyard nginx server. Click <strong>Open panel</strong> from the topology canvas to configure build settings, upload files, or trigger a deploy.
								</div>
							</Card>
						{:else}
							<div class="ov-block-head"><Terminal size={13} /><span>{typeLabel(service.type)}</span></div>
							<Card padding="10px 12px">
								<code class="info-val">{service.directory_path || '—'}</code>
							</Card>
						{/if}
					</div>

					<!-- Internal connection — shown for all service types -->
					<div class="ov-block">
						<div class="ov-block-head"><Network size={13} /><span>Internal Connection</span></div>
						{#if connInfoLoading}
							<Card padding="10px 12px">
								<div class="conn-loading"><Spinner size={14} /><span>Loading…</span></div>
							</Card>
						{:else}
							<KeyValueList items={connItems} keyWidth="60px" value={connValue} action={connAction} />
						{/if}
					</div>

					<!-- Danger zone -->
					{#if canDelete}
						<div class="ov-block">
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
									<Button
										variant="danger-outline"
										size="sm"
										onclick={() => { showDeleteConfirm = true; deleteError = ''; }}
									>
										<Trash2 size={12} />
										Delete
									</Button>
								</div>
							</Card>
						</div>
					{/if}
				</div>

			<!-- ── Deploy ── -->
			{:else if activeTab === 'deploy'}
				<div class="deploy-section">
					<div class="deploy-trigger-row">
						<Button variant="primary" size="sm" disabled={isDeploying || isRestarting || isDeploymentRunning || !canDeploy} onclick={triggerDeploy} title={isDeploymentRunning ? 'A deployment is already running' : canDeploy ? '' : 'Insufficient permissions'}>
							{#if isDeploying || isDeploymentRunning}
								<Spinner size={12} tone="current" />{isDeploymentRunning && !isDeploying ? 'Running…' : 'Deploying…'}
							{:else}
								<Play size={14} />Deploy
							{/if}
						</Button>
						<Button variant="secondary" size="sm" disabled={isDeploying || isRestarting || isDeploymentRunning || !canDeploy} onclick={triggerRedeploy} title={isDeploymentRunning ? 'A deployment is already running' : canDeploy ? 'Redeploy last successful build' : 'Insufficient permissions'}>
							<RefreshCw size={14} />Redeploy
						</Button>
						<Button variant="secondary" size="sm" disabled={isRestarting || isDeploying || isDeploymentRunning || !canDeploy} onclick={triggerRestart} title={isDeploymentRunning ? 'A deployment is already running' : canDeploy ? 'Restart containers without rebuilding' : 'Insufficient permissions'}>
							{#if isRestarting}
								<Spinner size={12} tone="current" />Restarting…
							{:else}
								<RefreshCw size={14} />Restart
							{/if}
						</Button>
					</div>

					<!-- Latest deployment steps -->
					{#if latestDeployment}
						<Card>
							<div class="dep-card-header">
								<span class="dep-card-label">Latest deployment</span>
								<Badge tone={deployBadgeTone(latestDeployment.status)}>{latestDeployment.status}</Badge>
							</div>
							<div class="dep-card-meta">
								<span class="font-mono">{latestDeployment.source_ref || '–'}</span>
								<span class="meta-sep">·</span>
								<span>{formatTime(latestDeployment.created_at)}</span>
							</div>
							{#if steps.length > 0}
								<div class="steps-list">
									{#each steps as step (step.id)}
										<ListRow
											iconTone={stepTone(step.status)}
											title={step.name.replace(/_/g, ' ')}
											meta={step.started_at ? formatTime(step.started_at) : undefined}
										>
											{#snippet icon()}
												{#if step.status === 'success'}<Check size={14} />
												{:else if step.status === 'running'}<Loader2 size={14} class="spin-icon" />
												{:else if step.status === 'failed'}<X size={14} />
												{:else if step.status === 'skipped'}<Minus size={14} />
												{:else}<Circle size={12} />{/if}
											{/snippet}
										</ListRow>
									{/each}
								</div>
							{/if}
						</Card>
					{:else}
						<EmptyState message="No deployments yet. Click Deploy to start." />
					{/if}
				</div>

			<!-- ── Logs (deployment list) ── -->
			{:else if activeTab === 'logs'}
				<div class="logs-section">
					<!-- Webhook trigger URL -->
					<Card>
						<div class="webhook-header">
							<span class="webhook-label">Deployment webhook</span>
							<Tabs
								tabs={webhookProviderTabs}
								value={webhookProvider}
								onChange={(id) => webhookProvider = id as typeof webhookProvider}
								ariaLabel="Webhook provider"
							/>
						</div>

						{#if isLoadingWebhook}
							<div class="webhook-loading"><Spinner size={14} /><span>Loading…</span></div>
						{:else}
							<KeyValueList items={webhookItems} keyWidth="40px">
								{#snippet action()}
									<Button variant="secondary" size="sm" onclick={copyWebhookUrl} disabled={!webhookToken || isRotatingWebhook}>
										{#if webhookCopied}
											<CheckCircle2 size={13} />Copied
										{:else}
											<Copy size={13} />Copy
										{/if}
									</Button>
								{/snippet}
							</KeyValueList>

							<div class="webhook-actions">
								{#if rotateConfirm}
									<span class="webhook-rotate-confirm-text">This will invalidate the current URL. Continue?</span>
									<Button variant="danger-outline" size="sm" onclick={rotateWebhook} disabled={isRotatingWebhook}>
										{#if isRotatingWebhook}<Spinner size={12} tone="current" />Rotating…{:else}Yes, rotate{/if}
									</Button>
									<Button variant="secondary" size="sm" onclick={() => rotateConfirm = false}>Cancel</Button>
								{:else}
									<Button variant="secondary" size="sm" onclick={rotateWebhook} disabled={isRotatingWebhook}>
										<RefreshCw size={12} />Rotate URL
									</Button>
								{/if}
							</div>
							{#if service.git_provider_id && (webhookProvider === 'github' || webhookProvider === 'gitlab')}
								<InlineAlert tone="info">Webhook is auto-registered on {webhookProvider === 'github' ? 'GitHub' : 'GitLab'} when auto deploy is enabled.</InlineAlert>
							{/if}
						{/if}
					</Card>
					<div class="logs-intro">Select a deployment to view its logs.</div>
					{#if deployments.length === 0}
						<EmptyState message="No deployments yet." />
					{:else}
						<Card padding="2px 14px">
							{#each deployments as dep (dep.id)}
								<ListRow
									title={dep.source_ref || 'manual'}
									meta={`${formatTime(dep.created_at)} · ${dep.triggered_by}`}
								>
									{#snippet icon()}
										<StatusDot status={toDotStatus(deployStatusClass(dep.status))} />
									{/snippet}
									{#snippet trailing()}
										<div class="dep-row-trailing">
											<Badge tone={deployBadgeTone(dep.status)}>{dep.status}</Badge>
											{#if dep.status === 'success' && dep.deployed_image}
												<Button
													variant="ghost"
													size="icon"
													title="Rollback to this deployment"
													aria-label="Rollback to this deployment"
													disabled={!!isRollingBack}
													onclick={(e) => triggerRollback(dep, e)}
												>
													{#if isRollingBack === dep.id}<Spinner size={12} />{:else}<Undo2 size={14} />{/if}
												</Button>
											{/if}
											<Button
												variant="ghost"
												size="icon"
												title="View logs"
												aria-label="View deployment logs"
												onclick={() => openDeploymentLogs(dep)}
											>
												<ChevronRight size={14} />
											</Button>
										</div>
									{/snippet}
								</ListRow>
							{/each}
						</Card>
					{/if}
				</div>

			<!-- ── Git deploy config ── -->
			{:else if activeTab === 'git'}
				<GitSettingsSection
					providers={orgGitProviders}
					loadingProviders={loadingGitProviders}
					bind:providerId={gitProviderId}
					providerDefaultLabel="Legacy Global Settings Token (Default)"
					showAutoDeployToggle={true}
					bind:autoDeploy={gitAutoDeploy}
					bind:strategy={gitDeployStrategy}
					bind:branch={gitBranch}
					bind:tagPattern={gitDeployTagPattern}
					bind:prBranch={gitDeployBranch}
					deployDisabled={!gitAutoDeploy}
					onSave={saveGitConfig}
					saving={gitSaving}
					saveOk={gitSaveOk}
					saveError={gitSaveError}
					strategyWebhookStatus={webhookRegStatus}
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
					repoUrl={service?.git_repo_url ?? undefined}
					repoIsLink={true}
				/>

			<!-- ── Replicas ── -->
			{:else if activeTab === 'replicas'}
<div class="replicas-section">
					{#if replicaDeleteError}
						<div class="replica-delete-error" role="alert">
							<div class="replica-delete-error-msg"><InlineAlert tone="error">{replicaDeleteError}</InlineAlert></div>
							<Button variant="ghost" size="icon" aria-label="Dismiss error" title="Dismiss" onclick={() => replicaDeleteError = null}>
								<X size={14} />
							</Button>
						</div>
					{/if}
					{#if isLoadingContainers}
						<div class="loading-inline"><Spinner size={14} /><span>Loading…</span></div>
					{:else if containers.length === 0}
						<EmptyState message="No replicas." />
					{:else}
						<ul class="replica-list">
							{#each containers as c (c.id)}
								{@const terminal = isTerminal(c.status)}
								<li class="replica-item" class:replica-stopped={terminal}>
									<StatusDot status={toDotStatus(c.status)} />
									<div class="replica-info">
										<div class="replica-name-row">
											<span class="replica-name">replica-{c.replica_index ?? '?'}</span>
											{#if terminal}
												<Badge tone="neutral">{statusLabel(c.status)}</Badge>
											{/if}
										</div>
										<div class="replica-meta">
											<span class="replica-cid-row">
												<span class="font-mono">{c.docker_container_id.slice(0, 12)}</span>
												<Button
													variant="ghost"
													size="icon"
													title="Copy container ID"
													aria-label="Copy container ID"
													onclick={(e) => { e.stopPropagation(); navigator.clipboard.writeText(c.docker_container_id.slice(0, 12)); }}
												><Copy size={10} /></Button>
											</span>
											{#if c.node_id}
												{@const node = nodeMap.get(c.node_id)}
												<span class="meta-sep">·</span>
												<span title={c.node_id}>
													<Badge tone="neutral">
														{#if node}
															<span class="node-role-dot node-role-{node.role}"></span>
															{node.hostname}
														{:else}
															{c.node_id.slice(0, 10)}
														{/if}
													</Badge>
												</span>
											{/if}
											{#if !terminal && c.started_at}
												<span class="meta-sep">·</span>
												<span>started {formatTime(c.started_at)}</span>
											{/if}
											{#if terminal && c.finished_at}
												<span class="meta-sep">·</span>
												<span>stopped {formatTime(c.finished_at)}</span>
											{/if}
											{#if c.exit_code !== null && c.exit_code !== undefined}
												<span class="meta-sep">·</span>
												<span class="exit-code" class:exit-nonzero={c.exit_code !== 0}>
													exit {c.exit_code}
												</span>
											{/if}
										</div>
									</div>
									<div class="replica-actions">
										<Button variant="ghost" size="icon" title="View logs" aria-label="View logs" onclick={() => openContainerLogs(c)}>
											<FileText size={14} />
										</Button>
										{#if terminal}
											<Button
												variant="ghost"
												size="icon"
												title="Remove record"
												aria-label="Remove record"
												disabled={deletingContainerId === c.id}
												onclick={() => deleteContainerRecord(c.id)}
											>
												{#if deletingContainerId === c.id}
													<Spinner size={12} tone="current" />
												{:else}
													<Trash2 size={14} />
												{/if}
											</Button>
										{/if}
									</div>
								</li>
							{/each}
						</ul>
					{/if}
				</div>

			<!-- ── Volumes ── -->
			{:else if activeTab === 'volumes'}
				<div class="tab-stack">
					{#if volumeError}
						<div role="alert"><InlineAlert tone="error">{volumeError}</InlineAlert></div>
					{/if}

					{#if isLoadingVolumes}
						<div class="loading-inline"><Spinner size={14} /><span>Loading…</span></div>
					{:else if volumes.length === 0}
						<EmptyState message="No volumes attached.">
							{#snippet icon()}<HardDrive size={28} />{/snippet}
						</EmptyState>
					{:else}
						<ActivityList>
							{#each volumes as v (v.id)}
								<ListRow title={v.name} meta={v.mount_path}>
									{#snippet icon()}<HardDrive size={14} />{/snippet}
									{#snippet trailing()}
										{#if v.size_mb != null}
											<Badge tone="blue">{fmtVolumeSize(v.size_mb)}</Badge>
										{/if}
									{/snippet}
								</ListRow>
							{/each}
						</ActivityList>
					{/if}
				</div>

			<!-- ── Domains ── -->
			{:else if activeTab === 'domains'}
				<div class="tab-stack">
					<!-- Header bar -->
					<div class="domain-header-bar">
						<span class="domain-header-title">Custom Domains</span>
						<Button variant="primary" size="sm" onclick={openAddDomainPanel}>
							<Plus size={12} />
							Add Domain
						</Button>
					</div>

					{#if domainError}
						<div role="alert"><InlineAlert tone="error">{domainError}</InlineAlert></div>
					{/if}

					{#if isLoadingDomains}
						<div class="loading-inline"><Spinner size={14} /><span>Loading…</span></div>
					{:else if domains.length === 0}
						<EmptyState message="No domains configured.">
							{#snippet icon()}<Globe size={28} />{/snippet}
						</EmptyState>
						<div class="domain-empty-action">
							<Button variant="secondary" size="sm" onclick={openAddDomainPanel}>
								<Plus size={12} /> Add your first domain
							</Button>
						</div>
					{:else}
						<ActivityList>
							{#each domains as d (d.id)}
								{@const dnsState = dnsCheckState[d.id] ?? 'idle'}
								{@const addrs = dnsCheckAddresses[d.id] ?? []}
								<div class="domain-item">
									<ListRow title={d.hostname}>
										{#snippet icon()}<Globe size={14} />{/snippet}
										{#snippet trailing()}
											<div class="domain-actions">
												<!-- DNS validate button -->
												<Button
													variant="secondary"
													size="sm"
													onclick={() => validateDns(d.id)}
													disabled={dnsState === 'checking'}
													title="Validate DNS"
												>
													{#if dnsState === 'checking'}
														<Spinner size={12} tone="current" />
														Checking…
													{:else if dnsState === 'ok'}
														<CheckCircle2 size={12} />
														DNS OK
													{:else if dnsState === 'fail'}
														<AlertCircle size={12} />
														No DNS
													{:else}
														<Globe size={12} />
														Check DNS
													{/if}
												</Button>
												<Button variant="ghost" size="icon" onclick={() => removeDomain(d.id)} title="Remove domain" aria-label="Remove domain">
													<X size={13} />
												</Button>
											</div>
										{/snippet}
									</ListRow>
									<div class="domain-badges">
										{#if d.tls_enabled}
											<Badge tone="green"><Shield size={9} />&nbsp;{d.cert_provider}</Badge>
										{:else}
											<Badge tone="neutral"><ShieldOff size={9} />&nbsp;HTTP</Badge>
										{/if}
										{#if d.port}
											<Badge tone="blue">:{d.port}</Badge>
										{/if}
										{#if d.cloudflare_record_id}
											<span title="DNS record managed by Cloudflare"><Badge tone="yellow">Cloudflare</Badge></span>
										{/if}
									</div>
									{#if dnsState === 'ok' && addrs.length > 0}
										<div class="domain-dns" role="status">
											<InlineAlert tone="success">Resolves to: {addrs.join(', ')}</InlineAlert>
										</div>
									{:else if dnsState === 'fail'}
										<div class="domain-dns" role="status">
											<InlineAlert tone="error">DNS lookup failed — domain does not resolve.</InlineAlert>
										</div>
									{/if}
								</div>
							{/each}
						</ActivityList>
					{/if}
				</div>
			<!-- ── Settings ── -->
			{:else if activeTab === 'settings'}
				<div class="settings-section">
					<InlineAlert tone="info">
						Changes saved here take effect on the next <strong>Redeploy</strong>.
					</InlineAlert>

					<!-- Docker Image & Registry -->
					<Card padding="14px">
						<div class="settings-group">
							<div class="settings-group-header">
								<Box size={13} />
								<span class="settings-group-title">Docker Image</span>
								<span class="settings-group-desc">Image to pull for the next deployment.</span>
							</div>
							{#if service.type === 'database'}
								<FormField label="Presets">
									<div class="preset-btns">
										{#each ['postgres:16', 'mysql:8', 'redis:7-alpine', 'mongo:7', 'mariadb:11'] as preset}
											<Button
												variant={editImage === preset ? 'primary' : 'secondary'}
												size="sm"
												onclick={() => editImage = preset}
											>{preset}</Button>
										{/each}
									</div>
								</FormField>
							{:else if service.type === 'static'}
								<FormField label="Presets">
									<div class="preset-btns">
										{#each ['nginx:alpine', 'nginx:stable-alpine', 'httpd:alpine'] as preset}
											<Button
												variant={editImage === preset ? 'primary' : 'secondary'}
												size="sm"
												onclick={() => editImage = preset}
											>{preset}</Button>
										{/each}
									</div>
								</FormField>
							{/if}
							<FormField label="Image" for="edit-image">
								<TextField
									id="edit-image"
									type="text"
									placeholder={service.type === 'database' ? 'postgres:16' : service.type === 'static' ? 'nginx:alpine' : 'nginx:latest'}
									bind:value={editImage}
									spellcheck="false"
								/>
							</FormField>
							<div class="settings-group-header settings-group-header-sub">
								<span class="settings-group-title">Registry Credentials</span>
								<span class="settings-group-desc">Leave blank to keep existing values.</span>
							</div>
							{#if isLoadingSettingsEnvs}
								<div class="loading-inline"><Spinner size={14} /><span>Loading…</span></div>
							{:else}
								<FormField label="Registry URL" for="edit-reg-url">
									<TextField
										id="edit-reg-url"
										type="text"
										placeholder="registry-1.docker.io"
										bind:value={editRegistryUrl}
										spellcheck="false"
									/>
								</FormField>
								<div class="settings-row">
									<FormField label="Username" for="edit-reg-user">
										<TextField
											id="edit-reg-user"
											type="text"
											placeholder="myuser"
											bind:value={editRegistryUser}
											autocomplete="off"
										/>
									</FormField>
									<FormField label={registryPassIsSet ? 'Password / Token (set)' : 'Password / Token'} for="edit-reg-pass">
										<TextField
											id="edit-reg-pass"
											type="password"
											placeholder={registryPassIsSet ? '(unchanged)' : '••••••••'}
											bind:value={editRegistryPass}
											autocomplete="new-password"
										/>
									</FormField>
								</div>
							{/if}
						</div>
					</Card>

					<!-- Artifact Source (Shipyard registry binding) -->
					{#if isLoadingArtifactSource}
						<Card padding="14px">
							<div class="loading-inline"><Spinner size={14} /><span>Loading…</span></div>
						</Card>
					{:else if artifactSource}
						<Card padding="14px">
							<div class="settings-group">
								<div class="settings-group-header">
									<Box size={13} />
									<span class="settings-group-title">Artifact Source</span>
									<span class="settings-group-desc">Bound to an image in the Shipyard registry.</span>
								</div>
								<FormField label="Image">
									<div class="settings-static font-mono">{artifactSource.repo}:{artifactSource.tag}</div>
								</FormField>
								<Checkbox bind:checked={editAutoDeployOnPush} label="Auto-deploy on push" />
								<span class="settings-checkbox-hint">
									Redeploy automatically when a new
									<code>:{artifactSource.tag}</code>
									image is pushed to the registry.
								</span>
							</div>
						</Card>
					{/if}

					<!-- Replicas -->
					<Card padding="14px">
						<div class="settings-group">
							<div class="settings-group-header">
								<span class="settings-group-title">Replicas</span>
								<span class="settings-group-desc">Number of container instances to run.</span>
							</div>
							<FormField label="Instance count" for="edit-replicas">
								<div class="replica-stepper">
									<Button
										variant="secondary"
										size="icon"
										aria-label="Decrease instance count"
										onclick={() => editReplicas = Math.max(0, (editReplicas ?? 0) - 1)}
										disabled={(editReplicas ?? 0) <= 0}
									>−</Button>
									<div class="stepper-input">
										<TextField
											id="edit-replicas"
											type="number"
											min="0"
											max="20"
											value={String(editReplicas ?? '')}
											oninput={(e) => { const v = parseInt((e.target as HTMLInputElement).value, 10); editReplicas = isNaN(v) ? null : v; }}
										/>
									</div>
									<Button
										variant="secondary"
										size="icon"
										aria-label="Increase instance count"
										onclick={() => editReplicas = Math.min(20, (editReplicas ?? 0) + 1)}
										disabled={(editReplicas ?? 0) >= 20}
									>+</Button>
								</div>
							</FormField>
						</div>
					</Card>

					<!-- Resource limits -->
					<Card padding="14px">
						<div class="settings-group">
							<div class="settings-group-header">
								<span class="settings-group-title">Resource Limits</span>
								<span class="settings-group-desc">Limit CPU and memory per container. Leave blank for no limit.</span>
							</div>
							<div class="settings-fields-row">
								<FormField label="CPU limit (cores)" for="edit-cpu">
									<TextField
										id="edit-cpu"
										type="number"
										min="0.1"
										max="64"
										step="0.1"
										placeholder="e.g. 0.5"
										value={String(editCpuLimit ?? '')}
										oninput={(e) => { const v = parseFloat((e.target as HTMLInputElement).value); editCpuLimit = isNaN(v) ? null : v; }}
									/>
								</FormField>
								<FormField label="Memory limit (MB)" for="edit-mem">
									<TextField
										id="edit-mem"
										type="number"
										min="32"
										step="32"
										placeholder="e.g. 512"
										value={String(editMemLimit ?? '')}
										oninput={(e) => { const v = parseInt((e.target as HTMLInputElement).value, 10); editMemLimit = isNaN(v) ? null : v; }}
									/>
								</FormField>
							</div>
						</div>
					</Card>

					<!-- Port mapping -->
					<Card padding="14px">
						<div class="settings-group">
							<div class="settings-group-header">
								<span class="settings-group-title">Port Mapping</span>
								<span class="settings-group-desc">Ports exposed by this service. Format: <code>80</code> or <code>host:container</code>.</span>
							</div>
							<div class="port-editor">
								{#each editPorts as port, i (i)}
									<div class="port-row">
										<div class="port-input">
											<TextField
												type="text"
												placeholder="e.g. 3000 or 8080:80"
												value={port}
												oninput={(e) => updatePort(i, (e.target as HTMLInputElement).value)}
												spellcheck="false"
												aria-label="Port mapping"
											/>
										</div>
										<Button variant="ghost" size="icon" onclick={() => removePort(i)} title="Remove" aria-label="Remove port">
											<X size={13} />
										</Button>
									</div>
								{/each}
								<div>
									<Button variant="secondary" size="sm" onclick={addPort}>
										<Plus size={12} />
										Add Port
									</Button>
								</div>
							</div>
						</div>
					</Card>

					<!-- Volume Mounts -->
					<Card padding="14px">
						<div class="settings-group">
							<div class="settings-group-header">
								<HardDrive size={13} />
								<span class="settings-group-title">Volume Mounts</span>
								<span class="settings-group-desc">Bind named volumes or host paths into the container.</span>
							</div>
							{#if volumeAdvice && volumeAdvice.unmounted_volume_paths.length > 0}
								<InlineAlert tone="warning">
									<div class="settings-warn">
										<AlertTriangle size={13} />
										<div>
											This image declares
											{volumeAdvice.unmounted_volume_paths.length === 1 ? 'a volume' : 'volumes'}
											with no named volume or bind mount:
											<span class="settings-warn-paths">
												{#each volumeAdvice.unmounted_volume_paths as p}<code>{p}</code>{/each}
											</span>
											Docker creates a fresh anonymous volume there on every redeploy —
											<strong>data written to {volumeAdvice.unmounted_volume_paths.length === 1 ? 'it' : 'them'} will not persist</strong>.
											Add a mount below with a matching path.
										</div>
									</div>
								</InlineAlert>
							{/if}
							{#if isLoadingSettingsEnvs}
								<div class="loading-inline"><Spinner size={14} /><span>Loading…</span></div>
							{:else}
								<VolumeMountList {projectId} bind:mounts={editVolumeMounts} />
							{/if}
						</div>
					</Card>

					<!-- Networks -->
					<Card padding="14px">
						<div class="settings-group">
							<div class="settings-group-header-row">
								<div class="settings-group-header">
									<Network size={13} />
									<span class="settings-group-title">Networks</span>
									<span class="settings-group-desc">Docker networks this service is connected to.</span>
								</div>
								<Button variant="secondary" size="sm" onclick={openNetworkPickerForSettings}>
									<Plus size={11} />Add
								</Button>
							</div>
							{#if isLoadingSettingsNetworks}
								<div class="loading-inline"><Spinner size={14} /><span>Loading…</span></div>
							{:else if editNetworks.length === 0}
								<div class="settings-empty">No networks attached.</div>
							{:else}
								<div class="settings-network-list">
									{#each editNetworks as net (net.id)}
										<div class="settings-network-row">
											<Network size={12} class="net-icon" />
											<span class="net-name">{net.name}</span>
											<span class="net-driver">{net.driver}</span>
											<Button variant="ghost" size="icon" onclick={() => removeSettingsNetwork(net.id)} title="Detach" aria-label="Detach network">
												<X size={12} />
											</Button>
										</div>
									{/each}
								</div>
							{/if}
						</div>
					</Card>

					<!-- Save feedback -->
					{#if settingsSaveError}
						<div role="alert"><InlineAlert tone="error">{settingsSaveError}</InlineAlert></div>
					{/if}
					{#if settingsSaveSuccess}
						<div role="status">
							<InlineAlert tone="success">
								<CheckCircle size={13} /> Saved — click Redeploy to apply changes.
							</InlineAlert>
						</div>
					{/if}

					<!-- Save button -->
					<div class="settings-footer">
						<Button variant="primary" onclick={saveSettings} disabled={isSavingSettings}>
							{#if isSavingSettings}
								<Spinner size={12} tone="current" />Saving…
							{:else}
								<CheckCircle size={14} />Save Changes
							{/if}
						</Button>
						<Button variant="secondary" onclick={() => initSettingsFromService()} disabled={isSavingSettings}>
							Reset
						</Button>
					</div>
				</div>
			{/if}
		</div>

		<!-- Footer actions -->
		<div class="panel-footer">
			<Button variant="primary" size="sm" disabled={isDeploying || !canDeploy} onclick={triggerDeploy} title={canDeploy ? '' : 'Insufficient permissions'}>
				<Play size={13} />Deploy
			</Button>
			<Button variant="secondary" size="sm" disabled={isStopping || !canDeploy} onclick={triggerStop} title={canDeploy ? '' : 'Insufficient permissions'}>
				{#if isStopping}
					<Spinner size={13} tone="current" />Stopping…
				{:else}
					<Square size={13} />Stop
				{/if}
			</Button>
			<Button variant="secondary" size="sm" onclick={() => showEnvPanel = true}>
				<Settings size={13} />Env
			</Button>
		</div>
	{/if}
</div>

<style>
	/* ── Layout ── */
	.panel-content {
		display: flex;
		flex-direction: column;
		height: 100%;
		position: relative;
	}

	/* ── Deploying banner ── */
	.deploying-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 16px;
		background: color-mix(in srgb, #3b82f6 10%, transparent);
		border-bottom: 1px solid color-mix(in srgb, #3b82f6 25%, transparent);
		font-size: 12.5px;
		color: #1d4ed8;
		flex-shrink: 0;
	}
	:global(.dark) .deploying-banner { color: #93c5fd; }

	/* ── Stopping banner ── */
	.stopping-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 16px;
		background: color-mix(in srgb, #f59e0b 10%, transparent);
		border-bottom: 1px solid color-mix(in srgb, #f59e0b 25%, transparent);
		font-size: 12.5px;
		color: #92400e;
		flex-shrink: 0;
	}
	:global(.dark) .stopping-banner {
		color: #fbbf24;
	}

	/* ── Loading / Error ── */
	.loading-state, .error-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		flex: 1;
		color: var(--text-muted);
		font-size: 13px;
		padding: 24px;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ── Header ── */
	.svc-header {
		padding: 12px 12px 0;
		flex-shrink: 0;
	}
	.meta-sep { color: var(--text-dim); }
	.header-actions { display: flex; flex-wrap: wrap; gap: 6px; align-items: center; padding-top: 10px; border-top: 1px solid var(--border); }
	.tabs-wrap { flex-shrink: 0; padding: 0 8px; }

	/* ── Tab content ── */
	.tab-content {
		flex: 1;
		overflow-y: auto;
		overflow-x: hidden;
	}

	/* ── Overview ── */
	.overview-wrap {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 12px;
	}
	.ov-block { display: flex; flex-direction: column; gap: 6px; }
	.ov-block-head {
		display: flex;
		align-items: center;
		gap: 7px;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.kv-text { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.image-tag {
		font-family: var(--font-mono);
		font-size: 13px;
		color: var(--text-primary);
		background: var(--bg-base);
		padding: 4px 8px;
		border-radius: var(--radius-sm);
		display: inline-block;
		word-break: break-all;
	}
	.ports-row {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
		margin-top: 4px;
	}
	.info-val {
		font-family: var(--font-mono);
		color: var(--text-primary);
		word-break: break-all;
	}

	.info-hint {
		font-size: 11px;
		color: var(--text-muted);
		margin-top: 6px;
		line-height: 1.5;
	}

	/* ── Connection info card ─────────────────────────────────────── */
	.conn-loading { display: flex; align-items: center; gap: 8px; font-size: 12px; color: var(--text-muted); }
	.conn-val {
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-primary);
		word-break: break-all;
	}
	.conn-url {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-secondary);
		word-break: break-all;
	}
	.conn-masked {
		filter: blur(5px);
		user-select: none;
		pointer-events: none;
		transition: filter 0.2s;
	}

	.section-action {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	/* ── Danger zone ── */
	.danger-header {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-bottom: 8px;
		font-size: 11px;
		font-weight: 600;
		color: var(--accent-red);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.danger-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.danger-info { display: flex; flex-direction: column; gap: 2px; }
	.danger-title { font-size: 13px; font-weight: 500; color: var(--text-primary); }
	.danger-desc { font-size: 11px; color: var(--text-muted); }

	/* ── Deploy tab ── */
	.deploy-section {
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
	.deploy-trigger-row { display: flex; flex-wrap: wrap; gap: 8px; }
	.dep-card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}
	.dep-card-label {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
	}
	.dep-card-meta {
		display: flex;
		align-items: center;
		gap: 6px;
		margin-top: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.steps-list { margin-top: 6px; }

	/* ── Logs tab (webhook + deployment list) ── */
	.logs-section {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 14px;
	}
	.webhook-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 8px;
		margin-bottom: 10px;
	}
	.webhook-label {
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		color: var(--text-muted);
	}
	.webhook-loading { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-dim); }
	.webhook-actions {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		margin-top: 10px;
	}
	.webhook-rotate-confirm-text { font-size: 11px; color: var(--text-muted); flex: 1 1 100%; }
	.logs-intro { font-size: 12px; color: var(--text-muted); }
	.dep-row-trailing { display: flex; align-items: center; gap: 4px; }

	/* ── Replicas ── */
	.replicas-section { padding: 4px 0; }
	.loading-inline { display: flex; align-items: center; gap: 8px; padding: 16px; color: var(--text-muted); font-size: 13px; }
	.replica-list { list-style: none; margin: 0; padding: 0; }
	.replica-item {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		transition: background var(--transition-fast);
	}
	.replica-item:last-child { border-bottom: none; }
	.replica-item:hover { background: var(--bg-elevated); }
	.replica-item.replica-stopped { opacity: 0.72; }
	.replica-item.replica-stopped:hover { opacity: 1; }
	.replica-info { flex: 1; display: flex; flex-direction: column; gap: 3px; min-width: 0; }
	.replica-name-row { display: flex; align-items: center; gap: 6px; }
	.replica-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
	.replica-meta {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		color: var(--text-muted);
		flex-wrap: wrap;
	}
	.replica-cid-row {
		display: inline-flex;
		align-items: center;
		gap: 3px;
	}
	.exit-code { font-family: var(--font-mono); font-size: 10px; }
	.exit-nonzero { color: var(--accent-red); }
	.replica-actions { display: flex; align-items: center; gap: 2px; flex-shrink: 0; }
	.replica-delete-error {
		display: flex; align-items: center; gap: 4px;
		margin: 8px 12px;
	}
	.replica-delete-error-msg { flex: 1; min-width: 0; }

	.node-role-dot {
		width: 5px; height: 5px; border-radius: 50%; flex-shrink: 0;
	}
	.node-role-manager { background: var(--accent); }
	.node-role-worker  { background: var(--accent-green); }

	.tab-stack { display: flex; flex-direction: column; gap: 12px; padding: 12px; }
	.domain-header-bar { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	.domain-empty-action { display: flex; justify-content: center; margin-top: -12px; }
	.domain-dns { padding: 0 0 10px 42px; }
	.domain-header-title {
		font-size: 12px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
	}
	.domain-item {
		display: flex; flex-direction: column;
		border-bottom: 1px solid var(--border);
	}
	.domain-item:last-child { border-bottom: none; }
	.domain-item :global(.ui-list-row) { border-bottom: none; padding-bottom: 6px; }
	.domain-badges { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; padding: 0 0 10px 42px; }

	.domain-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }
	:global(.spin-icon) { animation: spin 1s linear infinite; }

	/* Replica dropdown */
	.clog-replica-select {
		background: rgba(255,255,255,0.05);
		border: 1px solid rgba(255,255,255,0.1);
		border-radius: 4px;
		color: #E5E7EB;
		font-size: 12px;
		font-weight: 600;
		font-family: var(--font-mono);
		padding: 2px 6px;
		cursor: pointer;
		outline: none;
	}
	.clog-replica-select:focus { border-color: rgba(37,99,235,0.5); }

	/* ── Footer ── */
	.panel-footer {
		border-top: 1px solid var(--border);
		padding: 10px 14px;
		display: flex;
		align-items: center;
		gap: 8px;
		flex-shrink: 0;
		background: var(--bg-surface);
	}

	.font-mono { font-family: var(--font-mono); }

	/* ── Settings tab ── */
	.settings-section {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 12px;
	}
	.settings-section :global(strong) { color: var(--text-primary); }
	.settings-group {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.settings-group-title {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
	}
	.settings-group-desc {
		font-size: 11px;
		color: var(--text-muted);
	}
	.settings-group-desc code {
		font-family: var(--font-mono);
		background: var(--bg-elevated);
		padding: 1px 4px;
		border-radius: 3px;
	}
	.settings-fields-row, .settings-row { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
	.settings-group-header-sub { margin-top: 6px; }

	/* Replica stepper */
	.replica-stepper { display: flex; align-items: center; gap: 8px; width: fit-content; }
	.stepper-input { width: 72px; }

	/* Port editor */
	.port-editor { display: flex; flex-direction: column; gap: 6px; }
	.port-row { display: flex; align-items: center; gap: 6px; }
	.port-input { flex: 1; min-width: 0; }
	.settings-footer {
		display: flex;
		gap: 8px;
	}

	.settings-static {
		min-height: 30px; display: flex; align-items: center;
		padding: 0 10px; font-size: 12px; color: var(--text-secondary);
		background: var(--bg-base); border: 1px solid var(--border);
		border-radius: var(--radius-sm);
	}
	.settings-checkbox-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; }
	.settings-checkbox-hint code {
		font-family: var(--font-mono); font-size: 10px;
		background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px;
		border: 1px solid var(--border);
	}

	.settings-warn { display: flex; align-items: flex-start; gap: 8px; }
	.settings-warn :global(svg) { flex-shrink: 0; margin-top: 2px; }
	.settings-warn strong { color: var(--text-primary); }
	.settings-warn-paths { display: inline-flex; flex-wrap: wrap; gap: 4px; margin: 0 3px; }
	.settings-warn-paths code {
		font-family: var(--font-mono); font-size: 11px;
		background: var(--bg-elevated); padding: 1px 5px; border-radius: 3px;
		border: 1px solid var(--border);
	}

	.preset-btns { display: flex; flex-wrap: wrap; gap: 5px; }

	.settings-group-header {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
	}

	.settings-group-header-row {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 8px;
	}

	.settings-empty {
		font-size: 12px;
		color: var(--text-dim);
		padding: 6px 0;
	}

	.settings-network-list {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}
	.settings-network-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 10px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		font-size: 12px;
	}
	:global(.net-icon) { color: var(--text-dim); flex-shrink: 0; }
	.net-name { flex: 1; font-weight: 500; color: var(--text-primary); }
	.net-driver { font-size: 11px; color: var(--text-muted); font-family: var(--font-mono); }

</style>
