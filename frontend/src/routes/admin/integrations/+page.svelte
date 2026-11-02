<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
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
		Unlink,
		Radio,
		Send,
		ExternalLink
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
	import type {
		Integration,
		CreatedIntegrationKey,
		AdminOrg,
		AdminWebhook,
		CreatedAdminWebhook,
		WebhookTestResult
	} from '$lib/api/types';

	// ─── State ────────────────────────────────────────────────────────────────
	let integrations = $state<Integration[]>([]);
	let allOrgs = $state<AdminOrg[]>([]);
	let webhooksByIntegration = $state<Record<string, AdminWebhook[]>>({});
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

	// Webhook Registration Modal
	let showCreateWebhookModal = $state(false);
	let targetWebhookIntegrationId = $state<string | null>(null);
	let targetWebhookIntegrationName = $state<string>('');
	let webhookUrl = $state('');
	let webhookEventsPattern = $state('*');
	let webhookOrgExternalId = $state('');
	let creatingWebhook = $state(false);
	let createWebhookError = $state('');

	// One-time Webhook Secret Reveal Modal
	let showCreatedWebhookModal = $state(false);
	let createdWebhookInfo = $state<CreatedAdminWebhook | null>(null);
	let webhookSecretRevealed = $state(false);

	// Test Webhook Modal / Execution
	let testingWebhookId = $state<string | null>(null);
	let showTestResultModal = $state(false);
	let testResult = $state<WebhookTestResult | null>(null);
	let testWebhookUrl = $state('');

	// Delete Webhook Dialog
	let deleteWebhookDialogOpen = $state(false);
	let deleteWebhookTarget = $state<{ integrationId: string; webhookId: string; url: string } | null>(null);
	let deletingWebhook = $state(false);

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
				await loadWebhooksForIntegrations(integrationsRes.data);
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

	async function loadWebhooksForIntegrations(items: Integration[]) {
		const results = await Promise.all(
			items.map((i) => api.getAdminIntegrationWebhooks(i.id))
		);
		const map: Record<string, AdminWebhook[]> = {};
		for (let idx = 0; idx < items.length; idx++) {
			map[items[idx].id] = results[idx].data ?? [];
		}
		webhooksByIntegration = map;
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
			const res = await api.deleteAdminIntegration(deleteIntegrationId);
			if (res.data) {
				deleteDialogOpen = false;
				deleteIntegrationId = null;
				await loadData();
			} else {
				error = res.error?.message ?? 'Failed to delete integration';
			}
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
			mintKeyError = 'At least one scope must be selected';
			return;
		}

		mintingKey = true;
		mintKeyError = '';
		try {
			const res = await api.createAdminIntegrationKey(targetIntegrationId, {
				name,
				scopes
			});

			if (res.data) {
				showMintKeyModal = false;
				createdKeyInfo = res.data;
				keyRevealed = false;
				showCreatedKeyModal = true;
				await loadData();
			} else {
				mintKeyError = res.error?.message ?? 'Failed to mint key';
			}
		} catch (e: any) {
			mintKeyError = e?.message ?? 'Failed to mint key';
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
			const res = await api.revokeAdminIntegrationKey(
				revokeTarget.integrationId,
				revokeTarget.keyId
			);
			if (res.data) {
				revokeDialogOpen = false;
				revokeTarget = null;
				await loadData();
			} else {
				error = res.error?.message ?? 'Failed to revoke key';
			}
		} catch (e: any) {
			error = e?.message ?? 'Failed to revoke key';
		}
	}

	// ─── Webhook Actions ──────────────────────────────────────────────────────
	function openCreateWebhookModal(integrationId: string, name: string) {
		targetWebhookIntegrationId = integrationId;
		targetWebhookIntegrationName = name;
		webhookUrl = '';
		webhookEventsPattern = '*';
		webhookOrgExternalId = '';
		createWebhookError = '';
		showCreateWebhookModal = true;
	}

	async function handleCreateWebhook() {
		if (!targetWebhookIntegrationId) return;
		const url = webhookUrl.trim();
		if (!url) {
			createWebhookError = 'Endpoint URL is required';
			return;
		}

		creatingWebhook = true;
		createWebhookError = '';
		try {
			const events = webhookEventsPattern
				.split(',')
				.map((s) => s.trim())
				.filter(Boolean);

			const res = await api.createAdminIntegrationWebhook(targetWebhookIntegrationId, {
				url,
				events: events.length > 0 ? events : ['*'],
				org: webhookOrgExternalId.trim() || undefined
			});

			if (res.data) {
				showCreateWebhookModal = false;
				createdWebhookInfo = res.data;
				webhookSecretRevealed = false;
				showCreatedWebhookModal = true;
				await loadData();
			} else {
				createWebhookError = res.error?.message ?? 'Failed to register webhook';
			}
		} catch (e: any) {
			createWebhookError = e?.message ?? 'Failed to register webhook';
		} finally {
			creatingWebhook = false;
		}
	}

	function openDeleteWebhookDialog(integrationId: string, webhook: AdminWebhook) {
		deleteWebhookTarget = { integrationId, webhookId: webhook.id, url: webhook.url };
		deleteWebhookDialogOpen = true;
	}

	async function confirmDeleteWebhook() {
		if (!deleteWebhookTarget) return;
		deletingWebhook = true;
		try {
			const res = await api.deleteAdminIntegrationWebhook(
				deleteWebhookTarget.integrationId,
				deleteWebhookTarget.webhookId
			);
			if (res.data) {
				deleteWebhookDialogOpen = false;
				deleteWebhookTarget = null;
				await loadData();
			} else {
				error = res.error?.message ?? 'Failed to delete webhook';
			}
		} catch (e: any) {
			error = e?.message ?? 'Failed to delete webhook';
		} finally {
			deletingWebhook = false;
		}
	}

	async function handleTestWebhook(integrationId: string, webhook: AdminWebhook) {
		testingWebhookId = webhook.id;
		testWebhookUrl = webhook.url;
		try {
			const res = await api.testAdminIntegrationWebhook(integrationId, webhook.id);
			if (res.data) {
				testResult = res.data;
				showTestResultModal = true;
			} else {
				testResult = {
					ok: false,
					error: res.error?.message ?? 'Request failed'
				};
				showTestResultModal = true;
			}
		} catch (e: any) {
			testResult = {
				ok: false,
				error: e?.message ?? 'Network error while triggering test'
			};
			showTestResultModal = true;
		} finally {
			testingWebhookId = null;
		}
	}

	// ─── Org Actions ──────────────────────────────────────────────────────────
	function openLinkOrgModal(integration: Integration) {
		linkTargetIntegrationId = integration.id;
		selectedOrgId = allOrgs.length > 0 ? allOrgs[0].id : '';
		externalId = '';
		linkOrgError = '';
		showLinkOrgModal = true;
	}

	async function handleLinkOrg() {
		if (!linkTargetIntegrationId || !selectedOrgId) return;

		linkingOrg = true;
		linkOrgError = '';
		try {
			const res = await api.linkAdminIntegrationOrg(
				linkTargetIntegrationId,
				selectedOrgId,
				externalId.trim() ? { external_id: externalId.trim() } : {}
			);

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
			const res = await api.unlinkAdminIntegrationOrg(
				unlinkTarget.integrationId,
				unlinkTarget.orgId
			);
			if (res.data) {
				unlinkDialogOpen = false;
				unlinkTarget = null;
				await loadData();
			} else {
				error = res.error?.message ?? 'Failed to unlink organization';
			}
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
					onclick={loadData}
					disabled={loading}
					aria-label="Refresh integrations list"
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
		<InlineAlert tone="error" title="Error">
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
				{@const webhooks = webhooksByIntegration[integration.id] ?? []}
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
									<span class="meta-sep">·</span>
									<span>{webhooks.length} webhook{webhooks.length === 1 ? '' : 's'}</span>
								</div>
							</div>
						</div>

						<div class="card-actions">
							<Button variant="secondary" size="sm" onclick={() => openMintKeyModal(integration)}>
								<Key size={13} />
								Mint Key
							</Button>
							<Button variant="secondary" size="sm" onclick={() => openCreateWebhookModal(integration.id, integration.name)}>
								<Radio size={13} />
								Add Webhook
							</Button>
							<Button variant="secondary" size="sm" onclick={() => goto(`/admin/integrations/${integration.id}/orgs`)}>
								<Building2 size={13} />
								Manage Orgs
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

					<!-- Sections: Keys, Webhooks & Linked Orgs -->
					<div class="card-columns">
						<!-- Column 1: Keys -->
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

						<!-- Column 2: Webhooks -->
						<div class="column-box">
							<div class="column-header">
								<span class="column-title">
									<Radio size={12} />
									Outbound Webhooks
								</span>
								<button
									type="button"
									class="text-link-action"
									onclick={() => openCreateWebhookModal(integration.id, integration.name)}
								>
									<Plus size={11} /> Register
								</button>
							</div>

							{#if webhooks.length === 0}
								<div class="sub-empty">
									No webhooks registered. Click <strong>Register</strong> to send lifecycle events (e.g. <code>publish.succeeded</code>) to external orchestrators.
								</div>
							{:else}
								<div class="items-list">
									{#each webhooks as webhook (webhook.id)}
										<div class="item-row">
											<div class="item-main">
												<div class="item-title-row">
													<span class="item-name truncate-url" title={webhook.url}>
														{webhook.url}
													</span>
													{#if webhook.enabled}
														<Badge tone="green">Enabled</Badge>
													{:else}
														<Badge tone="neutral">Disabled</Badge>
													{/if}
												</div>

												<div class="item-meta-row">
													<div class="scopes-wrap">
														{#each webhook.events as evt}
															<span class="scope-chip scope-chip--manage">{evt}</span>
														{/each}
													</div>
													{#if webhook.org}
														<span class="meta-sep">·</span>
														<span class="code-val">org: {webhook.org}</span>
													{/if}
												</div>

												<div class="item-dates">
													Created {formatDate(webhook.created_at)}
												</div>
											</div>

											<div class="webhook-row-actions">
												<button
													type="button"
													class="row-action-btn"
													disabled={testingWebhookId === webhook.id}
													onclick={() => handleTestWebhook(integration.id, webhook)}
													title="Send signed test ping from Shipyard"
												>
													{#if testingWebhookId === webhook.id}
														<Spinner size={11} />
														<span>Testing…</span>
													{:else}
														<Send size={11} />
														<span>Send Test</span>
													{/if}
												</button>
												<button
													type="button"
													class="row-action-btn row-action-btn--danger"
													onclick={() => openDeleteWebhookDialog(integration.id, webhook)}
													title="Delete webhook"
												>
													<Trash2 size={11} />
												</button>
											</div>
										</div>
									{/each}
								</div>
							{/if}
						</div>

						<!-- Column 3: Linked Orgs -->
						<div class="column-box">
							<div class="column-header">
								<span class="column-title">
									<Building2 size={12} />
									Linked Organizations
								</span>
								<button
									type="button"
									class="text-link-action"
									onclick={() => goto(`/admin/integrations/${integration.id}/orgs`)}
								>
									<ExternalLink size={11} /> Manage ({integration.org_count})
								</button>
							</div>

							{#if integration.orgs.length === 0}
								<div class="sub-empty">
									No organizations linked yet. Click <strong>Link Org</strong> or manage them in the dedicated view.
								</div>
							{:else}
								<div class="items-list">
									{#each integration.orgs.slice(0, 4) as org (org.id)}
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
												<Unlink size={11} />
												Unlink
											</button>
										</div>
									{/each}
								</div>

								<div class="column-footer-link">
									<button
										type="button"
										class="view-paginated-btn"
										onclick={() => goto(`/admin/integrations/${integration.id}/orgs`)}
									>
										View all {integration.org_count} linked orgs in dedicated page →
									</button>
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
	<form onsubmit={(e) => { e.preventDefault(); void handleCreateIntegration(); }} class="dialog-form">
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
	<form onsubmit={(e) => { e.preventDefault(); void handleMintKey(); }} class="dialog-form">
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

<!-- Register Webhook Modal -->
<Modal
	bind:open={showCreateWebhookModal}
	title="Register Outbound Webhook for '{targetWebhookIntegrationName}'"
>
	<form onsubmit={(e) => { e.preventDefault(); void handleCreateWebhook(); }} class="dialog-form">
		{#if createWebhookError}
			<InlineAlert tone="error">{createWebhookError}</InlineAlert>
		{/if}

		<div class="info-callout">
			Shipyard will send HTTP <code>POST</code> events (signed with an HMAC-SHA256 secret) to this receiver whenever publish, deployment, or database events occur.
		</div>

		<FormField label="Receiver URL" required hint="e.g. https://nomi.example.com/api/webhooks/shipyard">
			<TextField
				bind:value={webhookUrl}
				placeholder="https://nomi.example.com/api/webhooks/shipyard"
				autofocus
			/>
		</FormField>

		<FormField label="Subscribed Events" hint="Comma-separated patterns: '*' for all events, or 'publish.*,deployment.*'">
			<TextField
				bind:value={webhookEventsPattern}
				placeholder="*"
			/>
		</FormField>

		<FormField label="Limit to Organization (Optional)" hint="Leave empty to subscribe to all organizations linked to this integration.">
			<TextField
				bind:value={webhookOrgExternalId}
				placeholder="e.g. org_2Nf8sK (or org slug)"
			/>
		</FormField>

		<div class="modal-buttons">
			<Button variant="secondary" onclick={() => showCreateWebhookModal = false}>Cancel</Button>
			<Button
				variant="primary"
				type="submit"
				disabled={creatingWebhook || !webhookUrl.trim()}
			>
				{#if creatingWebhook}
					<Spinner size={14} /> Registering…
				{:else}
					Register Webhook
				{/if}
			</Button>
		</div>
	</form>
</Modal>

<!-- One-Time Webhook Secret Reveal Modal -->
<Modal
	bind:open={showCreatedWebhookModal}
	title="Webhook Registered — Secret Key"
>
	{#if createdWebhookInfo}
		<div class="dialog-form">
			<InlineAlert tone="warning">
				Store this webhook signing secret now. It is shown only once and will not be displayed again.
			</InlineAlert>

			<FormField label="Signing Secret (HMAC-SHA256)">
				<div class="reveal-key-row">
					<input
						type={webhookSecretRevealed ? 'text' : 'password'}
						readonly
						value={createdWebhookInfo.secret}
						class="key-mono-input"
					/>
					<Button
						variant="secondary"
						size="sm"
						onclick={() => webhookSecretRevealed = !webhookSecretRevealed}
						title={webhookSecretRevealed ? 'Hide secret' : 'Show secret'}
					>
						{#if webhookSecretRevealed}
							<EyeOff size={14} />
						{:else}
							<Eye size={14} />
						{/if}
					</Button>
					<Button
						variant="primary"
						size="sm"
						onclick={() => copyToClipboard(createdWebhookInfo!.secret, 'webhook-secret')}
					>
						{#if copiedId === 'webhook-secret'}
							<Check size={14} /> Copied
						{:else}
							<Copy size={14} /> Copy
						{/if}
					</Button>
				</div>
			</FormField>

			<div class="terminal-snippet">
				<p class="terminal-snippet-title">Nomi environment configuration:</p>
				<pre class="terminal-code">SHIPYARD_WEBHOOK_SECRET={createdWebhookInfo.secret}</pre>
			</div>

			<div class="modal-buttons">
				<Button variant="primary" onclick={() => showCreatedWebhookModal = false}>
					I Have Stored the Webhook Secret
				</Button>
			</div>
		</div>
	{/if}
</Modal>

<!-- Webhook Test Result Modal -->
<Modal
	bind:open={showTestResultModal}
	title="Webhook Test Outcome"
>
	{#if testResult}
		<div class="dialog-form">
			<div class="test-target-info">
				Target: <code>{testWebhookUrl}</code>
			</div>

			{#if testResult.ok}
				<InlineAlert tone="success" title="Ping Delivered Successfully">
					{testResult.message ?? `Endpoint responded with HTTP ${testResult.status_code}.`}
				</InlineAlert>
				<p class="test-explainer">
					Shipyard dispatched a signed test <code>ping</code> event with valid <code>Shipyard-Signature</code> and <code>Shipyard-Event-Id</code> headers. Your endpoint responded with a 2xx success code.
				</p>
			{:else}
				<InlineAlert tone="error" title="Delivery Failed">
					{testResult.error ?? 'The endpoint did not respond successfully.'}
					{#if testResult.status_code}
						(HTTP {testResult.status_code})
					{/if}
				</InlineAlert>
				<p class="test-explainer">
					Verify that your endpoint is publicly reachable, accepts HTTP POST requests with <code>content-type: application/json</code>, and returns a 2xx status code within 10 seconds.
				</p>
			{/if}

			<div class="modal-buttons">
				<Button variant="primary" onclick={() => showTestResultModal = false}>
					Close
				</Button>
			</div>
		</div>
	{/if}
</Modal>

<!-- Delete Webhook Confirm Dialog -->
<ConfirmDialog
	bind:open={deleteWebhookDialogOpen}
	title="Delete Webhook Endpoint"
	message="Are you sure you want to delete the webhook '{deleteWebhookTarget?.url}'? Shipyard will stop delivering platform events to this endpoint."
	confirmLabel={deletingWebhook ? 'Deleting…' : 'Delete Webhook'}
	danger={true}
	onConfirm={confirmDeleteWebhook}
/>

<!-- Link Org Modal -->
<Modal
	bind:open={showLinkOrgModal}
	title="Link Organization to Integration"
>
	<form onsubmit={(e) => { e.preventDefault(); void handleLinkOrg(); }} class="dialog-form">
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
	title="Delete Platform Integration"
	message="Are you sure you want to delete integration '{deleteIntegrationName}'? All associated integration keys and webhooks will be immediately revoked."
	confirmLabel="Delete Integration"
	danger={true}
	onConfirm={handleDeleteIntegration}
/>

<!-- Confirm Unlink Org Dialog -->
<ConfirmDialog
	bind:open={unlinkDialogOpen}
	title="Unlink Organization"
	message="Are you sure you want to unlink '{unlinkTarget?.orgName}'? The organization will remain in Shipyard, but external platform keys will no longer be able to manage it."
	confirmLabel="Unlink Organization"
	danger={true}
	onConfirm={handleUnlinkOrg}
/>

<style>
	/* ── Drydock layout ── */
	.page {
		display: flex;
		flex-direction: column;
		gap: 24px;
		max-width: 1360px;
		margin: 0 auto;
		padding: 24px 24px 64px;
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
		padding: 64px;
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
		flex-wrap: wrap;
	}
	.meta-sep {
		color: var(--text-dim);
	}

	.card-actions {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
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

	/* ── Three-column content ── */
	.card-columns {
		display: grid;
		grid-template-columns: 1fr;
		gap: 20px;
	}
	@media (min-width: 1080px) {
		.card-columns {
			grid-template-columns: 1fr 1fr 1fr;
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
		gap: 8px;
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

	.text-link-action {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		font-weight: 500;
		color: var(--accent);
		background: transparent;
		border: none;
		cursor: pointer;
		padding: 2px 4px;
		border-radius: var(--radius-sm);
		transition: background var(--transition-fast);
	}
	.text-link-action:hover {
		background: var(--accent-muted);
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
		align-items: flex-start;
		justify-content: space-between;
		gap: 12px;
		padding: 10px 12px;
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
	}

	.item-main {
		display: flex;
		flex-direction: column;
		gap: 4px;
		min-width: 0;
		flex: 1;
	}

	.item-title-row {
		display: flex;
		align-items: center;
		gap: 8px;
		flex-wrap: wrap;
	}

	.item-name {
		font-family: var(--font-sans);
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.truncate-url {
		font-family: var(--font-mono);
		font-size: 11px;
		max-width: 220px;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
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
		flex-wrap: wrap;
		font-size: 11px;
	}

	.copy-prefix-btn {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-family: var(--font-mono);
		font-size: 11px;
		color: var(--text-secondary);
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

	.webhook-row-actions {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}

	.row-action-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		font-family: var(--font-sans);
		font-size: 11.5px;
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

	.column-footer-link {
		padding-top: 4px;
	}
	.view-paginated-btn {
		width: 100%;
		text-align: center;
		font-size: 11.5px;
		font-weight: 500;
		color: var(--accent);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 6px;
		cursor: pointer;
		transition: background var(--transition-fast), border-color var(--transition-fast);
	}
	.view-paginated-btn:hover {
		background: var(--bg-hover);
		border-color: var(--border-hover);
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

	.test-target-info {
		font-size: 12px;
		color: var(--text-muted);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 6px 10px;
	}
	.test-target-info code {
		font-family: var(--font-mono);
		color: var(--text-primary);
	}

	.test-explainer {
		font-size: 12px;
		color: var(--text-muted);
		line-height: 1.5;
		margin: 0;
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
