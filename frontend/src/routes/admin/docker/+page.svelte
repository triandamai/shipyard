<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

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
		mode: string; ports: string[]; labels: Record<string, string>;
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
	interface ImageSummary { id: string; tags: string[]; size: number; created: number; }

	let containers = $state<ContainerSummary[]>([]);
	let services   = $state<ServiceSummary[]>([]);
	let volumes    = $state<VolumeSummary[]>([]);
	let networks   = $state<NetworkSummary[]>([]);
	let images     = $state<ImageSummary[]>([]);

	let loadingC = $state(false); let loadingS = $state(false);
	let loadingV = $state(false); let loadingN = $state(false);
	let loadingI = $state(false);
	let search = $state('');

	async function loadContainers() {
		loadingC = true;
		const r = await api.get<ContainerSummary[]>('/admin/docker/containers');
		if (r.data) containers = r.data;
		loadingC = false;
	}
	async function loadServices() {
		loadingS = true;
		const r = await api.get<ServiceSummary[]>('/admin/docker/services');
		if (r.data) services = r.data;
		loadingS = false;
	}
	async function loadVolumes() {
		loadingV = true;
		const r = await api.get<VolumeSummary[]>('/admin/docker/volumes');
		if (r.data) volumes = r.data;
		loadingV = false;
	}
	async function loadNetworks() {
		loadingN = true;
		const r = await api.get<NetworkSummary[]>('/admin/docker/networks');
		if (r.data) networks = r.data;
		loadingN = false;
	}
	async function loadImages() {
		loadingI = true;
		const r = await api.get<ImageSummary[]>('/admin/docker/images');
		if (r.data) images = r.data;
		loadingI = false;
	}

	async function switchTab(t: Tab) {
		activeTab = t; search = '';
		if (t === 'containers' && containers.length === 0) await loadContainers();
		if (t === 'services'   && services.length === 0)   await loadServices();
		if (t === 'volumes'    && volumes.length === 0)    await loadVolumes();
		if (t === 'networks'   && networks.length === 0)   await loadNetworks();
		if (t === 'images'     && images.length === 0)     await loadImages();
	}

	async function refresh() {
		if (activeTab === 'containers') { containers = []; await loadContainers(); }
		if (activeTab === 'services')   { services = [];   await loadServices(); }
		if (activeTab === 'volumes')    { volumes = [];    await loadVolumes(); }
		if (activeTab === 'networks')   { networks = [];   await loadNetworks(); }
		if (activeTab === 'images')     { images = [];     await loadImages(); }
	}

	let pruning = $state(false);
	let pruneMsg = $state('');

	async function prune(what: 'containers' | 'volumes' | 'images') {
		if (!confirm(`Prune unused ${what}? This cannot be undone.`)) return;
		pruning = true;
		pruneMsg = '';
		const r = await api.post<{ message: string }>(`/admin/docker/prune/${what}`);
		pruneMsg = r.data?.message ?? r.error?.message ?? 'Done';
		pruning = false;
		setTimeout(() => (pruneMsg = ''), 4000);
		await refresh();
	}

	let loading = $derived(
		(activeTab === 'containers' && loadingC) ||
		(activeTab === 'services'   && loadingS) ||
		(activeTab === 'volumes'    && loadingV) ||
		(activeTab === 'networks'   && loadingN) ||
		(activeTab === 'images'     && loadingI)
	);

	function fmtBytes(bytes: number): string {
		if (bytes === 0) return '0 B';
		const k = 1024;
		const s = ['B','KB','MB','GB','TB'];
		const i = Math.floor(Math.log(bytes) / Math.log(k));
		return `${(bytes / Math.pow(k, i)).toFixed(1)} ${s[i]}`;
	}

	function containerName(c: ContainerSummary): string {
		return c.names[0]?.replace(/^\//, '') ?? c.id.slice(0, 12);
	}

	let filteredContainers = $derived(
		containers.filter(c => !search || containerName(c).includes(search) || c.image.includes(search))
	);
	let filteredServices = $derived(
		services.filter(s => !search || s.name.includes(search) || s.image.includes(search))
	);
	let filteredVolumes = $derived(
		volumes.filter(v => !search || v.name.includes(search))
	);
	let filteredNetworks = $derived(
		networks.filter(n => !search || n.name.includes(search))
	);
	let filteredImages = $derived(
		images.filter(i => !search || i.tags.some(t => t.includes(search)))
	);

	function stateColor(s: string): string {
		if (s === 'running') return 'var(--ok)';
		if (s === 'exited' || s === 'dead') return 'var(--danger)';
		return 'var(--text-3)';
	}

	onMount(() => loadContainers());
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Docker</h1>
			<p class="sub">Platform Docker daemon — containers, services, volumes, networks, images.</p>
		</div>
		<button class="refresh-btn" onclick={refresh}>
			<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
			Refresh
		</button>
	</header>

	<div class="toolbar">
		<div class="tabs">
			{#each (['containers','services','volumes','networks','images'] as Tab[]) as t}
				<button class="tab" class:active={activeTab === t} onclick={() => switchTab(t)}>
					{t.charAt(0).toUpperCase() + t.slice(1)}
				</button>
			{/each}
		</div>
		<div style="display:flex;align-items:center;gap:8px">
			{#if activeTab === 'containers' || activeTab === 'volumes' || activeTab === 'images'}
				<button class="prune-btn" disabled={pruning} onclick={() => prune(activeTab as 'containers'|'volumes'|'images')}>
					<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd"/></svg>
					{pruning ? 'Pruning…' : 'Prune Unused'}
				</button>
			{/if}
			{#if pruneMsg}<span class="prune-msg">{pruneMsg}</span>{/if}
			<label class="search">
				<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="12" height="12"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
				<input type="text" placeholder="Search…" bind:value={search} />
			</label>
		</div>
	</div>

	{#if loading}
		<div class="tbl">{#each [0,1,2,3] as _}<div class="sk-row"><div class="sk" style="width:140px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div><div class="sk" style="width:70px;height:18px;border-radius:999px"></div></div>{/each}</div>
	{:else if activeTab === 'containers'}
		{#if filteredContainers.length === 0}
			<div class="empty">No containers.</div>
		{:else}
			<div class="tbl">
				<div class="thead">
					<span style="flex:2">Name</span>
					<span style="flex:3">Image</span>
					<span style="flex:1">State</span>
					<span style="flex:2">Status</span>
				</div>
				{#each filteredContainers as c}
					<div class="trow">
						<div class="mono" style="flex:2;font-size:12px">{containerName(c)}</div>
						<div class="cell img" style="flex:3">{c.image}</div>
						<div style="flex:1">
							<span class="dot" style="background:{stateColor(c.state)}"></span>
							<span class="cell" style="color:{stateColor(c.state)}">{c.state}</span>
						</div>
						<div class="cell muted" style="flex:2">{c.status}</div>
					</div>
				{/each}
			</div>
			<div class="card-list">
				{#each filteredContainers as c}
					<div class="m-card">
						<div class="m-card-title mono">{containerName(c)}</div>
						<div class="m-card-row"><span class="m-key">State</span><span style="color:{stateColor(c.state)}">{c.state}</span></div>
						<div class="m-card-row"><span class="m-key">Status</span><span class="muted">{c.status}</span></div>
						<div class="m-card-row"><span class="m-key">Image</span><span class="mono" style="font-size:11px;word-break:break-all">{c.image}</span></div>
					</div>
				{/each}
			</div>
		{/if}

	{:else if activeTab === 'services'}
		{#if filteredServices.length === 0}
			<div class="empty">No services.</div>
		{:else}
			<div class="tbl">
				<div class="thead">
					<span style="flex:2">Name</span>
					<span style="flex:3">Image</span>
					<span style="flex:0.8">Mode</span>
					<span style="flex:0.9">Replicas</span>
				</div>
				{#each filteredServices as s}
					<div class="trow">
						<div class="mono" style="flex:2;font-size:12px">{s.name}</div>
						<div class="cell img" style="flex:3">{s.image}</div>
						<div class="cell" style="flex:0.8">{s.mode}</div>
						<div style="flex:0.9">
							<span class="rep-pill" class:rep-ok={s.replicas_running >= s.replicas_desired} class:rep-warn={s.replicas_running < s.replicas_desired}>
								{s.replicas_running}/{s.replicas_desired}
							</span>
						</div>
					</div>
				{/each}
			</div>
			<div class="card-list">
				{#each filteredServices as s}
					<div class="m-card">
						<div class="m-card-title mono">{s.name}</div>
						<div class="m-card-row"><span class="m-key">Replicas</span><span class="rep-pill" class:rep-ok={s.replicas_running >= s.replicas_desired} class:rep-warn={s.replicas_running < s.replicas_desired}>{s.replicas_running}/{s.replicas_desired}</span></div>
						<div class="m-card-row"><span class="m-key">Mode</span><span>{s.mode}</span></div>
						<div class="m-card-row"><span class="m-key">Image</span><span class="mono" style="font-size:11px;word-break:break-all">{s.image}</span></div>
					</div>
				{/each}
			</div>
		{/if}

	{:else if activeTab === 'volumes'}
		{#if filteredVolumes.length === 0}
			<div class="empty">No volumes.</div>
		{:else}
			<div class="tbl">
				<div class="thead">
					<span style="flex:2">Name</span>
					<span style="flex:1">Driver</span>
					<span style="flex:1">Scope</span>
					<span style="flex:3">Mountpoint</span>
				</div>
				{#each filteredVolumes as v}
					<div class="trow">
						<div class="mono" style="flex:2;font-size:12px">{v.name}</div>
						<div class="cell" style="flex:1">{v.driver}</div>
						<div class="cell" style="flex:1">{v.scope}</div>
						<div class="mono muted" style="flex:3;font-size:11px">{v.mountpoint}</div>
					</div>
				{/each}
			</div>
			<div class="card-list">
				{#each filteredVolumes as v}
					<div class="m-card">
						<div class="m-card-title mono">{v.name}</div>
						<div class="m-card-row"><span class="m-key">Driver</span><span>{v.driver}</span></div>
						<div class="m-card-row"><span class="m-key">Scope</span><span>{v.scope}</span></div>
						<div class="m-card-row"><span class="m-key">Mount</span><span class="mono muted" style="font-size:11px;word-break:break-all">{v.mountpoint}</span></div>
					</div>
				{/each}
			</div>
		{/if}

	{:else if activeTab === 'networks'}
		{#if filteredNetworks.length === 0}
			<div class="empty">No networks.</div>
		{:else}
			<div class="tbl">
				<div class="thead">
					<span style="flex:2">Name</span>
					<span style="flex:1">Driver</span>
					<span style="flex:1">Scope</span>
					<span style="flex:1.5">Subnet</span>
					<span style="flex:0.6">Containers</span>
				</div>
				{#each filteredNetworks as n}
					<div class="trow">
						<div class="mono" style="flex:2;font-size:12px">{n.name}</div>
						<div class="cell" style="flex:1">{n.driver}</div>
						<div class="cell" style="flex:1">{n.scope}</div>
						<div class="mono muted" style="flex:1.5;font-size:11.5px">{n.ipam_subnet ?? '—'}</div>
						<div class="cell" style="flex:0.6">{n.containers}</div>
					</div>
				{/each}
			</div>
			<div class="card-list">
				{#each filteredNetworks as n}
					<div class="m-card">
						<div class="m-card-title mono">{n.name}</div>
						<div class="m-card-row"><span class="m-key">Driver</span><span>{n.driver}</span></div>
						<div class="m-card-row"><span class="m-key">Scope</span><span>{n.scope}</span></div>
						<div class="m-card-row"><span class="m-key">Subnet</span><span class="mono muted">{n.ipam_subnet ?? '—'}</span></div>
						<div class="m-card-row"><span class="m-key">Containers</span><span>{n.containers}</span></div>
					</div>
				{/each}
			</div>
		{/if}

	{:else if activeTab === 'images'}
		{#if filteredImages.length === 0}
			<div class="empty">No images.</div>
		{:else}
			<div class="tbl">
				<div class="thead">
					<span style="flex:1">ID</span>
					<span style="flex:3">Tags</span>
					<span style="flex:0.8">Size</span>
				</div>
				{#each filteredImages as img}
					<div class="trow">
						<div class="mono muted" style="flex:1;font-size:11px">{img.id.replace('sha256:','').slice(0,12)}</div>
						<div class="cell" style="flex:3">{img.tags.join(', ') || '&lt;none&gt;'}</div>
						<div class="cell" style="flex:0.8">{fmtBytes(img.size)}</div>
					</div>
				{/each}
			</div>
			<div class="card-list">
				{#each filteredImages as img}
					<div class="m-card">
						<div class="m-card-title mono" style="font-size:11.5px">{img.id.replace('sha256:','').slice(0,12)}</div>
						<div class="m-card-row"><span class="m-key">Tags</span><span style="font-size:11.5px">{img.tags.join(', ') || '&lt;none&gt;'}</span></div>
						<div class="m-card-row"><span class="m-key">Size</span><span>{fmtBytes(img.size)}</span></div>
					</div>
				{/each}
			</div>
		{/if}
	{/if}
</div>

<style>
	.p { max-width:1080px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:20px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }
	.refresh-btn { display:flex; align-items:center; gap:6px; padding:6px 12px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.refresh-btn:hover { background:var(--surface-2); }
	.prune-btn { display:flex; align-items:center; gap:5px; padding:5px 11px; height:30px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:600; cursor:pointer; border:1px solid rgba(220,38,38,0.2); background:var(--danger-soft); color:var(--danger); transition:background .15s; font-family:var(--font); }
	.prune-btn:hover:not(:disabled) { background:rgba(220,38,38,0.14); }
	.prune-btn:disabled { opacity:.45; cursor:not-allowed; }
	.prune-msg { font-size:11.5px; color:var(--ok); font-weight:500; }

	.toolbar { display:flex; align-items:center; justify-content:space-between; margin-bottom:14px; gap:12px; }
	.tabs { display:flex; gap:2px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); padding:3px; }
	.tab { padding:5px 12px; border-radius:5px; font-size:12px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.tab.active { background:var(--surface); color:var(--text); box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.search { position:relative; display:flex; align-items:center; cursor:text; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search input { height:32px; padding:0 10px 0 27px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12px; color:var(--text); outline:none; width:180px; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search input::placeholder { color:var(--text-3); }
	.search input:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.mono { font-family:var(--mono); color:var(--text); }
	.img { font-size:11.5px; font-family:var(--mono); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
	.dot { display:inline-block; width:6px; height:6px; border-radius:50%; margin-right:5px; }
	.rep-pill { display:inline-flex; padding:2px 8px; border-radius:999px; font-size:11px; font-weight:700; }
	.rep-ok { background:var(--ok-soft); color:var(--ok); }
	.rep-warn { background:var(--warn-soft); color:var(--warn); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-row { display:flex; align-items:center; gap:12px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.card-list { display:none; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-title { font-size:13px; font-weight:600; color:var(--text); margin-bottom:8px; font-family:var(--mono); }
	.m-card-row { display:flex; justify-content:space-between; align-items:flex-start; padding:5px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); gap:8px; }
	.m-card-row:last-child { border-bottom:none; }
	.m-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; flex-shrink:0; }

	@media (max-width: 680px) {
		.p { padding:20px 12px; }
		.tbl { display:none; }
		.card-list { display:block; }
		.toolbar { flex-direction:column; align-items:flex-start; gap:8px; }
		.tabs { width:100%; overflow-x:auto; }
		.search input { width:100%; }
		.hdr { flex-direction:column; align-items:flex-start; gap:8px; }
	}
</style>
