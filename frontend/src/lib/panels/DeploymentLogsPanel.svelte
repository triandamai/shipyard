<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { ChevronRight } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { subscribeToDeploymentSteps } from '$lib/mqtt/subscriptions';
	import { eventBus } from '$lib/mqtt/eventBus';
	import type { Deployment, DeploymentStep, DeploymentLog, MqttPayload } from '$lib/api/types';
	import { formatDistanceToNow } from 'date-fns';
	import { StatusDot, Badge, Spinner } from '$lib/components/ui';
	import type { DotStatus } from '$lib/utils/status';

	interface Props {
		orgId:      string;
		projectId:  string;
		serviceId:  string;
		/** Initial deployment object. Status will update live via MQTT if active. */
		deployment: Deployment;
	}

	let { orgId, projectId, serviceId, deployment }: Props = $props();

	// ── State ───────────────────────────────────────────────────────────────────
	let dep       = $state<Deployment>({ ...deployment });
	let steps     = $state<DeploymentStep[]>([]);
	let logs      = $state<DeploymentLog[]>([]);
	let loading   = $state(true);
	let isLive    = $state(false);
	let expanded  = $state<Set<string>>(new Set());
	let listEl    = $state<HTMLDivElement | null>(null);

	let mqttCleanup: (() => void) | null = null;

	// ── Derived ─────────────────────────────────────────────────────────────────
	const STEP_TERMINAL = new Set(['success', 'failed', 'skipped']);

	let stepLogsMap = $derived(
		logs.reduce((acc: Record<string, DeploymentLog[]>, log) => {
			const key = log.step_id || '__global__';
			if (!acc[key]) acc[key] = [];
			acc[key].push(log);
			return acc;
		}, {})
	);
	let globalLogs = $derived(stepLogsMap['__global__'] ?? []);

	// ── Helpers ─────────────────────────────────────────────────────────────────
	// Page-local mapping that preserves the old header colors
	// (toDotStatus has no `success`, which would turn green into grey).
	function headerDot(status: string): DotStatus {
		if (status === 'success') return 'running';
		if (status === 'failed') return 'failed';
		if (status === 'running') return 'pending';
		if (status === 'queued' || status === 'pending') return 'deploying';
		return 'stopped';
	}

	function badgeTone(status: string): 'green' | 'red' | 'yellow' | 'blue' | 'neutral' {
		if (status === 'success') return 'green';
		if (status === 'failed') return 'red';
		if (status === 'running') return 'yellow';
		if (status === 'queued' || status === 'pending') return 'blue';
		return 'neutral';
	}

	function stepIcon(s: string) {
		switch (s) {
			case 'success': return '✓';
			case 'running': return '⟳';
			case 'failed':  return '✗';
			case 'skipped': return '–';
			default:        return '○';
		}
	}

	function stepDuration(step: DeploymentStep): string {
		if (!step.started_at || !step.finished_at) return '';
		const ms = new Date(step.finished_at).getTime() - new Date(step.started_at).getTime();
		return ms < 1000 ? `${ms}ms` : `${(ms / 1000).toFixed(1)}s`;
	}

	function logLevelClass(level: string) {
		if (level === 'error') return 'log-error';
		if (level === 'warn')  return 'log-warn';
		return 'log-info';
	}

	function formatTime(ts: string | null | undefined): string {
		if (!ts) return '–';
		try { return formatDistanceToNow(new Date(ts), { addSuffix: true }); }
		catch { return ts ?? '–'; }
	}

	function allTerminal(ss: DeploymentStep[]) {
		return ss.length > 0 && ss.every(s => STEP_TERMINAL.has(s.status));
	}

	function toggle(stepId: string) {
		const next = new Set(expanded);
		if (next.has(stepId)) next.delete(stepId);
		else next.add(stepId);
		expanded = next;
	}

	function scrollBottom() {
		requestAnimationFrame(() => { if (listEl) listEl.scrollTop = listEl.scrollHeight; });
	}

	$effect(() => { logs.length; scrollBottom(); });

	// ── MQTT ─────────────────────────────────────────────────────────────────────
	function finalize() {
		mqttCleanup?.();
		mqttCleanup = null;
		const anyFailed = steps.some(s => s.status === 'failed');
		dep = { ...dep, status: anyFailed ? 'failed' : 'success' };
		isLive = false;
	}

	function startMqtt() {
		const unsub = subscribeToDeploymentSteps(orgId, projectId, serviceId, dep.id);
		const prefix = `platform/orgs/${orgId}/projects/${projectId}/services/${serviceId}/deployments/${dep.id}/steps/`;

		const handler = (_type: string, evt: unknown) => {
			const topic = _type as string;
			const payload = evt as MqttPayload;
			if (!topic.startsWith(prefix)) return;

			if (topic.endsWith('/status')) {
				const meta = payload.meta as any;
				if (!meta?.step_id || !meta?.status) return;
				steps = steps.map(s => s.id === meta.step_id ? { ...s, status: meta.status } : s);
				if (meta.status === 'running') {
					expanded = new Set([...expanded, meta.step_id as string]);
					scrollBottom();
				}
				if (allTerminal(steps)) finalize();

			} else if (topic.endsWith('/log')) {
				const meta = payload.meta as any;
				if (!payload.message) return;
				logs = [...logs, {
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
		mqttCleanup = () => { eventBus.off('*', handler as any); unsub(); isLive = false; };
	}

	// ── Load ─────────────────────────────────────────────────────────────────────
	onMount(async () => {
		const isActive = dep.status === 'running' || dep.status === 'pending' || dep.status === 'queued';
		if (isActive) startMqtt();

		const [stepsRes, logsRes] = await Promise.all([
			api.get<DeploymentStep[]>(`/deployments/${dep.id}/steps`),
			api.get<DeploymentLog[]>(`/deployments/${dep.id}/logs`),
		]);

		if (stepsRes.data) steps = stepsRes.data.sort((a, b) => a.order_index - b.order_index);
		if (logsRes.data) {
			// Merge HTTP (authoritative) with any MQTT logs that arrived during the fetch
			const mqttPending = logs;
			const httpKeys = new Set(logsRes.data.map(l => `${l.message}|${l.timestamp}`));
			const mqttOnly = mqttPending.filter(l => !httpKeys.has(`${l.message}|${l.timestamp}`));
			logs = [...logsRes.data, ...mqttOnly];
		}

		// Auto-expand: failed step first (shows the error immediately), then running, then last
		const failedStep  = steps.find(s => s.status === 'failed');
		const runningStep = steps.find(s => s.status === 'running');
		const lastStep    = steps[steps.length - 1];
		const autoExpand  = failedStep ?? runningStep ?? lastStep;
		if (autoExpand) expanded = new Set([autoExpand.id]);

		scrollBottom();
		loading = false;

		if (isActive && allTerminal(steps)) finalize();
	});

	onDestroy(() => { mqttCleanup?.(); });
</script>

<!-- ── Header strip ──────────────────────────────────────────────────────────── -->
<div class="dep-header">
	<StatusDot status={headerDot(dep.status)} />
	<span class="dep-id">{dep.id.slice(0, 8)}</span>
	{#if dep.source_ref}
		<span class="dep-ref">{dep.source_ref}</span>
	{/if}
	<span class="dep-time">{formatTime(dep.created_at)}</span>
	{#if isLive}
		<Badge tone="green">LIVE</Badge>
	{/if}
	<Badge tone={badgeTone(dep.status)}>{dep.status}</Badge>
</div>

<!-- ── Body ─────────────────────────────────────────────────────────────────── -->
{#if loading}
	<div class="state-row"><Spinner size={16} /> Loading logs…</div>
{:else if steps.length === 0 && globalLogs.length === 0}
	<div class="state-row muted">No log entries recorded for this deployment.</div>
{:else}
	<div class="accordion-list" bind:this={listEl}>
		{#each steps as step (step.id)}
			{@const stepLogs = stepLogsMap[step.id] ?? []}
			{@const isExpanded = expanded.has(step.id)}
			{@const dur = stepDuration(step)}
			<div class="accordion-item" class:acc-expanded={isExpanded}>
				<button class="accordion-header" onclick={() => toggle(step.id)}>
					<span class="acc-icon acc-{step.status}">{stepIcon(step.status)}</span>
					<span class="acc-name">{step.name.replace(/_/g, ' ')}</span>
					<span class="acc-count">{stepLogs.length}</span>
					{#if dur}<span class="acc-dur">{dur}</span>{/if}
					<span class="acc-chevron" class:rotated={isExpanded}>
						<ChevronRight size={13} />
					</span>
				</button>
				{#if isExpanded}
					<div class="acc-logs">
						{#if stepLogs.length === 0}
							<div class="acc-empty">No output for this step.</div>
						{:else}
							{#each stepLogs as log (log.id)}
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
			{@const isExpanded = expanded.has('__global__')}
			<div class="accordion-item" class:acc-expanded={isExpanded}>
				<button class="accordion-header" onclick={() => toggle('__global__')}>
					<span class="acc-icon acc-pending">○</span>
					<span class="acc-name">General</span>
					<span class="acc-count">{globalLogs.length}</span>
					<span class="acc-chevron" class:rotated={isExpanded}>
						<ChevronRight size={13} />
					</span>
				</button>
				{#if isExpanded}
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
	</div>
{/if}

<style>
	/* ── Header ── */
	.dep-header {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		flex-shrink: 0;
		flex-wrap: wrap;
	}

	.dep-id {
		font-size: 12px;
		font-family: var(--font-mono);
		font-weight: 700;
		color: var(--text-primary);
	}

	.dep-ref {
		font-size: 11px;
		color: var(--text-dim);
		font-family: var(--font-mono);
		max-width: 140px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.dep-time {
		font-size: 11px;
		color: var(--text-dim);
		flex: 1;
	}

	/* ── States ── */
	.state-row {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 32px 16px;
		font-size: 13px;
		color: var(--text-muted);
		justify-content: center;
	}
	.state-row.muted { color: var(--text-dim); }

	/* ── Accordion ── */
	.accordion-list {
		flex: 1;
		overflow-y: auto;
		padding: 8px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.accordion-item {
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		overflow: hidden;
		background: var(--bg-elevated);
	}
	.accordion-item.acc-expanded {
		border-color: color-mix(in srgb, var(--accent) 40%, var(--border));
	}

	.accordion-header {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 8px 12px;
		background: none;
		border: none;
		cursor: pointer;
		text-align: left;
		font-size: 12px;
		color: var(--text-primary);
	}
	.accordion-header:hover { background: var(--bg-surface); }

	.acc-icon {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 9px;
		font-weight: 700;
		flex-shrink: 0;
	}
	.acc-success { background: color-mix(in srgb, var(--accent-green) 15%, transparent); color: var(--accent-green); }
	.acc-running { background: color-mix(in srgb, var(--accent-yellow) 15%, transparent); color: var(--accent-yellow); }
	.acc-failed  { background: color-mix(in srgb, var(--accent-red) 15%, transparent); color: var(--accent-red); }
	.acc-skipped { background: var(--bg-surface); color: var(--text-dim); }
	.acc-pending { background: var(--bg-surface); color: var(--text-dim); }

	.acc-name  { flex: 1; font-weight: 500; }
	.acc-count { font-size: 10px; color: var(--text-dim); background: var(--bg-surface); padding: 1px 6px; border-radius: 100px; }
	.acc-dur   { font-size: 10px; color: var(--text-dim); }
	.acc-chevron { color: var(--text-dim); transition: transform 0.15s; display: flex; align-items: center; }
	.acc-chevron.rotated { transform: rotate(90deg); }

	.acc-logs {
		padding: 8px;
		background: var(--bg-base);
		display: flex;
		flex-direction: column;
		gap: 1px;
		max-height: 320px;
		overflow-y: auto;
	}

	.acc-empty { font-size: 11px; color: var(--text-dim); padding: 4px 6px; }

	/* ── Log entries ── */
	.log-entry {
		display: grid;
		grid-template-columns: 6rem 3rem 1fr;
		gap: 6px;
		font-size: 11px;
		font-family: var(--font-mono);
		line-height: 1.5;
		padding: 1px 4px;
		border-radius: 2px;
	}
	.log-ts  { color: var(--text-dim); }
	.log-lvl { font-weight: 700; }
	.log-msg { word-break: break-all; color: var(--text-primary); }

	.log-info  .log-lvl { color: var(--accent); }
	.log-warn  .log-lvl { color: var(--accent-yellow); }
	.log-error .log-lvl { color: var(--accent-red); }
	.log-info  { background: transparent; }
	.log-warn  { background: color-mix(in srgb, var(--accent-yellow) 5%, transparent); }
	.log-error { background: color-mix(in srgb, var(--accent-red) 8%, transparent); }

</style>
