<script lang="ts">
	import { onMount } from 'svelte';
	import { Copy, Trash2, Plus, Key, Eye, EyeOff, Check, X } from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import FormField from '$lib/components/ui/FormField.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';
	import Checkbox from '$lib/components/ui/Checkbox.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import api from '$lib/api/client';
	import type { ApiKeyItem, CreatedApiKey, ApiKeyScope } from '$lib/api/types';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';

	let orgId    = $derived($orgStore.activeOrg?.id ?? '');
	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let myPerms  = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let canKeysRead  = $derived(
		can(myRole, myPerms, perm(orgId, 'keys', 'read')) ||
		can(myRole, myPerms, perm(orgId, 'settings', 'read'))
	);
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

	const ALL_SCOPES: { value: ApiKeyScope; label: string; desc: string; group?: string }[] = [
		{ value: 'read',             label: 'Read',            desc: 'View projects, services, and deployments' },
		{ value: 'deploy',           label: 'Deploy',          desc: 'Trigger deployments' },
		{ value: 'write',            label: 'Write',           desc: 'Create and update services' },
		{ value: 'admin',            label: 'Admin',           desc: 'Manage API keys and org settings' },
		{ value: 'registry:view',    label: 'Registry — View',    desc: 'Pull images and browse artifacts  (shipyard:<org>:registry:view)',    group: 'registry' },
		{ value: 'registry:manage',  label: 'Registry — Manage',  desc: 'Push and delete artifacts  (shipyard:<org>:registry:manage)',          group: 'registry' },
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
		try {
			await api.revokeApiKey(orgId, keyId);
			await load();
		} catch {
			error = 'Failed to revoke key';
		} finally {
			revoking = null;
			confirmRevoke = null;
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
		<Button onclick={() => { showCreate = true; createError = ''; }}>
			<Plus size={14} />
			New Key
		</Button>
	</div>

	<!-- One-time key reveal modal -->
	<Modal
		bind:open={() => createdKey !== null, (v) => { if (!v) createdKey = null; }}
		title="API Key Created"
		dismissible={false}
	>
		{#if createdKey}
			<div class="reveal-body">
				<InlineAlert tone="warning">Copy this key now. It will never be shown again.</InlineAlert>
				<div class="key-display">
					<code class="key-value" class:blurred={!keyVisible}>
						{createdKey.key}
					</code>
					<div class="key-actions">
						<Button variant="ghost" size="icon" onclick={() => (keyVisible = !keyVisible)} aria-label="Toggle visibility">
							{#if keyVisible}<EyeOff size={14} />{:else}<Eye size={14} />{/if}
						</Button>
						<Button variant="ghost" size="icon" onclick={copyKey} aria-label="Copy key">
							{#if keyCopied}<Check size={14} />{:else}<Copy size={14} />{/if}
						</Button>
					</div>
				</div>
				<div class="key-meta">
					<div class="meta-row"><span class="meta-label">Name</span><span>{createdKey.name}</span></div>
					<div class="meta-row"><span class="meta-label">Prefix</span><code>{createdKey.key_prefix}</code></div>
					<div class="meta-row">
						<span class="meta-label">Scopes</span>
						<div class="scope-chips">
							{#each createdKey.scopes as s}<Badge tone="blue">{s}</Badge>{/each}
						</div>
					</div>
					{#if createdKey.expires_at}
						<div class="meta-row"><span class="meta-label">Expires</span><span>{formatDate(createdKey.expires_at)}</span></div>
					{/if}
				</div>
			</div>
		{/if}
		{#snippet footer()}
			<Button onclick={copyKey}>
				{#if keyCopied}<Check size={14} /> Copied!{:else}<Copy size={14} /> Copy Key{/if}
			</Button>
			<Button variant="secondary" onclick={() => (createdKey = null)}>Done</Button>
		{/snippet}
	</Modal>

	<!-- Create key panel -->
	{#if showCreate}
		<Card padding="0">
			<div class="panel-header">
				<h3>New API Key</h3>
				<Button variant="ghost" size="icon" onclick={() => (showCreate = false)} aria-label="Close"><X size={14} /></Button>
			</div>
			<div class="panel-body">
				<FormField label="Name" for="key-name">
					<TextField id="key-name" type="text" bind:value={newName} placeholder="e.g. CI/CD Pipeline" />
				</FormField>
				<FormField label="Scopes">
					<div class="scope-list">
						{#each ALL_SCOPES as s, i}
							{#if i > 0 && s.group === 'registry' && ALL_SCOPES[i - 1].group !== 'registry'}
								<div class="scope-divider">
									<span>Registry permissions</span>
								</div>
							{/if}
							<div class="scope-toggle" class:active={newScopes.includes(s.value)}>
								<Checkbox
									checked={newScopes.includes(s.value)}
									label={s.label}
									onchange={() => toggleScope(s.value)}
								/>
								<span class="scope-toggle-desc">{s.desc}</span>
							</div>
						{/each}
					</div>
				</FormField>
				<FormField label="Expiry (optional)" for="key-expiry">
					<TextField id="key-expiry" type="date" bind:value={newExpiry} min={new Date().toISOString().slice(0, 10)} />
				</FormField>
				{#if createError}
					<div role="alert"><InlineAlert tone="error">{createError}</InlineAlert></div>
				{/if}
			</div>
			<div class="panel-footer">
				<Button onclick={handleCreate} disabled={creating}>
					{creating ? 'Creating…' : 'Create Key'}
				</Button>
				<Button variant="secondary" onclick={() => (showCreate = false)}>Cancel</Button>
			</div>
		</Card>
	{/if}

	<!-- Revoke confirmation -->
	<ConfirmDialog
		bind:open={() => confirmRevoke !== null, (v) => { if (!v) confirmRevoke = null; }}
		title="Revoke API key"
		message="Revoke this key? Anything using it will stop working."
		confirmLabel="Revoke"
		onConfirm={() => { if (confirmRevoke) return handleRevoke(confirmRevoke); }}
	/>

	<!-- Keys table -->
	{#if loading}
		<div class="loading-state"><Spinner size={16} /> Loading…</div>
	{:else if error}
		<div role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{:else if keys.length === 0}
		<EmptyState message="No API keys yet. Create one to get started.">
			{#snippet icon()}<Key size={32} />{/snippet}
		</EmptyState>
	{:else}
		<div class="table-wrap">
			<DataTable
				items={keys}
				rowKey={(k) => k.id}
				columns={[
					{ key: 'name', label: 'Name' },
					{ key: 'prefix', label: 'Prefix' },
					{ key: 'scopes', label: 'Scopes' },
					{ key: 'last_used', label: 'Last Used' },
					{ key: 'expires', label: 'Expires' },
					{ key: 'created', label: 'Created' },
					{ key: 'actions', label: '', width: '60px' }
				]}
				searchable={false}
				emptyMessage="No API keys."
			>
				{#snippet row(k)}
					<tr class={isExpired(k.expires_at) ? 'expired' : ''}>
						<td class="name-cell">{k.name}</td>
						<td><code class="prefix">{k.key_prefix}…</code></td>
						<td>
							<div class="scope-chips">
								{#each k.scopes as s}<Badge tone="blue">{s}</Badge>{/each}
							</div>
						</td>
						<td class="muted">{k.last_used_at ? relativeTime(k.last_used_at) : '—'}</td>
						<td class="muted" class:expired-text={isExpired(k.expires_at)}>
							{k.expires_at ? formatDate(k.expires_at) : '—'}
						</td>
						<td class="muted">{formatDate(k.created_at)}</td>
						<td class="action-cell">
							<Button
								variant="danger-outline"
								size="icon"
								disabled={revoking === k.id}
								onclick={() => handleRevoke(k.id)}
								aria-label="Revoke key"
								title="Revoke key"
							>
								<Trash2 size={14} />
							</Button>
						</td>
					</tr>
				{/snippet}
			</DataTable>
		</div>

		<!-- Mobile cards -->
		<div class="mobile-cards">
			{#each keys as k (k.id)}
				<div class={isExpired(k.expires_at) ? 'key-card expired' : 'key-card'}>
					<Card padding="14px">
						<div class="card-inner">
							<div class="card-header">
								<span class="card-name">{k.name}</span>
								<code class="prefix">{k.key_prefix}…</code>
							</div>
							<div class="scope-chips">
								{#each k.scopes as s}<Badge tone="blue">{s}</Badge>{/each}
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
								<Button variant="danger-outline" size="sm" disabled={revoking === k.id} onclick={() => handleRevoke(k.id)}>
									<Trash2 size={13} /> Revoke
								</Button>
							</div>
						</div>
					</Card>
				</div>
			{/each}
		</div>
	{/if}
</div>
{/if}

<style>
	.page { display: flex; flex-direction: column; gap: 20px; }

	.page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
	}
	.header-text h2 { font-size: 16px; font-weight: 600; color: var(--text-primary); margin: 0 0 4px; }
	.header-text p { font-size: 13px; color: var(--text-muted); margin: 0; }

	.loading-state {
		display: flex; align-items: center; justify-content: center; gap: 8px;
		padding: 48px 16px; color: var(--text-muted); font-size: 13px;
	}

	/* ── Key reveal ── */
	.reveal-body { display: flex; flex-direction: column; gap: 16px; }
	.key-display {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
	}
	.key-value {
		flex: 1;
		font-size: 12px;
		font-family: var(--font-mono);
		word-break: break-all;
		color: var(--text-primary);
		transition: filter var(--transition-normal);
	}
	.key-value.blurred { filter: blur(4px); user-select: none; }
	.key-actions { display: flex; gap: 4px; flex-shrink: 0; }
	.key-meta { display: flex; flex-direction: column; gap: 8px; }
	.meta-row { display: flex; align-items: center; gap: 8px; font-size: 13px; color: var(--text-primary); }
	.meta-label { color: var(--text-muted); min-width: 60px; }

	/* ── Create panel ── */
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

	.scope-list { display: flex; flex-direction: column; gap: 6px; }
	.scope-divider { display: flex; align-items: center; gap: 8px; margin: 4px 0 2px; }
	.scope-divider::before, .scope-divider::after { content: ''; flex: 1; height: 1px; background: var(--border); }
	.scope-divider span {
		font-size: 10px; font-weight: 600; color: var(--text-muted);
		text-transform: uppercase; letter-spacing: 0.06em; white-space: nowrap;
	}
	.scope-toggle {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 8px 12px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		transition: border-color var(--transition-fast), background var(--transition-fast);
	}
	.scope-toggle.active { border-color: var(--accent); background: var(--accent-muted); }
	.scope-toggle-desc { font-size: 11px; color: var(--text-muted); text-align: right; }

	/* ── Table ── */
	.table-wrap { display: block; }
	:global(.table-wrap tr.expired td) { opacity: 0.5; }
	.name-cell { font-weight: 500; color: var(--text-primary); }
	.muted { color: var(--text-muted); }
	.expired-text { color: var(--accent-red); }
	.prefix { font-family: var(--font-mono); font-size: 12px; }
	.action-cell { text-align: right; }
	.scope-chips { display: flex; flex-wrap: wrap; gap: 4px; }

	/* ── Mobile cards ── */
	.mobile-cards { display: none; flex-direction: column; gap: 10px; }
	.key-card.expired { opacity: 0.55; }
	.card-inner { display: flex; flex-direction: column; gap: 10px; }
	.card-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	.card-name { font-weight: 600; font-size: 14px; color: var(--text-primary); }
	.card-rows { display: flex; flex-direction: column; gap: 6px; }
	.card-row { display: flex; justify-content: space-between; font-size: 12px; color: var(--text-muted); }
	.card-row span:last-child { color: var(--text-primary); }
	.card-footer { padding-top: 8px; border-top: 1px solid var(--border); }

	@media (max-width: 639px) {
		.table-wrap { display: none; }
		.mobile-cards { display: flex; }
		.page-header { flex-direction: column; gap: 10px; }
		.scope-toggle { flex-direction: column; align-items: flex-start; gap: 4px; }
		.scope-toggle-desc { text-align: left; }
	}
</style>
