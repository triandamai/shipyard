<script lang="ts">
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { toastStore } from '$lib/stores/toast.store';
	import { Plus, Trash2, ExternalLink, Copy, Database, Settings2 } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import { Button, Badge, Card, DataTable, EmptyState, FormField, KeyValueList, Skeleton, TextField } from '$lib/components/ui';
	import type { KeyValueItem } from '$lib/components/ui';

	let orgId = $derived($orgStore.activeOrg?.id ?? '');

	// ── State ─────────────────────────────────────────────────────────────────────

	type RegistryInfo = { hostname: string; storage_type: string };
	type ExternalRegistry = { id: string; name: string; registry_url: string; username: string | null };

	let info:     RegistryInfo | null = $state(null);
	let externals: ExternalRegistry[] = $state([]);

	let loadingInfo      = $state(true);
	let loadingExternals = $state(true);
	let showAddForm      = $state(false);
	let deletingId: string | null = $state(null);

	let form   = $state({ name: '', registry_url: '', username: '', password: '' });
	let saving = $state(false);

	// ── Load ─────────────────────────────────────────────────────────────────────

	async function loadAll() {
		if (!orgId) return;
		loadingInfo      = true;
		loadingExternals = true;

		const [infoRes, extRes] = await Promise.all([
			api.get<RegistryInfo>(`/orgs/${orgId}/registry/info`),
			api.get<ExternalRegistry[]>(`/orgs/${orgId}/registry/external-registries`),
		]);

		info      = infoRes.data  ?? null;
		externals = extRes.data   ?? [];

		loadingInfo      = false;
		loadingExternals = false;
	}

	onMount(loadAll);
	$effect(() => { if (orgId) loadAll(); });

	// ── CRUD ─────────────────────────────────────────────────────────────────────

	async function addRegistry() {
		if (!form.name.trim() || !form.registry_url.trim()) return;
		saving = true;
		const res = await api.post<ExternalRegistry>(`/orgs/${orgId}/registry/external-registries`, {
			name:         form.name.trim(),
			registry_url: form.registry_url.trim(),
			username:     form.username.trim() || undefined,
			password:     form.password || undefined,
		});
		saving = false;
		if (res.error) {
			toastStore.add({ type: 'error', title: 'Failed', message: res.error.message });
			return;
		}
		externals    = [...externals, res.data as ExternalRegistry];
		form         = { name: '', registry_url: '', username: '', password: '' };
		showAddForm  = false;
		toastStore.add({ type: 'success', title: 'Registry added', message: res.data?.name });
	}

	async function deleteRegistry(id: string) {
		deletingId = id;
		await api.delete(`/orgs/${orgId}/registry/external-registries/${id}`);
		externals  = externals.filter(r => r.id !== id);
		deletingId = null;
	}

	let infoItems = $derived.by((): KeyValueItem[] => {
		const i = info as RegistryInfo | null;
		if (!i) return [];
		return [
			{ key: 'Hostname', value: i.hostname || '(not configured)', mono: true },
			{ key: 'Storage Backend', value: i.storage_type },
			...(i.hostname ? [{ key: 'Login command', value: `docker login ${i.hostname}`, mono: true }] : [])
		];
	});

	function copyText(text: string) {
		navigator.clipboard.writeText(text);
		toastStore.add({ type: 'success', title: 'Copied', message: text });
	}
</script>

