<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { page } from '$app/state';
	import {
		Box, Layers, HardDrive, Network, RefreshCw,
		ChevronDown, ChevronRight, Trash2, Image
	} from '@lucide/svelte';
	import Tabs, { type TabItem } from '$lib/components/ui/Tabs.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import StatusDot from '$lib/components/ui/StatusDot.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import SearchInput from '$lib/components/ui/SearchInput.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import type { DotStatus } from '$lib/utils/status';

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let canDockerRead  = $derived(
		can(myRole, myPerms, perm(orgId, 'docker', 'read')) ||
		can(myRole, myPerms, perm(orgId, 'settings', 'read'))
	);
	let canDockerWrite = $derived(can(myRole, myPerms, perm(orgId, 'docker', 'write')));
	let canDockerAny   = $derived(canDockerRead || canDockerWrite);

	type Tab = 'containers' | 'services' | 'volumes' | 'networks' | 'images';
	let activeTab = $state<Tab>('containers');

	interface ContainerSummary {
		id: string; names: string[]; image: string;
		status: string; state: string; created: number;
		ports: string[]; labels: Record<string, string>;
	}
	interface ServiceSummary {
		id: string; name: string; image: string;
		replicas_running: number; replicas_desired: number;
		mode: string; ports: string[];
		labels: Record<string, string>;
		created_at: string | null; updated_at: string | null;
	}
	interface VolumeSummary {
		name: string; driver: string; mountpoint: string;
		scope: string; labels: Record<string, string>; created_at: string | null;
	}
	interface NetworkSummary {
		id: string; name: string; driver: string; scope: string;
		internal: boolean; attachable: boolean; ipam_subnet: string | null;
		labels: Record<string, string>; containers: number;
	}

	interface ImageSummary {
		id: string; tags: string[]; size: number; created: number;
	}

	let containers = $state<ContainerSummary[]>([]);
	let services   = $state<ServiceSummary[]>([]);
	let volumes    = $state<VolumeSummary[]>([]);
	let networks   = $state<NetworkSummary[]>([]);
	let images     = $state<ImageSummary[]>([]);

	let loadingC = $state(false), loadingS = $state(false);
	let loadingV = $state(false), loadingN = $state(false);
	let loadingI = $state(false);

	let search = $state('');
	let expanded = $state<string | null>(null);

	function dockerUrl(path: string) { return `${path}?org_id=${orgId}`; }

	async function loadContainers() {
		loadingC = true;
		const r = await api.get<ContainerSummary[]>(dockerUrl('/admin/docker/containers'));
		if (r.data) containers = r.data;
		loadingC = false;
	}
	async function loadServices() {
		loadingS = true;
		const r = await api.get<ServiceSummary[]>(dockerUrl('/admin/docker/services'));
		if (r.data) services = r.data;
		loadingS = false;
	}
	async function loadVolumes() {
		loadingV = true;
		const r = await api.get<VolumeSummary[]>(dockerUrl('/admin/docker/volumes'));
		if (r.data) volumes = r.data;
		loadingV = false;
	}
	async function loadNetworks() {
		loadingN = true;
		const r = await api.get<NetworkSummary[]>(dockerUrl('/admin/docker/networks'));
		if (r.data) networks = r.data;
		loadingN = false;
	}
	async function loadImages() {
		loadingI = true;
		const r = await api.get<ImageSummary[]>(dockerUrl('/admin/docker/images'));
		if (r.data) images = r.data;
		loadingI = false;
	}

	async function switchTab(t: Tab) {
		activeTab = t;
		search = '';
		expanded = null;
		if (t === 'containers' && containers.length === 0) await loadContainers();
		if (t === 'services'   && services.length === 0)   await loadServices();
		if (t === 'volumes'    && volumes.length === 0)     await loadVolumes();
		if (t === 'networks'   && networks.length === 0)    await loadNetworks();
		if (t === 'images'     && images.length === 0)      await loadImages();
	}

	async function refresh() {
		search = ''; expanded = null;
		if (activeTab === 'containers') { containers = []; await loadContainers(); }
		if (activeTab === 'services')   { services = [];   await loadServices(); }
		if (activeTab === 'volumes')    { volumes = [];    await loadVolumes(); }
		if (activeTab === 'networks')   { networks = [];   await loadNetworks(); }
		if (activeTab === 'images')     { images = [];     await loadImages(); }
	}

	let pruning       = $state(false);
	let pruneConfirm  = $state(false);
	let pruneResult   = $state<string | null>(null);

	async function pruneContainers() {
		if (!canDockerWrite) return;
		if (!pruneConfirm) { pruneConfirm = true; return; }
		pruning = true;
		pruneConfirm = false;
		pruneResult = null;
		const r = await api.post<{ removed: number }>(dockerUrl('/admin/docker/containers/prune'), {});
		if (r.data) {
			pruneResult = `Removed ${r.data.removed} stopped container${r.data.removed !== 1 ? 's' : ''}.`;
			containers = [];
			await loadContainers();
		} else {
			pruneResult = r.error?.message ?? 'Prune failed.';
		}
		pruning = false;
		setTimeout(() => { pruneResult = null; }, 4000);
	}

	let pruningImages      = $state(false);
	let pruneImagesConfirm = $state(false);
	let pruneImagesResult  = $state<string | null>(null);

	async function pruneImages() {
		if (!canDockerWrite) return;
		if (!pruneImagesConfirm) { pruneImagesConfirm = true; return; }
		pruningImages = true;
		pruneImagesConfirm = false;
		pruneImagesResult = null;
		const r = await api.post<{ removed: number }>(dockerUrl('/admin/docker/images/prune'), {});
		if (r.data) {
			pruneImagesResult = `Removed ${r.data.removed} unused image${r.data.removed !== 1 ? 's' : ''}.`;
			images = [];
			await loadImages();
		} else {
			pruneImagesResult = r.error?.message ?? 'Prune failed.';
		}
		pruningImages = false;
		setTimeout(() => { pruneImagesResult = null; }, 4000);
	}

	let pruningVolumes      = $state(false);
	let pruneVolumesConfirm = $state(false);
	let pruneVolumesResult  = $state<string | null>(null);

	async function pruneVolumes() {
		if (!canDockerWrite) return;
		if (!pruneVolumesConfirm) { pruneVolumesConfirm = true; return; }
		pruningVolumes = true;
		pruneVolumesConfirm = false;
		pruneVolumesResult = null;
		const r = await api.post<{ removed: number }>(dockerUrl('/admin/docker/volumes/prune'), {});
		if (r.data) {
			pruneVolumesResult = `Removed ${r.data.removed} unused volume${r.data.removed !== 1 ? 's' : ''}.`;
			volumes = [];
			await loadVolumes();
		} else {
			pruneVolumesResult = r.error?.message ?? 'Prune failed.';
		}
		pruningVolumes = false;
		setTimeout(() => { pruneVolumesResult = null; }, 4000);
	}

	function toggle(id: string) { expanded = expanded === id ? null : id; }

	// Page-local mapping that keeps each container state's old dot colour family
	// (toDotStatus has no created/paused/dead/restarting).
	const stateDot: Record<string, DotStatus> = {
		running: 'running', exited: 'stopped', created: 'deploying',
		paused: 'warning', dead: 'failed', restarting: 'pending',
	};
	function dotFor(state: string): DotStatus { return stateDot[state] ?? 'stopped'; }

	function ago(unixSecs: number): string {
		const diff = Math.floor(Date.now() / 1000) - unixSecs;
		if (diff < 60)   return `${diff}s ago`;
		if (diff < 3600) return `${Math.floor(diff / 60)}m ago`;
		if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
		return `${Math.floor(diff / 86400)}d ago`;
	}

	function fmtBytes(bytes: number): string {
		if (bytes < 1024)        return `${bytes} B`;
		if (bytes < 1024 ** 2)   return `${(bytes / 1024).toFixed(1)} KB`;
		if (bytes < 1024 ** 3)   return `${(bytes / 1024 ** 2).toFixed(1)} MB`;
		return `${(bytes / 1024 ** 3).toFixed(2)} GB`;
	}

	function shortImg(img: string): string {
		const parts = img.split('@sha256:');
		if (parts.length > 1) return parts[0] + '@' + parts[1].slice(0, 12);
		return img;
	}

	let q = $derived(search.toLowerCase());

	let filteredContainers = $derived(containers.filter(c =>
		!q || c.names.some(n => n.includes(q)) || c.image.includes(q) || c.state.includes(q)
	));
	let filteredServices = $derived(services.filter(s =>
		!q || s.name.includes(q) || s.image.includes(q)
	));
	let filteredVolumes = $derived(volumes.filter(v =>
		!q || v.name.includes(q) || v.driver.includes(q)
	));
	let filteredNetworks = $derived(networks.filter(n =>
		!q || n.name.includes(q) || n.driver.includes(q)
	));
	let filteredImages = $derived(images.filter(img =>
		!q || img.tags.some(t => t.includes(q)) || img.id.includes(q)
	));

	let isLoading = $derived(
		(activeTab === 'containers' && loadingC) ||
		(activeTab === 'services'   && loadingS) ||
		(activeTab === 'volumes'    && loadingV) ||
		(activeTab === 'networks'   && loadingN) ||
		(activeTab === 'images'     && loadingI)
	);

	const tabItems = $derived<TabItem[]>([
		{ id: 'containers', label: 'Containers', icon: Box,       badge: containers.length ? String(containers.length) : undefined },
		{ id: 'services',   label: 'Services',   icon: Layers,    badge: services.length   ? String(services.length)   : undefined },
		{ id: 'volumes',    label: 'Volumes',    icon: HardDrive, badge: volumes.length    ? String(volumes.length)    : undefined },
		{ id: 'networks',   label: 'Networks',   icon: Network,   badge: networks.length   ? String(networks.length)   : undefined },
		{ id: 'images',     label: 'Images',     icon: Image,     badge: images.length     ? String(images.length)     : undefined },
	]);

	const expCol = { key: 'exp', label: '', width: '28px' };

	onMount(() => { if (canDockerAny) loadContainers(); });
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canDockerAny}
	message="You need the 'View Docker' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#snippet labelChips(labels: Record<string, string>)}
	<div class="label-chips">
		{#each Object.entries(labels) as [k, v]}
			<span class="lchip"><b>{k}</b>={v}</span>
		{/each}
	</div>
{/snippet}

{#snippet pruneBar(label: string, busy: boolean, onclick: () => void)}
	<Button variant="danger-outline" size="sm" onclick={onclick} disabled={busy}>
		<Trash2 size={14} />
		{busy ? 'Pruning…' : label}
	</Button>
{/snippet}

{#if canDockerAny}
<div class="docker-page">

	<div class="toolbar">
		<Tabs
			tabs={tabItems}
			value={activeTab}
			onChange={(id) => switchTab(id as Tab)}
			ariaLabel="Docker resources"
		/>
		<div class="toolbar-right">
			{#if activeTab === 'containers'}
				{@render pruneBar('Prune stopped', pruning, pruneContainers)}
			{/if}
			{#if activeTab === 'volumes'}
				{@render pruneBar('Prune unused', pruningVolumes, pruneVolumes)}
			{/if}
			{#if activeTab === 'images'}
				{@render pruneBar('Prune unused', pruningImages, pruneImages)}
			{/if}
			<Button variant="secondary" size="sm" onclick={refresh} disabled={isLoading}>
				{#if isLoading}<Spinner size={14} tone="current" />{:else}<RefreshCw size={14} />{/if} Refresh
			</Button>
		</div>
	</div>

	{#each [pruneResult, pruneImagesResult, pruneVolumesResult] as msg}
		{#if msg}
			<div role="status"><InlineAlert tone={msg.startsWith('Removed') ? 'success' : 'error'}>{msg}</InlineAlert></div>
		{/if}
	{/each}

	<SearchInput bind:value={search} placeholder="Filter…" />

	<!-- ── Containers ── -->
	{#if activeTab === 'containers'}
		{#if loadingC}
			<div class="empty"><Spinner size={18} />Loading containers…</div>
		{:else if filteredContainers.length === 0}
			<EmptyState message="No containers">{#snippet icon()}<Box size={28} />{/snippet}</EmptyState>
		{:else}
			<div class="desktop-table">
				<DataTable
					items={filteredContainers}
					rowKey={(c: ContainerSummary) => c.id}
					searchable={false}
					columns={[
						expCol,
						{ key: 'name', label: 'Name' }, { key: 'image', label: 'Image' }, { key: 'state', label: 'State' },
						{ key: 'status', label: 'Status' }, { key: 'ports', label: 'Ports' }, { key: 'created', label: 'Created' }
					]}
				>
					{#snippet row(c: ContainerSummary)}
						{@const isExp = expanded === c.id}
						{@const name = c.names[0] ?? c.id.slice(0,12)}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<tr class="row" onclick={() => toggle(c.id)}>
							<td class="exp-cell">{#if isExp}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}</td>
							<td class="mono bold">{name}</td>
							<td class="mono dim">{shortImg(c.image)}</td>
							<td class="state-cell"><StatusDot status={dotFor(c.state)} /> {c.state}</td>
							<td class="dim">{c.status}</td>
							<td class="mono dim">{c.ports.slice(0,2).join(', ')}{c.ports.length>2?` +${c.ports.length-2}`:''}</td>
							<td class="dim ts">{ago(c.created)}</td>
						</tr>
						{#if isExp}
							<tr class="detail-row">
								<td colspan="7">
									<div class="detail-box">
										<div class="detail-field"><span class="dk">ID</span><span class="dv mono">{c.id}</span></div>
										<div class="detail-field"><span class="dk">Names</span><span class="dv">{c.names.join(', ')}</span></div>
										<div class="detail-field"><span class="dk">Image</span><span class="dv mono">{c.image}</span></div>
										<div class="detail-field"><span class="dk">Ports</span><span class="dv mono">{c.ports.join(', ') || '—'}</span></div>
										{#if Object.keys(c.labels).length}
											<div class="detail-field full"><span class="dk">Labels</span>{@render labelChips(c.labels)}</div>
										{/if}
									</div>
								</td>
							</tr>
						{/if}
					{/snippet}
				</DataTable>
			</div>

			<div class="mobile-cards">
				{#each filteredContainers as c (c.id)}
					{@const isExp = expanded === c.id}
					{@const name = c.names[0] ?? c.id.slice(0,12)}
					<div class="m-card">
						<button type="button" class="m-card-header" onclick={() => toggle(c.id)}>
							<div class="m-card-title-row">
								<StatusDot status={dotFor(c.state)} />
								<span class="m-card-title mono">{name}</span>
							</div>
							<span class="m-chevron">{#if isExp}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}</span>
						</button>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Image</span><span class="mono dim">{shortImg(c.image)}</span></div>
							<div class="m-row"><span class="m-label">Status</span><span class="dim">{c.status}</span></div>
							{#if c.ports.length}
								<div class="m-row"><span class="m-label">Ports</span><span class="mono dim">{c.ports.slice(0,3).join(', ')}{c.ports.length>3?` +${c.ports.length-3}`:''}</span></div>
							{/if}
							<div class="m-row"><span class="m-label">Created</span><span class="dim">{ago(c.created)}</span></div>
						</div>
						{#if isExp}
							<div class="m-detail">
								<div class="detail-field"><span class="dk">ID</span><span class="dv mono">{c.id}</span></div>
								<div class="detail-field"><span class="dk">Names</span><span class="dv">{c.names.join(', ')}</span></div>
								<div class="detail-field"><span class="dk">Image</span><span class="dv mono">{c.image}</span></div>
								<div class="detail-field"><span class="dk">Ports</span><span class="dv mono">{c.ports.join(', ') || '—'}</span></div>
								{#if Object.keys(c.labels).length}
									<div class="detail-field"><span class="dk">Labels</span>{@render labelChips(c.labels)}</div>
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	{/if}

	<!-- ── Services ── -->
	{#if activeTab === 'services'}
		{#if loadingS}
			<div class="empty"><Spinner size={18} />Loading services…</div>
		{:else if filteredServices.length === 0}
			<EmptyState message="No swarm services">{#snippet icon()}<Layers size={28} />{/snippet}</EmptyState>
		{:else}
			<div class="desktop-table">
				<DataTable
					items={filteredServices}
					rowKey={(s: ServiceSummary) => s.id}
					searchable={false}
					columns={[
						expCol,
						{ key: 'name', label: 'Name' }, { key: 'image', label: 'Image' }, { key: 'mode', label: 'Mode' },
						{ key: 'replicas', label: 'Replicas' }, { key: 'ports', label: 'Ports' }
					]}
				>
					{#snippet row(s: ServiceSummary)}
						{@const isExp = expanded === s.id}
						{@const healthy = s.replicas_running >= s.replicas_desired && s.replicas_desired > 0}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<tr class="row" onclick={() => toggle(s.id)}>
							<td class="exp-cell">{#if isExp}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}</td>
							<td class="mono bold">{s.name}</td>
							<td class="mono dim">{shortImg(s.image)}</td>
							<td><Badge tone="neutral">{s.mode}</Badge></td>
							<td><Badge tone={healthy ? 'green' : 'red'}>{s.replicas_running}/{s.replicas_desired}</Badge></td>
							<td class="mono dim">{s.ports.join(', ') || '—'}</td>
						</tr>
						{#if isExp}
							<tr class="detail-row">
								<td colspan="6">
									<div class="detail-box">
										<div class="detail-field"><span class="dk">ID</span><span class="dv mono">{s.id}</span></div>
										<div class="detail-field"><span class="dk">Created</span><span class="dv">{s.created_at ?? '—'}</span></div>
										<div class="detail-field"><span class="dk">Updated</span><span class="dv">{s.updated_at ?? '—'}</span></div>
										{#if Object.keys(s.labels).length}
											<div class="detail-field full"><span class="dk">Labels</span>{@render labelChips(s.labels)}</div>
										{/if}
									</div>
								</td>
							</tr>
						{/if}
					{/snippet}
				</DataTable>
			</div>

			<div class="mobile-cards">
				{#each filteredServices as s (s.id)}
					{@const isExp = expanded === s.id}
					{@const healthy = s.replicas_running >= s.replicas_desired && s.replicas_desired > 0}
					<div class="m-card">
						<button type="button" class="m-card-header" onclick={() => toggle(s.id)}>
							<div class="m-card-title-row">
								<span class="m-card-title mono">{s.name}</span>
								<Badge tone={healthy ? 'green' : 'red'}>{s.replicas_running}/{s.replicas_desired}</Badge>
							</div>
							<span class="m-chevron">{#if isExp}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}</span>
						</button>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Image</span><span class="mono dim">{shortImg(s.image)}</span></div>
							<div class="m-row"><span class="m-label">Mode</span><Badge tone="neutral">{s.mode}</Badge></div>
							{#if s.ports.length}
								<div class="m-row"><span class="m-label">Ports</span><span class="mono dim">{s.ports.join(', ')}</span></div>
							{/if}
						</div>
						{#if isExp}
							<div class="m-detail">
								<div class="detail-field"><span class="dk">ID</span><span class="dv mono">{s.id}</span></div>
								<div class="detail-field"><span class="dk">Created</span><span class="dv">{s.created_at ?? '—'}</span></div>
								<div class="detail-field"><span class="dk">Updated</span><span class="dv">{s.updated_at ?? '—'}</span></div>
								{#if Object.keys(s.labels).length}
									<div class="detail-field"><span class="dk">Labels</span>{@render labelChips(s.labels)}</div>
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	{/if}

	<!-- ── Volumes ── -->
	{#if activeTab === 'volumes'}
		{#if loadingV}
			<div class="empty"><Spinner size={18} />Loading volumes…</div>
		{:else if filteredVolumes.length === 0}
			<EmptyState message="No volumes">{#snippet icon()}<HardDrive size={28} />{/snippet}</EmptyState>
		{:else}
			<div class="desktop-table">
				<DataTable
					items={filteredVolumes}
					rowKey={(v: VolumeSummary) => v.name}
					searchable={false}
					columns={[
						expCol,
						{ key: 'name', label: 'Name' }, { key: 'driver', label: 'Driver' }, { key: 'scope', label: 'Scope' },
						{ key: 'mountpoint', label: 'Mountpoint' }
					]}
				>
					{#snippet row(v: VolumeSummary)}
						{@const isExp = expanded === v.name}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<tr class="row" onclick={() => toggle(v.name)}>
							<td class="exp-cell">{#if isExp}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}</td>
							<td class="mono bold">{v.name}</td>
							<td><Badge tone="neutral">{v.driver}</Badge></td>
							<td class="dim">{v.scope}</td>
							<td class="mono dim truncate">{v.mountpoint}</td>
						</tr>
						{#if isExp}
							<tr class="detail-row">
								<td colspan="5">
									<div class="detail-box">
										<div class="detail-field"><span class="dk">Mountpoint</span><span class="dv mono">{v.mountpoint}</span></div>
										{#if v.created_at}<div class="detail-field"><span class="dk">Created</span><span class="dv">{v.created_at}</span></div>{/if}
										{#if Object.keys(v.labels).length}
											<div class="detail-field full"><span class="dk">Labels</span>{@render labelChips(v.labels)}</div>
										{/if}
									</div>
								</td>
							</tr>
						{/if}
					{/snippet}
				</DataTable>
			</div>

			<div class="mobile-cards">
				{#each filteredVolumes as v (v.name)}
					{@const isExp = expanded === v.name}
					<div class="m-card">
						<button type="button" class="m-card-header" onclick={() => toggle(v.name)}>
							<span class="m-card-title mono">{v.name}</span>
							<span class="m-chevron">{#if isExp}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}</span>
						</button>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Driver</span><Badge tone="neutral">{v.driver}</Badge></div>
							<div class="m-row"><span class="m-label">Scope</span><span class="dim">{v.scope}</span></div>
							<div class="m-row"><span class="m-label">Mountpoint</span><span class="mono dim truncate">{v.mountpoint}</span></div>
						</div>
						{#if isExp}
							<div class="m-detail">
								<div class="detail-field"><span class="dk">Mountpoint</span><span class="dv mono">{v.mountpoint}</span></div>
								{#if v.created_at}<div class="detail-field"><span class="dk">Created</span><span class="dv">{v.created_at}</span></div>{/if}
								{#if Object.keys(v.labels).length}
									<div class="detail-field"><span class="dk">Labels</span>{@render labelChips(v.labels)}</div>
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	{/if}

	<!-- ── Networks ── -->
	{#if activeTab === 'networks'}
		{#if loadingN}
			<div class="empty"><Spinner size={18} />Loading networks…</div>
		{:else if filteredNetworks.length === 0}
			<EmptyState message="No networks">{#snippet icon()}<Network size={28} />{/snippet}</EmptyState>
		{:else}
			<div class="desktop-table">
				<DataTable
					items={filteredNetworks}
					rowKey={(n: NetworkSummary) => n.id}
					searchable={false}
					columns={[
						expCol,
						{ key: 'name', label: 'Name' }, { key: 'driver', label: 'Driver' }, { key: 'scope', label: 'Scope' },
						{ key: 'subnet', label: 'Subnet' }, { key: 'containers', label: 'Containers' }, { key: 'flags', label: 'Flags' }
					]}
				>
					{#snippet row(n: NetworkSummary)}
						{@const isExp = expanded === n.id}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<tr class="row" onclick={() => toggle(n.id)}>
							<td class="exp-cell">{#if isExp}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}</td>
							<td class="mono bold">{n.name}</td>
							<td><Badge tone="neutral">{n.driver}</Badge></td>
							<td class="dim">{n.scope}</td>
							<td class="mono dim">{n.ipam_subnet ?? '—'}</td>
							<td class="dim">{n.containers}</td>
							<td>
								<span class="flags">
									{#if n.internal}<Badge tone="blue">internal</Badge>{/if}
									{#if n.attachable}<Badge tone="blue">attachable</Badge>{/if}
								</span>
							</td>
						</tr>
						{#if isExp}
							<tr class="detail-row">
								<td colspan="7">
									<div class="detail-box">
										<div class="detail-field"><span class="dk">ID</span><span class="dv mono">{n.id}</span></div>
										<div class="detail-field"><span class="dk">Subnet</span><span class="dv mono">{n.ipam_subnet ?? '—'}</span></div>
										{#if Object.keys(n.labels).length}
											<div class="detail-field full"><span class="dk">Labels</span>{@render labelChips(n.labels)}</div>
										{/if}
									</div>
								</td>
							</tr>
						{/if}
					{/snippet}
				</DataTable>
			</div>

			<div class="mobile-cards">
				{#each filteredNetworks as n (n.id)}
					{@const isExp = expanded === n.id}
					<div class="m-card">
						<button type="button" class="m-card-header" onclick={() => toggle(n.id)}>
							<div class="m-card-title-row">
								<span class="m-card-title mono">{n.name}</span>
								<div class="m-flags">
									{#if n.internal}<Badge tone="blue">internal</Badge>{/if}
									{#if n.attachable}<Badge tone="blue">attachable</Badge>{/if}
								</div>
							</div>
							<span class="m-chevron">{#if isExp}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}</span>
						</button>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Driver</span><Badge tone="neutral">{n.driver}</Badge></div>
							<div class="m-row"><span class="m-label">Scope</span><span class="dim">{n.scope}</span></div>
							<div class="m-row"><span class="m-label">Subnet</span><span class="mono dim">{n.ipam_subnet ?? '—'}</span></div>
							<div class="m-row"><span class="m-label">Containers</span><span class="dim">{n.containers}</span></div>
						</div>
						{#if isExp}
							<div class="m-detail">
								<div class="detail-field"><span class="dk">ID</span><span class="dv mono">{n.id}</span></div>
								<div class="detail-field"><span class="dk">Subnet</span><span class="dv mono">{n.ipam_subnet ?? '—'}</span></div>
								{#if Object.keys(n.labels).length}
									<div class="detail-field"><span class="dk">Labels</span>{@render labelChips(n.labels)}</div>
								{/if}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	{/if}

	<!-- ── Images ── -->
	{#if activeTab === 'images'}
		{#if loadingI}
			<div class="empty"><Spinner size={18} />Loading images…</div>
		{:else if filteredImages.length === 0}
			<EmptyState message="No images">{#snippet icon()}<Image size={28} />{/snippet}</EmptyState>
		{:else}
			<div class="desktop-table">
				<DataTable
					items={filteredImages}
					rowKey={(img: ImageSummary) => img.id}
					searchable={false}
					columns={[
						{ key: 'tags', label: 'Tags' }, { key: 'id', label: 'ID' },
						{ key: 'size', label: 'Size' }, { key: 'created', label: 'Created' }
					]}
				>
					{#snippet row(img: ImageSummary)}
						<tr>
							<td>
								{#if img.tags.length}
									<div class="tag-list">
										{#each img.tags as t}
											<span class="img-tag">{t}</span>
										{/each}
									</div>
								{:else}
									<span class="dim">&#x3c;none&#x3e;</span>
								{/if}
							</td>
							<td class="mono dim">{img.id.replace('sha256:', '').slice(0, 12)}</td>
							<td class="dim">{fmtBytes(img.size)}</td>
							<td class="dim ts">{ago(img.created)}</td>
						</tr>
					{/snippet}
				</DataTable>
			</div>

			<div class="mobile-cards">
				{#each filteredImages as img (img.id)}
					<div class="m-card">
						<div class="m-card-header static">
							<div class="m-card-title-row tags-col">
								{#if img.tags.length}
									<div class="tag-list">
										{#each img.tags as t}
											<span class="img-tag">{t}</span>
										{/each}
									</div>
								{:else}
									<span class="dim none-label">&lt;none&gt;</span>
								{/if}
							</div>
						</div>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">ID</span><span class="mono dim">{img.id.replace('sha256:', '').slice(0, 12)}</span></div>
							<div class="m-row"><span class="m-label">Size</span><span class="dim">{fmtBytes(img.size)}</span></div>
							<div class="m-row"><span class="m-label">Created</span><span class="dim">{ago(img.created)}</span></div>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	{/if}

</div>

<ConfirmDialog
	bind:open={() => pruneConfirm, (v) => { pruneConfirm = v; }}
	title="Prune stopped containers"
	message="Remove all stopped containers?"
	confirmLabel="Confirm"
	onConfirm={pruneContainers}
/>
<ConfirmDialog
	bind:open={() => pruneVolumesConfirm, (v) => { pruneVolumesConfirm = v; }}
	title="Prune unused volumes"
	message="Remove all unused volumes?"
	confirmLabel="Confirm"
	onConfirm={pruneVolumes}
/>
<ConfirmDialog
	bind:open={() => pruneImagesConfirm, (v) => { pruneImagesConfirm = v; }}
	title="Prune unused images"
	message="Remove all unused images?"
	confirmLabel="Confirm"
	onConfirm={pruneImages}
/>
{/if}

<style>
	.docker-page { display: flex; flex-direction: column; gap: 14px; }

	.toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
	.toolbar-right { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }

	.empty {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; padding: 60px; color: var(--text-muted); font-size: 13px;
	}

	.row { cursor: pointer; }
	.exp-cell { color: var(--text-muted); width: 28px; }
	.mono { font-family: var(--font-mono); font-size: 12px; }
	.bold { font-weight: 600; color: var(--text-primary); }
	.dim  { color: var(--text-secondary); }
	.ts   { white-space: nowrap; }
	.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.state-cell { white-space: nowrap; }
	.state-cell :global(.ui-dot) { margin-right: 6px; vertical-align: middle; }
	.flags { display: inline-flex; gap: 4px; flex-wrap: wrap; }

	.detail-row td { padding: 0; background: var(--bg-base); }
	.detail-box {
		display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 8px; padding: 12px 14px;
	}
	.detail-field { display: flex; flex-direction: column; gap: 2px; }
	.detail-field.full { grid-column: 1 / -1; }
	.dk { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
	.dv { font-size: 12px; color: var(--text-primary); word-break: break-all; }

	.label-chips { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px; }
	.lchip {
		display: inline-block; padding: 2px 7px;
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); font-size: 11px; color: var(--text-secondary);
		max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	.tag-list { display: flex; flex-wrap: wrap; gap: 4px; }
	.img-tag {
		display: inline-block; padding: 2px 7px;
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); font-size: 11px; font-family: var(--font-mono);
		color: var(--text-secondary); max-width: 280px;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	/* ── Mobile cards ── */
	.mobile-cards { display: none; flex-direction: column; gap: 8px; }

	.m-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}
	.m-card-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 12px 14px; gap: 10px;
		width: 100%; background: none; border: none; cursor: pointer; text-align: left;
		font-family: var(--font-sans); color: var(--text-primary);
	}
	.m-card-header.static { cursor: default; }
	.m-card-title-row {
		display: flex; align-items: center; gap: 8px; min-width: 0; flex: 1;
	}
	.m-card-title-row.tags-col { flex-direction: column; align-items: flex-start; gap: 4px; }
	.none-label { font-size: 12px; }
	.m-card-title {
		font-size: 13px; font-weight: 600; color: var(--text-primary);
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0;
	}
	.m-chevron { flex-shrink: 0; color: var(--text-muted); }
	.m-flags { display: flex; gap: 4px; flex-shrink: 0; }
	.m-rows { border-top: 1px solid var(--border); }
	.m-row {
		display: flex; align-items: center; justify-content: space-between;
		padding: 8px 14px; font-size: 12px; color: var(--text-primary);
		border-bottom: 1px solid var(--border); gap: 12px;
	}
	.m-row:last-child { border-bottom: none; }
	.m-row .mono { max-width: 60%; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.m-label {
		font-size: 10px; font-weight: 600; color: var(--text-muted);
		text-transform: uppercase; letter-spacing: 0.05em; flex-shrink: 0;
	}
	.m-detail {
		border-top: 1px solid var(--border);
		background: var(--bg-base);
		display: grid; grid-template-columns: 1fr; gap: 6px;
		padding: 10px 14px;
	}

	@media (max-width: 639px) {
		.desktop-table { display: none; }
		.mobile-cards { display: flex; }

		.toolbar { gap: 8px; }
	}
</style>
