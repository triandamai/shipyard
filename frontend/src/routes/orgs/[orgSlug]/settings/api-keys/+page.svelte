<script lang="ts">
	import { onMount } from 'svelte';
	import { Copy, Trash2, Plus, Key, Eye, EyeOff, X, Check, Shield, AlertTriangle } from '@lucide/svelte';
	import api from '$lib/api/client';
	import type { ApiKeyItem, CreatedApiKey, ApiKeyScope } from '$lib/api/types';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let canKeysRead  = $derived(can(myRole, myPerms, perm(orgId, 'keys', 'read')));
	let canKeysWrite = $derived(can(myRole, myPerms, perm(orgId, 'keys', 'write')));

	// ─── State ────────────────────────────────────────────────────────────────
	let keys = $state<ApiKeyItem[]>([]);
	let loading = $state(true);
	let error = $state('');

	// Create form
	let showCreate = $state(false);
	let creating = $state(false);
	let createError = $state('');
	let newName = $state('');
	let newExpiry = $state('');
	let newScopes = $state<ApiKeyScope[]>(['read']);

	// One-time key reveal
	let createdKey = $state<CreatedApiKey | null>(null);
	let keyCopied = $state(false);
	let keyVisible = $state(false);

	// Revoke confirmation
	let revoking = $state<string | null>(null);
	let confirmRevoke = $state<string | null>(null);

	const ALL_SCOPES: { value: ApiKeyScope; label: string; desc: string }[] = [
		{ value: 'read',   label: 'Read',   desc: 'View projects, services, and deployments' },
		{ value: 'deploy', label: 'Deploy', desc: 'Trigger deployments' },
		{ value: 'write',  label: 'Write',  desc: 'Create and update services' },
		{ value: 'admin',  label: 'Admin',  desc: 'Manage API keys and org settings' },
	];

	// ─── Data loading ─────────────────────────────────────────────────────────
	async function load() {
		loading = true;
		error = '';
		try {
			const res = await api.listApiKeys(orgId);
			if (res.data) keys = res.data;
			else error = res.error?.message ?? 'Failed to load API keys';
		} catch {
			error = 'Failed to load API keys';
		} finally {
			loading = false;
		}
	}

	let canKeysAny = $derived(canKeysRead || canKeysWrite);
	onMount(() => { if (canKeysAny) load(); else loading = false; });

	// ─── Helpers ──────────────────────────────────────────────────────────────
	function toggleScope(scope: ApiKeyScope) {
		if (newScopes.includes(scope)) {
			newScopes = newScopes.filter((s) => s !== scope);
		} else {
			newScopes = [...newScopes, scope];
		}
	}

	function relativeTime(iso: string) {
		const diff = Date.now() - new Date(iso).getTime();
		const m = Math.floor(diff / 60000);
		if (m < 1) return 'just now';
		if (m < 60) return `${m}m ago`;
		const h = Math.floor(m / 60);
		if (h < 24) return `${h}h ago`;
		const d = Math.floor(h / 24);
		return `${d}d ago`;
	}

	function formatDate(iso: string) {
		return new Date(iso).toLocaleDateString(undefined, { dateStyle: 'medium' });
	}

	function isExpired(iso: string | null) {
		if (!iso) return false;
		return new Date(iso) < new Date();
	}

	async function copyKey() {
		if (!createdKey) return;
		await navigator.clipboard.writeText(createdKey.key);
		keyCopied = true;
		setTimeout(() => (keyCopied = false), 2000);
	}

	// ─── Create ───────────────────────────────────────────────────────────────
	async function handleCreate() {
		if (!newName.trim()) { createError = 'Name is required'; return; }
		if (newScopes.length === 0) { createError = 'At least one scope is required'; return; }
		creating = true;
		createError = '';
		try {
			const res = await api.createApiKey(orgId, {
				name: newName.trim(),
				scopes: newScopes,
				expires_at: newExpiry ? new Date(newExpiry).toISOString() : null,
			});
			if (res.data) {
				createdKey = res.data;
				keyVisible = false;
				keyCopied = false;
				showCreate = false;
				newName = '';
				newExpiry = '';
				newScopes = ['read'];
				await load();
			} else {
				createError = res.error?.message ?? 'Failed to create key';
			}
		} catch {
			createError = 'Failed to create key';
		} finally {
			creating = false;
		}
	}

	// ─── Revoke ───────────────────────────────────────────────────────────────
	async function handleRevoke(keyId: string) {
		if (confirmRevoke !== keyId) { confirmRevoke = keyId; return; }
		revoking = keyId;
		confirmRevoke = null;
		try {
			await api.revokeApiKey(orgId, keyId);
			await load();
		} catch {
			error = 'Failed to revoke key';
		} finally {
			revoking = null;
		}
	}
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canKeysAny}
	message="You need the 'View API keys' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if canKeysAny}