<div class="settings-page">
	<!-- ── Registry Info ── -->
	<section class="section">
		<div class="section-hd">
			<Settings2 size={14} />
			<h2>Registry Configuration</h2>
		</div>

		{#if loadingInfo}
			<Skeleton variant="card" height="88px" />
		{:else if info}
			{@const inf = info as RegistryInfo}
			<KeyValueList items={infoItems} keyWidth="140px">
				{#snippet value(item)}
					{#if item.key === 'Storage Backend'}
						<Badge tone={inf.storage_type === 's3' ? 'blue' : 'neutral'}>
							{inf.storage_type === 's3' ? 'S3 / MinIO' : 'Local Disk'}
						</Badge>
					{:else}
						<span class="kv-text">{item.value}</span>
					{/if}
				{/snippet}
				{#snippet action(item)}
					{#if item.key === 'Hostname' && inf.hostname}
						<Button variant="ghost" size="sm" onclick={() => copyText(inf.hostname)}>
							<Copy size={11} /> Copy
						</Button>
					{:else if item.key === 'Login command'}
						<Button variant="ghost" size="sm" onclick={() => copyText(`docker login ${inf.hostname}`)}>
							<Copy size={11} /> Copy
						</Button>
					{/if}
				{/snippet}
			</KeyValueList>
		{/if}
	</section>

	<!-- ── External Registries ── -->
	<section class="section">
		<div class="section-hd">
			<Database size={14} />
			<h2>External Registries</h2>
			<Button variant="secondary" size="sm" onclick={() => showAddForm = !showAddForm}>
				<Plus size={13} /> Add Registry
			</Button>
		</div>
		<p class="section-desc">
			Connect DockerHub, ECR, GCR, or any Docker-compatible registry. Credentials are encrypted at rest.
		</p>

		{#if showAddForm}
			<Card padding="16px 18px">
				<div class="form-card">
					<h3 class="form-title">Add External Registry</h3>
					<div class="form-grid">
						<FormField label="Name">
							<TextField placeholder="e.g. DockerHub" bind:value={form.name} />
						</FormField>
						<FormField label="Registry URL">
							<TextField placeholder="registry-1.docker.io" bind:value={form.registry_url} />
						</FormField>
						<FormField label="Username (optional)">
							<TextField placeholder="username" bind:value={form.username} />
						</FormField>
						<FormField label="Password / Token (optional)">
							<TextField type="password" placeholder="••••••••" bind:value={form.password} />
						</FormField>
					</div>
					<div class="form-actions">
						<Button variant="ghost" size="sm" onclick={() => { showAddForm = false; form = { name: '', registry_url: '', username: '', password: '' }; }}>
							Cancel
						</Button>
						<Button size="sm" onclick={addRegistry} disabled={saving || !form.name.trim() || !form.registry_url.trim()}>
							{saving ? 'Saving…' : 'Save Registry'}
						</Button>
					</div>
				</div>
			</Card>
		{/if}

		{#if loadingExternals}
			<Skeleton variant="card" height="80px" />
		{:else if externals.length === 0 && !showAddForm}
			<EmptyState
				message="No external registries configured."
				sub="Add one to pull images from DockerHub, ECR, GCR, or any private registry."
			>
				{#snippet icon()}<Database size={24} />{/snippet}
			</EmptyState>
		{:else if externals.length > 0}
			<DataTable
				items={externals}
				rowKey={(r: ExternalRegistry) => r.id}
				searchable={false}
				columns={[
					{ key: 'name', label: 'Name' },
					{ key: 'registry_url', label: 'Registry URL' },
					{ key: 'username', label: 'Username' },
					{ key: 'actions', label: '', width: '48px' }
				]}
			>
				{#snippet row(reg)}
					<tr>
						<td class="fw">{reg.name}</td>
						<td>
							<div class="url-cell">
								<span class="mono">{reg.registry_url}</span>
								<a class="ext-link" href="https://{reg.registry_url}" target="_blank" rel="noopener noreferrer" aria-label="Open registry">
									<ExternalLink size={11} />
								</a>
							</div>
						</td>
						<td class="muted">{reg.username ?? '—'}</td>
						<td class="action-col">
							<Button
								variant="ghost"
								size="icon"
								aria-label="Delete registry"
								title="Delete registry"
								onclick={() => deleteRegistry(reg.id)}
								disabled={deletingId === reg.id}
							>
								<Trash2 size={13} />
							</Button>
						</td>
					</tr>
				{/snippet}
			</DataTable>
		{/if}
	</section>
</div>

<style>
	.settings-page {
		padding: 20px 32px 40px;
		display: flex;
		flex-direction: column;
		gap: 32px;
		max-width: 760px;
	}

	.section { display: flex; flex-direction: column; gap: 12px; }
	.section-hd {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-muted);
	}
	.section-hd h2 {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
		margin: 0;
		flex: 1;
	}
	.section-desc {
		font-size: 13px;
		color: var(--text-muted);
		margin: -4px 0 0;
		line-height: 1.5;
	}

	.kv-text { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }

	.form-card { display: flex; flex-direction: column; gap: 14px; }
	.form-title {
		font-size: 13px;
		font-weight: 600;
		margin: 0;
		color: var(--text-primary);
	}
	.form-grid {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 12px;
	}
	.form-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
		padding-top: 4px;
	}

	.fw { font-weight: 500; color: var(--text-primary); }
	.muted { color: var(--text-muted); font-size: 12px; }
	.action-col { text-align: right; }
	.mono { font-family: var(--font-mono); font-size: 12px; color: var(--text-primary); }

	.url-cell {
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.ext-link {
		color: var(--text-muted);
		display: inline-flex;
		align-items: center;
	}
	.ext-link:hover { color: var(--accent); }

	@media (max-width: 600px) {
		.settings-page { padding: 16px; }
		.form-grid { grid-template-columns: 1fr; }
	}
</style>
