<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { page } from '$app/state';
	import {
		Box, Layers, HardDrive, Network, RefreshCw,
		Search, ChevronDown, ChevronRight, Trash2, Image
	} from '@lucide/svelte';

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

	const stateColor: Record<string, string> = {
		running: '#16a34a', exited: '#9ca3af', created: '#2563eb',
		paused: '#d97706', dead: '#ef4444', restarting: '#f97316',
	};

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

	onMount(() => { if (canDockerAny) loadContainers(); });
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canDockerAny}
	message="You need the 'View Docker' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if canDockerAny}
<div class="docker-page">

	<div class="toolbar">
		<div class="tabs">
			<button class="tab" class:active={activeTab==='containers'} onclick={() => switchTab('containers')}>
				<Box size={13} /> Containers
				{#if containers.length}<span class="badge">{containers.length}</span>{/if}
			</button>
			<button class="tab" class:active={activeTab==='services'} onclick={() => switchTab('services')}>
				<Layers size={13} /> Services
				{#if services.length}<span class="badge">{services.length}</span>{/if}
			</button>
			<button class="tab" class:active={activeTab==='volumes'} onclick={() => switchTab('volumes')}>
				<HardDrive size={13} /> Volumes
				{#if volumes.length}<span class="badge">{volumes.length}</span>{/if}
			</button>
			<button class="tab" class:active={activeTab==='networks'} onclick={() => switchTab('networks')}>
				<Network size={13} /> Networks
				{#if networks.length}<span class="badge">{networks.length}</span>{/if}
			</button>
			<button class="tab" class:active={activeTab==='images'} onclick={() => switchTab('images')}>
				<Image size={13} /> Images
				{#if images.length}<span class="badge">{images.length}</span>{/if}
			</button>
		</div>
		<div class="toolbar-right">
			{#if activeTab === 'containers'}
				{#if pruneConfirm}
					<span class="prune-confirm-text">Remove all stopped containers?</span>
				{/if}
				<button
					class="prune-btn"
					class:danger={pruneConfirm}
					onclick={pruneContainers}
					disabled={pruning}
				>
					<Trash2 size={14} />
					{pruning ? 'Pruning…' : pruneConfirm ? 'Confirm' : 'Prune stopped'}
				</button>
				{#if pruneConfirm}
					<button class="cancel-btn" onclick={() => pruneConfirm = false}>Cancel</button>
				{/if}
			{/if}
			{#if activeTab === 'volumes'}
				{#if pruneVolumesConfirm}
					<span class="prune-confirm-text">Remove all unused volumes?</span>
				{/if}
				<button
					class="prune-btn"
					class:danger={pruneVolumesConfirm}
					onclick={pruneVolumes}
					disabled={pruningVolumes}
				>
					<Trash2 size={14} />
					{pruningVolumes ? 'Pruning…' : pruneVolumesConfirm ? 'Confirm' : 'Prune unused'}
				</button>
				{#if pruneVolumesConfirm}
					<button class="cancel-btn" onclick={() => pruneVolumesConfirm = false}>Cancel</button>
				{/if}
			{/if}
			{#if activeTab === 'images'}
				{#if pruneImagesConfirm}
					<span class="prune-confirm-text">Remove all unused images?</span>
				{/if}
				<button
					class="prune-btn"
					class:danger={pruneImagesConfirm}
					onclick={pruneImages}
					disabled={pruningImages}
				>
					<Trash2 size={14} />
					{pruningImages ? 'Pruning…' : pruneImagesConfirm ? 'Confirm' : 'Prune unused'}
				</button>
				{#if pruneImagesConfirm}
					<button class="cancel-btn" onclick={() => pruneImagesConfirm = false}>Cancel</button>
				{/if}
			{/if}
			<button class="refresh-btn" onclick={refresh} disabled={isLoading}>
				<RefreshCw size={14} class={isLoading ? 'spin' : ''} /> Refresh
			</button>
		</div>
	</div>

	{#if pruneResult}
		<div class="prune-toast">{pruneResult}</div>
	{/if}
	{#if pruneImagesResult}
		<div class="prune-toast">{pruneImagesResult}</div>
	{/if}
	{#if pruneVolumesResult}
		<div class="prune-toast">{pruneVolumesResult}</div>
	{/if}

	<div class="search-bar">
		<Search size={13} />
		<input class="search-input" placeholder="Filter…" bind:value={search} />
	</div>

	<!-- ── Containers ── -->
	{#if activeTab === 'containers'}
		{#if loadingC}
			<div class="empty"><div class="spinner"></div>Loading containers…</div>
		{:else if filteredContainers.length === 0}
			<div class="empty"><Box size={28} />No containers</div>
		{:else}
			<div class="card">
				<table class="tbl">
					<thead><tr>
						<th style="width:28px"></th>
						<th>Name</th><th>Image</th><th>State</th>
						<th>Status</th><th>Ports</th><th>Created</th>
					</tr></thead>
					<tbody>
						{#each filteredContainers as c (c.id)}
							{@const isExp = expanded === c.id}
							{@const name = c.names[0] ?? c.id.slice(0,12)}
							<tr class="row" class:exp={isExp} onclick={() => toggle(c.id)}>
								<td class="exp-cell">{#if isExp}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}</td>
								<td class="mono bold">{name}</td>
								<td class="mono dim">{shortImg(c.image)}</td>
								<td>
									<span class="state-dot" style="background:{stateColor[c.state]??'#9ca3af'}"></span>
									{c.state}
								</td>
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
												<div class="detail-field full"><span class="dk">Labels</span>
													<div class="label-chips">
														{#each Object.entries(c.labels) as [k,v]}
															<span class="lchip"><b>{k}</b>={v}</span>
														{/each}
													</div>
												</div>
											{/if}
										</div>
									</td>
								</tr>
							{/if}
						{/each}
					</tbody>
				</table>
			</div>

			<div class="mobile-cards">
				{#each filteredContainers as c (c.id)}
					{@const isExp = expanded === c.id}
					{@const name = c.names[0] ?? c.id.slice(0,12)}
					<div class="m-card">
						<button class="m-card-header" onclick={() => toggle(c.id)}>
							<div class="m-card-title-row">
								<span class="state-dot" style="background:{stateColor[c.state]??'#9ca3af'}"></span>
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
									<div class="detail-field"><span class="dk">Labels</span>
										<div class="label-chips" style="margin-top:4px">
											{#each Object.entries(c.labels) as [k,v]}
												<span class="lchip"><b>{k}</b>={v}</span>
											{/each}
										</div>
									</div>
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
			<div class="empty"><div class="spinner"></div>Loading services…</div>
		{:else if filteredServices.length === 0}
			<div class="empty"><Layers size={28} />No swarm services</div>
		{:else}
			<div class="card">
				<table class="tbl">
					<thead><tr>
						<th style="width:28px"></th>
						<th>Name</th><th>Image</th><th>Mode</th>
						<th>Replicas</th><th>Ports</th>
					</tr></thead>
					<tbody>
						{#each filteredServices as s (s.id)}
							{@const isExp = expanded === s.id}
							{@const healthy = s.replicas_running >= s.replicas_desired && s.replicas_desired > 0}
							<tr class="row" class:exp={isExp} onclick={() => toggle(s.id)}>
								<td class="exp-cell">{#if isExp}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}</td>
								<td class="mono bold">{s.name}</td>
								<td class="mono dim">{shortImg(s.image)}</td>
								<td><span class="mode-chip">{s.mode}</span></td>
								<td>
									<span class="replica-badge" class:healthy class:degraded={!healthy}>
										{s.replicas_running}/{s.replicas_desired}
									</span>
								</td>
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
												<div class="detail-field full"><span class="dk">Labels</span>
													<div class="label-chips">
														{#each Object.entries(s.labels) as [k,v]}
															<span class="lchip"><b>{k}</b>={v}</span>
														{/each}
													</div>
												</div>
											{/if}
										</div>
									</td>
								</tr>
							{/if}
						{/each}
					</tbody>
				</table>
			</div>

			<div class="mobile-cards">
				{#each filteredServices as s (s.id)}
					{@const isExp = expanded === s.id}
					{@const healthy = s.replicas_running >= s.replicas_desired && s.replicas_desired > 0}
					<div class="m-card">
						<button class="m-card-header" onclick={() => toggle(s.id)}>
							<div class="m-card-title-row">
								<span class="m-card-title mono">{s.name}</span>
								<span class="replica-badge" class:healthy class:degraded={!healthy}>{s.replicas_running}/{s.replicas_desired}</span>
							</div>
							<span class="m-chevron">{#if isExp}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}</span>
						</button>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Image</span><span class="mono dim">{shortImg(s.image)}</span></div>
							<div class="m-row"><span class="m-label">Mode</span><span class="mode-chip">{s.mode}</span></div>
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
									<div class="detail-field"><span class="dk">Labels</span>
										<div class="label-chips" style="margin-top:4px">
											{#each Object.entries(s.labels) as [k,v]}
												<span class="lchip"><b>{k}</b>={v}</span>
											{/each}
										</div>
									</div>
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
			<div class="empty"><div class="spinner"></div>Loading volumes…</div>
		{:else if filteredVolumes.length === 0}
			<div class="empty"><HardDrive size={28} />No volumes</div>
		{:else}
			<div class="card">
				<table class="tbl">
					<thead><tr>
						<th style="width:28px"></th>
						<th>Name</th><th>Driver</th><th>Scope</th><th>Mountpoint</th>
					</tr></thead>
					<tbody>
						{#each filteredVolumes as v (v.name)}
							{@const isExp = expanded === v.name}
							<tr class="row" class:exp={isExp} onclick={() => toggle(v.name)}>
								<td class="exp-cell">{#if isExp}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}</td>
								<td class="mono bold">{v.name}</td>
								<td><span class="mode-chip">{v.driver}</span></td>
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
												<div class="detail-field full"><span class="dk">Labels</span>
													<div class="label-chips">
														{#each Object.entries(v.labels) as [k,lv]}
															<span class="lchip"><b>{k}</b>={lv}</span>
														{/each}
													</div>
												</div>
											{/if}
										</div>
									</td>
								</tr>
							{/if}
						{/each}
					</tbody>
				</table>
			</div>

			<div class="mobile-cards">
				{#each filteredVolumes as v (v.name)}
					{@const isExp = expanded === v.name}
					<div class="m-card">
						<button class="m-card-header" onclick={() => toggle(v.name)}>
							<span class="m-card-title mono">{v.name}</span>
							<span class="m-chevron">{#if isExp}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}</span>
						</button>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Driver</span><span class="mode-chip">{v.driver}</span></div>
							<div class="m-row"><span class="m-label">Scope</span><span class="dim">{v.scope}</span></div>
							<div class="m-row"><span class="m-label">Mountpoint</span><span class="mono dim truncate">{v.mountpoint}</span></div>
						</div>
						{#if isExp}
							<div class="m-detail">
								<div class="detail-field"><span class="dk">Mountpoint</span><span class="dv mono">{v.mountpoint}</span></div>
								{#if v.created_at}<div class="detail-field"><span class="dk">Created</span><span class="dv">{v.created_at}</span></div>{/if}
								{#if Object.keys(v.labels).length}
									<div class="detail-field"><span class="dk">Labels</span>
										<div class="label-chips" style="margin-top:4px">
											{#each Object.entries(v.labels) as [k,lv]}
												<span class="lchip"><b>{k}</b>={lv}</span>
											{/each}
										</div>
									</div>
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
			<div class="empty"><div class="spinner"></div>Loading networks…</div>
		{:else if filteredNetworks.length === 0}
			<div class="empty"><Network size={28} />No networks</div>
		{:else}
			<div class="card">
				<table class="tbl">
					<thead><tr>
						<th style="width:28px"></th>
						<th>Name</th><th>Driver</th><th>Scope</th>
						<th>Subnet</th><th>Containers</th><th>Flags</th>
					</tr></thead>
					<tbody>
						{#each filteredNetworks as n (n.id)}
							{@const isExp = expanded === n.id}
							<tr class="row" class:exp={isExp} onclick={() => toggle(n.id)}>
								<td class="exp-cell">{#if isExp}<ChevronDown size={12}/>{:else}<ChevronRight size={12}/>{/if}</td>
								<td class="mono bold">{n.name}</td>
								<td><span class="mode-chip">{n.driver}</span></td>
								<td class="dim">{n.scope}</td>
								<td class="mono dim">{n.ipam_subnet ?? '—'}</td>
								<td class="dim">{n.containers}</td>
								<td>
									{#if n.internal}<span class="flag-chip">internal</span>{/if}
									{#if n.attachable}<span class="flag-chip">attachable</span>{/if}
								</td>
							</tr>
							{#if isExp}
								<tr class="detail-row">
									<td colspan="7">
										<div class="detail-box">
											<div class="detail-field"><span class="dk">ID</span><span class="dv mono">{n.id}</span></div>
											<div class="detail-field"><span class="dk">Subnet</span><span class="dv mono">{n.ipam_subnet ?? '—'}</span></div>
											{#if Object.keys(n.labels).length}
												<div class="detail-field full"><span class="dk">Labels</span>
													<div class="label-chips">
														{#each Object.entries(n.labels) as [k,v]}
															<span class="lchip"><b>{k}</b>={v}</span>
														{/each}
													</div>
												</div>
											{/if}
										</div>
									</td>
								</tr>
							{/if}
						{/each}
					</tbody>
				</table>
			</div>

			<div class="mobile-cards">
				{#each filteredNetworks as n (n.id)}
					{@const isExp = expanded === n.id}
					<div class="m-card">
						<button class="m-card-header" onclick={() => toggle(n.id)}>
							<div class="m-card-title-row">
								<span class="m-card-title mono">{n.name}</span>
								<div class="m-flags">
									{#if n.internal}<span class="flag-chip">internal</span>{/if}
									{#if n.attachable}<span class="flag-chip">attachable</span>{/if}
								</div>
							</div>
							<span class="m-chevron">{#if isExp}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}</span>
						</button>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Driver</span><span class="mode-chip">{n.driver}</span></div>
							<div class="m-row"><span class="m-label">Scope</span><span class="dim">{n.scope}</span></div>
							<div class="m-row"><span class="m-label">Subnet</span><span class="mono dim">{n.ipam_subnet ?? '—'}</span></div>
							<div class="m-row"><span class="m-label">Containers</span><span class="dim">{n.containers}</span></div>
						</div>
						{#if isExp}
							<div class="m-detail">
								<div class="detail-field"><span class="dk">ID</span><span class="dv mono">{n.id}</span></div>
								<div class="detail-field"><span class="dk">Subnet</span><span class="dv mono">{n.ipam_subnet ?? '—'}</span></div>
								{#if Object.keys(n.labels).length}
									<div class="detail-field"><span class="dk">Labels</span>
										<div class="label-chips" style="margin-top:4px">
											{#each Object.entries(n.labels) as [k,v]}
												<span class="lchip"><b>{k}</b>={v}</span>
											{/each}
										</div>
									</div>
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
			<div class="empty"><div class="spinner"></div>Loading images…</div>
		{:else if filteredImages.length === 0}
			<div class="empty"><Image size={28} />No images</div>
		{:else}
			<div class="card">
				<table class="tbl">
					<thead><tr>
						<th>Tags</th><th>ID</th><th>Size</th><th>Created</th>
					</tr></thead>
					<tbody>
						{#each filteredImages as img (img.id)}
							<tr class="row no-expand">
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
						{/each}
					</tbody>
				</table>
			</div>

			<div class="mobile-cards">
				{#each filteredImages as img (img.id)}
					<div class="m-card">
						<div class="m-card-header" style="cursor:default">
							<div class="m-card-title-row" style="flex-direction:column;align-items:flex-start;gap:4px">
								{#if img.tags.length}
									<div class="tag-list">
										{#each img.tags as t}
											<span class="img-tag">{t}</span>
										{/each}
									</div>
								{:else}
									<span class="dim" style="font-size:12px">&lt;none&gt;</span>
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
{/if}

<style>
	.docker-page { display: flex; flex-direction: column; gap: 14px; }

	.toolbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; flex-wrap: wrap; }
	.toolbar-right { display: flex; align-items: center; gap: 8px; }
	.tabs { display: flex; gap: 4px; }
	.tab {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px; font-size: 12px; font-weight: 500;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius); color: var(--text-muted);
		cursor: pointer; transition: all var(--transition-fast);
	}
	.tab:hover { color: var(--text-primary); border-color: var(--border-hover); }
	.tab.active { background: var(--accent); border-color: var(--accent); color: #fff; }
	.tab.active .badge { background: rgba(255,255,255,0.25); }
	.badge {
		background: var(--bg-muted); color: var(--text-muted);
		border-radius: 10px; padding: 1px 6px; font-size: 11px; font-weight: 600;
	}
	.refresh-btn {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px; font-size: 12px; font-weight: 500;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius); color: var(--text-secondary);
		cursor: pointer; transition: all var(--transition-fast);
	}
	.refresh-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.refresh-btn:disabled { opacity: 0.5; cursor: default; }

	.prune-btn {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px; font-size: 12px; font-weight: 500;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius); color: var(--text-secondary);
		cursor: pointer; transition: all var(--transition-fast);
	}
	.prune-btn:hover:not(:disabled) { border-color: #ef4444; color: #ef4444; }
	.prune-btn.danger { background: #fef2f2; border-color: #ef4444; color: #ef4444; font-weight: 600; }
	.prune-btn:disabled { opacity: 0.5; cursor: default; }

	.cancel-btn {
		padding: 6px 10px; font-size: 12px; font-weight: 500;
		background: transparent; border: 1px solid var(--border);
		border-radius: var(--radius); color: var(--text-muted); cursor: pointer;
	}
	.cancel-btn:hover { border-color: var(--border-hover); color: var(--text-primary); }

	.prune-confirm-text { font-size: 12px; color: #ef4444; font-weight: 500; white-space: nowrap; }

	.prune-toast {
		padding: 10px 14px;
		background: #f0fdf4; border: 1px solid #bbf7d0;
		border-radius: var(--radius); color: #15803d;
		font-size: 13px; font-weight: 500;
	}

	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.search-bar {
		display: flex; align-items: center; gap: 8px;
		padding: 8px 12px;
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius); color: var(--text-muted);
	}
	.search-input {
		flex: 1; border: none; outline: none; background: transparent;
		font-size: 13px; color: var(--text-primary); font-family: var(--font-sans);
	}
	.search-input::placeholder { color: var(--text-muted); }

	.empty {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; padding: 60px; color: var(--text-muted); font-size: 13px;
	}
	.spinner {
		width: 18px; height: 18px; border: 2px solid var(--border);
		border-top-color: var(--accent); border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}

	.card {
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius-lg); overflow: hidden;
	}

	.tbl { width: 100%; border-collapse: collapse; font-size: 13px; }
	.tbl thead th {
		padding: 9px 12px; text-align: left;
		font-size: 11px; font-weight: 600; text-transform: uppercase; letter-spacing: 0.04em;
		color: var(--text-muted); background: var(--bg-muted);
		border-bottom: 1px solid var(--border);
	}
	.row td { padding: 9px 12px; border-bottom: 1px solid var(--border); vertical-align: middle; }
	.row:last-child td { border-bottom: none; }
	.row:hover td { background: var(--bg-muted); cursor: pointer; }
	.row.exp td { background: var(--bg-muted); }

	.exp-cell { color: var(--text-muted); width: 28px; }
	.mono { font-family: var(--font-mono, monospace); font-size: 12px; }
	.bold { font-weight: 600; color: var(--text-primary); }
	.dim  { color: var(--text-secondary); }
	.ts   { white-space: nowrap; }
	.truncate { max-width: 280px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.state-dot {
		display: inline-block; width: 7px; height: 7px;
		border-radius: 50%; margin-right: 6px; vertical-align: middle;
	}

	.mode-chip {
		display: inline-block; padding: 2px 7px;
		background: var(--bg-muted); border: 1px solid var(--border);
		border-radius: 4px; font-size: 11px; font-weight: 500; color: var(--text-secondary);
	}

	.replica-badge {
		display: inline-block; padding: 2px 8px;
		border-radius: 4px; font-size: 12px; font-weight: 600;
	}
	.replica-badge.healthy  { background: #f0fdf4; color: #16a34a; border: 1px solid #bbf7d0; }
	.replica-badge.degraded { background: #fef2f2; color: #dc2626; border: 1px solid #fecaca; }

	.flag-chip {
		display: inline-block; margin-right: 4px; padding: 1px 6px;
		background: #eff6ff; color: #2563eb; border: 1px solid #bfdbfe;
		border-radius: 4px; font-size: 10px; font-weight: 600;
	}

	.detail-row td { padding: 0; background: var(--bg-base); }
	.detail-box {
		display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr));
		gap: 8px; padding: 12px 14px;
		border-bottom: 1px solid var(--border);
	}
	.detail-field { display: flex; flex-direction: column; gap: 2px; }
	.detail-field.full { grid-column: 1 / -1; }
	.dk { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
	.dv { font-size: 12px; color: var(--text-primary); word-break: break-all; }

	.label-chips { display: flex; flex-wrap: wrap; gap: 4px; margin-top: 4px; }
	.lchip {
		display: inline-block; padding: 2px 7px;
		background: var(--bg-muted); border: 1px solid var(--border);
		border-radius: 4px; font-size: 11px; color: var(--text-secondary);
		max-width: 320px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}

	.row.no-expand:hover td { cursor: default; }

	.tag-list { display: flex; flex-wrap: wrap; gap: 4px; }
	.img-tag {
		display: inline-block; padding: 2px 7px;
		background: var(--bg-muted); border: 1px solid var(--border);
		border-radius: 4px; font-size: 11px; font-family: var(--font-mono, monospace);
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
	}
	.m-card-title-row {
		display: flex; align-items: center; gap: 8px; min-width: 0; flex: 1;
	}
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
		.card { display: none; }
		.mobile-cards { display: flex; }

		.toolbar { gap: 8px; }
		.tabs { flex-wrap: wrap; }
		.toolbar-right { flex-wrap: wrap; }
		.prune-confirm-text { display: none; }
	}
</style>
