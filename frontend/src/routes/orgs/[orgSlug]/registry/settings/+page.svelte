<script lang="ts">
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { toastStore } from '$lib/stores/toast.store';
	import { Plus, Trash2, ExternalLink, Copy, Database, Settings2 } from '@lucide/svelte';
	import { onMount } from 'svelte';

	let orgId = $derived($orgStore.activeOrg?.id ?? '');

	// ── State ─────────────────────────────────────────────────────────────────────

	let info:     { hostname: string; storage_type: string } | null = $state(null);
	let externals: { id: string; name: string; registry_url: string; username: string | null }[] = $state([]);

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
			api.get(`/orgs/${orgId}/registry/info`),
			api.get(`/orgs/${orgId}/registry/external-registries`),
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
		const res = await api.post(`/orgs/${orgId}/registry/external-registries`, {
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
		externals    = [...externals, res.data];
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
			<div class="skeleton" style="height:88px"></div>
		{:else if info}
			<div class="info-box">
				<div class="info-row">
					<span class="info-key">Hostname</span>
					<div class="info-val-group">
						<code class="mono">{info.hostname || '(not configured)'}</code>
						{#if info.hostname}
							<button class="copy-link" onclick={() => copyText(info?.hostname ?? '')}>
								<Copy size={11} /> Copy
							</button>
						{/if}
					</div>
				</div>
				<div class="info-row">
					<span class="info-key">Storage Backend</span>
					<span class="badge" class:badge-blue={info.storage_type === 's3'} class:badge-grey={info.storage_type !== 's3'}>
						{info.storage_type === 's3' ? 'S3 / MinIO' : 'Local Disk'}
					</span>
				</div>
				{#if info.hostname}
					<div class="info-row">
						<span class="info-key">Login command</span>
						<div class="info-val-group">
							<code class="mono cmd">docker login {info.hostname}</code>
							<button class="copy-link" onclick={() => copyText(`docker login ${info?.hostname}`)}>
								<Copy size={11} /> Copy
							</button>
						</div>
					</div>
				{/if}
			</div>
		{/if}
	</section>

	<!-- ── External Registries ── -->
	<section class="section">
		<div class="section-hd">
			<Database size={14} />
			<h2>External Registries</h2>
			<button class="add-btn" onclick={() => showAddForm = !showAddForm}>
				<Plus size={13} /> Add Registry
			</button>
		</div>
		<p class="section-desc">
			Connect DockerHub, ECR, GCR, or any Docker-compatible registry. Credentials are encrypted at rest.
		</p>

		{#if showAddForm}
			<div class="form-card">
				<h3 class="form-title">Add External Registry</h3>
				<div class="form-grid">
					<label class="field">
						<span>Name</span>
						<input class="input" placeholder="e.g. DockerHub" bind:value={form.name} />
					</label>
					<label class="field">
						<span>Registry URL</span>
						<input class="input" placeholder="registry-1.docker.io" bind:value={form.registry_url} />
					</label>
					<label class="field">
						<span>Username <em class="opt">(optional)</em></span>
						<input class="input" placeholder="username" bind:value={form.username} />
					</label>
					<label class="field">
						<span>Password / Token <em class="opt">(optional)</em></span>
						<input class="input" type="password" placeholder="••••••••" bind:value={form.password} />
					</label>
				</div>
				<div class="form-actions">
					<button class="btn btn-ghost btn-sm" onclick={() => { showAddForm = false; form = { name: '', registry_url: '', username: '', password: '' }; }}>
						Cancel
					</button>
					<button class="btn btn-primary btn-sm" onclick={addRegistry} disabled={saving || !form.name.trim() || !form.registry_url.trim()}>
						{saving ? 'Saving…' : 'Save Registry'}
					</button>
				</div>
			</div>
		{/if}

		{#if loadingExternals}
			<div class="skeleton" style="height:80px"></div>
		{:else if externals.length === 0 && !showAddForm}
			<div class="empty">
				<Database size={24} />
				<p>No external registries configured.</p>
				<span>Add one to pull images from DockerHub, ECR, GCR, or any private registry.</span>
			</div>
		{:else if externals.length > 0}
			<div class="table-wrap">
				<table class="table">
					<thead>
						<tr>
							<th>Name</th>
							<th>Registry URL</th>
							<th>Username</th>
							<th></th>
						</tr>
					</thead>
					<tbody>
						{#each externals as reg}
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
									<button
										class="danger-btn"
										onclick={() => deleteRegistry(reg.id)}
										disabled={deletingId === reg.id}
										aria-label="Delete registry"
									>
										<Trash2 size={13} />
									</button>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
			</div>
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

	/* ── Section ── */
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

	.add-btn {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 12px;
		font-weight: 500;
		padding: 5px 12px;
		border-radius: 7px;
		border: 1px solid var(--border);
		background: var(--surface);
		color: var(--text-primary);
		cursor: pointer;
		transition: background var(--transition-fast);
	}
	.add-btn:hover { background: var(--surface-2); }

	/* ── Info box ── */
	.info-box {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg, 10px);
		padding: 0 16px;
	}
	.info-row {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 11px 0;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
	}
	.info-row:last-child { border-bottom: none; }
	.info-key {
		width: 140px;
		flex-shrink: 0;
		font-size: 12px;
		font-weight: 500;
		color: var(--text-muted);
	}
	.info-val-group {
		display: flex;
		align-items: center;
		gap: 10px;
	}
	.mono { font-family: var(--font-mono, monospace); font-size: 12px; color: var(--text-primary); }
	.cmd  { background: var(--surface-2); padding: 3px 10px; border-radius: 5px; }

	.copy-link {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 11px;
		font-weight: 500;
		color: var(--accent);
		background: none;
		border: none;
		cursor: pointer;
		padding: 0;
	}
	.copy-link:hover { text-decoration: underline; }

	.badge {
		display: inline-flex;
		align-items: center;
		font-size: 11px;
		font-weight: 600;
		padding: 2px 9px;
		border-radius: 999px;
	}
	.badge-blue { background: rgba(59,130,246,0.1); color: #3b82f6; border: 1px solid rgba(59,130,246,0.2); }
	.badge-grey { background: var(--surface-2); color: var(--text-muted); border: 1px solid var(--border); }

	/* ── Add form ── */
	.form-card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg, 10px);
		padding: 16px 18px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}
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
	.field {
		display: flex;
		flex-direction: column;
		gap: 5px;
		font-size: 12px;
		font-weight: 500;
		color: var(--text-muted);
	}
	.opt { font-style: normal; font-weight: 400; }
	.input {
		padding: 7px 10px;
		font-size: 13px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		border-radius: 7px;
		color: var(--text-primary);
		font-family: inherit;
		transition: border-color var(--transition-fast);
	}
	.input:focus { outline: none; border-color: var(--accent); }
	.form-actions {
		display: flex;
		gap: 8px;
		justify-content: flex-end;
		padding-top: 4px;
	}

	/* ── Table ── */
	.table-wrap {
		border: 1px solid var(--border);
		border-radius: var(--radius-lg, 10px);
		overflow: hidden;
	}
	.table {
		width: 100%;
		border-collapse: collapse;
		font-size: 13px;
	}
	.table th {
		background: var(--surface-2);
		border-bottom: 1px solid var(--border);
		padding: 8px 14px;
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.table td {
		padding: 10px 14px;
		border-bottom: 1px solid var(--border);
		color: var(--text-primary);
		vertical-align: middle;
	}
	.table tr:last-child td { border-bottom: none; }
	.table tr:hover td { background: var(--surface-2); }

	.fw { font-weight: 500; }
	.muted { color: var(--text-muted); font-size: 12px; }
	.action-col { width: 40px; text-align: right; }

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

	.danger-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border-radius: 7px;
		border: none;
		background: none;
		color: var(--text-muted);
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast);
	}
	.danger-btn:hover { background: rgba(239,68,68,0.1); color: #ef4444; }
	.danger-btn:disabled { opacity: 0.4; cursor: not-allowed; }

	/* ── Empty ── */
	.empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 8px;
		padding: 40px 20px;
		color: var(--text-muted);
		background: var(--surface);
		border: 1px dashed var(--border);
		border-radius: var(--radius-lg, 10px);
		text-align: center;
	}
	.empty p    { font-size: 13px; font-weight: 600; margin: 0; color: var(--text-primary); }
	.empty span { font-size: 12px; color: var(--text-muted); max-width: 340px; }

	/* ── Skeleton ── */
	.skeleton {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg, 10px);
		animation: pulse 1.5s ease-in-out infinite;
	}
	@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:0.4} }

	@media (max-width: 600px) {
		.settings-page { padding: 16px; }
		.form-grid { grid-template-columns: 1fr; }
	}
</style>
