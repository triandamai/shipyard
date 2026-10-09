<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import { page } from '$app/stores';
	import {
		Globe, Save, Check, AlertCircle, Star,
		RefreshCw, Terminal, Zap, PackageOpen
	} from '@lucide/svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import FormField from '$lib/components/ui/FormField.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import IconBadge from '$lib/components/ui/IconBadge.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	interface PlatformSettings {
		main_domain?: string;
		traefik_network?: string;
		traefik_entrypoint_http?: string;
		traefik_entrypoint_https?: string;
		traefik_cert_resolver?: string;
		max_parallel_deployments?: number;
	}

	// ── Version info ────────────────────────────────────────────────────────────
	interface VersionInfo {
		current: string;
		git_sha: string;
		build_date: string;
		update_available: boolean;
		remote_sha: string | null;
	}
	let versionInfo    = $state<VersionInfo | null>(null);
	let loadingVersion = $state(false);
	let checkingUpdate = $state(false);

	function formatBuildDate(iso: string): string {
		if (!iso || iso === 'unknown') return '';
		try {
			return new Date(iso).toLocaleString('en-US', {
				year: 'numeric', month: 'short', day: 'numeric',
				hour: '2-digit', minute: '2-digit', timeZoneName: 'short'
			});
		} catch { return iso; }
	}

	async function checkForUpdates() {
		checkingUpdate = true;
		const res = await api.get<VersionInfo>('/admin/version?force=true');
		if (res.data) versionInfo = res.data;
		checkingUpdate = false;
	}

	// ── Update state ────────────────────────────────────────────────────────────
	type UpdateStatus = 'idle' | 'running' | 'done' | 'error' | 'disconnected';
	let updateStatus   = $state<UpdateStatus>('idle');
	let updateLog      = $state<string[]>([]);
	let updateLogEl    = $state<HTMLDivElement | null>(null);
	let updateSource: EventSource | null = null;

	function startUpdate() {
		if (!canUpdate || updateStatus === 'running') return;
		updateLog = [];
		updateStatus = 'running';

		updateSource?.close();
		updateSource = new EventSource('/api/admin/update/stream');

		updateSource.onmessage = (e) => {
			if (!e.data?.trim()) return;
			updateLog = [...updateLog, e.data];
			if (updateLogEl) requestAnimationFrame(() => {
				if (updateLogEl) updateLogEl.scrollTop = updateLogEl.scrollHeight;
			});
		};

		updateSource.addEventListener('done', (e: MessageEvent) => {
			updateLog = [...updateLog, `✓ ${e.data}`];
			updateStatus = 'done';
			updateSource?.close();
			updateSource = null;
		});

		updateSource.addEventListener('error', (e: MessageEvent) => {
			if (e.data) {
				updateLog = [...updateLog, `✗ ${e.data}`];
				updateStatus = 'error';
				updateSource?.close();
				updateSource = null;
			}
		});

		// onerror fires when the SSE connection drops — expected when backend restarts
		updateSource.onerror = () => {
			if (updateStatus === 'running') {
				updateLog = [...updateLog, '⟳ Connection lost — services are restarting. Reload the page when ready.'];
				updateStatus = 'disconnected';
			}
			updateSource?.close();
			updateSource = null;
		};
	}

	function clearUpdateLog() {
		updateLog = [];
		updateStatus = 'idle';
	}

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let canUpdate = $derived(can(myRole, myPerms, perm(orgId, 'system', 'update')));

	interface InfraDetail {
		cpu_cores: number;
		memory_gb: number;
		region: string;
		nodes: { id: string; name: string; status: string; provider: string }[];
	}
	interface ComputeNodeItem {
		id: string; name: string; provider: string; region: string;
		status: string; cpu_cores: number; ram_mb: number;
	}
	let infra = $state<InfraDetail | null>(null);

	interface OrgBilling {
		tier: string;
		sub_status: string;
		plan?: {
			cpu_cores: number;
			memory_mb: number;
			max_replicas: number;
			node_count: number;
			max_members: number;
			max_projects: number;
		};
	}
	let billing = $state<OrgBilling | null>(null);

	let settings    = $state<PlatformSettings>({});
	let loading     = $state(true);
	let saving      = $state(false);
	let saved       = $state(false);
	let saveError   = $state('');

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true; saved = false; saveError = '';
		try {
			const res = await api.put<PlatformSettings>('/settings', settings);
			if (res.error) saveError = res.error.message;
			else { saved = true; setTimeout(() => (saved = false), 3000); }
		} finally { saving = false; }
	}

	onMount(async () => {
		const res = await api.get<PlatformSettings>('/settings');
		if (res.data) settings = res.data;
		loading = false;

		loadingVersion = true;
		const vRes = await api.get<VersionInfo>('/admin/version');
		if (vRes.data) versionInfo = vRes.data;
		loadingVersion = false;

		if (orgId) {
			const nodesRes = await api.get<ComputeNodeItem[]>(`/orgs/${orgId}/nodes`);
			if (nodesRes.data && Array.isArray(nodesRes.data)) {
				const nodes = nodesRes.data;
				infra = {
					cpu_cores: nodes.reduce((s, n) => s + (n.cpu_cores ?? 0), 0),
					memory_gb: Math.round(nodes.reduce((s, n) => s + (n.ram_mb ?? 0), 0) / 1024),
					region: nodes[0]?.region ?? '—',
					nodes: nodes.map(n => ({ id: n.id, name: n.name, status: n.status, provider: n.provider })),
				};
			} else {
				infra = { cpu_cores: 0, memory_gb: 0, region: '—', nodes: [] };
			}

			const billRes = await api.get<OrgBilling>(`/orgs/${orgId}/billing`);
			if (billRes.data) billing = billRes.data;
		}
	});
