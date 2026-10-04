<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm, isAdminRole } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { Cloud, AlertCircle, Trash2, RefreshCw, Check } from '@lucide/svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import FormField from '$lib/components/ui/FormField.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import Checkbox from '$lib/components/ui/Checkbox.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import IconBadge from '$lib/components/ui/IconBadge.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import type { CloudflareConnectionStatus, SandboxPreviewDnsStatus, Organization } from '$lib/api/types';

	let orgId   = $derived($orgStore.activeOrg?.id ?? '');
	let myRole  = $derived($orgStore.myMembership?.role ?? null);
	let myPerms = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let isAdmin = $derived(isAdminRole(myRole));

	let canRead  = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'providers', 'read'))  || can(myRole, myPerms, perm(orgId, 'settings', 'read')));
	let canWrite = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'providers', 'write')) || can(myRole, myPerms, perm(orgId, 'settings', 'write')));

	let loading    = $state(true);
	let status     = $state<CloudflareConnectionStatus | null>(null);
	let tokenInput = $state('');
	let connecting = $state(false);
	let errorMsg   = $state('');

	async function loadStatus() {
		if (!orgId) return;
		const res = await api.getCloudflareConnection(orgId);
		if (res.data) status = res.data;
	}

	async function connect(e: Event) {
		e.preventDefault();
		if (!orgId || !tokenInput.trim()) return;
		connecting = true; errorMsg = '';
		const res = await api.connectCloudflare(orgId, tokenInput.trim());
		connecting = false;
		if (res.error) {
			errorMsg = res.error.message;
		} else {
			tokenInput = '';
			await loadStatus();
		}
	}

	let disconnectOpen = $state(false);

	async function disconnect() {
		if (!orgId) return;
		await api.disconnectCloudflare(orgId);
		await loadStatus();
	}

	// ── Platform: sandbox preview DNS (owner/superadmin only; backend enforces) ──
	let previewDns   = $state<SandboxPreviewDnsStatus | null>(null);
	let selectedOwnerOrgId = $state('');
	let selectedProxied = $state(false);
	let savingOwner  = $state(false);
	let ownerError   = $state('');
	let syncing      = $state(false);
	let syncMsg      = $state('');
	let myOrgs       = $state<Organization[]>([]);

	async function loadPreviewDns() {
		const res = await api.getSandboxPreviewDns();
		if (res.data) {
			previewDns = res.data;
			selectedOwnerOrgId = res.data.owner_org_id ?? '';
			selectedProxied = res.data.proxied;
		}
	}

	async function saveOwner() {
		if (!selectedOwnerOrgId) return;
		savingOwner = true;
		ownerError = '';
		const res = await api.setSandboxPreviewDnsOwner(selectedOwnerOrgId, selectedProxied);
		savingOwner = false;
		if (res.error) {
			ownerError = res.error.message;
			return;
		}
		await loadPreviewDns();
	}

	async function syncNow() {
		syncing = true; syncMsg = '';
		const res = await api.syncSandboxPreviewDns();
		syncing = false;
		if (res.error) {
			syncMsg = `✗ ${res.error.message}`;
		} else {
			syncMsg = '✓ Synced';
			await loadPreviewDns();
		}
	}

	onMount(async () => {
		// Only owners/superadmins can actually read this; the API client marks
		// this call `silent403`, so a Forbidden response here is treated as an
		// ordinary ApiResponse error (previewDns stays null and the section
		// simply doesn't render) instead of triggering the app-wide "Access
		// Restricted" dialog for every other viewer of this page.
		await loadPreviewDns();
		const orgsRes = await api.getOrgs();
		if (orgsRes.data) myOrgs = orgsRes.data;
		loading = false;
	});

	$effect(() => {
		if (orgId) loadStatus();
	});
</script>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canRead}
	message="You need the 'View providers' or 'View settings' permission to access this page."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if loading}
	<div class="loading"><Spinner size={18} /><span>Loading…</span></div>
{:else if canRead}

