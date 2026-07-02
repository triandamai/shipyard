<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Radio, RefreshCw, Users, BookOpen, Rss, Search, ChevronDown, ChevronRight } from '@lucide/svelte';

	type Tab = 'clients' | 'subscriptions' | 'topics';
	let activeTab = $state<Tab>('clients');

	let clients       = $state<any[]>([]);
	let subscriptions = $state<any[]>([]);
	let topics        = $state<any[]>([]);

	let loadingClients = $state(false);
	let loadingSubscriptions = $state(false);
	let loadingTopics  = $state(false);

	let clientSearch = $state('');
	let subSearch    = $state('');
	let topicSearch  = $state('');

	let expandedClient = $state<string | null>(null);

	async function loadClients() {
		loadingClients = true;
		const res = await api.get<any>('/admin/mqtt/clients');
		if (res.data) {
			const raw = res.data;
			clients = Array.isArray(raw) ? raw : (raw.items ?? raw.data ?? []);
		}
		loadingClients = false;
	}

	async function loadSubscriptions() {
		loadingSubscriptions = true;
		const res = await api.get<any>('/admin/mqtt/subscriptions');
		if (res.data) {
			const raw = res.data;
			subscriptions = Array.isArray(raw) ? raw : (raw.items ?? raw.data ?? []);
		}
		loadingSubscriptions = false;
	}

	async function loadTopics() {
		loadingTopics = true;
		const res = await api.get<any>('/admin/mqtt/topics');
		if (res.data) {
			const raw = res.data;
			topics = Array.isArray(raw) ? raw : (raw.items ?? raw.data ?? []);
		}
		loadingTopics = false;
	}

	async function switchTab(t: Tab) {
		activeTab = t;
		if (t === 'clients'       && clients.length === 0)       await loadClients();
		if (t === 'subscriptions' && subscriptions.length === 0) await loadSubscriptions();
		if (t === 'topics'        && topics.length === 0)         await loadTopics();
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

	onMount(() => loadClients());
</script>

<div class="mqtt-page">
	<div class="page-toolbar">
		<div class="inner-tabs">
			<button class="inner-tab" class:active={activeTab === 'clients'} onclick={() => switchTab('clients')}>
				<Users size={13} /> Clients
				{#if clients.length > 0}<span class="badge">{clients.length}</span>{/if}
			</button>
			<button class="inner-tab" class:active={activeTab === 'subscriptions'} onclick={() => switchTab('subscriptions')}>
				<Rss size={13} /> Subscriptions
				{#if subscriptions.length > 0}<span class="badge">{subscriptions.length}</span>{/if}
			</button>
			<button class="inner-tab" class:active={activeTab === 'topics'} onclick={() => switchTab('topics')}>
				<BookOpen size={13} /> Topics
				{#if topics.length > 0}<span class="badge">{topics.length}</span>{/if}
			</button>
		</div>
		<button class="refresh-btn" onclick={refresh}>
			<RefreshCw size={14} />
			Refresh
		</button>
	</div>

	<!-- Clients -->
	{#if activeTab === 'clients'}
		<div class="search-bar">
			<Search size={13} class="search-icon" />
			<input class="search-input" placeholder="Filter by client ID, username, IP…" bind:value={clientSearch} />
		</div>

		{#if loadingClients}
			<div class="empty-state"><div class="spinner"></div> Loading clients…</div>
		{:else if filteredClients.length === 0}
			<div class="empty-state"><Radio size={28} class="empty-icon" /> No connected clients</div>
		{:else}
			<div class="table-wrap">
				<table class="data-table">
					<thead>
						<tr>
							<th></th>
							<th>Client ID</th>
							<th>Username</th>
							<th>Address</th>
							<th>Protocol</th>
							<th>Connected at</th>
							<th>Keep-alive</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredClients as c (c.client_id ?? c.clientid)}
							{@const id = c.client_id ?? c.clientid ?? '—'}
							{@const expanded = expandedClient === id}
							<tr class="data-row" class:expanded onclick={() => expandedClient = expanded ? null : id}>
								<td class="expand-cell">
									{#if expanded}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
								</td>
								<td class="mono client-id">{id}</td>
								<td>{c.username ?? '—'}</td>
								<td class="mono">{c.remote_addr ?? c.ipaddress ?? '—'}</td>
								<td><span class="proto-chip">MQTT {c.protocol ?? c.mqtt_ver ?? ''}</span></td>
								<td class="ts">{connectedAt(c.connected_at ?? c.created_at)}</td>
								<td>{c.keepalive ?? c.keep_alive ?? '—'}s</td>
							</tr>
							{#if expanded}
								<tr class="detail-row">
									<td colspan="7">
										<div class="detail-grid">
											{#each Object.entries(c) as [k, v]}
												<div class="detail-kv">
													<span class="detail-k">{k}</span>
													<span class="detail-v mono">{JSON.stringify(v)}</span>
												</div>
											{/each}
										</div>
									</td>
								</tr>
							{/if}
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	{/if}

	<!-- Subscriptions -->
	{#if activeTab === 'subscriptions'}
		<div class="search-bar">
			<Search size={13} class="search-icon" />
			<input class="search-input" placeholder="Filter by topic or client ID…" bind:value={subSearch} />
		</div>

		{#if loadingSubscriptions}
			<div class="empty-state"><div class="spinner"></div> Loading subscriptions…</div>
		{:else if filteredSubs.length === 0}
			<div class="empty-state"><Rss size={28} class="empty-icon" /> No active subscriptions</div>
		{:else}
			<div class="table-wrap">
				<table class="data-table">
					<thead>
						<tr>
							<th>Topic</th>
							<th>Client ID</th>
							<th>QoS</th>
							<th>No-local</th>
							<th>Retain-as-published</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredSubs as s}
							<tr class="data-row">
								<td class="mono topic-cell">{s.topic ?? '—'}</td>
								<td class="mono">{s.client_id ?? s.clientid ?? '—'}</td>
								<td><span class="qos-chip qos-{s.qos ?? 0}">QoS {s.qos ?? 0}</span></td>
								<td>{s.no_local ? 'yes' : 'no'}</td>
								<td>{s.retain_as_published ? 'yes' : 'no'}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	{/if}

	<!-- Topics -->
	{#if activeTab === 'topics'}
		<div class="search-bar">
			<Search size={13} class="search-icon" />
			<input class="search-input" placeholder="Filter topics…" bind:value={topicSearch} />
		</div>

		{#if loadingTopics}
			<div class="empty-state"><div class="spinner"></div> Loading topics…</div>
		{:else if filteredTopics.length === 0}
			<div class="empty-state"><BookOpen size={28} class="empty-icon" /> No topics</div>
		{:else}
			<div class="table-wrap">
				<table class="data-table">
					<thead>
						<tr>
							<th>Topic</th>
							<th>Subscribers</th>
						</tr>
					</thead>
					<tbody>
						{#each filteredTopics as t}
							<tr class="data-row">
								<td class="mono topic-cell">{t.topic ?? t.name ?? '—'}</td>
								<td>{t.subscribers_count ?? t.subs_count ?? '—'}</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
		{/if}
	{/if}
</div>

<style>
	.mqtt-page { display: flex; flex-direction: column; gap: 16px; }

	.page-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}

	.inner-tabs { display: flex; gap: 4px; }
	.inner-tab {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px;
		font-size: 12px; font-weight: 500;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		color: var(--text-muted);
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.inner-tab:hover { color: var(--text-primary); border-color: var(--border-hover); }
	.inner-tab.active { background: var(--accent); border-color: var(--accent); color: #fff; }

	.badge {
		background: rgba(255,255,255,0.25);
		border-radius: 10px;
		padding: 1px 6px;
		font-size: 11px;
		font-weight: 600;
	}
	.inner-tab:not(.active) .badge { background: var(--bg-muted); color: var(--text-muted); }

	.refresh-btn {
		display: flex; align-items: center; gap: 6px;
		padding: 6px 12px;
		font-size: 12px; font-weight: 500;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		color: var(--text-secondary);
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.refresh-btn:hover { border-color: var(--accent); color: var(--accent); }

	.search-bar {
		display: flex; align-items: center; gap: 8px;
		padding: 8px 12px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius);
	}
	.search-input {
		flex: 1; border: none; outline: none;
		background: transparent;
		font-size: 13px; color: var(--text-primary);
		font-family: var(--font-sans);
	}
	.search-input::placeholder { color: var(--text-muted); }

	.empty-state {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; padding: 60px 0;
		color: var(--text-muted); font-size: 13px;
	}

	.spinner {
		width: 20px; height: 20px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.8s linear infinite;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	.table-wrap {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	.data-table { width: 100%; border-collapse: collapse; font-size: 13px; }
	.data-table thead th {
		padding: 10px 14px;
		text-align: left;
		font-size: 11px; font-weight: 600; letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--text-muted);
		background: var(--bg-muted);
		border-bottom: 1px solid var(--border);
	}
	.data-table thead th:first-child { width: 32px; }
	.data-row td { padding: 10px 14px; border-bottom: 1px solid var(--border); color: var(--text-primary); vertical-align: middle; }
	.data-row:last-child td { border-bottom: none; }
	.data-row:hover td { background: var(--bg-muted); }
	.data-row.expanded td { background: var(--bg-muted); }

	.expand-cell { cursor: pointer; color: var(--text-muted); width: 32px; }

	.detail-row td { padding: 0; background: var(--bg-base); }
	.detail-grid {
		display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 6px; padding: 12px 14px;
		border-bottom: 1px solid var(--border);
	}
	.detail-kv { display: flex; flex-direction: column; gap: 2px; }
	.detail-k { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
	.detail-v { font-size: 12px; color: var(--text-primary); word-break: break-all; }

	.mono { font-family: var(--font-mono, 'JetBrains Mono', monospace); font-size: 12px; }
	.client-id { max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.topic-cell { max-width: 380px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.ts { white-space: nowrap; color: var(--text-secondary); font-size: 12px; }

	.proto-chip {
		display: inline-block; padding: 2px 7px;
		background: var(--bg-muted); border: 1px solid var(--border);
		border-radius: 4px; font-size: 11px; font-weight: 500; color: var(--text-secondary);
	}

	.qos-chip {
		display: inline-block; padding: 2px 7px;
		border-radius: 4px; font-size: 11px; font-weight: 600;
	}
	.qos-0 { background: #f0fdf4; color: #16a34a; border: 1px solid #bbf7d0; }
	.qos-1 { background: #fffbeb; color: #d97706; border: 1px solid #fde68a; }
	.qos-2 { background: #eff6ff; color: #2563eb; border: 1px solid #bfdbfe; }
</style>