<div class="page">
	<div class="page-header">
		<div class="header-text">
			<h2>API Keys</h2>
			<p>Manage programmatic access keys for the Shipyard Open API.</p>
		</div>
		<button class="btn-primary" onclick={() => { showCreate = true; createError = ''; }}>
			<Plus size={14} />
			New Key
		</button>
	</div>

	<!-- One-time key reveal modal -->
	{#if createdKey}
		<div class="modal-backdrop" role="dialog" aria-modal="true" aria-label="New API Key Created">
			<div class="modal">
				<div class="modal-header">
					<div class="modal-title-row">
						<Key size={16} />
						<h3>API Key Created</h3>
					</div>
					<button class="icon-btn" onclick={() => (createdKey = null)} aria-label="Close"><X size={16} /></button>
				</div>
				<div class="modal-body">
					<div class="key-alert">
						<AlertTriangle size={14} />
						<span>Copy this key now. It will never be shown again.</span>
					</div>
					<div class="key-display">
						<code class="key-value" class:blurred={!keyVisible}>
							{createdKey.key}
						</code>
						<div class="key-actions">
							<button class="icon-btn" onclick={() => (keyVisible = !keyVisible)} aria-label="Toggle visibility">
								{#if keyVisible}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
							</button>
							<button class="icon-btn" class:copied={keyCopied} onclick={copyKey} aria-label="Copy key">
								{#if keyCopied}<Check size={14} />{:else}<Copy size={14} />{/if}
							</button>
						</div>
					</div>
					<div class="key-meta">
						<div class="meta-row"><span class="meta-label">Name</span><span>{createdKey.name}</span></div>
						<div class="meta-row"><span class="meta-label">Prefix</span><code>{createdKey.key_prefix}</code></div>
						<div class="meta-row">
							<span class="meta-label">Scopes</span>
							<div class="scope-chips">
								{#each createdKey.scopes as s}<span class="scope-chip">{s}</span>{/each}
							</div>
						</div>
						{#if createdKey.expires_at}
							<div class="meta-row"><span class="meta-label">Expires</span><span>{formatDate(createdKey.expires_at)}</span></div>
						{/if}
					</div>
				</div>
				<div class="modal-footer">
					<button class="btn-primary" onclick={copyKey}>
						{#if keyCopied}<Check size={14} /> Copied!{:else}<Copy size={14} /> Copy Key{/if}
					</button>
					<button class="btn-secondary" onclick={() => (createdKey = null)}>Done</button>
				</div>
			</div>
		</div>
	{/if}

	<!-- Create key panel -->
	{#if showCreate}
		<div class="create-panel">
			<div class="panel-header">
				<h3>New API Key</h3>
				<button class="icon-btn" onclick={() => (showCreate = false)} aria-label="Close"><X size={14} /></button>
			</div>
			<div class="panel-body">
				<div class="field">
					<label for="key-name">Name</label>
					<input id="key-name" type="text" bind:value={newName} placeholder="e.g. CI/CD Pipeline" />
				</div>
				<div class="field">
					<label>Scopes</label>
					<div class="scope-list">
						{#each ALL_SCOPES as s}
							<button
								class="scope-toggle"
								class:active={newScopes.includes(s.value)}
								onclick={() => toggleScope(s.value)}
								type="button"
							>
								<div class="scope-toggle-label">
									<Shield size={12} />
									{s.label}
								</div>
								<span class="scope-toggle-desc">{s.desc}</span>
							</button>
						{/each}
					</div>
				</div>
				<div class="field">
					<label for="key-expiry">Expiry (optional)</label>
					<input id="key-expiry" type="date" bind:value={newExpiry} min={new Date().toISOString().slice(0, 10)} />
				</div>
				{#if createError}
					<p class="field-error">{createError}</p>
				{/if}
			</div>
			<div class="panel-footer">
				<button class="btn-primary" onclick={handleCreate} disabled={creating}>
					{creating ? 'Creating…' : 'Create Key'}
				</button>
				<button class="btn-secondary" onclick={() => (showCreate = false)}>Cancel</button>
			</div>
		</div>
	{/if}

	<!-- Keys table -->
	{#if loading}
		<div class="empty-state">Loading…</div>
	{:else if error}
		<div class="error-state">{error}</div>
	{:else if keys.length === 0}
		<div class="empty-state">
			<Key size={32} />
			<p>No API keys yet. Create one to get started.</p>
		</div>
	{:else}
		<div class="table-wrap">
			<table>
				<thead>
					<tr>
						<th>Name</th>
						<th>Prefix</th>
						<th>Scopes</th>
						<th>Last Used</th>
						<th>Expires</th>
						<th>Created</th>
						<th></th>
					</tr>
				</thead>
				<tbody>
					{#each keys as k (k.id)}
						<tr class:expired={isExpired(k.expires_at)}>
							<td class="name-cell">{k.name}</td>
							<td><code class="prefix">{k.key_prefix}…</code></td>
							<td>
								<div class="scope-chips">
									{#each k.scopes as s}<span class="scope-chip">{s}</span>{/each}
								</div>
							</td>
							<td class="muted">{k.last_used_at ? relativeTime(k.last_used_at) : '—'}</td>
							<td class="muted" class:expired-text={isExpired(k.expires_at)}>
								{k.expires_at ? formatDate(k.expires_at) : '—'}
							</td>
							<td class="muted">{formatDate(k.created_at)}</td>
							<td class="action-cell">
								{#if confirmRevoke === k.id}
									<div class="confirm-row">
										<span class="confirm-label">Revoke?</span>
										<button
											class="btn-danger-sm"
											onclick={() => handleRevoke(k.id)}
											disabled={revoking === k.id}
										>
											{revoking === k.id ? '…' : 'Yes'}
										</button>
										<button class="btn-ghost-sm" onclick={() => (confirmRevoke = null)}>No</button>
									</div>
								{:else}
									<button
										class="icon-btn danger"
										onclick={() => handleRevoke(k.id)}
										aria-label="Revoke key"
									>
										<Trash2 size={14} />
									</button>
								{/if}
							</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<!-- Mobile cards -->
		<div class="mobile-cards">
			{#each keys as k (k.id)}
				<div class="card" class:expired={isExpired(k.expires_at)}>
					<div class="card-header">
						<span class="card-name">{k.name}</span>
						<code class="prefix">{k.key_prefix}…</code>
					</div>
					<div class="scope-chips">
						{#each k.scopes as s}<span class="scope-chip">{s}</span>{/each}
					</div>
					<div class="card-rows">
						{#if k.last_used_at}
							<div class="card-row"><span>Last used</span><span>{relativeTime(k.last_used_at)}</span></div>
						{/if}
						{#if k.expires_at}
							<div class="card-row">
								<span>Expires</span>
								<span class:expired-text={isExpired(k.expires_at)}>{formatDate(k.expires_at)}</span>
							</div>
						{/if}
						<div class="card-row"><span>Created</span><span>{formatDate(k.created_at)}</span></div>
					</div>
					<div class="card-footer">
						{#if confirmRevoke === k.id}
							<div class="confirm-row">
								<span class="confirm-label">Revoke this key?</span>
								<button class="btn-danger-sm" onclick={() => handleRevoke(k.id)} disabled={revoking === k.id}>
									{revoking === k.id ? '…' : 'Revoke'}
								</button>
								<button class="btn-ghost-sm" onclick={() => (confirmRevoke = null)}>Cancel</button>
							</div>
						{:else}
							<button class="btn-revoke" onclick={() => handleRevoke(k.id)}>
								<Trash2 size={13} /> Revoke
							</button>
						{/if}
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
{/if}

<style>
	.page {
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	.page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
	}

	.header-text h2 {
		font-size: 16px;
		font-weight: 600;
		color: var(--text-primary);
		margin: 0 0 4px;
	}
	.header-text p {
		font-size: 13px;
		color: var(--text-muted);
		margin: 0;
	}

	/* ── Buttons ── */
	.btn-primary {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		background: var(--accent);
		color: #fff;
		border: none;
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		white-space: nowrap;
	}
	.btn-primary:disabled { opacity: 0.6; cursor: not-allowed; }
	.btn-secondary {
		padding: 7px 14px;
		background: var(--bg-muted);
		color: var(--text-primary);
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
	}
	.icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		cursor: pointer;
		border-radius: 4px;
		transition: background 0.15s, color 0.15s;
	}
	.icon-btn:hover { background: var(--bg-muted); color: var(--text-primary); }
	.icon-btn.danger:hover { background: rgba(239,68,68,.1); color: #ef4444; }
	.icon-btn.copied { color: #16a34a; }
	.btn-danger-sm {
		padding: 3px 10px;
		background: #ef4444;
		color: #fff;
		border: none;
		border-radius: 4px;
		font-size: 12px;
		cursor: pointer;
	}
	.btn-danger-sm:disabled { opacity: 0.6; cursor: not-allowed; }
	.btn-ghost-sm {
		padding: 3px 10px;
		background: transparent;
		color: var(--text-muted);
		border: 1px solid var(--border);
		border-radius: 4px;
		font-size: 12px;
		cursor: pointer;
	}
	.btn-revoke {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 4px 10px;
		background: transparent;
		color: #ef4444;
		border: 1px solid rgba(239,68,68,.3);
		border-radius: 4px;
		font-size: 12px;
		cursor: pointer;
	}

	/* ── Modal ── */
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0,0,0,.45);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 200;
		padding: 16px;
	}
	.modal {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 10px;
		width: 100%;
		max-width: 480px;
		overflow: hidden;
		box-shadow: 0 20px 60px rgba(0,0,0,.25);
	}
	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px;
		border-bottom: 1px solid var(--border);
	}
	.modal-title-row {
		display: flex;
		align-items: center;
		gap: 8px;
		color: var(--text-primary);
	}
	.modal-title-row h3 { margin: 0; font-size: 15px; font-weight: 600; }
	.modal-body { padding: 20px; display: flex; flex-direction: column; gap: 16px; }
	.modal-footer {
		padding: 12px 20px;
		border-top: 1px solid var(--border);
		display: flex;
		gap: 8px;
	}

	.key-alert {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
		background: rgba(245,158,11,.08);
		border: 1px solid rgba(245,158,11,.3);
		border-radius: 6px;
		font-size: 13px;
		color: #b45309;
	}
	.key-display {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
		background: var(--bg-muted);
		border: 1px solid var(--border);
		border-radius: 6px;
	}
	.key-value {
		flex: 1;
		font-size: 12px;
		font-family: var(--font-mono);
		word-break: break-all;
		color: var(--text-primary);
		transition: filter 0.2s;
	}
	.key-value.blurred { filter: blur(4px); user-select: none; }
	.key-actions { display: flex; gap: 4px; flex-shrink: 0; }

	.key-meta { display: flex; flex-direction: column; gap: 8px; }
	.meta-row {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
	}
	.meta-label { color: var(--text-muted); min-width: 60px; }

	/* ── Create panel ── */
	.create-panel {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	.panel-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
	}
	.panel-header h3 { margin: 0; font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.panel-body { padding: 16px; display: flex; flex-direction: column; gap: 14px; }
	.panel-footer {
		padding: 12px 16px;
		border-top: 1px solid var(--border);
		display: flex;
		gap: 8px;
	}
	.field { display: flex; flex-direction: column; gap: 6px; }
	.field label { font-size: 12px; font-weight: 500; color: var(--text-muted); }
	.field input {
		padding: 7px 10px;
		background: var(--bg-input, var(--bg-muted));
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 13px;
		color: var(--text-primary);
		outline: none;
		width: 100%;
	}
	.field input:focus { border-color: var(--accent); }
	.field-error { font-size: 12px; color: #ef4444; margin: 0; }

	.scope-list { display: flex; flex-direction: column; gap: 6px; }
	.scope-toggle {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 12px;
		background: var(--bg-muted);
		border: 1px solid var(--border);
		border-radius: 6px;
		cursor: pointer;
		text-align: left;
		transition: border-color 0.15s, background 0.15s;
	}
	.scope-toggle.active { border-color: var(--accent); background: rgba(var(--accent-rgb, 99,102,241),.06); }
	.scope-toggle-label {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 13px;
		font-weight: 500;
		color: var(--text-primary);
	}
	.scope-toggle-desc { font-size: 11px; color: var(--text-muted); }

	/* ── Table ── */
	.table-wrap {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}
	table { width: 100%; border-collapse: collapse; font-size: 13px; }
	thead { background: var(--bg-muted); }
	th {
		padding: 10px 14px;
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	td { padding: 11px 14px; border-top: 1px solid var(--border); vertical-align: middle; }
	tr.expired td { opacity: 0.5; }
	.name-cell { font-weight: 500; color: var(--text-primary); }
	.muted { color: var(--text-muted); }
	.expired-text { color: #ef4444; }
	.prefix { font-family: var(--font-mono); font-size: 12px; }
	.action-cell { text-align: right; }
	.confirm-row { display: flex; align-items: center; gap: 6px; justify-content: flex-end; }
	.confirm-label { font-size: 12px; color: var(--text-muted); }

	/* ── Scope chips ── */
	.scope-chips { display: flex; flex-wrap: wrap; gap: 4px; }
	.scope-chip {
		padding: 2px 7px;
		background: rgba(var(--accent-rgb, 99,102,241),.08);
		color: var(--accent);
		border: 1px solid rgba(var(--accent-rgb, 99,102,241),.2);
		border-radius: 10px;
		font-size: 11px;
		font-weight: 500;
	}

	/* ── Empty / error states ── */
	.empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		padding: 48px 16px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.error-state {
		padding: 16px;
		background: rgba(239,68,68,.08);
		border: 1px solid rgba(239,68,68,.2);
		border-radius: 8px;
		color: #ef4444;
		font-size: 13px;
	}

	/* ── Mobile cards ── */
	.mobile-cards { display: none; flex-direction: column; gap: 10px; }
	.card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 8px;
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.card.expired { opacity: 0.55; }
	.card-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}
	.card-name { font-weight: 600; font-size: 14px; color: var(--text-primary); }
	.card-rows { display: flex; flex-direction: column; gap: 6px; }
	.card-row {
		display: flex;
		justify-content: space-between;
		font-size: 12px;
		color: var(--text-muted);
	}
	.card-row span:last-child { color: var(--text-primary); }
	.card-footer { padding-top: 4px; border-top: 1px solid var(--border); }

	@media (max-width: 639px) {
		.table-wrap { display: none; }
		.mobile-cards { display: flex; }
		.page-header { flex-direction: column; gap: 10px; }
		.page-header .btn-primary { align-self: flex-start; }
	}
</style>
