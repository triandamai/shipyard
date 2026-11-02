<script lang="ts">
	import { onMount } from 'svelte';
	import {
		Blocks,
		Key,
		Plus,
		Trash2,
		Copy,
		Check,
		RefreshCw,
		Building2,
		Eye,
		EyeOff,
		Link as LinkIcon,
		Unlink
	} from '@lucide/svelte';
	import {
		Button,
		Badge,
		Spinner,
		TextField,
		Checkbox,
		FormField,
		PageHeader,
		EmptyState,
		InlineAlert,
		Modal,
		ConfirmDialog
	} from '$lib/components/ui';
	import api from '$lib/api/client';
	import type { Integration, CreatedIntegrationKey, AdminOrg } from '$lib/api/types';

	// ─── State ────────────────────────────────────────────────────────────────
	let integrations = $state<Integration[]>([]);
	let allOrgs = $state<AdminOrg[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	// Create Integration Modal
	let showCreateModal = $state(false);
	let newIntegrationName = $state('');
	let creatingIntegration = $state(false);
	let createIntegrationError = $state('');

	// Mint Key Modal
	let showMintKeyModal = $state(false);
	let targetIntegrationId = $state<string | null>(null);
	let targetIntegrationName = $state<string>('');
	let newKeyName = $state('');
	let keyScopeManage = $state(true);
	let keyScopeReveal = $state(false);
	let mintingKey = $state(false);
	let mintKeyError = $state('');

	// One-time Key Reveal Modal
	let showCreatedKeyModal = $state(false);
	let createdKeyInfo = $state<CreatedIntegrationKey | null>(null);
	let keyRevealed = $state(false);

	// Link Org Modal
	let showLinkOrgModal = $state(false);
	let linkTargetIntegrationId = $state<string | null>(null);
	let selectedOrgId = $state('');
	let externalId = $state('');
	let linkingOrg = $state(false);
	let linkOrgError = $state('');

	// Confirm Dialogs
	let deleteDialogOpen = $state(false);
	let deleteIntegrationId = $state<string | null>(null);
	let deleteIntegrationName = $state<string>('');

	let revokeDialogOpen = $state(false);
	let revokeTarget = $state<{ integrationId: string; keyId: string; keyName: string } | null>(null);

	let unlinkDialogOpen = $state(false);
	let unlinkTarget = $state<{ integrationId: string; orgId: string; orgName: string } | null>(null);

	// Copied feedback for IDs / prefixes
	let copiedId = $state<string | null>(null);

	// ─── Lifecycle ────────────────────────────────────────────────────────────
	onMount(() => {
		void loadData();
	});

	async function loadData() {
		loading = true;
		error = null;
		try {
			const [integrationsRes, orgsRes] = await Promise.all([
				api.getAdminIntegrations(),
				api.getAdminOrgs()
			]);

			if (integrationsRes.data) {
				integrations = integrationsRes.data;
			} else {
				error = integrationsRes.error?.message ?? 'Failed to load integrations';
			}

			if (orgsRes.data) {
				allOrgs = orgsRes.data;
			}
		} catch (e: any) {
			error = e?.message ?? 'Failed to load integrations';
		} finally {
			loading = false;
		}
	}

	function formatDate(iso: string) {
		return new Date(iso).toLocaleDateString(undefined, {
			year: 'numeric',
			month: 'short',
			day: 'numeric'
		});
	}

	async function copyToClipboard(text: string, id: string) {
		try {
			await navigator.clipboard.writeText(text);
			copiedId = id;
			setTimeout(() => {
				if (copiedId === id) copiedId = null;
			}, 2000);
		} catch {
			// ignore
		}
	}

	// ─── Integration Actions ──────────────────────────────────────────────────
	function openCreateModal() {
		newIntegrationName = '';
		createIntegrationError = '';
		showCreateModal = true;
	}

	async function handleCreateIntegration() {
		const name = newIntegrationName.trim();
		if (!name) {
			createIntegrationError = 'Name is required';
			return;
		}

		creatingIntegration = true;
		createIntegrationError = '';
		try {
			const res = await api.createAdminIntegration({ name });
			if (res.data) {
				showCreateModal = false;
				await loadData();
			} else {
				createIntegrationError = res.error?.message ?? 'Failed to create integration';
			}
		} catch (e: any) {
			createIntegrationError = e?.message ?? 'Failed to create integration';
		} finally {
			creatingIntegration = false;
		}
	}

	function confirmDeleteIntegration(id: string, name: string) {
		deleteIntegrationId = id;
		deleteIntegrationName = name;
		deleteDialogOpen = true;
	}

	async function handleDeleteIntegration() {
		if (!deleteIntegrationId) return;
		try {
			await api.deleteAdminIntegration(deleteIntegrationId);
			deleteDialogOpen = false;
			deleteIntegrationId = null;
			await loadData();
		} catch (e: any) {
			error = e?.message ?? 'Failed to delete integration';
		}
	}

	// ─── Key Actions ──────────────────────────────────────────────────────────
	function openMintKeyModal(integration: Integration) {
		targetIntegrationId = integration.id;
		targetIntegrationName = integration.name;
		newKeyName = `${integration.name}-agent`;
		keyScopeManage = true;
		keyScopeReveal = false;
		mintKeyError = '';
		showMintKeyModal = true;
	}

	async function handleMintKey() {
		if (!targetIntegrationId) return;
		const name = newKeyName.trim();
		if (!name) {
			mintKeyError = 'Key name is required';
			return;
		}

		const scopes: string[] = [];
		if (keyScopeManage) scopes.push('manage');
		if (keyScopeReveal) scopes.push('secrets:reveal');

		if (scopes.length === 0) {
			mintKeyError = 'Select at least one scope';
			return;
		}

		mintingKey = true;
		mintKeyError = '';
		try {
			const res = await api.createAdminIntegrationKey(targetIntegrationId, { name, scopes });
			if (res.data) {
				showMintKeyModal = false;
				createdKeyInfo = res.data;
				keyRevealed = false;
				showCreatedKeyModal = true;
				await loadData();
			} else {
				mintKeyError = res.error?.message ?? 'Failed to create key';
			}
		} catch (e: any) {
			mintKeyError = e?.message ?? 'Failed to create key';
		} finally {
			mintingKey = false;
		}
	}

	function openRevokeDialog(integrationId: string, keyId: string, keyName: string) {
		revokeTarget = { integrationId, keyId, keyName };
		revokeDialogOpen = true;
	}

	async function handleRevokeKey() {
		if (!revokeTarget) return;
		try {
			await api.revokeAdminIntegrationKey(revokeTarget.integrationId, revokeTarget.keyId);
			revokeDialogOpen = false;
			revokeTarget = null;
			await loadData();
		} catch (e: any) {
			error = e?.message ?? 'Failed to revoke key';
		}
	}

	// ─── Org Linking Actions ──────────────────────────────────────────────────
	function openLinkOrgModal(integration: Integration) {
		linkTargetIntegrationId = integration.id;
		selectedOrgId = allOrgs[0]?.id ?? '';
		externalId = '';
		linkOrgError = '';
		showLinkOrgModal = true;
	}

	async function handleLinkOrg() {
		if (!linkTargetIntegrationId || !selectedOrgId) return;

		linkingOrg = true;
		linkOrgError = '';
		try {
			const res = await api.linkAdminIntegrationOrg(linkTargetIntegrationId, selectedOrgId, {
				external_id: externalId.trim() || undefined
			});
			if (res.data) {
				showLinkOrgModal = false;
				await loadData();
			} else {
				linkOrgError = res.error?.message ?? 'Failed to link organization';
			}
		} catch (e: any) {
			linkOrgError = e?.message ?? 'Failed to link organization';
		} finally {
			linkingOrg = false;
		}
	}

	function openUnlinkDialog(integrationId: string, orgId: string, orgName: string) {
		unlinkTarget = { integrationId, orgId, orgName };
		unlinkDialogOpen = true;
	}

	async function handleUnlinkOrg() {
		if (!unlinkTarget) return;
		try {
			await api.unlinkAdminIntegrationOrg(unlinkTarget.integrationId, unlinkTarget.orgId);
			unlinkDialogOpen = false;
			unlinkTarget = null;
			await loadData();
		} catch (e: any) {
			error = e?.message ?? 'Failed to unlink organization';
		}
	}
</script>

<div class="page">
	<!-- Page Header with Drydock eyebrow -->
	<PageHeader
		eyebrow="Platform"
		title="Integrations"
		subtitle="Configure external agent orchestrators (like Nomi) that manage organizations, dev databases, and apps."
	>
		{#snippet actions()}
			<div class="header-actions">
				<Button
					variant="secondary"
					size="sm"
					onclick={() => loadData()}
					disabled={loading}
					aria-label="Refresh integrations"
				>
					{#if loading}
						<Spinner size={14} />
					{:else}
						<RefreshCw size={14} />
					{/if}
				</Button>
				<Button variant="primary" size="sm" onclick={openCreateModal}>
					<Plus size={14} />
					New Integration
				</Button>
			</div>
		{/snippet}
	</PageHeader>

	<!-- Global Error Alert -->
	{#if error}
		<InlineAlert tone="error">
			{error}
		</InlineAlert>
	{/if}

	<!-- Main Content Area -->
	{#if loading && integrations.length === 0}
		<div class="loading-state">
			<Spinner size={28} />
		</div>
	{:else if integrations.length === 0}
		<EmptyState
			message="No platform integrations"
			sub="Create an integration (such as 'nomi') to issue scoped platform keys (ship_int_...) for external agent orchestrators."
		>
			{#snippet icon()}
				<Blocks size={32} />
			{/snippet}
		</EmptyState>
		<div class="empty-cta">
			<Button variant="primary" onclick={openCreateModal}>
				<Plus size={14} />
				Create Integration
			</Button>
		</div>
	{:else}
		<div class="integrations-list">
			{#each integrations as integration (integration.id)}
				<div class="integration-card">
					<!-- Integration Header -->
					<div class="card-header">
						<div class="header-left">
							<div class="integration-icon-tile">
								<Blocks size={20} />
							</div>
							<div class="header-details">
								<div class="title-row">
									<h2 class="integration-title">
										{integration.name}
									</h2>
									<button
										type="button"
										class="id-chip"
										onclick={() => copyToClipboard(integration.id, integration.id)}
										title="Click to copy Integration ID"
									>
										<span>{integration.id.slice(0, 8)}…{integration.id.slice(-4)}</span>
										{#if copiedId === integration.id}
											<Check size={11} class="id-copied" />
										{:else}
											<Copy size={11} />
										{/if}
									</button>
								</div>
								<div class="meta-row">
									<span>Created {formatDate(integration.created_at)}</span>
									<span class="meta-sep">·</span>
									<span>{integration.org_count} linked org{integration.org_count === 1 ? '' : 's'}</span>
									<span class="meta-sep">·</span>
									<span>{integration.keys.filter(k => !k.revoked_at).length} active key{integration.keys.filter(k => !k.revoked_at).length === 1 ? '' : 's'}</span>
								</div>
							</div>
						</div>

						<div class="card-actions">
							<Button variant="secondary" size="sm" onclick={() => openMintKeyModal(integration)}>
								<Key size={13} />
								Mint Key
							</Button>
							<Button variant="secondary" size="sm" onclick={() => openLinkOrgModal(integration)}>
								<LinkIcon size={13} />
								Link Org
							</Button>
							<button
								type="button"
								class="delete-btn"
								title="Delete integration"
								aria-label="Delete integration {integration.name}"
								onclick={() => confirmDeleteIntegration(integration.id, integration.name)}
							>
								<Trash2 size={13} />
							</button>
						</div>
					</div>

					<!-- Sections: Keys & Linked Orgs -->
					<div class="card-columns">
						<!-- Keys Column -->
						<div class="column-box">
							<div class="column-header">
								<span class="column-title">
									<Key size={12} />
									Integration Keys
								</span>
								<span class="column-hint">
									Prefix: <code>ship_int_</code>
								</span>
							</div>

							{#if integration.keys.length === 0}
								<div class="sub-empty">
									No keys minted yet. Click <strong>Mint Key</strong> to create credentials for this integration.
								</div>
							{:else}
								<div class="items-list">
									{#each integration.keys as key (key.id)}
										<div class="item-row">
											<div class="item-main">
												<div class="item-title-row">
													<span class="item-name">{key.name}</span>
													{#if key.revoked_at}
														<Badge tone="red">Revoked</Badge>
													{:else}
														<Badge tone="green">Active</Badge>
													{/if}
												</div>

												<div class="item-meta-row">
													<button
														type="button"
														class="copy-prefix-btn"
														onclick={() => copyToClipboard(key.prefix, key.id)}
														title="Copy key prefix"
													>
														<span>{key.prefix}…</span>
														{#if copiedId === key.id}
															<Check size={10} class="id-copied" />
														{:else}
															<Copy size={10} />
														{/if}
													</button>
													<span class="meta-sep">·</span>
													<div class="scopes-wrap">
														{#each key.scopes as sc}
															<span class="scope-chip scope-chip--{sc === 'secrets:reveal' ? 'reveal' : 'manage'}">
																{sc}
															</span>
														{/each}
													</div>
												</div>

												<div class="item-dates">
													Created {formatDate(key.created_at)}
													{#if key.last_used_at}
														· Used {formatDate(key.last_used_at)}
													{/if}
												</div>
											</div>

											{#if !key.revoked_at}
												<button
													type="button"
													class="row-action-btn row-action-btn--danger"
													onclick={() => openRevokeDialog(integration.id, key.id, key.name)}
												>
													Revoke
												</button>
											{/if}
										</div>
									{/each}
								</div>
							{/if}
						</div>

						<!-- Linked Orgs Column -->
						<div class="column-box">
							<div class="column-header">
								<span class="column-title">
									<Building2 size={12} />
									Linked Organizations
								</span>
								<span class="column-hint">
									{integration.org_count} total
								</span>
							</div>

							{#if integration.orgs.length === 0}
								<div class="sub-empty">
									No organizations linked yet. Organizations created by this integration's key will automatically appear here.
								</div>
							{:else}
								<div class="items-list">
									{#each integration.orgs as org (org.id)}
										<div class="item-row">
											<div class="item-main">
												<div class="item-title-row">
													<span class="item-name">{org.name}</span>
													<span class="org-slug">({org.slug})</span>
												</div>
												{#if org.external_id}
													<div class="item-dates">
														External ID: <code class="code-val">{org.external_id}</code>
													</div>
												{/if}
											</div>

											<button
												type="button"
												class="row-action-btn"
												onclick={() => openUnlinkDialog(integration.id, org.id, org.name)}
												title="Unlink organization"
											>
												<Unlink size={12} />
												Unlink
											</button>
										</div>
									{/each}
								</div>
							{/if}
						</div>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<!-- ─── Modals ──────────────────────────────────────────────────────────────── -->

<!-- Create Integration Modal -->
<Modal
	bind:open={showCreateModal}
	title="Create Platform Integration"
>
	<form onsubmit={(e) => { e.preventDefault(); handleCreateIntegration(); }} class="dialog-form">
		{#if createIntegrationError}
			<InlineAlert tone="error">{createIntegrationError}</InlineAlert>
		{/if}

		<FormField label="Integration Name" hint="e.g. 'nomi', 'deploy-agent', or your platform's name (1–64 characters).">
			<TextField
				bind:value={newIntegrationName}
				placeholder="nomi"
				autofocus
			/>
		</FormField>

		<div class="info-callout">
			<p class="info-callout-title">What is a platform integration?</p>
			<p>An integration represents an external orchestrator (like Nomi) that manages tenant organizations, dev databases, and published apps via dedicated <code>ship_int_...</code> credentials.</p>
		</div>

		<div class="modal-buttons">
			<Button variant="secondary" onclick={() => showCreateModal = false}>Cancel</Button>
			<Button variant="primary" type="submit" disabled={creatingIntegration || !newIntegrationName.trim()}>
				{#if creatingIntegration}
					<Spinner size={14} />
				{:else}
					Create Integration
				{/if}
			</Button>
		</div>
	</form>
</Modal>

<!-- Mint Key Modal -->
<Modal
	bind:open={showMintKeyModal}
	title="Mint Integration Key for '{targetIntegrationName}'"
>
	<form onsubmit={(e) => { e.preventDefault(); handleMintKey(); }} class="dialog-form">
		{#if mintKeyError}
			<InlineAlert tone="error">{mintKeyError}</InlineAlert>
		{/if}

		<FormField label="Key Identifier / Name" hint="Helps identify where this key is stored (e.g. 'nomi-agent', 'nomi-server').">
			<TextField
				bind:value={newKeyName}
				placeholder="nomi-agent"
				autofocus
			/>
		</FormField>

		<div class="scope-fieldset">
			<span class="scope-legend">Key Scopes</span>
			<div class="scope-options">
				<label class="scope-option">
					<Checkbox bind:checked={keyScopeManage} />
					<div class="scope-desc">
						<span class="scope-name">manage</span>
						<p class="scope-sub">
							Full orchestration — create & manage apps, environments, dev databases, publish zips, and domains.
						</p>
					</div>
				</label>
				<label class="scope-option scope-option--bordered">
					<Checkbox bind:checked={keyScopeReveal} />
					<div class="scope-desc">
						<span class="scope-name scope-name--reveal">secrets:reveal</span>
						<p class="scope-sub">
							Secret reveal capability — decrypt and read raw environment variables and database passwords. Recommended for server-side code only.
						</p>
					</div>
				</label>
			</div>
		</div>

		<div class="modal-buttons">
			<Button variant="secondary" onclick={() => showMintKeyModal = false}>Cancel</Button>
			<Button variant="primary" type="submit" disabled={mintingKey || !newKeyName.trim() || (!keyScopeManage && !keyScopeReveal)}>
				{#if mintingKey}
					<Spinner size={14} />
				{:else}
					Mint Key
				{/if}
			</Button>
		</div>
	</form>
</Modal>

<!-- One-Time Key Reveal Modal -->
<Modal
	bind:open={showCreatedKeyModal}
	title="Integration Key Generated"
>
	{#if createdKeyInfo}
		<div class="dialog-form">
			<InlineAlert tone="warning">
				{createdKeyInfo.note ?? 'Store this key now. It will not be shown again.'}
			</InlineAlert>

			<FormField label="Full API Key">
				<div class="reveal-key-row">
					<input
						type={keyRevealed ? 'text' : 'password'}
						readonly
						value={createdKeyInfo.key}
						class="key-mono-input"
					/>
					<Button
						variant="secondary"
						size="sm"
						onclick={() => keyRevealed = !keyRevealed}
						title={keyRevealed ? 'Hide key' : 'Show key'}
					>
						{#if keyRevealed}
							<EyeOff size={14} />
						{:else}
							<Eye size={14} />
						{/if}
					</Button>
					<Button
						variant="primary"
						size="sm"
						onclick={() => copyToClipboard(createdKeyInfo!.key, 'created-key')}
					>
						{#if copiedId === 'created-key'}
							<Check size={14} />
							Copied
						{:else}
							<Copy size={14} />
							Copy
						{/if}
					</Button>
				</div>
			</FormField>

			<div class="terminal-snippet">
				<p class="terminal-snippet-title">Quick setup snippet for Nomi:</p>
				<pre class="terminal-code">SHIPYARD_URL={typeof window !== 'undefined' ? window.location.origin : 'https://your-shipyard.com'}/openapi/v1/integration
SHIPYARD_KEY={createdKeyInfo.key}</pre>
			</div>

			<div class="modal-buttons">
				<Button variant="primary" onclick={() => showCreatedKeyModal = false}>
					I Have Stored the Key
				</Button>
			</div>
		</div>
	{/if}
</Modal>

<!-- Link Org Modal -->
<Modal
	bind:open={showLinkOrgModal}
	title="Link Organization to Integration"
>
	<form onsubmit={(e) => { e.preventDefault(); handleLinkOrg(); }} class="dialog-form">
		{#if linkOrgError}
			<InlineAlert tone="error">{linkOrgError}</InlineAlert>
		{/if}

		<FormField label="Select Organization">
			<select
				bind:value={selectedOrgId}
				class="drydock-select"
			>
				{#each allOrgs as org}
					<option value={org.id}>{org.name} ({org.slug})</option>
				{/each}
			</select>
		</FormField>

		<FormField label="External ID (Optional)" hint="The identifier your platform uses for this organization. Defaults to the organization's slug.">
			<TextField
				bind:value={externalId}
				placeholder="org_2Nf8sK"
			/>
		</FormField>

		<div class="modal-buttons">
			<Button variant="secondary" onclick={() => showLinkOrgModal = false}>Cancel</Button>
			<Button variant="primary" type="submit" disabled={linkingOrg || !selectedOrgId}>
				{#if linkingOrg}
					<Spinner size={14} />
				{:else}
					Link Organization
				{/if}
			</Button>
		</div>
	</form>
</Modal>

<!-- Confirm Revoke Key Dialog -->
<ConfirmDialog
	bind:open={revokeDialogOpen}
	title="Revoke Integration Key"
	message="Are you sure you want to revoke key '{revokeTarget?.keyName}'? Any services or agent tools using this key will immediately be denied access."
	confirmLabel="Revoke Key"
	danger={true}
	onConfirm={handleRevokeKey}
/>

<!-- Confirm Delete Integration Dialog -->
<ConfirmDialog
	bind:open={deleteDialogOpen}
	title="Delete Integration"
	message="Are you sure you want to delete integration '{deleteIntegrationName}'? All its keys will be revoked, and any linked organizations will be unlinked."
	confirmLabel="Delete Integration"
	danger={true}
	onConfirm={handleDeleteIntegration}
/>

<!-- Confirm Unlink Org Dialog -->
<ConfirmDialog
	bind:open={unlinkDialogOpen}
	title="Unlink Organization"
	message="Unlink organization '{unlinkTarget?.orgName}' from this integration? The organization and its data will remain untouched, but the integration's keys will no longer be able to manage it."
	confirmLabel="Unlink Organization"
	danger={false}
	onConfirm={handleUnlinkOrg}
/>

<style>
	/* ── Drydock layout ── */
	.page {
		padding: 24px 28px 40px;
		display: flex;
		flex-direction: column;
		gap: 20px;
		max-width: 1100px;
	}

	.header-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.loading-state {
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 64px 0;
	}

	.empty-cta {
		display: flex;
		justify-content: center;
		margin-top: -8px;
	}

	.integrations-list {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	/* ── Integration card ── */
	.integration-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.card-header {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding-bottom: 16px;
		border-bottom: 1px solid var(--border);
	}
	@media (min-width: 768px) {
		.card-header {
			flex-direction: row;
			align-items: center;
			justify-content: space-between;
		}
	}

	.header-left {
		display: flex;
		align-items: flex-start;
		gap: 12px;
	}

	.integration-icon-tile {
		width: 36px;
		height: 36px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text-primary);
		flex-shrink: 0;
	}

	.header-details {
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.title-row {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.integration-title {
		font-family: var(--font-sans);
		font-size: 16px;
		font-weight: 600;
		color: var(--text-primary);
		margin: 0;
	}

	.id-chip {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		padding: 2px 6px;
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-elevated);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: color var(--transition-fast), border-color var(--transition-fast);
	}
	.id-chip:hover {
		color: var(--text-primary);
		border-color: var(--border-hover);
	}

	:global(.id-copied) {
		color: var(--accent-green);
	}

	.meta-row {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--text-muted);
	}
	.meta-sep {
		color: var(--text-dim);
	}

	.card-actions {
		display: flex;
		align-items: center;
		gap: 8px;
	}

	.delete-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 30px;
		height: 30px;
		border-radius: var(--radius-md);
		border: 1px solid transparent;
		background: transparent;
		color: var(--text-muted);
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
	}
	.delete-btn:hover {
		background: var(--accent-red-muted);
		color: var(--accent-red);
		border-color: color-mix(in srgb, var(--accent-red) 30%, transparent);
	}

	/* ── Two-column content ── */
	.card-columns {
		display: grid;
		grid-template-columns: 1fr;
		gap: 20px;
	}
	@media (min-width: 900px) {
		.card-columns {
			grid-template-columns: 1fr 1fr;
		}
	}

	.column-box {
		display: flex;
		flex-direction: column;
		gap: 10px;
	}

	.column-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		font-size: 11px;
	}

	.column-title {
		display: flex;
		align-items: center;
		gap: 6px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.04em;
		color: var(--text-muted);
	}

	.column-hint {
		color: var(--text-dim);
		font-family: var(--font-mono);
	}
	.column-hint code {
		color: var(--text-secondary);
	}

	.sub-empty {
		padding: 14px 16px;
		border: 1px dashed var(--border);
		border-radius: var(--radius-md);
		text-align: center;
		font-size: 12px;
		color: var(--text-muted);
		line-height: 1.5;
	}

	.items-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.item-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
		padding: 10px 12px;
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
	}

	.item-main {
		min-width: 0;
		flex: 1;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.item-title-row {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.item-name {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-primary);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.org-slug {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-muted);
	}

	.item-meta-row {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
	}

	.copy-prefix-btn {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-muted);
		background: transparent;
		border: none;
		cursor: pointer;
		padding: 0;
		transition: color var(--transition-fast);
	}
	.copy-prefix-btn:hover {
		color: var(--text-primary);
	}

	.scopes-wrap {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		flex-wrap: wrap;
	}

	.scope-chip {
		font-family: var(--font-mono);
		font-size: 10px;
		padding: 1px 6px;
		border-radius: var(--radius-sm);
		border: 1px solid;
	}
	.scope-chip--manage {
		background: var(--accent-muted);
		color: var(--accent);
		border-color: color-mix(in srgb, var(--accent) 30%, transparent);
	}
	.scope-chip--reveal {
		background: var(--accent-yellow-muted);
		color: var(--accent-yellow-text);
		border-color: color-mix(in srgb, var(--accent-yellow) 30%, transparent);
	}

	.item-dates {
		font-size: 11px;
		color: var(--text-muted);
	}

	.code-val {
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-surface);
		padding: 1px 4px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		color: var(--text-primary);
	}

	.row-action-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-family: var(--font-sans);
		font-size: 12px;
		font-weight: 500;
		padding: 4px 8px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		background: var(--bg-surface);
		color: var(--text-secondary);
		cursor: pointer;
		transition: background var(--transition-fast), color var(--transition-fast), border-color var(--transition-fast);
		flex-shrink: 0;
	}
	.row-action-btn:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}
	.row-action-btn--danger {
		border-color: transparent;
		color: var(--accent-red);
		background: transparent;
	}
	.row-action-btn--danger:hover {
		background: var(--accent-red-muted);
		border-color: color-mix(in srgb, var(--accent-red) 30%, transparent);
	}

	/* ── Modals & forms ── */
	.dialog-form {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.info-callout {
		padding: 10px 12px;
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		font-size: 12px;
		color: var(--text-muted);
		line-height: 1.5;
	}
	.info-callout-title {
		font-weight: 600;
		color: var(--text-primary);
		margin-bottom: 2px;
	}

	.scope-fieldset {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.scope-legend {
		font-size: 12.5px;
		font-weight: 500;
		color: var(--text-secondary);
	}
	.scope-options {
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		padding: 8px 12px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.scope-option {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		cursor: pointer;
	}
	.scope-option--bordered {
		padding-top: 8px;
		border-top: 1px solid var(--border);
	}
	.scope-desc {
		display: flex;
		flex-direction: column;
		gap: 2px;
		font-size: 12px;
	}
	.scope-name {
		font-family: var(--font-mono);
		font-weight: 600;
		color: var(--accent);
	}
	.scope-name--reveal {
		color: var(--accent-yellow);
	}
	.scope-sub {
		color: var(--text-muted);
		line-height: 1.4;
		margin: 0;
	}

	.reveal-key-row {
		display: flex;
		align-items: center;
		gap: 8px;
	}
	.key-mono-input {
		flex: 1;
		font-family: var(--font-mono);
		font-size: 12px;
		padding: 6px 10px;
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		border: 1px solid var(--border-control);
		color: var(--text-primary);
	}
	.key-mono-input:focus {
		outline: none;
		box-shadow: var(--focus-ring);
	}

	.terminal-snippet {
		border-radius: var(--radius-md);
		background: var(--terminal-bg);
		border: 1px solid var(--terminal-border);
		padding: 12px;
		display: flex;
		flex-direction: column;
		gap: 6px;
	}
	.terminal-snippet-title {
		font-size: 11.5px;
		font-weight: 500;
		color: var(--terminal-dim);
		margin: 0;
	}
	.terminal-code {
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--terminal-green);
		background: transparent;
		padding: 0;
		margin: 0;
		overflow-x: auto;
		white-space: pre;
	}

	.drydock-select {
		width: 100%;
		padding: 8px 10px;
		font-family: var(--font-sans);
		font-size: 13px;
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		border: 1px solid var(--border-control);
		color: var(--text-primary);
	}
	.drydock-select:focus {
		outline: none;
		box-shadow: var(--focus-ring);
	}

	.modal-buttons {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding-top: 8px;
	}
</style>
