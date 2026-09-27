<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm, isAdminRole } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { Cloud, Loader2, AlertCircle, Trash2, RefreshCw, Check } from '@lucide/svelte';
	import type { CloudflareConnectionStatus, SandboxPreviewDnsStatus } from '$lib/api/types';

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

	async function disconnect() {
		if (!orgId) return;
		if (!confirm('Disconnect this Cloudflare account? Existing DNS records it created will NOT be deleted automatically.')) return;
		await api.disconnectCloudflare(orgId);
		await loadStatus();
	}

	// ── Platform: sandbox preview DNS (owner/superadmin only; backend enforces) ──
	let previewDns   = $state<SandboxPreviewDnsStatus | null>(null);
	let selectedOwnerOrgId = $state('');
	let savingOwner  = $state(false);
	let syncing      = $state(false);
	let syncMsg      = $state('');

	async function loadPreviewDns() {
		const res = await api.getSandboxPreviewDns();
		if (res.data) {
			previewDns = res.data;
			selectedOwnerOrgId = res.data.owner_org_id ?? '';
		}
	}

	async function saveOwner() {
		if (!selectedOwnerOrgId) return;
		savingOwner = true;
		await api.setSandboxPreviewDnsOwner(selectedOwnerOrgId);
		savingOwner = false;
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
	<div class="loading"><div class="spinner"></div><span>Loading…</span></div>
{:else if canRead}

<div class="cf-page">
	<section class="settings-section">
		<div class="section-header">
			<div class="section-icon"><Cloud size={16} /></div>
			<div style="flex: 1;">
				<h2 class="section-title">Cloudflare</h2>
				<p class="section-desc">Connect a Cloudflare account to automatically create DNS records when you add a domain to a service.</p>
			</div>
		</div>

		{#if errorMsg}
			<div class="error-banner" style="margin: 0 20px 12px;">
				<AlertCircle size={14} /><span>{errorMsg}</span>
			</div>
		{/if}

		{#if status?.account_id}
			<div class="cf-connected">
				<div class="cf-connected-main">
					<span class="cf-account-name">{status.account_name || status.account_id}</span>
					<span class="cf-zone-count">{status.zones.length} zone{status.zones.length === 1 ? '' : 's'} available</span>
				</div>
				{#if canWrite}
					<button class="cf-btn disconnect" onclick={disconnect}><Trash2 size={13} /> Disconnect</button>
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
				<div class="field">
					<span class="field-label">Cloudflare API Token</span>
					<input class="field-input font-mono" type="password" placeholder="Paste an API token with Zone:DNS:Edit permission…" bind:value={tokenInput} autocomplete="off" />
					<p class="field-hint">Create one at <a href="https://dash.cloudflare.com/profile/api-tokens" target="_blank" rel="noopener noreferrer" style="color: var(--accent); text-decoration: underline;">dash.cloudflare.com/profile/api-tokens</a> with "Zone / DNS / Edit" permission for the zones you want Shipyard to manage.</p>
				</div>
				<div class="save-bar">
					<button type="submit" class="cf-btn connect" disabled={connecting}>
						{#if connecting}<Loader2 size={12} class="spin" /> Connecting…{:else}Connect{/if}
					</button>
				</div>
			</form>
		{:else}
			<div class="empty-state">No Cloudflare account connected.</div>
		{/if}
	</section>

	{#if previewDns}
		<section class="settings-section">
			<div class="section-header">
				<div class="section-icon" style="background: rgba(139,92,246,0.1); color: #8B5CF6;"><RefreshCw size={16} /></div>
				<div style="flex: 1;">
					<h2 class="section-title">Sandbox Preview DNS</h2>
					<p class="section-desc">Platform-wide: one wildcard DNS record for <code>*.{previewDns.preview_base_domain}</code>, owned by whichever org's Cloudflare connection covers that zone. Only visible/usable by platform owners or superadmins.</p>
				</div>
			</div>
			<div class="fields">
				<div class="field">
					<span class="field-label">DNS-owner organization</span>
					<input class="field-input font-mono" placeholder="org id" bind:value={selectedOwnerOrgId} />
					<p class="field-hint">Paste the id of the org whose Cloudflare connection covers <code>{previewDns.preview_base_domain}</code>'s zone, then Save.</p>
				</div>
				<div class="save-bar" style="justify-content: flex-start; gap: 8px;">
					<button class="cf-btn connect" onclick={saveOwner} disabled={savingOwner || !selectedOwnerOrgId}>
						{#if savingOwner}<Loader2 size={12} class="spin" /> Saving…{:else}Save Owner{/if}
					</button>
					<button class="cf-btn connect" onclick={syncNow} disabled={syncing || !previewDns.owner_org_id}>
						{#if syncing}<Loader2 size={12} class="spin" /> Syncing…{:else}<RefreshCw size={13} /> Sync DNS record{/if}
					</button>
					{#if syncMsg}<span class="sync-msg">{syncMsg}</span>{/if}
				</div>
				{#if previewDns.cloudflare_record_id}
					<p class="field-hint"><Check size={12} style="display:inline;vertical-align:-2px;" /> Last synced record: <code>{previewDns.cloudflare_record_id}</code></p>
				{/if}
			</div>
		</section>
	{/if}
</div>

{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }
	.spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.8s linear infinite; }

	.cf-page { display: flex; flex-direction: column; gap: 20px; }
	.settings-section { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; }
	.section-header { display: flex; gap: 14px; padding: 18px 20px; align-items: center; border-bottom: 1px solid var(--border); background: var(--bg-elevated); }
	.section-icon { width: 32px; height: 32px; border-radius: var(--radius-md); background: rgba(37,99,235,0.1); color: var(--accent); display: flex; align-items: center; justify-content: center; flex-shrink: 0; margin-top: 1px; }
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }
	.section-desc code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 11px; }

	.cf-connected { display: flex; align-items: center; gap: 14px; padding: 14px 20px; }
	.cf-connected-main { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
	.cf-account-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.cf-zone-count { font-size: 12px; color: var(--text-dim); }
	.cf-zone-list { list-style: none; margin: 0; padding: 0 20px 16px; display: flex; flex-direction: column; gap: 4px; }
	.cf-zone-list li { font-size: 12px; font-family: var(--font-mono); color: var(--text-secondary); }

	.cf-connect-form { padding: 18px 20px; display: flex; flex-direction: column; gap: 12px; }
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
	.field-input { background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px; font-family: var(--font-sans); padding: 8px 10px; outline: none; width: 100%; box-sizing: border-box; }
	.field-input.font-mono { font-family: var(--font-mono); }
	.field-input:focus { border-color: var(--accent); }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; margin: 2px 0 0; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }

	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }
	.save-bar { display: flex; justify-content: flex-end; align-items: center; }
	.sync-msg { font-size: 12px; color: var(--text-muted); }

	.empty-state { padding: 30px; text-align: center; color: var(--text-muted); font-size: 13px; }
	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	.cf-btn { font-size: 12px; font-weight: 500; padding: 5px 12px; border-radius: var(--radius-sm); cursor: pointer; transition: all var(--transition-fast); border: 1px solid transparent; display: inline-flex; align-items: center; gap: 5px; }
	.cf-btn.connect { background: var(--accent); color: white; border-color: var(--accent); }
	.cf-btn.connect:hover:not(:disabled) { opacity: 0.85; }
	.cf-btn.connect:disabled { opacity: 0.5; cursor: default; }
	.cf-btn.disconnect { background: transparent; color: #EF4444; border-color: rgba(239,68,68,0.4); }
	.cf-btn.disconnect:hover { background: rgba(239,68,68,0.08); }
</style>