<div class="cf-page">
	<Card padding="0">
		<div class="section-header">
			<IconBadge tone="blue"><Cloud size={16} /></IconBadge>
			<div class="section-text">
				<h2 class="section-title">Cloudflare</h2>
				<p class="section-desc">Connect a Cloudflare account to automatically create DNS records when you add a domain to a service.</p>
			</div>
		</div>

		{#if errorMsg}
			<div role="alert" class="cf-alert"><InlineAlert tone="error">{errorMsg}</InlineAlert></div>
		{/if}

		{#if status?.account_id}
			<div class="cf-connected">
				<div class="cf-connected-main">
					<span class="cf-account-name">{status.account_name || status.account_id}</span>
					<span class="cf-zone-count">{status.zones.length} zone{status.zones.length === 1 ? '' : 's'} available</span>
				</div>
				{#if canWrite}
					<Button variant="danger-outline" size="sm" onclick={() => (disconnectOpen = true)}><Trash2 size={13} /> Disconnect</Button>
				{/if}
			</div>
			{#if status.zones.length > 0}
				<ul class="cf-zone-list">
					{#each status.zones as zone (zone.id)}
						<li>{zone.name}</li>
					{/each}
				</ul>
			{/if}
		{:else if canWrite}
			<form class="cf-connect-form" onsubmit={connect}>
				<FormField label="Cloudflare API Token" for="cf-token">
					<TextField id="cf-token" type="password" placeholder="Paste an API token with Zone:DNS:Edit permission…" bind:value={tokenInput} autocomplete="off" />
					<p class="field-hint">Create one at <a href="https://dash.cloudflare.com/profile/api-tokens" target="_blank" rel="noopener noreferrer">dash.cloudflare.com/profile/api-tokens</a> with "Zone / DNS / Edit" permission for the zones you want Shipyard to manage.</p>
				</FormField>
				<div class="save-bar">
					<Button type="submit" size="sm" disabled={connecting}>
						{#if connecting}<Spinner size={12} tone="current" /> Connecting…{:else}Connect{/if}
					</Button>
				</div>
			</form>
		{:else}
			<EmptyState message="No Cloudflare account connected." />
		{/if}
	</Card>

	{#if previewDns}
		<Card padding="0">
			<div class="section-header">
				<IconBadge tone="yellow"><RefreshCw size={16} /></IconBadge>
				<div class="section-text">
					<h2 class="section-title">Sandbox Preview DNS</h2>
					<p class="section-desc">Platform-wide: one wildcard DNS record for <code>*.{previewDns.preview_base_domain}</code>, owned by whichever org's Cloudflare connection covers that zone. Only visible/usable by platform owners or superadmins.</p>
				</div>
			</div>
			<div class="fields">
				<FormField label="DNS-owner organization" for="cf-owner-org">
					{#if myOrgs.length > 0}
						<Select
							id="cf-owner-org"
							bind:value={selectedOwnerOrgId}
							options={[
								{ value: '', label: 'Select an organization…', disabled: true },
								...myOrgs.map((org) => ({ value: org.id, label: org.name })),
							]}
						/>
					{:else}
						<TextField id="cf-owner-org" placeholder="org id (UUID)" bind:value={selectedOwnerOrgId} />
					{/if}
					<p class="field-hint">The org whose Cloudflare connection covers <code>{previewDns.preview_base_domain}</code>'s zone — almost always your own platform-operator org, not a customer org.</p>
				</FormField>
				<div class="field">
					<Checkbox bind:checked={selectedProxied} label="Proxy through Cloudflare (orange cloud)" />
					<p class="field-hint proxied-warning">
						<AlertCircle size={12} class="inline-icon" />
						Not recommended: Shipyard issues its own TLS certificate per sandbox preview via Let's Encrypt,
						which requires Cloudflare to pass the ACME HTTP challenge straight through. Proxying intercepts
						that challenge instead, so enabling this will break HTTPS for every sandbox preview URL unless
						you separately terminate TLS at Cloudflare's edge yourself. Leave unchecked (DNS-only) unless
						you specifically know you need this.
					</p>
				</div>
				{#if ownerError}
					<div role="alert"><InlineAlert tone="error">{ownerError}</InlineAlert></div>
				{/if}
				<div class="save-bar save-bar--start">
					<Button size="sm" onclick={saveOwner} disabled={savingOwner || !selectedOwnerOrgId}>
						{#if savingOwner}<Spinner size={12} tone="current" /> Saving…{:else}Save Owner{/if}
					</Button>
					<Button size="sm" onclick={syncNow} disabled={syncing || !previewDns.owner_org_id}>
						{#if syncing}<Spinner size={12} tone="current" /> Syncing…{:else}<RefreshCw size={13} /> Sync DNS record{/if}
					</Button>
					{#if syncMsg}<span class="sync-msg" role="status">{syncMsg}</span>{/if}
				</div>
				{#if previewDns.cloudflare_record_id}
					<p class="field-hint"><Check size={12} class="inline-icon" /> Last synced record: <code>{previewDns.cloudflare_record_id}</code></p>
				{/if}
			</div>
		</Card>
	{/if}
</div>

<ConfirmDialog
	bind:open={disconnectOpen}
	title="Disconnect Cloudflare"
	message="Disconnect this Cloudflare account? Existing DNS records it created will NOT be deleted automatically."
	confirmLabel="Disconnect"
	onConfirm={disconnect}
/>

{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }

	.cf-page { display: flex; flex-direction: column; gap: 20px; }
	.section-header { display: flex; gap: 14px; padding: 18px 20px; align-items: center; border-bottom: 1px solid var(--border); background: var(--bg-elevated); border-radius: var(--radius-lg) var(--radius-lg) 0 0; }
	.section-text { flex: 1; min-width: 0; }
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }
	.section-desc code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 11px; }

	.cf-alert { margin: 12px 20px; }
	.cf-connected { display: flex; align-items: center; gap: 14px; padding: 14px 20px; }
	.cf-connected-main { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
	.cf-account-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.cf-zone-count { font-size: 12px; color: var(--text-dim); }
	.cf-zone-list { list-style: none; margin: 0; padding: 0 20px 16px; display: flex; flex-direction: column; gap: 4px; }
	.cf-zone-list li { font-size: 12px; font-family: var(--font-mono); color: var(--text-secondary); }

	.cf-connect-form { padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; }
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; margin: 2px 0 0; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }
	.field-hint a { color: var(--accent); text-decoration: underline; }
	.field-hint.proxied-warning { color: var(--accent-yellow); }
	.field-hint :global(.inline-icon) { display: inline; vertical-align: -2px; }

	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }
	.save-bar { display: flex; justify-content: flex-end; align-items: center; }
	.save-bar--start { justify-content: flex-start; gap: 8px; }
	.sync-msg { font-size: 12px; color: var(--text-muted); }
</style>
