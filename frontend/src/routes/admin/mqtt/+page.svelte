<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	type Tab = 'clients' | 'subscriptions' | 'topics';
	let activeTab = $state<Tab>('clients');

	let clients       = $state<any[]>([]);
	let subscriptions = $state<any[]>([]);
	let topics        = $state<any[]>([]);

	let loadingC = $state(false);
	let loadingS = $state(false);
	let loadingT = $state(false);

	let clientSearch = $state('');
	let subSearch    = $state('');
	let topicSearch  = $state('');

	async function loadClients() {
		loadingC = true;
		const r = await api.get<any>('/admin/mqtt/clients');
		if (r.data) clients = Array.isArray(r.data) ? r.data : (r.data.items ?? r.data.data ?? []);
		loadingC = false;
	}
	async function loadSubscriptions() {
		loadingS = true;
		const r = await api.get<any>('/admin/mqtt/subscriptions');
		if (r.data) subscriptions = Array.isArray(r.data) ? r.data : (r.data.items ?? r.data.data ?? []);
		loadingS = false;
	}
	async function loadTopics() {
		loadingT = true;
		const r = await api.get<any>('/admin/mqtt/topics');
		if (r.data) topics = Array.isArray(r.data) ? r.data : (r.data.items ?? r.data.data ?? []);
		loadingT = false;
	}

	async function switchTab(t: Tab) {
		activeTab = t;
		if (t === 'clients'       && clients.length === 0)       await loadClients();
		if (t === 'subscriptions' && subscriptions.length === 0) await loadSubscriptions();
		if (t === 'topics'        && topics.length === 0)        await loadTopics();
	}

	async function refresh() {
		if (activeTab === 'clients')       { clients = [];       await loadClients(); }
		if (activeTab === 'subscriptions') { subscriptions = []; await loadSubscriptions(); }
		if (activeTab === 'topics')        { topics = [];        await loadTopics(); }
	}

	let filteredClients = $derived(
		clients.filter(c => {
			const q = clientSearch.toLowerCase();
			return !q || (c.client_id ?? c.clientid ?? '').toLowerCase().includes(q)
				|| (c.username ?? '').toLowerCase().includes(q)
				|| (c.remote_addr ?? c.ipaddress ?? '').toLowerCase().includes(q);
		})
	);
	let filteredSubs = $derived(
		subscriptions.filter(s => {
			const q = subSearch.toLowerCase();
			return !q || (s.topic ?? '').toLowerCase().includes(q)
				|| (s.client_id ?? s.clientid ?? '').toLowerCase().includes(q);
		})
	);
	let filteredTopics = $derived(
		topics.filter(t => {
			const q = topicSearch.toLowerCase();
			return !q || (t.topic ?? t.name ?? '').toLowerCase().includes(q);
		})
	);

	function connectedAt(ts: any): string {
		if (!ts) return '—';
		try { return new Date(typeof ts === 'number' ? ts * 1000 : ts).toLocaleString(); }
		catch { return String(ts); }
	}

	let logLines = $state<string[]>([]);
	let logConnected = $state(false);
	let logEs: EventSource | null = null;
	let logEl = $state<HTMLDivElement | null>(null);

	function openMqttLogs() {
		logEs?.close();
		logLines = [];
		logConnected = false;
		const es = new EventSource('/api/admin/mqtt/logs/stream');
		es.onopen = () => { logConnected = true; };
		es.onmessage = (e) => {
			logLines = [...logLines.slice(-499), e.data];
			requestAnimationFrame(() => { if (logEl) logEl.scrollTop = logEl.scrollHeight; });
		};
		es.onerror = () => { logConnected = false; };
		logEs = es;
	}

	function closeMqttLogs() {
		logEs?.close(); logEs = null; logConnected = false;
	}

	import { onDestroy } from 'svelte';
	onMount(() => loadClients());
	onDestroy(() => closeMqttLogs());
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">MQTT Broker</h1>
			<p class="sub">Platform-wide broker monitoring — clients, subscriptions, topics.</p>
		</div>
		<button class="refresh-btn" onclick={refresh}>
			<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
			Refresh
		</button>
	</header>

	<div class="tabs">
		<button class="tab" class:active={activeTab === 'clients'} onclick={() => switchTab('clients')}>
			Clients {#if clients.length > 0}<span class="badge">{clients.length}</span>{/if}
		</button>
		<button class="tab" class:active={activeTab === 'subscriptions'} onclick={() => switchTab('subscriptions')}>
			Subscriptions {#if subscriptions.length > 0}<span class="badge">{subscriptions.length}</span>{/if}
		</button>
		<button class="tab" class:active={activeTab === 'topics'} onclick={() => switchTab('topics')}>
			Topics {#if topics.length > 0}<span class="badge">{topics.length}</span>{/if}
		</button>
	</div>

	{#if activeTab === 'clients'}
		<div class="search-wrap">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
			<input class="search-inp" placeholder="Filter clients…" bind:value={clientSearch} />
		</div>
		{#if loadingC}
			<div class="tbl">{#each [0,1,2] as _}<div class="sk-row"><div class="sk" style="width:120px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div></div>{/each}</div>
		{:else if filteredClients.length === 0}
			<div class="empty">No connected clients.</div>
		{:else}
			<div class="tbl">
				<div class="thead">
					<span style="flex:2">Client ID</span>
					<span style="flex:1.5">Username</span>
					<span style="flex:1.5">Address</span>
					<span style="flex:1">Protocol</span>
					<span style="flex:1.5">Connected at</span>
				</div>
				{#each filteredClients as c}
					<div class="trow">
						<div class="mono" style="flex:2;font-size:12px">{c.client_id ?? c.clientid ?? '—'}</div>
						<div class="cell" style="flex:1.5">{c.username ?? '—'}</div>
						<div class="mono" style="flex:1.5;font-size:11.5px">{c.remote_addr ?? c.ipaddress ?? '—'}</div>
						<div class="cell" style="flex:1">{c.proto_ver ?? c.protocol ?? '—'}</div>
						<div class="cell" style="flex:1.5">{connectedAt(c.connected_at ?? c.connected_epoch)}</div>
					</div>
				{/each}
			</div>
		{/if}

	{:else if activeTab === 'subscriptions'}
		<div class="search-wrap">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
			<input class="search-inp" placeholder="Filter subscriptions…" bind:value={subSearch} />
		</div>
		{#if loadingS}
			<div class="tbl">{#each [0,1,2] as _}<div class="sk-row"><div class="sk" style="width:120px;height:12px"></div><div class="sk" style="flex:1;height:12px"></div></div>{/each}</div>
		{:else if filteredSubs.length === 0}
			<div class="empty">No subscriptions.</div>
		{:else}
			<div class="tbl">
				<div class="thead">
					<span style="flex:3">Topic</span>
					<span style="flex:2">Client ID</span>
					<span style="flex:0.6">QoS</span>
				</div>
				{#each filteredSubs as s}
					<div class="trow">
						<div class="mono" style="flex:3;font-size:12px">{s.topic ?? '—'}</div>
						<div class="mono" style="flex:2;font-size:11.5px">{s.client_id ?? s.clientid ?? '—'}</div>
						<div class="cell" style="flex:0.6">{s.qos ?? '—'}</div>
					</div>
				{/each}
			</div>
		{/if}

	{:else if activeTab === 'topics'}
		<div class="search-wrap">
			<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="13" height="13"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
			<input class="search-inp" placeholder="Filter topics…" bind:value={topicSearch} />
		</div>
		{#if loadingT}
			<div class="tbl">{#each [0,1,2] as _}<div class="sk-row"><div class="sk" style="width:200px;height:12px"></div></div>{/each}</div>
		{:else if filteredTopics.length === 0}
			<div class="empty">No topics.</div>
		{:else}
			<div class="tbl">
				<div class="thead"><span>Topic</span></div>
				{#each filteredTopics as t}
					<div class="trow">
						<span class="mono" style="font-size:12px">{t.topic ?? t.name ?? '—'}</span>
					</div>
				{/each}
			</div>
		{/if}
	{/if}

	<div class="log-section">
		<div class="log-hdr">
			<span class="log-title">Broker Logs</span>
			{#if !logEs}
				<button class="log-connect-btn" onclick={openMqttLogs}>Connect</button>
			{:else}
				<div style="display:flex;gap:6px;align-items:center">
					<span class="conn-dot" class:conn-ok={logConnected}></span>
					<span style="font-size:11.5px;color:var(--text-3)">{logConnected ? 'Live' : 'Connecting…'}</span>
					<button class="copy-btn" onclick={openMqttLogs}>Reconnect</button>
					<button class="copy-btn" onclick={() => { logLines = []; }}>Clear</button>
					<button class="copy-btn" onclick={closeMqttLogs}>Disconnect</button>
				</div>
			{/if}
		</div>
		{#if logEs}
			<div class="log-shell">
				<div class="log-body" bind:this={logEl}>
					{#if logLines.length === 0}
						<div class="log-empty">Waiting for log entries…</div>
					{:else}
						{#each logLines as line}
							<div class="log-line">{line}</div>
						{/each}
					{/if}
				</div>
			</div>
		{/if}
	</div>
</div>

<style>
	.p { max-width:1000px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:20px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }
	.refresh-btn { display:flex; align-items:center; gap:6px; padding:6px 12px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.refresh-btn:hover { background:var(--surface-2); }

	.tabs { display:flex; gap:2px; margin-bottom:14px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); padding:3px; width:fit-content; }
	.tab { display:flex; align-items:center; gap:6px; padding:5px 14px; border-radius:5px; font-size:12.5px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.tab.active { background:var(--surface); color:var(--text); box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.badge { display:inline-flex; align-items:center; justify-content:center; height:16px; padding:0 5px; border-radius:999px; font-size:10px; font-weight:700; background:var(--accent-soft); color:var(--accent); }

	.search-wrap { position:relative; display:flex; align-items:center; margin-bottom:12px; }
	.si { position:absolute; left:9px; color:var(--text-3); pointer-events:none; }
	.search-inp { height:32px; padding:0 10px 0 28px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:260px; transition:border-color .15s, box-shadow .15s; font-family:var(--font); }
	.search-inp::placeholder { color:var(--text-3); }
	.search-inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.tbl { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 16px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 16px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.cell { font-size:12.5px; color:var(--text-2); }
	.mono { font-family:var(--mono); color:var(--text); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-row { display:flex; align-items:center; gap:12px; padding:13px 16px; border-bottom:1px solid var(--border); }
	.sk-row:last-child { border-bottom:none; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.log-section { margin-top:28px; }
	.log-hdr { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:10px; }
	.log-title { font-size:13px; font-weight:700; color:var(--text); letter-spacing:-0.01em; }
	.log-connect-btn { padding:5px 14px; height:28px; border-radius:var(--radius-sm); font-size:12px; font-weight:600; cursor:pointer; border:none; background:var(--accent); color:#fff; transition:opacity .15s; font-family:var(--font); }
	.log-connect-btn:hover { opacity:.88; }
	.copy-btn { padding:3px 10px; height:24px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.copy-btn:hover { background:var(--surface-2); }
	.conn-dot { width:7px; height:7px; border-radius:50%; background:var(--text-3); transition:background .3s; }
	.conn-dot.conn-ok { background:var(--ok); }
	.log-shell { background:#0f1117; border:1px solid rgba(255,255,255,.08); border-radius:var(--radius); overflow:hidden; }
	.log-body { height:320px; overflow-y:auto; padding:12px 14px; font-family:var(--mono); font-size:11.5px; line-height:1.6; }
	.log-line { color:#c9d1d9; white-space:pre-wrap; word-break:break-all; }
	.log-empty { color:#6e7681; font-style:italic; }
</style>
