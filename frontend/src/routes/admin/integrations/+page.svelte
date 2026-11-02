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
		Card,
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
	let keyCopied = $state(false);

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
				keyCopied = false;
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

<div class="space-y-6">
	<!-- Page Header -->
	<PageHeader
		title="Platform Integrations"
		subtitle="Configure and manage external platforms (e.g. Nomi) that orchestrate organizations, databases, and apps."
	>
		{#snippet actions()}
			<div class="flex items-center gap-2">
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
						<RefreshCw class="w-4 h-4" />
					{/if}
				</Button>
				<Button variant="primary" size="sm" onclick={openCreateModal}>
					<Plus class="w-4 h-4 mr-1.5" />
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
		<div class="flex items-center justify-center p-16">
			<Spinner size={32} />
		</div>
	{:else if integrations.length === 0}
		<EmptyState
			message="No platform integrations"
			sub="Create an integration (like 'nomi') to issue scoped platform keys (ship_int_...) for external agent orchestrators."
		>
			{#snippet icon()}
				<Blocks class="w-10 h-10 text-[var(--text-dim)]" />
			{/snippet}
		</EmptyState>
		<div class="flex justify-center -mt-2">
			<Button variant="primary" onclick={openCreateModal}>
				<Plus class="w-4 h-4 mr-1.5" />
				Create Integration
			</Button>
		</div>
	{:else}
		<div class="space-y-6">
			{#each integrations as integration (integration.id)}
				<Card padding="20px">
					<!-- Integration Header -->
					<div class="flex flex-col md:flex-row md:items-center justify-between gap-4 pb-5 border-b border-[var(--border)]">
						<div class="flex items-start gap-3">
							<div class="p-2.5 rounded-lg bg-[var(--bg-hover)] border border-[var(--border)] text-[var(--text-primary)]">
								<Blocks class="w-6 h-6 text-indigo-400" />
							</div>
							<div>
								<div class="flex items-center gap-2.5 flex-wrap">
									<h2 class="text-lg font-semibold text-[var(--text-primary)]">
										{integration.name}
									</h2>
									<button
										type="button"
										class="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-mono rounded bg-[var(--bg-hover)] text-[var(--text-secondary)] hover:text-[var(--text-primary)] border border-[var(--border)] transition-colors cursor-pointer"
										onclick={() => copyToClipboard(integration.id, integration.id)}
										title="Click to copy Integration ID"
									>
										<span>{integration.id.slice(0, 8)}…{integration.id.slice(-4)}</span>
										{#if copiedId === integration.id}
											<Check class="w-3 h-3 text-emerald-400" />
										{:else}
											<Copy class="w-3 h-3" />
										{/if}
									</button>
								</div>
								<div class="flex items-center gap-3 mt-1 text-xs text-[var(--text-secondary)]">
									<span>Created {formatDate(integration.created_at)}</span>
									<span>•</span>
									<span>{integration.org_count} linked org{integration.org_count === 1 ? '' : 's'}</span>
									<span>•</span>
									<span>{integration.keys.filter(k => !k.revoked_at).length} active key{integration.keys.filter(k => !k.revoked_at).length === 1 ? '' : 's'}</span>
								</div>
							</div>
						</div>

						<div class="flex items-center gap-2">
							<Button variant="secondary" size="sm" onclick={() => openMintKeyModal(integration)}>
								<Key class="w-3.5 h-3.5 mr-1.5 text-amber-400" />
								Mint Key
							</Button>
							<Button variant="secondary" size="sm" onclick={() => openLinkOrgModal(integration)}>
								<LinkIcon class="w-3.5 h-3.5 mr-1.5 text-indigo-400" />
								Link Org
							</Button>
							<Button
								variant="danger-outline"
								size="sm"
								onclick={() => confirmDeleteIntegration(integration.id, integration.name)}
								title="Delete integration"
							>
								<Trash2 class="w-4 h-4" />
							</Button>
						</div>
					</div>

					<!-- Sections: Keys & Linked Orgs -->
					<div class="grid grid-cols-1 lg:grid-cols-2 gap-6 mt-6">
						<!-- Keys Column -->
						<div class="space-y-3">
							<div class="flex items-center justify-between">
								<h3 class="text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] flex items-center gap-1.5">
									<Key class="w-3.5 h-3.5" />
									Integration Keys
								</h3>
								<span class="text-xs text-[var(--text-secondary)]">
									Prefix: <code class="font-mono text-indigo-300">ship_int_</code>
								</span>
							</div>

							{#if integration.keys.length === 0}
								<div class="p-4 rounded-lg border border-dashed border-[var(--border)] text-center text-xs text-[var(--text-secondary)]">
									No keys minted yet. Click <strong>Mint Key</strong> to create credentials for this integration.
								</div>
							{:else}
								<div class="space-y-2">
									{#each integration.keys as key (key.id)}
										<div class="p-3 rounded-lg border border-[var(--border)] bg-[var(--bg-hover)] flex items-center justify-between gap-3 text-sm">
											<div class="min-w-0 flex-1 space-y-1">
												<div class="flex items-center gap-2 flex-wrap">
													<span class="font-medium text-[var(--text-primary)] truncate">
														{key.name}
													</span>
													{#if key.revoked_at}
														<Badge tone="red">Revoked</Badge>
													{:else}
														<Badge tone="green">Active</Badge>
													{/if}
												</div>

												<div class="flex items-center gap-2 text-xs">
													<button
														type="button"
														class="inline-flex items-center gap-1 font-mono text-[var(--text-secondary)] hover:text-[var(--text-primary)] cursor-pointer"
														onclick={() => copyToClipboard(key.prefix, key.id)}
														title="Copy key prefix"
													>
														<span>{key.prefix}…</span>
														{#if copiedId === key.id}
															<Check class="w-3 h-3 text-emerald-400" />
														{:else}
															<Copy class="w-3 h-3" />
														{/if}
													</button>
													<span class="text-[var(--text-secondary)]">•</span>
													<div class="flex items-center gap-1 flex-wrap">
														{#each key.scopes as sc}
															<span class="px-1.5 py-0.5 rounded text-[10px] font-mono {sc === 'secrets:reveal' ? 'bg-amber-500/15 text-amber-300 border border-amber-500/30' : 'bg-indigo-500/15 text-indigo-300 border border-indigo-500/30'}">
																{sc}
															</span>
														{/each}
													</div>
												</div>

												<div class="text-[11px] text-[var(--text-secondary)]">
													Created {formatDate(key.created_at)}
													{#if key.last_used_at}
														• Used {formatDate(key.last_used_at)}
													{/if}
												</div>
											</div>

											{#if !key.revoked_at}
												<Button
													variant="danger-outline"
													size="sm"
													onclick={() => openRevokeDialog(integration.id, key.id, key.name)}
												>
													Revoke
												</Button>
											{/if}
										</div>
									{/each}
								</div>
							{/if}
						</div>

						<!-- Linked Orgs Column -->
						<div class="space-y-3">
							<div class="flex items-center justify-between">
								<h3 class="text-xs font-semibold uppercase tracking-wider text-[var(--text-secondary)] flex items-center gap-1.5">
									<Building2 class="w-3.5 h-3.5" />
									Linked Organizations
								</h3>
								<span class="text-xs text-[var(--text-secondary)]">
									{integration.org_count} total
								</span>
							</div>

							{#if integration.orgs.length === 0}
								<div class="p-4 rounded-lg border border-dashed border-[var(--border)] text-center text-xs text-[var(--text-secondary)]">
									No organizations linked yet. Organizations created by this integration via the API will automatically appear here.
								</div>
							{:else}
								<div class="space-y-2">
									{#each integration.orgs as org (org.id)}
										<div class="p-3 rounded-lg border border-[var(--border)] bg-[var(--bg-hover)] flex items-center justify-between gap-3 text-sm">
											<div class="min-w-0 flex-1 space-y-0.5">
												<div class="flex items-center gap-2">
													<span class="font-medium text-[var(--text-primary)] truncate">
														{org.name}
													</span>
													<span class="text-xs font-mono text-[var(--text-secondary)]">
														({org.slug})
													</span>
												</div>
												{#if org.external_id}
													<div class="text-xs text-[var(--text-secondary)]">
														External ID: <code class="font-mono text-indigo-300">{org.external_id}</code>
													</div>
												{/if}
											</div>

											<Button
												variant="secondary"
												size="sm"
												onclick={() => openUnlinkDialog(integration.id, org.id, org.name)}
												title="Unlink organization"
											>
												<Unlink class="w-3.5 h-3.5 mr-1" />
												Unlink
											</Button>
										</div>
									{/each}
								</div>
							{/if}
						</div>
					</div>
				</Card>
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
	<form onsubmit={(e) => { e.preventDefault(); handleCreateIntegration(); }} class="space-y-4">
		{#if createIntegrationError}
			<InlineAlert tone="error">{createIntegrationError}</InlineAlert>
		{/if}

		<FormField label="Integration Name" hint="e.g. 'nomi', 'deploy-agent', or your platform's name. (1-64 characters)">
			<TextField
				bind:value={newIntegrationName}
				placeholder="nomi"
				autofocus
			/>
		</FormField>

		<div class="p-3 rounded bg-[var(--bg-hover)] border border-[var(--border)] text-xs text-[var(--text-secondary)] space-y-1">
			<p class="font-medium text-[var(--text-primary)]">What is a platform integration?</p>
			<p>An integration represents an external orchestrator (like Nomi) that automates tenant organizations, dev databases, domains, and published applications using dedicated <code>ship_int_...</code> credentials.</p>
		</div>

		<div class="flex justify-end gap-2 pt-2">
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
	<form onsubmit={(e) => { e.preventDefault(); handleMintKey(); }} class="space-y-4">
		{#if mintKeyError}
			<InlineAlert tone="error">{mintKeyError}</InlineAlert>
		{/if}

		<FormField label="Key Identifier / Name" hint="Helps you distinguish where this key is installed (e.g. 'nomi-agent', 'nomi-server').">
			<TextField
				bind:value={newKeyName}
				placeholder="nomi-agent"
				autofocus
			/>
		</FormField>

		<div class="space-y-2">
			<label class="block text-sm font-medium text-[var(--text-primary)]">
				Key Scopes
			</label>
			<div class="space-y-2 p-3 rounded bg-[var(--bg-hover)] border border-[var(--border)]">
				<label class="flex items-start gap-2.5 cursor-pointer">
					<Checkbox bind:checked={keyScopeManage} />
					<div class="text-xs">
						<span class="font-mono font-medium text-indigo-300">manage</span>
						<p class="text-[var(--text-secondary)] mt-0.5">
							Full orchestration — create & manage apps, environments, dev databases, publish zips, and domains.
						</p>
					</div>
				</label>
				<label class="flex items-start gap-2.5 cursor-pointer pt-2 border-t border-[var(--border)]">
					<Checkbox bind:checked={keyScopeReveal} />
					<div class="text-xs">
						<span class="font-mono font-medium text-amber-300">secrets:reveal</span>
						<p class="text-[var(--text-secondary)] mt-0.5">
							Secret reveal capability — allows decrypting and reading raw environment variables and database passwords. Recommended only for server-side containers.
						</p>
					</div>
				</label>
			</div>
		</div>

		<div class="flex justify-end gap-2 pt-2">
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
		<div class="space-y-4">
			<InlineAlert tone="warning">
				{createdKeyInfo.note ?? 'Store this key now. It will not be shown again.'}
			</InlineAlert>

			<FormField label="Full API Key">
				<div class="flex items-center gap-2">
					<input
						type={keyRevealed ? 'text' : 'password'}
						readonly
						value={createdKeyInfo.key}
						class="flex-1 px-3 py-2 text-xs font-mono rounded bg-[var(--bg-hover)] border border-[var(--border)] text-[var(--text-primary)] select-all"
					/>
					<Button
						variant="secondary"
						size="sm"
						onclick={() => keyRevealed = !keyRevealed}
						title={keyRevealed ? 'Hide key' : 'Show key'}
					>
						{#if keyRevealed}
							<EyeOff class="w-4 h-4" />
						{:else}
							<Eye class="w-4 h-4" />
						{/if}
					</Button>
					<Button
						variant="primary"
						size="sm"
						onclick={() => copyToClipboard(createdKeyInfo!.key, 'created-key')}
					>
						{#if copiedId === 'created-key'}
							<Check class="w-4 h-4 mr-1 text-emerald-400" />
							Copied
						{:else}
							<Copy class="w-4 h-4 mr-1" />
							Copy
						{/if}
					</Button>
				</div>
			</FormField>

			<div class="p-3 rounded bg-[var(--bg-hover)] border border-[var(--border)] space-y-2 text-xs">
				<p class="font-medium text-[var(--text-primary)]">Quick setup snippet for Nomi:</p>
				<pre class="p-2 rounded bg-black/40 font-mono text-[11px] text-emerald-300 overflow-x-auto">SHIPYARD_URL={typeof window !== 'undefined' ? window.location.origin : 'https://your-shipyard.com'}/openapi/v1/integration
SHIPYARD_KEY={createdKeyInfo.key}</pre>
			</div>

			<div class="flex justify-end pt-2">
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
	title="Link Existing Organization to Integration"
>
	<form onsubmit={(e) => { e.preventDefault(); handleLinkOrg(); }} class="space-y-4">
		{#if linkOrgError}
			<InlineAlert tone="error">{linkOrgError}</InlineAlert>
		{/if}

		<FormField label="Select Organization">
			<select
				bind:value={selectedOrgId}
				class="w-full px-3 py-2 text-sm rounded bg-[var(--bg-hover)] border border-[var(--border)] text-[var(--text-primary)] focus:outline-none focus:ring-1 focus:ring-indigo-500"
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

		<div class="flex justify-end gap-2 pt-2">
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
