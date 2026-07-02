<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import {
		Play, Square, RefreshCw, Trash2, AlertTriangle,
		GitBranch, Box, FileCode, Terminal, Settings, X,
		ChevronRight, CheckCircle, XCircle, Clock, Loader,
		Eye, Copy, Globe, Plus, Shield, ShieldOff, FileText,
		CheckCircle2, AlertCircle, Loader2, Network, HardDrive
	} from '@lucide/svelte';
	import DomainAddPanel from './resources/DomainAddPanel.svelte';
	import NetworkPickerPanel from './resources/NetworkPickerPanel.svelte';
	import VolumeMountList from '$lib/components/VolumeMountList.svelte';
	import type { VolumeMount } from '$lib/components/VolumeMountList.svelte';
	import { formatDistanceToNow } from 'date-fns';

	import { api } from '$lib/api/client';
	import { serviceStore } from '$lib/stores/service.store';
	import { containerStore } from '$lib/stores/container.store';
	import { deploymentStore } from '$lib/stores/deployment.store';
	import { uiStore } from '$lib/stores/ui.store';
	import { orgStore } from '$lib/stores/org.store';
	import { can } from '$lib/auth/permissions';
	import { subscribeToService, subscribeToDeployment, subscribeToDeploymentSteps } from '$lib/mqtt/subscriptions';
	import { eventBus } from '$lib/mqtt/eventBus';
	import EnvManagerPanel from './EnvManagerPanel.svelte';
	import type {
		Service, Container, Deployment, DeploymentStep,
		DeploymentLog, MqttPayload, ContainerStatus, Domain, ContainerStats,
		Network as NetworkType
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
	}

	let { serviceId, projectId, orgId, onDeleted }: Props = $props();

	// ── Permission gates ─────────────────────────────────────────────
	let myRole  = $derived($orgStore.myMembership?.role ?? null);
	let myPerms = $derived($orgStore.myMembership?.permissions ?? []);
	let canDeploy = $derived(can(myRole, myPerms, 'service:deploy'));
	let canWrite  = $derived(can(myRole, myPerms, 'service:write'));
	let canDelete = $derived(can(myRole, myPerms, 'service:delete'));

	// ── Tabs ─────────────────────────────────────────────────────────
	type Tab = 'overview' | 'deploy' | 'logs' | 'replicas' | 'domains' | 'settings' | 'monitor';
	let activeTab = $state<Tab>('overview');

	// ── Core state ───────────────────────────────────────────────────
	let service = $state<Service | null>(null);
	let containers = $state<Container[]>([]);
	let deployments = $state<Deployment[]>([]);
	let steps = $state<DeploymentStep[]>([]);

	let isLoadingService = $state(true);
	let isLoadingContainers = $state(false);
	let isDeploying = $state(false);
	let isStopping = $state(false);
	let isRestarting = $state(false);
	let serviceError = $state<string | null>(null);

	// ── Deployment log viewer ────────────────────────────────────────
	let selectedDeployment = $state<Deployment | null>(null);
	let depLogs = $state<DeploymentLog[]>([]);
	let depSteps = $state<DeploymentStep[]>([]);
	let isLoadingLogs = $state(false);
	let expandedSteps = $state<Set<string>>(new Set());
	let isLive = $state(false);
	let depMqttCleanup: (() => void) | null = null;

	// ── Domains ──────────────────────────────────────────────────────
	let domains = $state<Domain[]>([]);
	let isLoadingDomains = $state(false);
	let domainError = $state('');
	let dnsCheckState = $state<Record<string, 'idle' | 'checking' | 'ok' | 'fail'>>({});
	let dnsCheckAddresses = $state<Record<string, string[]>>({});

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

	const CLOG_TAIL_OPTIONS = [50, 100, 200, 500, 1000] as const;

	// ── Env panel ────────────────────────────────────────────────────
	let showEnvPanel = $state(false);

	// ── Settings edit state ──────────────────────────────────────────
	let editReplicas = $state(1);
	let editPorts = $state<string[]>([]);
	let editImage = $state('');
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

	// ── Danger zone / Delete ─────────────────────────────────────────
	let showDeleteConfirm = $state(false);
	let deleteSlugInput = $state('');
	let isDeleting = $state(false);
	let deleteError = $state('');

	// ── Webhook ───────────────────────────────────────────────────────
	let webhookToken     = $state('');
	let webhookProvider  = $state<'github' | 'gitlab' | 'gitea'>('github');
	let isLoadingWebhook = $state(false);
	let webhookCopied    = $state(false);
	let isRotatingWebhook = $state(false);
	let rotateConfirm    = $state(false);

	// ── MQTT cleanup ─────────────────────────────────────────────────
	let unsubscribeService: (() => void) | null = null;
	let unsubscribeDeployment: (() => void) | null = null;

	// ── Monitor tab ───────────────────────────────────────────────────────────
	const HISTORY_LEN = 30;

	let monitorTarget    = $state<Container | null>(null);
	let statsSource: EventSource | null = null;
	let currentStats     = $state<ContainerStats | null>(null);
	let monitorLoading   = $state(false);
	let monitorError     = $state('');
	let netRxDeltaPerSec = $state(0);
	let netTxDeltaPerSec = $state(0);

	let cpuHistory      = $state<number[]>([]);
	let memHistory      = $state<number[]>([]);
	let netRxHistory    = $state<number[]>([]);
	let netTxHistory    = $state<number[]>([]);
	let blkReadHistory  = $state<number[]>([]);
	let blkWriteHistory = $state<number[]>([]);

	function addToHistory(hist: number[], val: number): number[] {
		const next = [...hist, val];
		return next.length > HISTORY_LEN ? next.slice(-HISTORY_LEN) : next;
	}

	function formatBytes(bytes: number, decimals = 1): string {
		if (bytes <= 0) return '0 B';
		const k = 1024;
		const sizes = ['B', 'KB', 'MB', 'GB', 'TB'];
		const i = Math.min(Math.floor(Math.log(bytes) / Math.log(k)), sizes.length - 1);
		return `${(bytes / Math.pow(k, i)).toFixed(decimals)} ${sizes[i]}`;
	}

	function sparklinePaths(data: number[]): { line: string; area: string } {
		if (data.length < 2) return { line: '', area: '' };
		const max = Math.max(...data, 0.001);
		const W = 200, H = 50, PAD = 4;
		const pts = data.map((v, i) => ({
			x: (i / (data.length - 1)) * W,
			y: (H - PAD) - (v / max) * (H - PAD * 2) + PAD
		}));
		const line = pts.map((p, i) => `${i === 0 ? 'M' : 'L'}${p.x.toFixed(1)},${p.y.toFixed(1)}`).join(' ');
		const area = `${line} L${W},${H} L0,${H} Z`;
		return { line, area };
	}

	function connectStats(c: Container) {
		disconnectStats();
		monitorLoading = true;
		monitorError   = '';

		const cid = c.docker_container_id;
		const es  = new EventSource(`/api/services/${serviceId}/containers/${cid}/stats`);
		statsSource = es;

		es.onmessage = (e) => {
			if (!e.data?.trim()) return;
			let stats: ContainerStats;
			try { stats = JSON.parse(e.data); } catch { return; }

			const prev = currentStats;
			const rxDelta = prev ? Math.max(0, stats.net_rx_bytes - prev.net_rx_bytes) : 0;
			const txDelta = prev ? Math.max(0, stats.net_tx_bytes - prev.net_tx_bytes) : 0;

			cpuHistory      = addToHistory(cpuHistory,      stats.cpu_percent);
			memHistory      = addToHistory(memHistory,      stats.memory_percent);
			netRxHistory    = addToHistory(netRxHistory,    rxDelta);
			netTxHistory    = addToHistory(netTxHistory,    txDelta);
			blkReadHistory  = addToHistory(blkReadHistory,  stats.block_read_bytes);
			blkWriteHistory = addToHistory(blkWriteHistory, stats.block_write_bytes);

			netRxDeltaPerSec = rxDelta;
			netTxDeltaPerSec = txDelta;
			currentStats     = stats;
			monitorLoading   = false;
			monitorError     = '';
		};

		es.addEventListener('error', (e: MessageEvent) => {
			monitorError   = e.data ?? 'Stats stream error';
			monitorLoading = false;
		});

		es.onerror = () => {
			if (monitorLoading) {
				monitorError   = 'Could not connect to stats stream';
				monitorLoading = false;
				es.close();
				statsSource = null;
			}
		};
	}

	function disconnectStats() {
		statsSource?.close();
		statsSource = null;
	}

	function selectMonitorTarget(c: Container) {
		monitorTarget    = c;
		currentStats     = null;
		cpuHistory       = [];
		memHistory       = [];
		netRxHistory     = [];
		netTxHistory     = [];
		blkReadHistory   = [];
		blkWriteHistory  = [];
		netRxDeltaPerSec = 0;
		netTxDeltaPerSec = 0;
		monitorError     = '';
		connectStats(c);
	}

	// ── Derived ──────────────────────────────────────────────────────
	let latestDeployment = $derived(deployments[0] ?? null);
	let deleteSlugValid = $derived(deleteSlugInput === (service?.slug ?? ''));
	let runningContainers = $derived(containers.filter(c => c.status === 'running'));

	let stepLogsMap = $derived(
		depLogs.reduce((acc: Record<string, DeploymentLog[]>, log) => {
			const key = log.step_id || '__global__';
			if (!acc[key]) acc[key] = [];
			acc[key].push(log);
			return acc;
		}, {})
	);
	let globalLogs = $derived(stepLogsMap['__global__'] ?? []);

	// ── Status helpers ───────────────────────────────────────────────
	function statusClass(status: string) {
		switch (status) {
			case 'running':               return 'running';
			case 'pending':
			case 'preparing':             return 'pending';
			case 'failed':
			case 'rejected':              return 'failed';
			default:                      return 'stopped';
		}
	}

	function statusLabel(status: string): string {
		const map: Record<string, string> = {
			running: 'Running', pending: 'Pending', preparing: 'Preparing',
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

	function stepStatusIcon(s: string): string {
		switch (s) {
			case 'success': return '✓';
			case 'running': return '⟳';
			case 'failed':  return '✗';
			case 'skipped': return '–';
			default:        return '○';
		}
	}

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

	function stepDuration(step: DeploymentStep): string {
		if (!step.started_at || !step.finished_at) return '';
		const ms = new Date(step.finished_at).getTime() - new Date(step.started_at).getTime();
		return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
	}

	function toggleStep(stepId: string) {
		const next = new Set(expandedSteps);
		if (next.has(stepId)) next.delete(stepId);
		else next.add(stepId);
		expandedSteps = next;
	}

	const STEP_TERMINAL = new Set(['success', 'failed', 'skipped']);

	function allStepsTerminal(steps: DeploymentStep[]) {
		return steps.length > 0 && steps.every(s => STEP_TERMINAL.has(s.status));
	}

	function finalizeDepMqtt() {
		depMqttCleanup?.();
		depMqttCleanup = null;
		if (selectedDeployment) {
			const anyFailed = depSteps.some(s => s.status === 'failed');
			const finalStatus: Deployment['status'] = anyFailed ? 'failed' : 'success';
			const depId = selectedDeployment.id;
			selectedDeployment = { ...selectedDeployment, status: finalStatus };
			deployments = deployments.map(d => d.id === depId ? { ...d, status: finalStatus } : d);
		}
	}

	function startDepMqtt(dep: Deployment) {
		// Subscribe to MQTT topics for this deployment's steps
		const unsubMqtt = subscribeToDeploymentSteps(orgId, projectId, serviceId, dep.id);
		const stepsPrefix = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}/deployments/${dep.id}/steps/`;

		const handler = (_type: string, evt: unknown) => {
			const topic = _type as string;
			const payload = evt as MqttPayload;
			if (!topic.startsWith(stepsPrefix)) return;

			if (topic.endsWith('/status')) {
				const meta = payload.meta as any;
				if (!meta?.step_id || !meta?.status) return;
				depSteps = depSteps.map(s =>
					s.id === meta.step_id ? { ...s, status: meta.status } : s
				);
				if (allStepsTerminal(depSteps)) finalizeDepMqtt();

			} else if (topic.endsWith('/log')) {
				const meta = payload.meta as any;
				if (!payload.message) return;
				depLogs = [...depLogs, {
					id: crypto.randomUUID(),
					deployment_id: dep.id,
					step_id: (meta?.step_id as string) ?? '',
					level: payload.level ?? 'info',
					message: payload.message,
					timestamp: payload.timestamp,
				}];
			}
		};

		eventBus.on('*', handler as any);
		isLive = true;

		depMqttCleanup = () => {
			eventBus.off('*', handler as any);
			unsubMqtt();
			isLive = false;
		};
	}

	async function openDeploymentLogs(dep: Deployment) {
		// Clean up any prior MQTT subscription
		depMqttCleanup?.();
		depMqttCleanup = null;

		selectedDeployment = dep;
		isLoadingLogs = true;
		depLogs = [];
		depSteps = [];
		expandedSteps = new Set();

		// Subscribe to MQTT FIRST (before HTTP fetch) so no events are missed
		const isActive = dep.status === 'running' || dep.status === 'pending' || dep.status === 'queued';
		if (isActive) startDepMqtt(dep);

		// ONE-TIME HTTP fetch for initial state (step names + existing logs)
		const [stepsRes, logsRes] = await Promise.all([
			api.get<DeploymentStep[]>(`/deployments/${dep.id}/steps`),
			api.get<DeploymentLog[]>(`/deployments/${dep.id}/logs`)
		]);

		if (stepsRes.data) depSteps = stepsRes.data.sort((a, b) => a.order_index - b.order_index);
		if (logsRes.data) depLogs = logsRes.data;
		isLoadingLogs = false;

		// If dep.status was stale and all steps are already done, finalize immediately
		if (isActive && allStepsTerminal(depSteps)) finalizeDepMqtt();
	}

	function closeDepLogs() {
		depMqttCleanup?.();
		depMqttCleanup = null;
		selectedDeployment = null;
		depLogs = [];
		depSteps = [];
		expandedSteps = new Set();
	}

	async function loadDomains() {
		isLoadingDomains = true;
		domainError = '';
		const res = await api.get<Domain[]>(`/services/${serviceId}/domains`);
		if (res.data) domains = res.data;
		else if (res.error) domainError = res.error.message;
		isLoadingDomains = false;
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

	async function deleteContainerRecord(containerId: string) {
		const res = await api.deleteContainer(serviceId, containerId);
		if (!res.error) containers = containers.filter(c => c.id !== containerId);
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

	function detectLevel(line: string): 'error' | 'warn' | 'debug' | 'info' {
		const l = line.toLowerCase();
		if (/\b(error|err|fatal|crit)\b/.test(l) || /level=error/.test(l)) return 'error';
		if (/\b(warn|warning)\b/.test(l) || /level=warn/.test(l)) return 'warn';
		if (/\b(debug|trace)\b/.test(l) || /level=debug/.test(l)) return 'debug';
		return 'info';
	}

	function clogLineClass(line: string): string {
		const lvl = detectLevel(line);
		switch (lvl) {
			case 'error': return 'clog-line clog-lvl-error';
			case 'warn':  return 'clog-line clog-lvl-warn';
			case 'debug': return 'clog-line clog-lvl-debug';
			default:      return 'clog-line';
		}
	}

	async function openContainerLogs(c: Container) {
		// Close any previous stream
		clogSource?.close();
		clogSource = null;
		clogStatus = 'idle';
		clogLines = [];
		clogError = '';

		containerLogsTarget = c;
		isLoadingContainerLogs = true;
		containerLogs = [];
		const res = await api.get<string[]>(
			`/services/${serviceId}/containers/${c.docker_container_id}/logs?tail=${clogTail}&timestamps=true`
		);
		if (res.data) containerLogs = res.data;
		isLoadingContainerLogs = false;
	}

	function closeContainerLogs() {
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
		const res = await api.post<Deployment>(`/services/${serviceId}/deploy`);
		if (res.data) {
			const depId = res.data.id;
			deployments = [res.data, ...deployments];
			deploymentStore.setActiveDeployment(res.data);
			unsubscribeDeployment?.();
			unsubscribeDeployment = subscribeToDeployment(orgId, projectId, serviceId, depId);

			// Listen for the completion event so the list row updates live.
			const depStatusTopic = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}/deployments/${depId}/status`;
			const onDepDone = (payload: MqttPayload) => {
				const evt = payload.event ?? '';
				if (!evt.startsWith('deployment.success') && !evt.startsWith('deployment.failed')) return;
				const finalStatus: Deployment['status'] = evt.includes('success') ? 'success' : 'failed';
				deployments = deployments.map(d => d.id === depId ? { ...d, status: finalStatus } : d);
				eventBus.off(depStatusTopic, onDepDone);
			};
			eventBus.on(depStatusTopic, onDepDone);

			await loadStepsForLatest();
		}
		isDeploying = false;
	}

	async function triggerStop() {
		if (!service || isStopping) return;
		isStopping = true;
		await api.post(`/services/${serviceId}/stop`);
		isStopping = false;
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

	async function deleteService() {
		if (!service || !deleteSlugValid || isDeleting) return;
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
		settingsSaveError = '';
		settingsSaveSuccess = false;
	}

	async function loadSettingsEnvs() {
		isLoadingSettingsEnvs = true;
		const res = await api.getServiceEnvs(serviceId);
		if (res.data) {
			for (const env of res.data) {
				if (env.key === 'DOCKER_REGISTRY')  editRegistryUrl  = (env as any).value ?? '';
				if (env.key === 'DOCKER_USERNAME')  editRegistryUser = (env as any).value ?? '';
				if (env.key === 'DOCKER_PASSWORD') {
					editRegistryPass = '';
					registryPassIsSet = true;
				}
				if (env.key === '__VOLUME_MOUNTS__') {
					try { editVolumeMounts = JSON.parse((env as any).value ?? '[]'); } catch { editVolumeMounts = []; }
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
			replicas: editReplicas,
			ports,
			...(image ? { image } : {}),
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

	function addPort() { editPorts = [...editPorts, '']; }
	function removePort(i: number) { editPorts = editPorts.filter((_, idx) => idx !== i); }
	function updatePort(i: number, val: string) {
		editPorts = editPorts.map((p, idx) => idx === i ? val : p);
	}

	// ── Tab change ───────────────────────────────────────────────────
	async function switchTab(tab: Tab) {
		if (activeTab === 'monitor' && tab !== 'monitor') disconnectStats();
		activeTab = tab;
		if (tab === 'replicas' && containers.length === 0) await loadContainers();
		if (tab === 'deploy' || tab === 'logs') await loadDeployments();
		if (tab === 'logs') void loadWebhookToken();
		if (tab === 'deploy' && latestDeployment && steps.length === 0) await loadStepsForLatest();
		if (tab === 'domains' && domains.length === 0) await loadDomains();
		if (tab === 'settings') {
			initSettingsFromService();
			editRegistryUrl = '';
			editRegistryUser = '';
			editRegistryPass = '';
			registryPassIsSet = false;
			editVolumeMounts = [];
			editNetworks = [];
			await Promise.all([loadSettingsEnvs(), loadSettingsNetworks()]);
		}
		if (tab === 'monitor') {
			if (containers.length === 0) await loadContainers();
			if (monitorTarget) {
				connectStats(monitorTarget);
			} else {
				const first = containers.find(c => c.status === 'running');
				if (first) selectMonitorTarget(first);
			}
		}
	}

	// ── MQTT handlers ────────────────────────────────────────────────
	function handleServiceStatus(payload: MqttPayload) {
		const meta = payload.meta as any;
		if (meta?.status && service) {
			service = { ...service, status: meta.status, replicas: meta.replicas ?? service.replicas };
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
		if (latestDeployment) await loadStepsForLatest();
		unsubscribeService = subscribeToService(orgId, projectId, serviceId);
		eventBus.on(serviceStatusTopic, handleServiceStatus);
		eventBus.on(serviceContainersTopic, handleContainers);
	});

	onDestroy(() => {
		unsubscribeService?.();
		unsubscribeDeployment?.();
		depMqttCleanup?.();
		clogSource?.close();
		disconnectStats();
		if (serviceStatusTopic) eventBus.off(serviceStatusTopic, handleServiceStatus);
		if (serviceContainersTopic) eventBus.off(serviceContainersTopic, handleContainers);
		serviceStore.setActiveService(null);
	});

	const tabs: { id: Tab; label: string }[] = [
		{ id: 'overview',  label: 'Overview'  },
		{ id: 'deploy',    label: 'Deploy'    },
		{ id: 'logs',      label: 'Logs'      },
		{ id: 'replicas',  label: 'Replicas'  },
		{ id: 'domains',   label: 'Domains'   },
		{ id: 'monitor',   label: 'Monitor'   },
		{ id: 'settings',  label: 'Settings'  },
	];
</script>

<!-- ─── Deployment Log Viewer Overlay ────────────────────────────────── -->
{#if selectedDeployment}
	<div class="log-overlay">
		<div class="log-overlay-header">
			<div class="log-overlay-title">
				<div class="log-dep-status status-dot {deployStatusClass(selectedDeployment.status)}"></div>
				<span>Deployment logs</span>
				<span class="log-dep-time">{formatTime(selectedDeployment.created_at)}</span>
				{#if isLive}
					<span class="live-badge">LIVE</span>
				{/if}
			</div>
			<button class="icon-btn" onclick={closeDepLogs}><X size={16} /></button>
		</div>

		{#if isLoadingLogs}
			<div class="log-loading"><div class="spinner-sm"></div> Loading…</div>
		{:else}
			<div class="accordion-list">
				{#each depSteps as step (step.id)}
					{@const logs = stepLogsMap[step.id] ?? []}
					{@const expanded = expandedSteps.has(step.id)}
					{@const dur = stepDuration(step)}
					<div class="accordion-item" class:acc-expanded={expanded}>
						<button class="accordion-header" onclick={() => toggleStep(step.id)}>
							<span class="acc-icon acc-{step.status}">{stepStatusIcon(step.status)}</span>
							<span class="acc-name">{step.name.replace(/_/g, ' ')}</span>
							<span class="acc-count">{logs.length}</span>
							{#if dur}
								<span class="acc-dur">{dur}</span>
							{/if}
							<span class="acc-chevron" class:rotated={expanded}>
								<ChevronRight size={13} />
							</span>
						</button>
						{#if expanded}
							<div class="acc-logs">
								{#if logs.length === 0}
									<div class="acc-empty">No output for this step.</div>
								{:else}
									{#each logs as log (log.id)}
										<div class="log-entry {logLevelClass(log.level)}">
											<span class="log-ts">{log.timestamp.slice(11, 19)}</span>
											<span class="log-lvl">{log.level.toUpperCase()}</span>
											<span class="log-msg">{log.message}</span>
										</div>
									{/each}
								{/if}
							</div>
						{/if}
					</div>
				{/each}

				{#if globalLogs.length > 0}
					{@const expanded = expandedSteps.has('__global__')}
					<div class="accordion-item" class:acc-expanded={expanded}>
						<button class="accordion-header" onclick={() => toggleStep('__global__')}>
							<span class="acc-icon acc-pending">○</span>
							<span class="acc-name">General</span>
							<span class="acc-count">{globalLogs.length}</span>
							<span class="acc-chevron" class:rotated={expanded}>
								<ChevronRight size={13} />
							</span>
						</button>
						{#if expanded}
							<div class="acc-logs">
								{#each globalLogs as log (log.id)}
									<div class="log-entry {logLevelClass(log.level)}">
										<span class="log-ts">{log.timestamp.slice(11, 19)}</span>
										<span class="log-lvl">{log.level.toUpperCase()}</span>
										<span class="log-msg">{log.message}</span>
									</div>
								{/each}
							</div>
						{/if}
					</div>
				{/if}

				{#if depSteps.length === 0 && globalLogs.length === 0}
					<div class="empty-logs-msg">No log entries recorded for this deployment.</div>
				{/if}
			</div>
		{/if}
	</div>
{/if}

<!-- ─── Env Manager Overlay ───────────────────────────────────────────── -->
{#if showEnvPanel && service}
	<div class="env-overlay">
		<div class="env-overlay-header">
			<span>Environment Variables — {service.name}</span>
			<button class="icon-btn" onclick={() => showEnvPanel = false}><X size={16} /></button>
		</div>
		<div class="env-overlay-body">
			<EnvManagerPanel serviceId={serviceId} serviceName={service.name} />
		</div>
	</div>
{/if}

<!-- ─── Container Logs Overlay (portalled, 2/3 screen) ───────────────── -->
{#if containerLogsTarget}
	<div use:portal class="clog-backdrop" role="presentation" onclick={(e) => { if (e.target === e.currentTarget) closeContainerLogs(); }}>
	<div class="clog-panel">
		<div class="log-overlay-header">
			<div class="log-overlay-title">
				<FileText size={14} />
				<span>replica-{containerLogsTarget.replica_index ?? '?'}</span>
				<span class="log-dep-time font-mono">{containerLogsTarget.docker_container_id.slice(0, 12)}</span>
			</div>

			<!-- Tail selector -->
			<div class="clog-tail-group">
				<span class="clog-tail-label">Lines</span>
				{#each CLOG_TAIL_OPTIONS as n}
					<button
						class="clog-tail-btn"
						class:active={clogTail === n}
						onclick={async () => {
							clogTail = n;
							if (containerLogsTarget) {
								disconnectContainerLogs();
								await openContainerLogs(containerLogsTarget);
							}
						}}
					>{n}</button>
				{/each}
			</div>

			<div class="log-overlay-controls">
				{#if clogStatus === 'connected'}
					<span class="clog-dot"></span>
					<span class="clog-status-label">Live</span>
					<button class="clog-ctrl-btn" onclick={disconnectContainerLogs}>
						<Square size={11} />Stop
					</button>
				{:else if clogStatus === 'connecting'}
					<Loader2 size={13} class="spin" />
					<span class="clog-status-label muted">Connecting…</span>
				{:else if clogStatus === 'error'}
					<span class="clog-status-label error">{clogError}</span>
					<button class="clog-ctrl-btn" onclick={connectContainerLogs}>
						<Play size={11} />Retry
					</button>
				{:else}
					<button class="clog-ctrl-btn primary" onclick={connectContainerLogs}>
						<Play size={11} />Connect
					</button>
				{/if}
			</div>
			<button class="icon-btn" onclick={closeContainerLogs}><X size={16} /></button>
		</div>

		{#if isLoadingContainerLogs}
			<div class="log-loading"><div class="spinner-sm"></div> Loading…</div>
		{:else}
			<div class="clog-lines" bind:this={clogEl}>
				{#if containerLogs.length === 0 && clogLines.length === 0}
					<div class="empty-logs-msg">
						{clogStatus === 'idle' ? 'Press Connect to stream live logs.' : 'No output yet…'}
					</div>
				{:else}
					{#each containerLogs as line, i (i)}
						<div class={clogLineClass(line)}>{line}</div>
					{/each}
					{#if clogLines.length > 0}
						<div class="clog-stream-divider">── live ──</div>
						{#each clogLines as line, i (i)}
							<div class="{clogLineClass(line)} clog-live">{line}</div>
						{/each}
					{/if}
				{/if}
			</div>
		{/if}
	</div>
	</div>
{/if}

<!-- ─── Delete Confirmation Modal (portalled to body) ────────────────── -->
{#if showDeleteConfirm && service}
	<div use:portal class="sdp-modal-backdrop" role="dialog" aria-modal="true">
		<div class="sdp-modal">
			<div class="sdp-modal-header">
				<AlertTriangle size={18} style="color:#EF4444;flex-shrink:0" />
				<span>Delete Service</span>
			</div>
			<div class="sdp-modal-body">
				<p class="sdp-modal-warning">
					This will permanently delete <strong>{service.name}</strong> and all its deployments,
					env vars, and configuration. If it's currently running on swarm it will be stopped first.
					<strong>This cannot be undone.</strong>
				</p>
				<div class="sdp-confirm-field">
					<label class="sdp-confirm-label">
						Type <code class="sdp-confirm-code">{service.slug}</code> to confirm
					</label>
					<input
						class="sdp-confirm-input"
						type="text"
						placeholder={service.slug}
						bind:value={deleteSlugInput}
						autocomplete="off"
						spellcheck="false"
					/>
				</div>
				{#if deleteError}
					<div class="sdp-delete-error">{deleteError}</div>
				{/if}
			</div>
			<div class="sdp-modal-footer">
				<button
					class="btn btn-ghost"
					onclick={() => { showDeleteConfirm = false; deleteSlugInput = ''; deleteError = ''; }}
					disabled={isDeleting}
				>
					Cancel
				</button>
				<button
					class="btn btn-danger"
					disabled={!deleteSlugValid || isDeleting}
					onclick={deleteService}
				>
					{#if isDeleting}
						<div class="btn-spinner-dark"></div>
						Deleting…
					{:else}
						<Trash2 size={13} />
						Delete Service
					{/if}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- ─── Main Panel ────────────────────────────────────────────────────── -->
<div class="panel-content">
	{#if isLoadingService}
		<div class="loading-state">
			<div class="spinner"></div>
			<span>Loading service…</span>
		</div>
	{:else if serviceError}
		<div class="error-state">
			<span>{serviceError}</span>
			<button class="btn btn-secondary btn-sm" onclick={loadService}>Retry</button>
		</div>
	{:else if service}
		<!-- Header -->
		<div class="svc-header">
			<div class="svc-identity">
				<span class="svc-name">{service.name}</span>
				<div class="svc-meta">
					<span class="status-dot {statusClass(service.status)}"></span>
					<span class="svc-status">{statusLabel(service.status)}</span>
					<span class="meta-sep">·</span>
					<span class="svc-type">{typeLabel(service.type)}</span>
					<span class="meta-sep">·</span>
					<span class="svc-replicas">{service.replicas} replica{service.replicas === 1 ? '' : 's'}</span>
				</div>
			</div>
			<div class="header-actions">
				<button class="btn btn-secondary btn-xs" onclick={() => showEnvPanel = true} title="Manage env vars">
					<Settings size={12} />
					Env
				</button>
			</div>
		</div>

		<!-- Tabs -->
		<div class="tabs-row">
			{#each tabs as tab}
				<button
					class="tab-btn"
					class:active={activeTab === tab.id}
					onclick={() => switchTab(tab.id)}
				>
					{tab.label}
				</button>
			{/each}
		</div>

		<!-- Tab content -->
		<div class="tab-content">

			<!-- ── Overview ── -->
			{#if activeTab === 'overview'}
				<div class="overview-wrap">

					<!-- Type-specific info card -->
					<div class="info-card">
						{#if service.type === 'docker' || service.type === 'database'}
							<div class="info-card-header">
								<Box size={13} />
								<span>Docker Image</span>
							</div>
							<div class="info-card-body">
								<code class="image-tag">{service.image || '—'}</code>
								{#if service.ports?.length}
									<div class="ports-row">
										{#each service.ports as port}
											<span class="port-badge">{port}</span>
										{/each}
									</div>
								{/if}
							</div>

						{:else if service.type === 'git'}
							<div class="info-card-header">
								<GitBranch size={13} />
								<span>Git Source</span>
							</div>
							<div class="info-card-body">
								<div class="info-row">
									<span class="info-key">Source path</span>
									<code class="info-val">{service.directory_path || '—'}</code>
								</div>
								<div class="info-row">
									<span class="info-key">Build type</span>
									<span class="info-val">Auto-detect (Dockerfile / Nixpacks)</span>
								</div>
							</div>

						{:else if service.type === 'docker_compose'}
							<div class="info-card-header">
								<FileCode size={13} />
								<span>Docker Compose</span>
							</div>
							<div class="info-card-body">
								<div class="info-row">
									<span class="info-key">Compose file</span>
									<code class="info-val">{service.directory_path || 'docker-compose.yml'}</code>
								</div>
							</div>
						{:else}
							<div class="info-card-header">
								<Terminal size={13} />
								<span>{typeLabel(service.type)}</span>
							</div>
							<div class="info-card-body">
								<code class="info-val">{service.directory_path || '—'}</code>
							</div>
						{/if}
					</div>

					<!-- Metadata grid -->
					<div class="overview-grid">
						<div class="field">
							<span class="field-label">Name</span>
							<span class="field-value">{service.name}</span>
						</div>
						<div class="field">
							<span class="field-label">Slug</span>
							<span class="field-value font-mono">{service.slug}</span>
						</div>
						<div class="field">
							<span class="field-label">Status</span>
							<span class="field-value">
								<span class="status-dot {statusClass(service.status)}"></span>
								{statusLabel(service.status)}
							</span>
						</div>
						<div class="field">
							<span class="field-label">Replicas</span>
							<span class="field-value">{service.replicas}</span>
						</div>
						<div class="field">
							<span class="field-label">Created</span>
							<span class="field-value">{formatTime(service.created_at)}</span>
						</div>
						<div class="field">
							<span class="field-label">Updated</span>
							<span class="field-value">{formatTime(service.updated_at)}</span>
						</div>
					</div>

					<!-- Manage Env button -->
					<div class="section-action">
						<button class="btn btn-secondary btn-sm full-w" onclick={() => showEnvPanel = true}>
							<Settings size={13} />
							Manage Environment Variables
						</button>
					</div>

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
									onclick={() => { showDeleteConfirm = true; deleteSlugInput = ''; deleteError = ''; }}
								>
									<Trash2 size={12} />
									Delete
								</button>
							</div>
						</div>
					</div>
					{/if}
				</div>

			<!-- ── Deploy ── -->
			{:else if activeTab === 'deploy'}
				<div class="deploy-section">
					<div class="deploy-trigger-row">
						<button class="btn btn-primary" disabled={isDeploying || isRestarting || !canDeploy} onclick={triggerDeploy} title={canDeploy ? '' : 'Insufficient permissions'}>
							{#if isDeploying}
								<div class="btn-spinner"></div>Deploying…
							{:else}
								<Play size={14} />Deploy
							{/if}
						</button>
						<button class="btn btn-secondary" disabled={isDeploying || isRestarting || !canDeploy} onclick={triggerRedeploy} title={canDeploy ? 'Redeploy last successful build' : 'Insufficient permissions'}>
							<RefreshCw size={14} />Redeploy
						</button>
						<button class="btn btn-secondary" disabled={isRestarting || isDeploying || !canDeploy} onclick={triggerRestart} title={canDeploy ? 'Restart containers without rebuilding' : 'Insufficient permissions'}>
							{#if isRestarting}
								<Loader2 size={14} class="spin" />Restarting…
							{:else}
								<RefreshCw size={14} />Restart
							{/if}
						</button>
					</div>

					<!-- Latest deployment steps -->
					{#if latestDeployment}
						<div class="latest-dep-card">
							<div class="dep-card-header">
								<span class="dep-card-label">Latest deployment</span>
								<span class="dep-card-badge {deployStatusClass(latestDeployment.status)}">
									{latestDeployment.status}
								</span>
							</div>
							<div class="dep-card-meta">
								<span class="font-mono">{latestDeployment.source_ref || '–'}</span>
								<span class="meta-sep">·</span>
								<span>{formatTime(latestDeployment.created_at)}</span>
							</div>
							{#if steps.length > 0}
								<div class="steps-list">
									{#each steps as step (step.id)}
										<div class="step-item"
											class:step-running={step.status === 'running'}
											class:step-failed={step.status === 'failed'}
											class:step-success={step.status === 'success'}>
											<span class="step-icon">{stepStatusIcon(step.status)}</span>
											<span class="step-name">{step.name.replace(/_/g, ' ')}</span>
											{#if step.started_at}
												<span class="step-time">{formatTime(step.started_at)}</span>
											{/if}
										</div>
									{/each}
								</div>
							{/if}
						</div>
					{:else}
						<div class="empty-state-msg">No deployments yet. Click Deploy to start.</div>
					{/if}
				</div>

			<!-- ── Logs (deployment list) ── -->
			{:else if activeTab === 'logs'}
				<div class="logs-section">
					<!-- Webhook trigger URL -->
					<div class="webhook-section">
						<div class="webhook-header">
							<span class="webhook-label">Deployment webhook</span>
							<div class="webhook-provider-tabs">
								{#each (['github', 'gitlab', 'gitea'] as const) as p}
									<button class:active={webhookProvider === p} onclick={() => webhookProvider = p}>
										{p.charAt(0).toUpperCase() + p.slice(1)}
									</button>
								{/each}
							</div>
						</div>

						{#if isLoadingWebhook}
							<div class="webhook-loading"><div class="spinner-xs"></div>Loading…</div>
						{:else}
							<div class="webhook-url-row">
								<input
									class="webhook-url-input"
									readonly
									value={webhookToken
										? `${window.location.origin}/api/webhooks/${webhookProvider}/${serviceId}/${webhookToken}`
										: `${window.location.origin}/api/webhooks/${webhookProvider}/${serviceId}/…`}
								/>
								<button class="webhook-copy-btn" onclick={copyWebhookUrl} disabled={!webhookToken || isRotatingWebhook}>
									{#if webhookCopied}
										<CheckCircle2 size={13} />Copied
									{:else}
										<Copy size={13} />Copy
									{/if}
								</button>
							</div>

							<div class="webhook-actions">
								{#if rotateConfirm}
									<span class="webhook-rotate-confirm-text">This will invalidate the current URL. Continue?</span>
									<button class="webhook-rotate-btn danger" onclick={rotateWebhook} disabled={isRotatingWebhook}>
										{#if isRotatingWebhook}<div class="spinner-xs"></div>Rotating…{:else}Yes, rotate{/if}
									</button>
									<button class="webhook-rotate-btn" onclick={() => rotateConfirm = false}>Cancel</button>
								{:else}
									<button class="webhook-rotate-btn" onclick={rotateWebhook} disabled={isRotatingWebhook}>
										<RefreshCw size={11} />Rotate URL
									</button>
								{/if}
							</div>
						{/if}
					</div>
					<div class="logs-intro">Select a deployment to view its logs.</div>
					{#if deployments.length === 0}
						<div class="empty-state-msg">No deployments yet.</div>
					{:else}
						<ul class="dep-list">
							{#each deployments as dep (dep.id)}
								<li>
									<button
										class="dep-list-row"
										onclick={() => openDeploymentLogs(dep)}
									>
										<span class="dep-row-status status-dot {deployStatusClass(dep.status)}"></span>
										<div class="dep-row-info">
											<span class="dep-row-ref font-mono">{dep.source_ref || 'manual'}</span>
											<span class="dep-row-meta">
												{formatTime(dep.created_at)}
												<span class="meta-sep">·</span>
												{dep.triggered_by}
											</span>
										</div>
										<span class="dep-row-badge {deployStatusClass(dep.status)}">{dep.status}</span>
										<ChevronRight size={14} class="dep-row-arrow" />
									</button>
								</li>
							{/each}
						</ul>
					{/if}
				</div>

			<!-- ── Replicas ── -->
			{:else if activeTab === 'replicas'}
				<div class="replicas-section">
					{#if isLoadingContainers}
						<div class="loading-inline"><div class="spinner-sm"></div><span>Loading…</span></div>
					{:else if containers.length === 0}
						<div class="empty-state-msg" style="padding: 24px 16px;">No replicas.</div>
					{:else}
						<ul class="replica-list">
							{#each containers as c (c.id)}
								{@const terminal = isTerminal(c.status)}
								<li class="replica-item" class:replica-stopped={terminal}>
									<span class="status-dot {statusClass(c.status)}"></span>
									<div class="replica-info">
										<div class="replica-name-row">
											<span class="replica-name">replica-{c.replica_index ?? '?'}</span>
											{#if terminal}
												<span class="stopped-badge">{statusLabel(c.status)}</span>
											{/if}
										</div>
										<div class="replica-meta">
											<span class="font-mono">{c.docker_container_id.slice(0, 12)}</span>
											{#if c.node_id}
												<span class="meta-sep">·</span>
												<span>{c.node_id}</span>
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
										<button class="btn btn-ghost btn-xs" onclick={() => openContainerLogs(c)} title="View logs">
											<FileText size={12} />
										</button>
										{#if terminal}
											<button
												class="btn btn-ghost btn-xs replica-del-btn"
												onclick={() => deleteContainerRecord(c.id)}
												title="Remove record"
											>
												<Trash2 size={12} />
											</button>
										{/if}
									</div>
								</li>
							{/each}
						</ul>
					{/if}
				</div>

			<!-- ── Domains ── -->
			{:else if activeTab === 'domains'}
				<div class="domains-section">
					<!-- Header bar -->
					<div class="domain-header-bar">
						<span class="domain-header-title">Custom Domains</span>
						<button class="btn btn-primary btn-sm" onclick={openAddDomainPanel}>
							<Plus size={12} />
							Add Domain
						</button>
					</div>

					{#if domainError}
						<div class="domain-error">{domainError}</div>
					{/if}

					{#if isLoadingDomains}
						<div class="loading-inline"><div class="spinner-sm"></div><span>Loading…</span></div>
					{:else if domains.length === 0}
						<div class="domain-empty">
							<Globe size={28} style="color:var(--text-dim);margin-bottom:8px" />
							<span>No domains configured.</span>
							<button class="btn btn-secondary btn-sm" onclick={openAddDomainPanel}>
								<Plus size={12} /> Add your first domain
							</button>
						</div>
					{:else}
						<ul class="domain-list">
							{#each domains as d (d.id)}
								{@const dnsState = dnsCheckState[d.id] ?? 'idle'}
								{@const addrs = dnsCheckAddresses[d.id] ?? []}
								<li class="domain-item">
									<div class="domain-item-top">
										<Globe size={13} style="flex-shrink:0;color:var(--text-dim);margin-top:1px" />
										<div class="domain-info">
											<span class="domain-hostname">{d.hostname}</span>
											<div class="domain-badges">
												{#if d.tls_enabled}
													<span class="badge badge-green"><Shield size={9} /> {d.cert_provider}</span>
												{:else}
													<span class="badge badge-dim"><ShieldOff size={9} /> HTTP</span>
												{/if}
												{#if d.port}
													<span class="badge badge-blue">:{d.port}</span>
												{/if}
											</div>
										</div>
										<div class="domain-actions">
											<!-- DNS validate button -->
											<button
												class="dns-check-btn"
												class:dns-ok={dnsState === 'ok'}
												class:dns-fail={dnsState === 'fail'}
												class:dns-checking={dnsState === 'checking'}
												onclick={() => validateDns(d.id)}
												disabled={dnsState === 'checking'}
												title="Validate DNS"
											>
												{#if dnsState === 'checking'}
													<Loader2 size={12} class="spin-icon" />
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
											</button>
											<button class="icon-btn" onclick={() => removeDomain(d.id)} title="Remove domain">
												<X size={13} />
											</button>
										</div>
									</div>
									{#if dnsState === 'ok' && addrs.length > 0}
										<div class="dns-result dns-result-ok">
											Resolves to: {addrs.join(', ')}
										</div>
									{:else if dnsState === 'fail'}
										<div class="dns-result dns-result-fail">
											DNS lookup failed — domain does not resolve.
										</div>
									{/if}
								</li>
							{/each}
						</ul>
					{/if}
				</div>
			<!-- ── Monitor ── -->
			{:else if activeTab === 'monitor'}
				<div class="monitor-section">

					<!-- Container replica selector (only shown when > 1 running) -->
					{#if runningContainers.length > 1}
						<div class="monitor-selector">
							{#each runningContainers as c (c.id)}
								<button
									class="monitor-sel-btn"
									class:active={monitorTarget?.id === c.id}
									onclick={() => selectMonitorTarget(c)}
								>
									replica-{c.replica_index ?? '?'}
								</button>
							{/each}
						</div>
					{/if}

					{#if runningContainers.length === 0}
						<div class="empty-state-msg">No running replicas to monitor.</div>

					{:else if monitorError}
						<div class="monitor-error">{monitorError}</div>

					{:else if monitorLoading && !currentStats}
						<div class="loading-inline" style="padding:20px 16px">
							<div class="spinner-sm"></div><span>Fetching metrics…</span>
						</div>

					{:else}
						<!-- 2×2 metric grid -->
						<div class="metric-grid">

							<!-- CPU Usage -->
							<div class="metric-card">
								<div class="metric-header">
									<span class="metric-label">CPU</span>
									<span class="metric-value cpu">{currentStats ? `${currentStats.cpu_percent.toFixed(1)}%` : '—'}</span>
								</div>
								{#each [sparklinePaths(cpuHistory)] as cpu}
									<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
										{#if cpu.line}
											<path d={cpu.area} fill="rgba(37,99,235,0.13)" />
											<path d={cpu.line} fill="none" stroke="#3B82F6" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
										{/if}
									</svg>
								{/each}
								<div class="metric-sub">
									{cpuHistory.length > 1
										? `avg ${(cpuHistory.reduce((a, b) => a + b, 0) / cpuHistory.length).toFixed(1)}%`
										: 'collecting…'}
								</div>
							</div>

							<!-- Memory Usage -->
							<div class="metric-card">
								<div class="metric-header">
									<span class="metric-label">Memory</span>
									<span class="metric-value mem">{currentStats ? `${currentStats.memory_percent.toFixed(1)}%` : '—'}</span>
								</div>
								{#each [sparklinePaths(memHistory)] as mem}
									<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
										{#if mem.line}
											<path d={mem.area} fill="rgba(16,185,129,0.13)" />
											<path d={mem.line} fill="none" stroke="#10B981" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
										{/if}
									</svg>
								{/each}
								<div class="metric-sub">
									{currentStats
										? `${formatBytes(currentStats.memory_usage_bytes)} / ${formatBytes(currentStats.memory_limit_bytes)}`
										: 'collecting…'}
								</div>
							</div>

							<!-- Network I/O -->
							<div class="metric-card">
								<div class="metric-header">
									<span class="metric-label">Network I/O</span>
								</div>
								<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
									{#each [sparklinePaths(netRxHistory)] as netRx}
										{#if netRx.line}
											<path d={netRx.area} fill="rgba(99,102,241,0.10)" />
											<path d={netRx.line} fill="none" stroke="#6366F1" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
										{/if}
									{/each}
									{#each [sparklinePaths(netTxHistory)] as netTx}
										{#if netTx.line}
											<path d={netTx.area} fill="rgba(244,114,182,0.10)" />
											<path d={netTx.line} fill="none" stroke="#F472B6" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
										{/if}
									{/each}
								</svg>
								<div class="metric-net-row">
									<span class="net-chip rx">↓ {formatBytes(netRxDeltaPerSec)}/s</span>
									<span class="net-chip tx">↑ {formatBytes(netTxDeltaPerSec)}/s</span>
								</div>
							</div>

							<!-- Block I/O -->
							<div class="metric-card">
								<div class="metric-header">
									<span class="metric-label">Block I/O</span>
								</div>
								<svg class="spark" viewBox="0 0 200 50" preserveAspectRatio="none">
									{#each [sparklinePaths(blkReadHistory)] as blkR}
										{#if blkR.line}
											<path d={blkR.area} fill="rgba(251,191,36,0.10)" />
											<path d={blkR.line} fill="none" stroke="#FBBF24" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
										{/if}
									{/each}
									{#each [sparklinePaths(blkWriteHistory)] as blkW}
										{#if blkW.line}
											<path d={blkW.area} fill="rgba(249,115,22,0.10)" />
											<path d={blkW.line} fill="none" stroke="#F97316" stroke-width="1.5" vector-effect="non-scaling-stroke" stroke-linecap="round" stroke-linejoin="round" />
										{/if}
									{/each}
								</svg>
								<div class="metric-net-row">
									<span class="net-chip blk-r">R {formatBytes(currentStats?.block_read_bytes ?? 0)}</span>
									<span class="net-chip blk-w">W {formatBytes(currentStats?.block_write_bytes ?? 0)}</span>
								</div>
							</div>
						</div>

						<!-- Footer: PIDs + last-updated -->
						<div class="monitor-footer">
							{#if currentStats}
								<span class="monitor-footer-pids">
									{currentStats.pids} PID{currentStats.pids !== 1 ? 's' : ''}
								</span>
								<span class="monitor-footer-ts">
									Updated {formatTime(currentStats.timestamp)}
								</span>
							{/if}
						</div>
					{/if}
				</div>

			<!-- ── Settings ── -->
			{:else if activeTab === 'settings'}
				<div class="settings-section">
					<div class="settings-hint">
						Changes saved here take effect on the next <strong>Redeploy</strong>.
					</div>

					<!-- Docker Image & Registry -->
					<div class="settings-group">
						<div class="settings-group-header">
							<Box size={13} />
							<span class="settings-group-title">Docker Image</span>
							<span class="settings-group-desc">Image to pull for the next deployment.</span>
						</div>
						<div class="settings-field">
							<label class="settings-label" for="edit-image">Image</label>
							<input
								id="edit-image"
								class="settings-input font-mono"
								type="text"
								placeholder="nginx:latest"
								bind:value={editImage}
								spellcheck="false"
							/>
						</div>
						<div class="settings-group-header" style="margin-top:10px">
							<span class="settings-group-title">Registry Credentials</span>
							<span class="settings-group-desc">Leave blank to keep existing values.</span>
						</div>
						{#if isLoadingSettingsEnvs}
							<div class="loading-inline"><div class="spinner-sm"></div><span>Loading…</span></div>
						{:else}
							<div class="settings-field">
								<label class="settings-label" for="edit-reg-url">Registry URL</label>
								<input
									id="edit-reg-url"
									class="settings-input font-mono"
									type="text"
									placeholder="registry-1.docker.io"
									bind:value={editRegistryUrl}
									spellcheck="false"
								/>
							</div>
							<div class="settings-row">
								<div class="settings-field" style="flex:1">
									<label class="settings-label" for="edit-reg-user">Username</label>
									<input
										id="edit-reg-user"
										class="settings-input"
										type="text"
										placeholder="myuser"
										bind:value={editRegistryUser}
										autocomplete="off"
									/>
								</div>
								<div class="settings-field" style="flex:1">
									<label class="settings-label" for="edit-reg-pass">
										Password / Token
										{#if registryPassIsSet}<span class="already-set-badge">set</span>{/if}
									</label>
									<input
										id="edit-reg-pass"
										class="settings-input font-mono"
										type="password"
										placeholder={registryPassIsSet ? '(unchanged)' : '••••••••'}
										bind:value={editRegistryPass}
										autocomplete="new-password"
									/>
								</div>
							</div>
						{/if}
					</div>

					<!-- Replicas -->
					<div class="settings-group">
						<div class="settings-group-header">
							<span class="settings-group-title">Replicas</span>
							<span class="settings-group-desc">Number of container instances to run.</span>
						</div>
						<div class="settings-field">
							<label class="settings-label" for="edit-replicas">Instance count</label>
							<div class="replica-stepper">
								<button
									class="stepper-btn"
									type="button"
									onclick={() => editReplicas = Math.max(0, editReplicas - 1)}
									disabled={editReplicas <= 0}
								>−</button>
								<input
									id="edit-replicas"
									class="stepper-input"
									type="number"
									min="0"
									max="20"
									bind:value={editReplicas}
								/>
								<button
									class="stepper-btn"
									type="button"
									onclick={() => editReplicas = Math.min(20, editReplicas + 1)}
									disabled={editReplicas >= 20}
								>+</button>
							</div>
						</div>
					</div>

					<!-- Port mapping -->
					<div class="settings-group">
						<div class="settings-group-header">
							<span class="settings-group-title">Port Mapping</span>
							<span class="settings-group-desc">Ports exposed by this service. Format: <code>80</code> or <code>host:container</code>.</span>
						</div>
						<div class="port-editor">
							{#each editPorts as port, i (i)}
								<div class="port-row">
									<input
										class="port-input"
										type="text"
										placeholder="e.g. 3000 or 8080:80"
										value={port}
										oninput={(e) => updatePort(i, (e.target as HTMLInputElement).value)}
										spellcheck="false"
									/>
									<button class="port-remove-btn" type="button" onclick={() => removePort(i)} title="Remove">
										<X size={13} />
									</button>
								</div>
							{/each}
							<button class="btn btn-secondary btn-sm" type="button" onclick={addPort}>
								<Plus size={12} />
								Add Port
							</button>
						</div>
					</div>

					<!-- Volume Mounts -->
					<div class="settings-group">
						<div class="settings-group-header">
							<HardDrive size={13} />
							<span class="settings-group-title">Volume Mounts</span>
							<span class="settings-group-desc">Bind named volumes or host paths into the container.</span>
						</div>
						{#if isLoadingSettingsEnvs}
							<div class="loading-inline"><div class="spinner-sm"></div><span>Loading…</span></div>
						{:else}
							<VolumeMountList {projectId} bind:mounts={editVolumeMounts} />
						{/if}
					</div>

					<!-- Networks -->
					<div class="settings-group">
						<div class="settings-group-header-row">
							<div class="settings-group-header" style="flex:1;margin-bottom:0">
								<Network size={13} />
								<span class="settings-group-title">Networks</span>
								<span class="settings-group-desc">Docker networks this service is connected to.</span>
							</div>
							<button class="btn btn-secondary btn-xs" type="button" onclick={openNetworkPickerForSettings}>
								<Plus size={11} />Add
							</button>
						</div>
						{#if isLoadingSettingsNetworks}
							<div class="loading-inline"><div class="spinner-sm"></div><span>Loading…</span></div>
						{:else if editNetworks.length === 0}
							<div class="settings-empty">No networks attached.</div>
						{:else}
							<div class="settings-network-list">
								{#each editNetworks as net (net.id)}
									<div class="settings-network-row">
										<Network size={12} class="net-icon" />
										<span class="net-name">{net.name}</span>
										<span class="net-driver">{net.driver}</span>
										<button class="net-remove-btn" type="button" onclick={() => removeSettingsNetwork(net.id)} title="Detach">
											<X size={12} />
										</button>
									</div>
								{/each}
							</div>
						{/if}
					</div>

					<!-- Save feedback -->
					{#if settingsSaveError}
						<div class="settings-error">{settingsSaveError}</div>
					{/if}
					{#if settingsSaveSuccess}
						<div class="settings-success">
							<CheckCircle size={13} /> Saved — click Redeploy to apply changes.
						</div>
					{/if}

					<!-- Save button -->
					<div class="settings-footer">
						<button
							class="btn btn-primary"
							onclick={saveSettings}
							disabled={isSavingSettings}
						>
							{#if isSavingSettings}
								<div class="btn-spinner"></div>Saving…
							{:else}
								<CheckCircle size={14} />Save Changes
							{/if}
						</button>
						<button class="btn btn-secondary" onclick={() => initSettingsFromService()} disabled={isSavingSettings}>
							Reset
						</button>
					</div>
				</div>
			{/if}
		</div>

		<!-- Footer actions -->
		<div class="panel-footer">
			<button class="btn btn-primary btn-sm" disabled={isDeploying || !canDeploy} onclick={triggerDeploy} title={canDeploy ? '' : 'Insufficient permissions'}>
				<Play size={13} />Deploy
			</button>
			<button class="btn btn-secondary btn-sm" disabled={isStopping || !canDeploy} onclick={triggerStop} title={canDeploy ? '' : 'Insufficient permissions'}>
				<Square size={13} />Stop
			</button>
			<button class="btn btn-secondary btn-sm" onclick={() => showEnvPanel = true}>
				<Settings size={13} />Env
			</button>
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

	.spinner {
		width: 28px; height: 28px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	.spinner-sm {
		width: 14px; height: 14px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	.btn-spinner {
		width: 12px; height: 12px;
		border: 2px solid rgba(255,255,255,0.3);
		border-top-color: #fff;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	.btn-spinner-dark {
		width: 12px; height: 12px;
		border: 2px solid rgba(0,0,0,0.2);
		border-top-color: #fff;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	/* ── Header ── */
	.svc-header {
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 8px;
	}
	.svc-identity { display: flex; flex-direction: column; gap: 4px; }
	.svc-name { font-size: 15px; font-weight: 700; color: var(--text-primary); }
	.svc-meta {
		display: flex; align-items: center; gap: 5px;
		font-size: 12px;
	}
	.svc-status { color: var(--text-secondary); }
	.svc-type { color: var(--text-muted); }
	.svc-replicas { color: var(--text-muted); }
	.meta-sep { color: var(--text-dim); }
	.header-actions { display: flex; gap: 6px; align-items: center; flex-shrink: 0; }

	/* ── Tabs ── */
	.tabs-row {
		display: flex;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		padding: 0 8px;
		overflow-x: auto;
		overflow-y: hidden;
		flex-wrap: nowrap;
		scrollbar-width: none;
	}
	.tabs-row::-webkit-scrollbar { display: none; }
	.tab-btn {
		padding: 9px 12px;
		font-size: 12px; font-weight: 500;
		font-family: var(--font-sans);
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--text-muted);
		cursor: pointer;
		transition: all var(--transition-fast);
		margin-bottom: -1px;
	}
	.tab-btn:hover { color: var(--text-primary); }
	.tab-btn.active { color: var(--accent); border-bottom-color: var(--accent); }

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
		gap: 0;
	}

	.info-card {
		margin: 12px 12px 0;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
		background: var(--bg-surface);
	}
	.info-card-header {
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 8px 12px;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.info-card-body {
		padding: 10px 12px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.image-tag {
		font-family: var(--font-mono);
		font-size: 13px;
		color: var(--text-primary);
		background: var(--bg-base);
		padding: 4px 8px;
		border-radius: var(--radius-sm);
		display: inline-block;
	}
	.ports-row {
		display: flex;
		gap: 6px;
		flex-wrap: wrap;
		margin-top: 4px;
	}
	.port-badge {
		font-family: var(--font-mono);
		font-size: 11px;
		background: rgba(37, 99, 235, 0.08);
		color: var(--accent);
		border: 1px solid rgba(37, 99, 235, 0.2);
		padding: 2px 7px;
		border-radius: 999px;
	}
	.info-row {
		display: flex;
		align-items: baseline;
		gap: 8px;
		font-size: 12px;
	}
	.info-key {
		color: var(--text-dim);
		min-width: 80px;
		flex-shrink: 0;
	}
	.info-val {
		font-family: var(--font-mono);
		color: var(--text-primary);
		word-break: break-all;
	}

	.overview-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1px;
		background: var(--border);
		margin: 12px 0 0;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 11px 16px;
		background: var(--bg-base);
	}
	.field-label {
		font-size: 10px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.field-value {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 13px;
		color: var(--text-primary);
		word-break: break-all;
	}

	.section-action {
		padding: 12px 12px 0;
	}
	.full-w { width: 100%; justify-content: center; }

	/* ── Danger zone ── */
	.danger-zone {
		margin: 12px;
		border: 1px solid rgba(239, 68, 68, 0.3);
		border-radius: var(--radius-md);
		overflow: hidden;
	}
	.danger-header {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 8px 12px;
		background: rgba(239, 68, 68, 0.05);
		border-bottom: 1px solid rgba(239, 68, 68, 0.2);
		font-size: 11px;
		font-weight: 600;
		color: #EF4444;
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.danger-body { padding: 10px 12px; }
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
	.deploy-trigger-row { display: flex; gap: 8px; }

	.latest-dep-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.dep-card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
	}
	.dep-card-label {
		font-size: 11px; font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.dep-card-badge {
		font-size: 11px;
		padding: 2px 8px;
		border-radius: 999px;
		text-transform: capitalize;
		font-weight: 500;
	}
	.dep-card-badge.running  { background: rgba(59,130,246,0.12); color: #3B82F6; }
	.dep-card-badge.success,
	.dep-card-badge.running-ok { background: rgba(16,185,129,0.12); color: #10B981; }
	.dep-card-badge.failed   { background: rgba(239,68,68,0.12);   color: #EF4444; }
	.dep-card-badge.stopped,
	.dep-card-badge.pending  { background: var(--bg-elevated);      color: var(--text-muted); }
	.dep-card-meta {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 12px;
		color: var(--text-muted);
	}

	.steps-list { display: flex; flex-direction: column; gap: 3px; margin-top: 4px; }
	.step-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 5px 8px;
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		font-size: 12px;
	}
	.step-item.step-running  { background: rgba(59,130,246,0.07); }
	.step-item.step-failed   { background: rgba(239,68,68,0.07); }
	.step-item.step-success  { background: rgba(16,185,129,0.07); }
	.step-icon { font-size: 11px; font-weight: 700; width: 14px; flex-shrink: 0; color: var(--text-muted); }
	.step-item.step-running .step-icon { color: #3B82F6; }
	.step-item.step-failed  .step-icon { color: #EF4444; }
	.step-item.step-success .step-icon { color: #10B981; }
	.step-name { flex: 1; color: var(--text-primary); }
	.step-time { font-size: 11px; color: var(--text-muted); flex-shrink: 0; }

	.empty-state-msg {
		padding: 28px 16px;
		text-align: center;
		color: var(--text-muted);
		font-size: 13px;
	}

	/* ── Webhook section ── */
	.webhook-section {
		border-bottom: 1px solid var(--border);
		padding: 10px 14px;
		display: flex;
		flex-direction: column;
		gap: 7px;
		background: var(--bg-surface);
		flex-shrink: 0;
	}
	.webhook-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}
	.webhook-label {
		font-size: 11px; font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}
	.webhook-provider-tabs { display: flex; gap: 2px; }
	.webhook-provider-tabs button {
		font-size: 10px; font-weight: 500; font-family: var(--font-sans);
		padding: 2px 8px;
		border-radius: 4px;
		border: 1px solid var(--border);
		background: transparent;
		color: var(--text-muted);
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.webhook-provider-tabs button:hover { color: var(--text-primary); }
	.webhook-provider-tabs button.active {
		border-color: var(--accent);
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
	.spinner-xs { width: 12px; height: 12px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; flex-shrink: 0; }
	.webhook-actions {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 6px 14px 10px;
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

	/* ── Logs tab (deployment list) ── */
	.logs-section { display: flex; flex-direction: column; height: 100%; }
	.logs-intro {
		padding: 10px 16px 6px;
		font-size: 12px;
		color: var(--text-muted);
		border-bottom: 1px solid var(--border);
	}
	.dep-list { list-style: none; margin: 0; padding: 4px 0; }
	.dep-list-row {
		display: flex;
		width: 100%;
		align-items: center;
		gap: 10px;
		padding: 10px 16px;
		background: transparent;
		border: none;
		cursor: pointer;
		text-align: left;
		font-family: var(--font-sans);
		border-bottom: 1px solid var(--border);
		transition: background var(--transition-fast);
		color: inherit;
	}
	.dep-list-row:hover { background: var(--bg-elevated); }
	.dep-row-info { flex: 1; display: flex; flex-direction: column; gap: 2px; min-width: 0; }
	.dep-row-ref { font-size: 13px; font-weight: 500; color: var(--text-primary); }
	.dep-row-meta { font-size: 11px; color: var(--text-muted); display: flex; align-items: center; gap: 4px; }
	.dep-row-badge {
		font-size: 10px;
		padding: 2px 7px;
		border-radius: 999px;
		text-transform: capitalize;
		font-weight: 500;
		flex-shrink: 0;
	}
	.dep-row-badge.running  { background: rgba(59,130,246,0.12); color: #3B82F6; }
	.dep-row-badge.running-ok,
	.dep-row-badge.success  { background: rgba(16,185,129,0.12); color: #10B981; }
	.dep-row-badge.failed   { background: rgba(239,68,68,0.12);  color: #EF4444; }
	.dep-row-badge.queued   { background: rgba(245,158,11,0.12); color: #F59E0B; }
	.dep-row-badge.stopped,
	.dep-row-badge.pending  { background: var(--bg-elevated);     color: var(--text-muted); }
	:global(.dep-row-arrow) { color: var(--text-dim); flex-shrink: 0; }

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
	.stopped-badge {
		font-size: 10px; font-weight: 600; padding: 1px 6px;
		border-radius: 99px; text-transform: capitalize;
		background: var(--bg-elevated); color: var(--text-muted);
		border: 1px solid var(--border);
	}
	.replica-meta {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		color: var(--text-muted);
		flex-wrap: wrap;
	}
	.exit-code { font-family: var(--font-mono); font-size: 10px; }
	.exit-nonzero { color: #EF4444; }
	.replica-actions { display: flex; align-items: center; gap: 2px; flex-shrink: 0; }
	.replica-del-btn { color: var(--text-dim); }
	.replica-del-btn:hover { color: #EF4444 !important; }

	/* ── Domains ── */
	.domains-section { display: flex; flex-direction: column; }

	.domain-header-bar {
		display: flex; align-items: center; justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-surface);
	}
	.domain-header-title {
		font-size: 12px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
	}

	.domain-error {
		margin: 8px 12px 0;
		font-size: 12px; color: #EF4444;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.2);
		border-radius: var(--radius-sm);
		padding: 6px 10px;
	}

	.domain-empty {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; padding: 40px 16px;
		color: var(--text-muted); font-size: 13px;
	}

	.domain-list { list-style: none; margin: 0; padding: 0; }
	.domain-item {
		display: flex; flex-direction: column;
		border-bottom: 1px solid var(--border);
	}
	.domain-item:last-child { border-bottom: none; }

	.domain-item-top {
		display: flex; align-items: flex-start; gap: 10px;
		padding: 11px 14px;
	}
	.domain-info { flex: 1; display: flex; flex-direction: column; gap: 4px; min-width: 0; }
	.domain-hostname {
		font-family: var(--font-mono); font-size: 13px;
		color: var(--text-primary); word-break: break-all;
	}
	.domain-badges { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }

	.badge {
		display: inline-flex; align-items: center; gap: 3px;
		font-size: 10px; font-weight: 600; padding: 1px 6px;
		border-radius: 999px; flex-shrink: 0;
	}
	.badge-green {
		background: rgba(16,185,129,0.1); color: #10B981;
		border: 1px solid rgba(16,185,129,0.25);
	}
	.badge-blue {
		background: rgba(59,130,246,0.1); color: #3B82F6;
		border: 1px solid rgba(59,130,246,0.25);
		font-family: var(--font-mono);
	}
	.badge-dim {
		background: var(--bg-elevated); color: var(--text-muted);
		border: 1px solid var(--border);
	}

	.domain-actions { display: flex; align-items: center; gap: 4px; flex-shrink: 0; }

	.dns-check-btn {
		display: flex; align-items: center; gap: 4px;
		font-size: 11px; font-weight: 600; font-family: var(--font-sans);
		padding: 4px 9px; border-radius: var(--radius-sm); cursor: pointer;
		border: 1px solid var(--border); background: var(--bg-elevated);
		color: var(--text-muted); transition: all var(--transition-fast);
	}
	.dns-check-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.dns-check-btn:disabled { opacity: 0.6; cursor: default; }
	.dns-check-btn.dns-ok    { border-color: rgba(16,185,129,0.4); color: #10B981; background: rgba(16,185,129,0.06); }
	.dns-check-btn.dns-fail  { border-color: rgba(239,68,68,0.4);  color: #EF4444; background: rgba(239,68,68,0.06); }
	.dns-check-btn.dns-checking { opacity: 0.7; }
	:global(.spin-icon) { animation: spin 1s linear infinite; }

	.dns-result {
		margin: 0 14px 10px 37px;
		font-size: 11px; font-family: var(--font-mono);
		padding: 5px 9px; border-radius: var(--radius-sm);
	}
	.dns-result-ok   { background: rgba(16,185,129,0.08); color: #10B981; border: 1px solid rgba(16,185,129,0.2); }
	.dns-result-fail { background: rgba(239,68,68,0.08);  color: #EF4444; border: 1px solid rgba(239,68,68,0.2); }

	/* ── Container logs panel (portalled, 2/3 screen) ── */
	:global(.clog-backdrop) {
		position: fixed;
		inset: 0;
		background: rgba(0,0,0,0.45);
		z-index: 800;
		display: flex;
		justify-content: flex-end;
	}

	:global(.clog-panel) {
		position: fixed;
		right: 0;
		top: 0;
		bottom: 0;
		width: 67vw;
		min-width: 480px;
		background: #0B1120;
		border-left: 1px solid rgba(0,0,0,0.3);
		display: flex;
		flex-direction: column;
		z-index: 801;
		box-shadow: -8px 0 32px rgba(0,0,0,0.4);
	}

	:global(.clog-panel) .log-overlay-header {
		background: #0F172A;
		border-bottom-color: rgba(0,0,0,0.2);
		flex-wrap: wrap;
		gap: 6px;
	}
	:global(.clog-panel) .log-overlay-title {
		color: #E5E7EB;
	}
	:global(.clog-panel) .log-dep-time {
		color: #6B7280;
	}

	/* Tail selector in container log header */
	.clog-tail-group {
		display: flex; align-items: center; gap: 2px; margin-right: 8px;
	}
	.clog-tail-label {
		font-size: 10px; font-weight: 600; color: #4B5563;
		text-transform: uppercase; letter-spacing: 0.06em;
		margin-right: 4px; font-family: var(--font-sans);
	}
	.clog-tail-btn {
		padding: 2px 7px;
		font-size: 10px; font-weight: 600; font-family: var(--font-mono);
		background: transparent; border: 1px solid transparent;
		border-radius: 3px; color: #4B5563; cursor: pointer;
		transition: all 0.12s;
	}
	.clog-tail-btn:hover { color: #9CA3AF; background: rgba(255,255,255,0.05); }
	.clog-tail-btn.active { background: rgba(255,255,255,0.08); border-color: rgba(255,255,255,0.15); color: #E5E7EB; }

	/* Live controls */
	.log-overlay-controls {
		display: flex; align-items: center; gap: 7px; margin-left: auto;
	}
	.clog-dot {
		width: 7px; height: 7px; border-radius: 50%;
		background: #22C55E; box-shadow: 0 0 5px #22C55E;
		animation: pulse 2s ease-in-out infinite; flex-shrink: 0;
	}
	@keyframes pulse { 0%, 100% { opacity: 1; } 50% { opacity: 0.4; } }
	.clog-status-label { font-size: 11px; font-weight: 500; color: #6B7280; }
	.clog-status-label.muted { color: #4B5563; }
	.clog-status-label.error { color: #EF4444; max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.clog-ctrl-btn {
		display: flex; align-items: center; gap: 4px;
		padding: 3px 9px; border-radius: 4px;
		font-size: 11px; font-weight: 500; font-family: var(--font-sans);
		background: rgba(255,255,255,0.05); border: 1px solid rgba(255,255,255,0.12);
		color: #9CA3AF; cursor: pointer;
		transition: all 0.15s;
	}
	.clog-ctrl-btn:hover { background: rgba(255,255,255,0.1); color: #E5E7EB; }
	.clog-ctrl-btn.primary { background: rgba(37,99,235,0.2); border-color: rgba(37,99,235,0.4); color: #60A5FA; }
	.clog-ctrl-btn.primary:hover { background: rgba(37,99,235,0.3); }

	.clog-lines {
		flex: 1;
		overflow-y: auto;
		padding: 6px 0;
		background: #0B1120;
		font-family: var(--font-mono);
	}

	/* Log line base */
	.clog-line {
		padding: 1.5px 16px;
		font-size: 11.5px;
		line-height: 1.65;
		color: #9CA3AF;
		white-space: pre-wrap;
		word-break: break-all;
		border-left: 2px solid transparent;
	}
	.clog-line:hover { background: rgba(255,255,255,0.03); }

	/* Live lines — subtle highlight, don't override level colors */
	.clog-line.clog-live { background: rgba(255,255,255,0.015); }

	/* Level tints on raw lines */
	.clog-lvl-error {
		color: #FCA5A5;
		background: rgba(239,68,68,0.05);
		border-left-color: #7F1D1D;
	}
	.clog-lvl-error:hover { background: rgba(239,68,68,0.09); }
	.clog-lvl-warn {
		color: #FDE68A;
		background: rgba(245,158,11,0.04);
		border-left-color: #78350F;
	}
	.clog-lvl-warn:hover { background: rgba(245,158,11,0.08); }
	.clog-lvl-debug { color: #4B5563; opacity: 0.75; }

	.clog-stream-divider {
		padding: 4px 16px;
		font-size: 10px;
		color: #374151;
		letter-spacing: 0.08em;
		user-select: none;
	}

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

	/* ── Log viewer overlay ── */
	.log-overlay {
		position: absolute;
		inset: 0;
		background: var(--bg-base);
		display: flex;
		flex-direction: column;
		z-index: 20;
	}
	.log-overlay-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		background: var(--bg-surface);
	}
	.log-overlay-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
	}
	.log-dep-status { flex-shrink: 0; }
	.log-dep-time { font-size: 11px; font-weight: 400; color: var(--text-muted); }
	.live-badge {
		font-size: 9px;
		font-weight: 700;
		letter-spacing: 0.08em;
		color: #10B981;
		background: rgba(16,185,129,0.12);
		border: 1px solid rgba(16,185,129,0.3);
		border-radius: 4px;
		padding: 1px 5px;
	}
	.log-loading {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 20px 16px;
		font-size: 13px;
		color: var(--text-muted);
	}

	/* Accordion */
	.accordion-list {
		flex: 1;
		overflow-y: auto;
		display: flex;
		flex-direction: column;
	}
	.accordion-item {
		border-bottom: 1px solid var(--border);
	}
	.accordion-header {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 9px 14px;
		background: transparent;
		border: none;
		cursor: pointer;
		font-family: var(--font-sans);
		font-size: 12px;
		text-align: left;
		color: var(--text-primary);
		transition: background var(--transition-fast);
	}
	.accordion-header:hover { background: var(--bg-elevated); }
	.acc-expanded > .accordion-header { background: var(--bg-elevated); }
	.acc-icon {
		font-size: 12px;
		font-weight: 700;
		width: 16px;
		flex-shrink: 0;
		text-align: center;
	}
	.acc-pending  { color: var(--text-dim); }
	.acc-running  { color: #3B82F6; animation: spin 1s linear infinite; display: inline-block; }
	.acc-success  { color: #10B981; }
	.acc-failed   { color: #EF4444; }
	.acc-skipped  { color: var(--text-dim); }
	.acc-name { flex: 1; font-weight: 500; color: var(--text-secondary); }
	.acc-expanded > .accordion-header .acc-name { color: var(--text-primary); }
	.acc-count {
		font-size: 10px;
		padding: 1px 6px;
		border-radius: 999px;
		background: var(--bg-hover);
		color: var(--text-muted);
		flex-shrink: 0;
	}
	.acc-dur {
		font-size: 10px;
		color: var(--text-dim);
		flex-shrink: 0;
	}
	.acc-chevron {
		flex-shrink: 0;
		color: var(--text-dim);
		display: flex;
		align-items: center;
		transition: transform var(--transition-fast);
	}
	.acc-chevron.rotated { transform: rotate(90deg); }

	.acc-logs {
		background: #0F172A;
		padding: 6px 0;
		font-family: var(--font-mono);
		border-top: 1px solid rgba(0,0,0,0.2);
	}
	.acc-empty {
		padding: 10px 14px;
		font-size: 11px;
		color: #4B5563;
		font-family: var(--font-mono);
	}

	.log-entry {
		display: flex;
		gap: 10px;
		padding: 1px 14px;
		font-size: 11px;
		line-height: 1.7;
	}
	.log-entry:hover { background: rgba(255,255,255,0.04); }
	.log-ts { color: #374151; flex-shrink: 0; }
	.log-lvl { flex-shrink: 0; font-weight: 600; width: 40px; }
	.log-msg { color: #9CA3AF; word-break: break-all; }
	.log-entry.log-error .log-lvl { color: #F87171; }
	.log-entry.log-error .log-msg { color: #FCA5A5; }
	.log-entry.log-warn  .log-lvl { color: #FBBF24; }
	.log-entry.log-warn  .log-msg { color: #FDE68A; }
	.log-entry.log-debug .log-lvl { color: #374151; }
	.log-entry.log-info  .log-lvl { color: #60A5FA; }
	.empty-logs-msg {
		padding: 32px 16px;
		text-align: center;
		color: var(--text-muted);
		font-size: 13px;
	}

	/* ── Env overlay ── */
	.env-overlay {
		position: absolute;
		inset: 0;
		background: var(--bg-base);
		display: flex;
		flex-direction: column;
		z-index: 20;
	}
	.env-overlay-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
		background: var(--bg-surface);
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
	}
	.env-overlay-body { flex: 1; overflow: hidden; }

	/* ── Delete modal (global — node is portalled to body) ── */
	:global(.sdp-modal-backdrop) {
		position: fixed;
		inset: 0;
		background: rgba(0,0,0,0.55);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 9999;
		padding: 24px;
	}
	:global(.sdp-modal) {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		width: 100%;
		max-width: 440px;
		overflow: hidden;
		box-shadow: 0 24px 64px rgba(0,0,0,0.35);
	}
	:global(.sdp-modal-header) {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 16px 20px;
		border-bottom: 1px solid var(--border);
		font-size: 15px;
		font-weight: 700;
		color: var(--text-primary);
	}
	:global(.sdp-modal-body) {
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	:global(.sdp-modal-warning) {
		font-size: 13px;
		color: var(--text-secondary);
		line-height: 1.6;
		margin: 0;
	}
	:global(.sdp-confirm-field) { display: flex; flex-direction: column; gap: 6px; }
	:global(.sdp-confirm-label) { font-size: 12px; color: var(--text-muted); }
	:global(.sdp-confirm-code) {
		font-family: var(--font-mono);
		background: var(--bg-elevated);
		padding: 1px 5px;
		border-radius: 3px;
		font-size: 12px;
		color: var(--text-primary);
	}
	:global(.sdp-confirm-input) {
		font-family: var(--font-mono);
		font-size: 13px;
		padding: 8px 10px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-base);
		color: var(--text-primary);
		outline: none;
		transition: border-color var(--transition-fast);
		width: 100%;
		box-sizing: border-box;
	}
	:global(.sdp-confirm-input:focus) { border-color: #EF4444; }
	:global(.sdp-delete-error) {
		font-size: 12px;
		color: #EF4444;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.2);
		border-radius: var(--radius-sm);
		padding: 8px 10px;
	}
	:global(.sdp-modal-footer) {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 14px 20px;
		border-top: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	/* ── Shared button variants ── */
	.icon-btn {
		background: transparent;
		border: none;
		cursor: pointer;
		color: var(--text-muted);
		padding: 4px;
		border-radius: var(--radius-sm);
		display: flex;
		align-items: center;
		justify-content: center;
		transition: color var(--transition-fast), background var(--transition-fast);
	}
	.icon-btn:hover { color: var(--text-primary); background: var(--bg-elevated); }

	.font-mono { font-family: var(--font-mono); }

	/* ── Settings tab ── */
	.settings-section {
		display: flex;
		flex-direction: column;
		gap: 0;
	}
	.settings-hint {
		padding: 8px 14px;
		font-size: 12px;
		color: var(--text-muted);
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
	}
	.settings-hint strong { color: var(--text-primary); }
	.settings-group {
		padding: 14px;
		border-bottom: 1px solid var(--border);
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
	.settings-field { display: flex; flex-direction: column; gap: 5px; }
	.settings-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}

	/* Replica stepper */
	.replica-stepper { display: flex; align-items: center; gap: 0; width: fit-content; }
	.stepper-btn {
		width: 30px; height: 30px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text-primary);
		font-size: 16px; line-height: 1;
		cursor: pointer;
		display: flex; align-items: center; justify-content: center;
		transition: background var(--transition-fast);
		flex-shrink: 0;
	}
	.stepper-btn:first-child { border-radius: var(--radius-sm) 0 0 var(--radius-sm); }
	.stepper-btn:last-child  { border-radius: 0 var(--radius-sm) var(--radius-sm) 0; }
	.stepper-btn:hover:not(:disabled) { background: var(--bg-hover); }
	.stepper-btn:disabled { opacity: 0.4; cursor: default; }
	.stepper-input {
		width: 52px; height: 30px;
		text-align: center;
		font-size: 14px; font-weight: 600;
		font-family: var(--font-mono);
		color: var(--text-primary);
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-left: none; border-right: none;
		outline: none;
		-moz-appearance: textfield;
	}
	.stepper-input::-webkit-outer-spin-button,
	.stepper-input::-webkit-inner-spin-button { -webkit-appearance: none; margin: 0; }

	/* Port editor */
	.port-editor { display: flex; flex-direction: column; gap: 6px; }
	.port-row { display: flex; align-items: center; gap: 6px; }
	.port-input {
		flex: 1;
		height: 30px;
		padding: 0 10px;
		font-family: var(--font-mono);
		font-size: 12px;
		color: var(--text-primary);
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		outline: none;
		transition: border-color var(--transition-fast);
	}
	.port-input:focus { border-color: var(--accent); }
	.port-remove-btn {
		width: 26px; height: 26px;
		background: transparent;
		border: none;
		cursor: pointer;
		color: var(--text-dim);
		display: flex; align-items: center; justify-content: center;
		border-radius: var(--radius-sm);
		transition: color var(--transition-fast), background var(--transition-fast);
		flex-shrink: 0;
	}
	.port-remove-btn:hover { color: #EF4444; background: rgba(239,68,68,0.08); }

	.settings-error {
		margin: 0 14px;
		padding: 8px 10px;
		font-size: 12px;
		color: #EF4444;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.2);
		border-radius: var(--radius-sm);
	}
	.settings-success {
		margin: 0 14px;
		padding: 8px 10px;
		font-size: 12px;
		color: #10B981;
		background: rgba(16,185,129,0.08);
		border: 1px solid rgba(16,185,129,0.2);
		border-radius: var(--radius-sm);
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.settings-footer {
		padding: 14px;
		display: flex;
		gap: 8px;
	}

	.settings-input {
		height: 30px;
		padding: 0 10px;
		font-size: 12px;
		color: var(--text-primary);
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		outline: none;
		transition: border-color var(--transition-fast);
		font-family: var(--font-sans);
		width: 100%;
	}
	.settings-input:focus { border-color: var(--accent); }
	.settings-input.font-mono { font-family: var(--font-mono); }

	.settings-row { display: flex; gap: 10px; }

	.settings-group-header {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
	}

	.settings-group-header-row {
		display: flex;
		align-items: flex-start;
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
	.net-remove-btn {
		width: 22px; height: 22px;
		background: transparent; border: none;
		cursor: pointer; color: var(--text-dim);
		display: flex; align-items: center; justify-content: center;
		border-radius: 3px; flex-shrink: 0;
		transition: color var(--transition-fast);
	}
	.net-remove-btn:hover { color: var(--accent-red); }

	.already-set-badge {
		font-size: 9px; font-weight: 600; text-transform: uppercase;
		padding: 1px 5px; border-radius: 99px;
		background: rgba(16,185,129,0.12);
		color: #10B981;
		letter-spacing: 0.05em;
		border: 1px solid rgba(16,185,129,0.25);
	}

	/* ── Monitor tab ── */
	.monitor-section {
		display: flex;
		flex-direction: column;
		height: 100%;
	}

	.monitor-selector {
		display: flex;
		flex-wrap: wrap;
		gap: 4px;
		padding: 8px 12px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-surface);
		flex-shrink: 0;
	}
	.monitor-sel-btn {
		font-size: 11px; font-weight: 500; font-family: var(--font-mono);
		padding: 3px 10px;
		border-radius: 99px;
		border: 1px solid var(--border);
		background: var(--bg-base);
		color: var(--text-muted);
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.monitor-sel-btn:hover { border-color: var(--accent); color: var(--accent); }
	.monitor-sel-btn.active {
		border-color: var(--accent);
		color: var(--accent);
		background: rgba(37,99,235,0.07);
	}

	.metric-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 1px;
		background: var(--border);
		flex: 1;
		align-content: start;
	}

	.metric-card {
		background: var(--bg-base);
		padding: 12px 12px 10px;
		display: flex;
		flex-direction: column;
		gap: 7px;
	}

	.metric-header {
		display: flex;
		align-items: baseline;
		justify-content: space-between;
		gap: 6px;
	}
	.metric-label {
		font-size: 10px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.07em;
		flex-shrink: 0;
	}
	.metric-value {
		font-size: 17px;
		font-weight: 700;
		font-family: var(--font-mono);
		line-height: 1;
	}
	.metric-value.cpu  { color: #3B82F6; }
	.metric-value.mem  { color: #10B981; }

	.spark {
		width: 100%;
		height: 46px;
		display: block;
		border-radius: 4px;
		background: var(--bg-elevated);
		overflow: visible;
	}

	.metric-sub {
		font-size: 10px;
		color: var(--text-muted);
		font-family: var(--font-mono);
	}

	.metric-net-row {
		display: flex;
		align-items: center;
		gap: 5px;
		flex-wrap: wrap;
	}
	.net-chip {
		font-size: 10px;
		font-weight: 600;
		font-family: var(--font-mono);
		padding: 2px 7px;
		border-radius: 99px;
	}
	.net-chip.rx    { background: rgba(99,102,241,0.12);  color: #4F46E5; border: 1px solid rgba(99,102,241,0.25); }
	.net-chip.tx    { background: rgba(219,39,119,0.10);  color: #BE185D; border: 1px solid rgba(219,39,119,0.25); }
	.net-chip.blk-r { background: rgba(217,119,6,0.10);   color: #B45309; border: 1px solid rgba(217,119,6,0.25); }
	.net-chip.blk-w { background: rgba(234,88,12,0.10);   color: #C2410C; border: 1px solid rgba(234,88,12,0.25); }

	.monitor-footer {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 12px;
		border-top: 1px solid var(--border);
		background: var(--bg-surface);
		flex-shrink: 0;
	}
	.monitor-footer-pids {
		font-size: 10px;
		font-weight: 600;
		color: var(--text-dim);
		font-family: var(--font-mono);
	}
	.monitor-footer-ts {
		font-size: 10px;
		color: var(--text-dim);
	}

	.monitor-error {
		margin: 12px;
		padding: 8px 10px;
		font-size: 12px;
		color: #EF4444;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.2);
		border-radius: var(--radius-sm);
	}

	@media (max-width: 639px) {
		.tabs-row {
			-webkit-overflow-scrolling: touch;
		}

		.header-actions {
			flex-wrap: wrap;
		}

		.overview-grid,
		.metric-grid {
			grid-template-columns: 1fr;
		}
	}
</style>
