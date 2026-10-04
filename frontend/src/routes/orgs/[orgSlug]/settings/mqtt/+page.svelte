<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { Radio, RefreshCw, Users, BookOpen, Rss, ChevronDown, ChevronRight } from '@lucide/svelte';
	import { Button, Badge, Tabs, DataTable, EmptyState, Spinner, SearchInput } from '$lib/components/ui';

	let orgId            = $derived($orgStore.activeOrg?.id ?? '');
	let myRole           = $derived($orgStore.myMembership?.role ?? null);
	let myPerms          = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let canViewMqtt      = $derived(can(myRole, myPerms, perm(orgId, 'settings', 'read')));

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

	let tabItems = $derived([
		{ id: 'clients', label: 'Clients', icon: Users, badge: clients.length > 0 ? String(clients.length) : undefined },
		{ id: 'subscriptions', label: 'Subscriptions', icon: Rss, badge: subscriptions.length > 0 ? String(subscriptions.length) : undefined },
		{ id: 'topics', label: 'Topics', icon: BookOpen, badge: topics.length > 0 ? String(topics.length) : undefined }
	]);

	const qosTone = (q: number) => (q === 1 ? 'yellow' : q === 2 ? 'blue' : 'green') as 'green' | 'yellow' | 'blue';

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

<PermissionDeniedDialog open={membershipLoaded && !!orgId && !canViewMqtt} onDismiss={() => history.back()} />