</script>

{#if loading}
	<div class="loading">
		<Spinner size={18} />
		<span>Loading settings…</span>
	</div>
{:else}
	<!-- Plan & Infrastructure -->
	<div class="plan-section">
	<Card padding="0">
		<div class="section-header">
			<IconBadge tone="yellow"><Star size={16} /></IconBadge>
			<div>
				<h2 class="section-title">Plan &amp; Architecture</h2>
				<p class="section-desc">Your current subscription plan and allocated compute resources.</p>
			</div>
			{#if billing}
				<span class="plan-tier-badge">
					<Badge tone={billing.tier === 'pro' ? 'blue' : billing.tier === 'max' ? 'yellow' : 'neutral'}>{billing.tier.toUpperCase()}</Badge>
				</span>
			{/if}
		</div>
		<div class="fields">
			{#if infra}
				<div class="infra-chips">
					<div class="infra-chip">
						<span class="infra-chip-label">Region</span>
						<span class="infra-chip-val region-val">{infra.region}</span>
					</div>
					<div class="infra-chip">
						<span class="infra-chip-label">CPU Cores</span>
						<span class="infra-chip-val">{infra.cpu_cores}</span>
					</div>
					<div class="infra-chip">
						<span class="infra-chip-label">Memory</span>
						<span class="infra-chip-val">{infra.memory_gb} <small>GB</small></span>
					</div>
					<div class="infra-chip">
						<span class="infra-chip-label">Nodes</span>
						<span class="infra-chip-val">{infra.nodes.length}</span>
					</div>
					{#if billing?.plan}
						<div class="infra-chip">
							<span class="infra-chip-label">Max Replicas</span>
							<span class="infra-chip-val">{billing.plan.max_replicas}</span>
						</div>
						<div class="infra-chip">
							<span class="infra-chip-label">Max Members</span>
							<span class="infra-chip-val">{billing.plan.max_members}</span>
						</div>
					{/if}
				</div>
				{#if infra.nodes.length > 0}
					<div class="infra-nodes">
						{#each infra.nodes as n}
							<div class="infra-node-row">
								<span class="infra-node-name">{n.name}</span>
								<span class="infra-node-provider">{n.provider}</span>
								<Badge tone={n.status === 'active' ? 'green' : 'red'}>{n.status}</Badge>
							</div>
						{/each}
					</div>
				{/if}
			{:else}
				<div class="plan-loading">
					{#each [0,1,2,3] as _}<div class="plan-sk"><Skeleton variant="text" height="60px" /></div>{/each}
				</div>
			{/if}
		</div>
	</Card>
	</div>

	<form class="settings-form" onsubmit={save}>

		<!-- Main Domain -->
		<Card padding="0">
			<div class="section-header">
				<IconBadge tone="blue"><Globe size={16} /></IconBadge>
				<div>
					<h2 class="section-title">Main Domain</h2>
					<p class="section-desc">Base domain for all deployed services (e.g. <code>example.com</code>). Services get subdomains like <code>my-service.example.com</code>.</p>
				</div>
			</div>
			<div class="fields">
				<FormField label="Base Domain" for="main-domain" hint="Leave blank to use manual domain assignments per service.">
					<TextField id="main-domain" type="text" bind:value={() => settings.main_domain ?? '', (v) => (settings.main_domain = v)} placeholder="example.com" />
				</FormField>
			</div>
		</Card>

		{#if saveError}
			<div role="alert"><InlineAlert tone="error">{saveError}</InlineAlert></div>
		{/if}

		<div class="save-bar">
			<Button type="submit" disabled={saving}>
				{#if saving}<Spinner size={12} tone="current" />Saving…
				{:else if saved}<Check size={14} />Saved
				{:else}<Save size={14} />Save Settings
				{/if}
			</Button>
		</div>
	</form>

	<!-- Platform Update (admin only — hidden from tenant view) -->
	{#if false && canUpdate}
	<div class="update-section">
	<Card padding="0">
		<div class="section-header">
			<IconBadge tone="blue"><PackageOpen size={16} /></IconBadge>
			<div>
				<h2 class="section-title">Platform Update</h2>
				<p class="section-desc">
					Pull the latest Docker images from the registry and restart all Shipyard services.
					The connection will drop briefly while the backend restarts — that's expected.
				</p>
			</div>
		</div>

		<!-- Version info bar -->
		<div class="version-info-bar">
			{#if loadingVersion}
				<span class="version-loading">Checking version…</span>
			{:else if versionInfo}
				{@const v = versionInfo as VersionInfo}
				<div class="version-chip">
					<span class="version-label">Running</span>
					<code class="version-sha">{v.git_sha}</code>
					{#if v.build_date && v.build_date !== 'unknown'}
						<span class="version-date">{formatBuildDate(v.build_date)}</span>
					{/if}
				</div>
				{#if v.update_available && v.remote_sha}
					{@const remoteSha = v.remote_sha}
					<Badge tone="yellow">Update available → <code>{remoteSha}</code></Badge>
				{:else}
					<Badge tone="green">Up to date</Badge>
				{/if}
				<span class="version-check-btn">
					<Button variant="secondary" size="sm" disabled={checkingUpdate} onclick={checkForUpdates}>
						{#if checkingUpdate}<Spinner size={11} tone="current" />Checking…{:else}<RefreshCw size={11} />Check{/if}
					</Button>
				</span>
			{/if}
		</div>

		<div class="update-body">
			<div class="update-actions">
				<Button disabled={updateStatus === 'running'} onclick={startUpdate}>
					{#if updateStatus === 'running'}
						<Spinner size={14} tone="current" />Running update…
					{:else}
						<RefreshCw size={14} />Pull &amp; Restart
					{/if}
				</Button>

				{#if updateStatus === 'done'}
					<Badge tone="green"><Check size={11} />Done</Badge>
				{:else if updateStatus === 'error'}
					<Badge tone="red"><AlertCircle size={11} />Failed</Badge>
				{:else if updateStatus === 'disconnected'}
					<Badge tone="yellow"><Zap size={11} />Restarting…</Badge>
				{/if}

				{#if updateLog.length > 0 && updateStatus !== 'running'}
					<span class="clear-log-btn">
						<Button variant="ghost" size="sm" onclick={clearUpdateLog}>Clear</Button>
					</span>
				{/if}
			</div>

			{#if updateLog.length > 0}
				<div class="update-log" bind:this={updateLogEl}>
					<div class="update-log-header">
						<Terminal size={11} />
						<span>Update output</span>
					</div>
					{#each updateLog as line, i (i)}
						<div class="update-log-line" class:log-done={line.startsWith('✓')} class:log-error={line.startsWith('✗')} class:log-restart={line.startsWith('⟳')}>
							{line}
						</div>
					{/each}
					{#if updateStatus === 'running'}
						<div class="update-log-cursor">▊</div>
					{/if}
				</div>
			{/if}

			{#if updateStatus === 'disconnected'}
				<InlineAlert tone="warning">
					<span class="update-reconnect-hint">
						Services are restarting. Reload this page in a few seconds to confirm the update completed.
						<Button variant="secondary" size="sm" onclick={() => window.location.reload()}>
							<RefreshCw size={12} />Reload now
						</Button>
					</span>
				</InlineAlert>
			{/if}
		</div>
	</Card>
	</div>
	{/if}
{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }

	.settings-form { display: flex; flex-direction: column; gap: 20px; }

	.section-header {
		display: flex;
		align-items: center;
		gap: 14px;
		padding: 18px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
	}
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }

	code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 12px; }

	.save-bar { display: flex; justify-content: flex-end; padding: 4px 0 8px; }

	/* ── Platform Update ── */
	.update-section { margin-top: 8px; }

	.version-info-bar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-base);
		flex-wrap: wrap;
		min-height: 42px;
	}
	.version-chip { display: flex; align-items: center; gap: 6px; }
	.version-label { font-size: 10px; font-weight: 600; color: var(--text-dim); }
	.version-sha {
		font-family: var(--font-mono);
		font-size: 12px;
		background: var(--bg-elevated);
		color: var(--text-primary);
		padding: 2px 7px;
		border-radius: 4px;
		border: 1px solid var(--border);
	}
	.version-date { font-size: 11px; color: var(--text-dim); }
	.version-loading { font-size: 12px; color: var(--text-dim); }
	.version-check-btn { margin-left: auto; }

	.update-body {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 18px 20px;
	}

	.update-actions {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}
	.clear-log-btn { margin-left: auto; }

	.update-log {
		background: var(--terminal-bg);
		border: 1px solid var(--terminal-border);
		border-radius: var(--radius-md);
		overflow-y: auto;
		max-height: 340px;
		font-family: var(--font-mono);
		font-size: 12px;
	}

	.update-log-header {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 7px 12px;
		border-bottom: 1px solid var(--terminal-border);
		color: var(--terminal-dim);
		font-size: 11px;
		font-family: var(--font-sans);
	}

	.update-log-line {
		padding: 2px 14px;
		color: var(--terminal-fg);
		white-space: pre-wrap;
		word-break: break-all;
		line-height: 1.6;
	}
	.update-log-line.log-done    { color: var(--terminal-green); }
	.update-log-line.log-error   { color: var(--terminal-red); }
	.update-log-line.log-restart { color: var(--terminal-yellow); }

	.update-log-cursor {
		padding: 2px 14px 6px;
		color: var(--terminal-fg);
		animation: blink 1s step-end infinite;
	}
	@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }

	.update-reconnect-hint {
		display: flex;
		align-items: center;
		gap: 12px;
		flex-wrap: wrap;
	}

	.plan-section { margin-bottom: 20px; }
	.plan-tier-badge { margin-left: auto; }
	.plan-loading { display: flex; gap: 10px; }
	.plan-sk { flex: 1; min-width: 90px; }

	.infra-chips { display:flex; flex-wrap:wrap; gap:10px; margin-bottom:12px; }
	.infra-chip { display:flex; flex-direction:column; gap:2px; background:var(--bg-elevated); border:1px solid var(--border); border-radius:var(--radius-sm); padding:10px 14px; min-width:100px; }
	.infra-chip-label { font-size:10px; font-weight:700; color:var(--text-dim); }
	.infra-chip-val { font-size:18px; font-weight:800; color:var(--text-primary); }
	.infra-chip-val small { font-size:13px; font-weight:600; }
	.region-val { font-size:14px; }
	.infra-nodes { display:flex; flex-direction:column; border:1px solid var(--border); border-radius:var(--radius-sm); overflow:hidden; }
	.infra-node-row { display:flex; align-items:center; gap:12px; padding:9px 14px; border-bottom:1px solid var(--border); font-size:12.5px; }
	.infra-node-row:last-child { border-bottom:none; }
	.infra-node-name { font-weight:500; color:var(--text-primary); flex:1; }
	.infra-node-provider { font-size:11px; color:var(--text-dim); }

	/* ── Responsive ── */
	@media (max-width: 639px) {
		.settings-form { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.fields { padding: 14px 16px; }
		.save-bar { padding: 0 0 4px; }
	}
</style>
