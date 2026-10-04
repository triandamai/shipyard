<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm, isAdminRole } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { GitBranch, GitMerge, GitFork, Key, Save, Check, Plus, Trash2 } from '@lucide/svelte';
	import Card from '$lib/components/ui/Card.svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import FormField from '$lib/components/ui/FormField.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';
	import Select from '$lib/components/ui/Select.svelte';
	import RadioGroup from '$lib/components/ui/RadioGroup.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import ListRow from '$lib/components/ui/ListRow.svelte';
	import ActivityList from '$lib/components/ui/ActivityList.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import IconBadge from '$lib/components/ui/IconBadge.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import type { GitProvider } from '$lib/api/types';

	interface PlatformSettings {
		git_webhook_secret?: string;
	}

	let orgId        = $derived($orgStore.activeOrg?.id ?? '');
	let myRole       = $derived($orgStore.myMembership?.role ?? null);
	let myPerms      = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let isAdmin      = $derived(isAdminRole(myRole));

	let canRead  = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'providers', 'read'))  || can(myRole, myPerms, perm(orgId, 'settings', 'read')));
	let canWrite = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'providers', 'write')) || can(myRole, myPerms, perm(orgId, 'settings', 'write')));

	let settings    = $state<PlatformSettings>({});
	let providersList = $state<GitProvider[]>([]);
	let loading     = $state(true);
	let oauthNotice = $state('');
	
	let addModalOpen = $state(false);
	let newName = $state('');
	let newType = $state<'github' | 'gitlab' | 'bitbucket' | 'gitea'>('github');
	let newAuthType = $state<'pat' | 'oauth'>('pat');
	let newToken = $state('');
	let adding = $state(false);
	let errorMsg = $state('');
	let addFormEl = $state<HTMLFormElement | null>(null);
	let disconnectOpen = $state(false);
	let disconnectId = $state('');

	let saveError   = $state('');
	let saved       = $state(false);
	let saving      = $state(false);

	import { page } from '$app/stores';
	$effect(() => {
		const connected = $page.url.searchParams.get('git_connected');
		const error     = $page.url.searchParams.get('git_error');
		if (connected) oauthNotice = `✓ ${connected.toUpperCase()} account connected successfully`;
		if (error)     oauthNotice = `✗ Connection failed: ${error}`;
	});

	$effect(() => {
		if (orgId) {
			loadProviders();
		}
	});

	async function loadProviders() {
		if (!orgId) return;
		const res = await api.listGitProviders(orgId);
		if (res.data) {
			providersList = res.data;
		}
	}

	async function addProvider(e: Event) {
		e.preventDefault();
		if (!orgId) return;

		if (newAuthType === 'pat') {
			if (!newName.trim()) { errorMsg = 'Please enter a nickname'; return; }
			if (!newToken.trim()) { errorMsg = 'Please enter a token'; return; }
			adding = true; errorMsg = '';
			const res = await api.createGitProvider(orgId, {
				name: newName.trim(),
				provider_type: newType,
				auth_type: 'pat',
				token: newToken.trim(),
			});
			adding = false;
			if (res.error) {
				errorMsg = res.error.message;
			} else {
				addModalOpen = false;
				newName = '';
				newToken = '';
				await loadProviders();
			}
		} else {
			// Connect via OAuth
			const returnTo = encodeURIComponent(window.location.pathname);
			window.location.href = `/api/auth/oauth/${newType}?org_id=${orgId}&return_to=${returnTo}`;
		}
	}

	function deleteProvider(id: string) {
		disconnectId = id;
		disconnectOpen = true;
	}

	async function confirmDisconnect() {
		const id = disconnectId;
		if (!orgId) return;
		const res = await api.deleteGitProvider(orgId, id);
		if (res.data !== undefined) {
			await loadProviders();
		}
	}

	async function saveWebhookSecret(e: SubmitEvent) {
		e.preventDefault();
		saving = true; saved = false; saveError = '';
		try {
			const res = await api.put<PlatformSettings>('/settings', {
				git_webhook_secret: settings.git_webhook_secret,
			});
			if (res.error) saveError = res.error.message;
			else { saved = true; setTimeout(() => (saved = false), 3000); }
		} finally { saving = false; }
	}

	onMount(async () => {
		const res = await api.get<PlatformSettings>('/settings');
		if (res.data) settings = res.data;
		loading = false;
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

<div class="providers-page">

	<!-- Git Providers -->
	<Card padding="0">
		<div class="section-header">
			<IconBadge tone="blue"><GitBranch size={16} /></IconBadge>
			<div class="section-text">
				<h2 class="section-title">Git Providers</h2>
				<p class="section-desc">Connect Git providers to deploy from private repositories.</p>
			</div>
			{#if canWrite}
				<Button size="sm" onclick={() => { addModalOpen = true; errorMsg = ''; }}>
					<Plus size={14} /> Connect Git Provider
				</Button>
			{/if}
		</div>

		{#if oauthNotice}
			<div class="oauth-notice" role="status">
				<InlineAlert tone={oauthNotice.startsWith('✗') ? 'error' : 'success'}>{oauthNotice}</InlineAlert>
			</div>
		{/if}

		<div class="git-provider-list">
			{#if providersList.length === 0}
				<EmptyState message="No Git accounts connected yet. Add one to deploy private repositories." />
			{:else}
				<ActivityList>
					{#each providersList as provider (provider.id)}
						<ListRow
							iconTone="blue"
							title={provider.name}
							meta={`${provider.username || 'OAuth Account'} • ${provider.provider_type.toUpperCase()} (${provider.auth_type.toUpperCase()})`}
						>
							{#snippet icon()}
								{#if provider.provider_type === 'github'}
									<GitBranch size={16} />
								{:else if provider.provider_type === 'gitlab'}
									<GitMerge size={16} />
								{:else}
									<GitFork size={16} />
								{/if}
							{/snippet}
							{#snippet trailing()}
								{#if canWrite}
									<Button variant="danger-outline" size="sm" onclick={() => deleteProvider(provider.id)}>
										<Trash2 size={13} /> Disconnect
									</Button>
								{/if}
							{/snippet}
						</ListRow>
					{/each}
				</ActivityList>
			{/if}
		</div>
	</Card>

	<!-- Webhook Secret -->
	<Card padding="0">
		<div class="section-header">
			<IconBadge tone="yellow"><Key size={16} /></IconBadge>
			<div class="section-text">
				<h2 class="section-title">Global Webhook Secret</h2>
				<p class="section-desc">Secure incoming push notifications from git hosts.</p>
			</div>
		</div>

		<form class="webhook-form" onsubmit={saveWebhookSecret}>
			<div class="fields">
				<FormField label="Webhook Secret" for="webhook-secret-input">
					<TextField
						id="webhook-secret-input"
						type="password"
						placeholder="Optional webhook validation secret…"
						bind:value={() => settings.git_webhook_secret ?? '', (v) => (settings.git_webhook_secret = v)}
						disabled={!canWrite}
						autocomplete="new-password"
					/>
					<p class="field-hint">
						Used to verify payload signatures from GitHub (as a secret key) or GitLab (in the <code>X-Gitlab-Token</code> header).
					</p>
				</FormField>

				{#if saveError}
					<div role="alert"><InlineAlert tone="error">{saveError}</InlineAlert></div>
				{/if}
			</div>

			{#if canWrite}
				<div class="save-bar">
					<Button type="submit" disabled={saving}>
						{#if saving}
							<Spinner size={12} tone="current" />Saving…
						{:else if saved}
							<Check size={14} />Saved!
						{:else}
							<Save size={14} />Save Secret
						{/if}
					</Button>
				</div>
			{/if}
		</form>
	</Card>

</div>

<!-- Add Provider Modal -->
<Modal bind:open={addModalOpen} title="Connect Git Account">
	{#snippet children()}
		<form bind:this={addFormEl} onsubmit={addProvider}>
			<button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
			{#if errorMsg}
				<div role="alert" class="modal-alert"><InlineAlert tone="error">{errorMsg}</InlineAlert></div>
			{/if}

			<div class="modal-fields">
				<FormField label="Account Nickname" for="git-nickname">
					<TextField
						id="git-nickname"
						type="text"
						placeholder="e.g. My Personal GitHub, Company GitLab"
						bind:value={newName}
					/>
				</FormField>

				<FormField label="Git Platform" for="git-platform">
					<Select
						id="git-platform"
						bind:value={() => newType, (v) => (newType = v as typeof newType)}
						options={[
							{ value: 'github', label: 'GitHub (github.com)' },
							{ value: 'gitlab', label: 'GitLab (gitlab.com)' },
							{ value: 'bitbucket', label: 'Bitbucket (bitbucket.org)' },
						]}
					/>
				</FormField>

				<FormField label="Connection Method">
					<RadioGroup
						name="auth_type"
						bind:value={() => newAuthType, (v) => (newAuthType = v as typeof newAuthType)}
						options={[
							{ value: 'pat', label: 'Personal Access Token' },
							{ value: 'oauth', label: 'OAuth Integration' },
						]}
					/>
				</FormField>

				{#if newAuthType === 'pat'}
					<FormField label="Personal Access Token" for="git-token">
						<TextField
							id="git-token"
							type="password"
							placeholder="Paste access token here…"
							bind:value={newToken}
							autocomplete="off"
						/>
						<p class="field-hint">
							{#if newType === 'github'}
								Create a token at <a href="https://github.com/settings/tokens" target="_blank" rel="noopener noreferrer">github.com/settings/tokens</a> with <code>repo</code> scope.
							{:else if newType === 'gitlab'}
								Create a token at <a href="https://gitlab.com/-/profile/personal_access_tokens" target="_blank" rel="noopener noreferrer">gitlab.com/-/profile/personal_access_tokens</a> with <code>read_repository</code> scope.
							{:else}
								Create an App Password at <a href="https://bitbucket.org/account/settings/app-passwords" target="_blank" rel="noopener noreferrer">bitbucket.org</a> with <code>Repositories Read</code>.
							{/if}
						</p>
					</FormField>
				{/if}
			</div>
		</form>
	{/snippet}
	{#snippet footer()}
		<Button variant="secondary" onclick={() => (addModalOpen = false)}>Cancel</Button>
		<Button disabled={adding} onclick={() => addFormEl?.requestSubmit()}>
			{#if adding}
				<Spinner size={12} tone="current" />Connecting…
			{:else if newAuthType === 'oauth'}
				Redirect to Connect
			{:else}
				Connect Account
			{/if}
		</Button>
	{/snippet}
</Modal>

<ConfirmDialog
	bind:open={disconnectOpen}
	title="Disconnect Git account"
	message="Are you sure you want to disconnect this Git account?"
	confirmLabel="Disconnect"
	onConfirm={confirmDisconnect}
/>

{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }

	.providers-page { display: flex; flex-direction: column; gap: 20px; }

	.section-header {
		display: flex; gap: 14px; padding: 18px 20px; align-items: center;
		border-bottom: 1px solid var(--border); background: var(--bg-elevated);
		border-radius: var(--radius-lg) var(--radius-lg) 0 0;
	}
	.section-text { flex: 1; min-width: 0; }
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc  { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.oauth-notice { margin: 12px 20px; }

	.modal-alert { margin-bottom: 12px; }
	.modal-fields { display: flex; flex-direction: column; gap: 16px; }

	.webhook-form { display: flex; flex-direction: column; }
	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; margin: 2px 0 0; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }
	.field-hint a { color: var(--accent); text-decoration: underline; }

	.save-bar { display: flex; justify-content: flex-end; padding: 0 20px 18px; }

	@media (max-width: 639px) {
		.providers-page { gap: 16px; }
		.section-header { padding: 14px 16px; flex-wrap: wrap; gap: 10px; }
		.fields { padding: 14px 16px; }
		.save-bar { padding: 0 16px 14px; }
	}
</style>