{#if canViewMqtt}
<div class="mqtt-page">
	<div class="page-toolbar">
		<Tabs tabs={tabItems} value={activeTab} onChange={(id) => switchTab(id as Tab)} />
		<Button variant="secondary" onclick={refresh}>
			<RefreshCw size={14} />
			Refresh
		</Button>
	</div>

	<!-- Clients -->
	{#if activeTab === 'clients'}
		<div class="search-bar"><SearchInput bind:value={clientSearch} placeholder="Filter by client ID, username, IP…" /></div>

		{#if loadingClients}
			<div class="loading"><Spinner size={20} /> Loading clients…</div>
		{:else if filteredClients.length === 0}
			<EmptyState message="No connected clients">
				{#snippet icon()}<Radio size={28} />{/snippet}
			</EmptyState>
		{:else}
			<div class="table-wrap">
				<DataTable
					items={filteredClients}
					rowKey={(c) => c.client_id ?? c.clientid}
					searchable={false}
					emptyMessage="No connected clients"
					columns={[
						{ key: 'expand', label: '', width: '32px' },
						{ key: 'client_id', label: 'Client ID' },
						{ key: 'username', label: 'Username' },
						{ key: 'addr', label: 'Address' },
						{ key: 'protocol', label: 'Protocol' },
						{ key: 'connected', label: 'Connected at' },
						{ key: 'keepalive', label: 'Keep-alive' }
					]}
				>
					{#snippet row(c)}
						{@const id = c.client_id ?? c.clientid ?? '—'}
						{@const expanded = expandedClient === id}
						<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
						<tr class="data-row" onclick={() => expandedClient = expanded ? null : id}>
							<td class="expand-cell">
								<button class="expand-btn" aria-expanded={expanded} aria-label={expanded ? 'Collapse client' : 'Expand client'}>
									{#if expanded}<ChevronDown size={13} />{:else}<ChevronRight size={13} />{/if}
								</button>
							</td>
							<td class="mono client-id">{id}</td>
							<td>{c.username ?? '—'}</td>
							<td class="mono">{c.remote_addr ?? c.ipaddress ?? '—'}</td>
							<td><Badge tone="neutral">MQTT {c.protocol ?? c.mqtt_ver ?? ''}</Badge></td>
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
					{/snippet}
				</DataTable>
			</div>

			<div class="mobile-cards">
				{#each filteredClients as c (c.client_id ?? c.clientid)}
					{@const id = c.client_id ?? c.clientid ?? '—'}
					{@const isExp = expandedClient === id}
					<div class="m-card">
						<button class="m-card-header" aria-expanded={isExp} onclick={() => expandedClient = isExp ? null : id}>
							<span class="m-card-title mono">{id}</span>
							<span class="m-chevron">{#if isExp}<ChevronDown size={14}/>{:else}<ChevronRight size={14}/>{/if}</span>
						</button>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Username</span><span>{c.username ?? '—'}</span></div>
							<div class="m-row"><span class="m-label">Address</span><span class="mono">{c.remote_addr ?? c.ipaddress ?? '—'}</span></div>
							<div class="m-row"><span class="m-label">Protocol</span><Badge tone="neutral">MQTT {c.protocol ?? c.mqtt_ver ?? ''}</Badge></div>
							<div class="m-row"><span class="m-label">Connected</span><span class="ts">{connectedAt(c.connected_at ?? c.created_at)}</span></div>
							<div class="m-row"><span class="m-label">Keep-alive</span><span>{c.keepalive ?? c.keep_alive ?? '—'}s</span></div>
						</div>
						{#if isExp}
							<div class="m-detail">
								{#each Object.entries(c) as [k, v]}
									<div class="detail-kv">
										<span class="detail-k">{k}</span>
										<span class="detail-v mono">{JSON.stringify(v)}</span>
									</div>
								{/each}
							</div>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	{/if}

	<!-- Subscriptions -->
	{#if activeTab === 'subscriptions'}
		<div class="search-bar"><SearchInput bind:value={subSearch} placeholder="Filter by topic or client ID…" /></div>

		{#if loadingSubscriptions}
			<div class="loading"><Spinner size={20} /> Loading subscriptions…</div>
		{:else if filteredSubs.length === 0}
			<EmptyState message="No active subscriptions">
				{#snippet icon()}<Rss size={28} />{/snippet}
			</EmptyState>
		{:else}
			<div class="table-wrap">
				<DataTable
					items={filteredSubs}
					rowKey={(s) => `${s.topic}\u0000${s.client_id ?? s.clientid}\u0000${filteredSubs.indexOf(s)}`}
					searchable={false}
					emptyMessage="No active subscriptions"
					columns={[
						{ key: 'topic', label: 'Topic' },
						{ key: 'client_id', label: 'Client ID' },
						{ key: 'qos', label: 'QoS' },
						{ key: 'no_local', label: 'No-local' },
						{ key: 'rap', label: 'Retain-as-published' }
					]}
				>
					{#snippet row(s)}
						<tr class="data-row">
							<td class="mono topic-cell">{s.topic ?? '—'}</td>
							<td class="mono">{s.client_id ?? s.clientid ?? '—'}</td>
							<td><Badge tone={qosTone(s.qos ?? 0)}>QoS {s.qos ?? 0}</Badge></td>
							<td>{s.no_local ? 'yes' : 'no'}</td>
							<td>{s.retain_as_published ? 'yes' : 'no'}</td>
						</tr>
					{/snippet}
				</DataTable>
			</div>

			<div class="mobile-cards">
				{#each filteredSubs as s}
					<div class="m-card">
						<div class="m-card-header m-card-header--static">
							<span class="m-card-title mono">{s.topic ?? '—'}</span>
							<Badge tone={qosTone(s.qos ?? 0)}>QoS {s.qos ?? 0}</Badge>
						</div>
						<div class="m-rows">
							<div class="m-row"><span class="m-label">Client ID</span><span class="mono">{s.client_id ?? s.clientid ?? '—'}</span></div>
							<div class="m-row"><span class="m-label">No-local</span><span>{s.no_local ? 'yes' : 'no'}</span></div>
							<div class="m-row"><span class="m-label">Retain-as-published</span><span>{s.retain_as_published ? 'yes' : 'no'}</span></div>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	{/if}

	<!-- Topics -->
	{#if activeTab === 'topics'}
		<div class="search-bar"><SearchInput bind:value={topicSearch} placeholder="Filter topics…" /></div>

		{#if loadingTopics}
			<div class="loading"><Spinner size={20} /> Loading topics…</div>
		{:else if filteredTopics.length === 0}
			<EmptyState message="No topics">
				{#snippet icon()}<BookOpen size={28} />{/snippet}
			</EmptyState>
		{:else}
			<div class="table-wrap">
				<DataTable
					items={filteredTopics}
					rowKey={(t) => `${t.topic ?? t.name}\u0000${filteredTopics.indexOf(t)}`}
					searchable={false}
					emptyMessage="No topics"
					columns={[
						{ key: 'topic', label: 'Topic' },
						{ key: 'subs', label: 'Subscribers' }
					]}
				>
					{#snippet row(t)}
						<tr class="data-row">
							<td class="mono topic-cell">{t.topic ?? t.name ?? '—'}</td>
							<td>{t.subscribers_count ?? t.subs_count ?? '—'}</td>
						</tr>
					{/snippet}
				</DataTable>
			</div>

			<div class="mobile-cards">
				{#each filteredTopics as t}
					<div class="m-card">
						<div class="m-card-header m-card-header--static">
							<span class="m-card-title mono">{t.topic ?? t.name ?? '—'}</span>
							<Badge tone="neutral">{t.subscribers_count ?? t.subs_count ?? '—'} sub{(t.subscribers_count ?? t.subs_count ?? 0) === 1 ? '' : 's'}</Badge>
						</div>
					</div>
				{/each}
			</div>
		{/if}
	{/if}
</div>
{/if}

<style>
	.mqtt-page { display: flex; flex-direction: column; gap: 16px; }

	.page-toolbar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		flex-wrap: wrap;
		gap: 12px;
	}

	.search-bar { max-width: 360px; }

	.loading {
		display: flex; align-items: center; justify-content: center;
		gap: 10px; padding: 60px 0;
		color: var(--text-muted); font-size: 13px;
	}

	.expand-cell { width: 32px; }
	.expand-btn {
		display: inline-flex; align-items: center;
		background: none; border: none; padding: 0; cursor: pointer;
		color: var(--text-muted);
	}
	.data-row { cursor: default; }
	.detail-row td { padding: 0; background: var(--bg-base); }
	.detail-grid {
		display: grid; grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
		gap: 6px; padding: 12px 14px;
		border-bottom: 1px solid var(--border);
	}
	.detail-kv { display: flex; flex-direction: column; gap: 2px; }
	.detail-k { font-size: 11px; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
	.detail-v { font-size: 12px; color: var(--text-primary); word-break: break-all; }

	.mono { font-family: var(--font-mono); font-size: 12px; }
	.client-id { max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.topic-cell { max-width: 380px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.ts { white-space: nowrap; color: var(--text-secondary); font-size: 12px; }

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
	.m-card-header--static { cursor: default; }
	.m-card-title {
		font-size: 13px; font-weight: 600; color: var(--text-primary);
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0;
	}
	.m-chevron { flex-shrink: 0; color: var(--text-muted); }
	.m-rows { border-top: 1px solid var(--border); }
	.m-row {
		display: flex; align-items: center; justify-content: space-between;
		padding: 8px 14px; font-size: 12px; color: var(--text-primary);
		border-bottom: 1px solid var(--border); gap: 12px;
	}
	.m-row:last-child { border-bottom: none; }
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
		.table-wrap { display: none; }
		.mobile-cards { display: flex; }
	}
</style>
