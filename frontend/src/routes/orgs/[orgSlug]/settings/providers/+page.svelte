<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm, isAdminRole } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { GitBranch, Key, Loader2, Save, Check, AlertCircle, Plus, Trash2 } from '@lucide/svelte';
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

	async function deleteProvider(id: string) {
		if (!orgId) return;
		if (!confirm('Are you sure you want to disconnect this Git account?')) return;
		const res = await api.deleteGitProvider(orgId, id);
		if (res.data !== undefined) {
			await loadProviders();
		}
	}

	function getProviderColor(type: string) {
		if (type === 'github') return '#24292f';
		if (type === 'gitlab') return '#FC6D26';
		if (type === 'bitbucket') return '#0052CC';
		return 'var(--accent)';
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
	<div class="loading"><div class="spinner"></div><span>Loading…</span></div>
{:else if canRead}

<div class="providers-page">

	<!-- Git Providers -->
	<section class="settings-section">
		<div class="section-header">
			<div class="section-icon"><GitBranch size={16} /></div>
			<div style="flex: 1;">
				<h2 class="section-title">Git Providers</h2>
				<p class="section-desc">Connect Git providers to deploy from private repositories.</p>
			</div>
			{#if canWrite}
				<button class="git-action-btn connect" onclick={() => { addModalOpen = true; errorMsg = ''; }}>
					<Plus size={14} /> Connect Git Provider
				</button>
			{/if}
		</div>

		{#if oauthNotice}
			<div class="oauth-notice" class:success={oauthNotice.startsWith('✓')} class:error={oauthNotice.startsWith('✗')}>
				{oauthNotice}
			</div>
		{/if}

		<div class="git-provider-list">
			{#if providersList.length === 0}
				<div class="empty-state">
					<p>No Git accounts connected yet. Add one to deploy private repositories.</p>
				</div>
			{:else}
				{#each providersList as provider (provider.id)}
					<div class="git-provider-item connected">
						<div class="git-provider-main">
							<div class="git-provider-logo" style="color:{getProviderColor(provider.provider_type)}">
								{#if provider.provider_type === 'github'}
									<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z"/></svg>
								{:else if provider.provider_type === 'gitlab'}
									<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M22.65 14.39L12 22.13 1.35 14.39a.84.84 0 0 1-.3-.94l1.22-3.78 2.44-7.51A.42.42 0 0 1 4.82 2a.43.43 0 0 1 .58 0 .42.42 0 0 1 .11.18l2.44 7.49h8.1l2.44-7.49a.42.42 0 0 1 .11-.18.43.43 0 0 1 .58 0 .42.42 0 0 1 .11.18l2.44 7.51L23 13.45a.84.84 0 0 1-.35.94z"/></svg>
								{:else}
									<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M.778 12C.778 5.773 5.772.778 12 .778c6.228 0 11.222 4.995 11.222 11.222 0 6.228-4.994 11.222-11.222 11.222C5.772 23.222.778 18.228.778 12zm11.907-6.258c-1.99 0-3.597 1.608-3.597 3.597 0 1.99 1.608 3.598 3.597 3.598s3.598-1.609 3.598-3.598c0-1.99-1.609-3.597-3.598-3.597zm-5.73 10.03c.598-1.806 2.286-3.116 4.283-3.116h2.895c1.997 0 3.685 1.31 4.283 3.116H6.955z"/></svg>
								{/if}
							</div>
							<div class="git-provider-info">
								<span class="git-provider-name">{provider.name}</span>
								<span class="git-provider-host">{provider.username || 'OAuth Account'} • {provider.provider_type.toUpperCase()} ({provider.auth_type.toUpperCase()})</span>
							</div>
							{#if canWrite}
								<div class="git-provider-actions">
									<button type="button" class="git-action-btn disconnect" onclick={() => deleteProvider(provider.id)}>
										<Trash2 size={13} /> Disconnect
									</button>
								</div>
							{/if}
						</div>
					</div>
				{/each}
			{/if}
		</div>
	</section>

	<!-- Webhook Secret -->
	<section class="settings-section">
		<div class="section-header">
			<div class="section-icon webhook-icon"><Key size={16} /></div>
			<div>
				<h2 class="section-title">Global Webhook Secret</h2>
				<p class="section-desc">Secure incoming push notifications from git hosts.</p>
			</div>
		</div>

		<form class="webhook-form" onsubmit={saveWebhookSecret}>
			<div class="fields">
				<div class="field">
					<label class="field-label" for="webhook-secret-input">Webhook Secret</label>
					<input
						id="webhook-secret-input"
						class="field-input font-mono"
						type="password"
						placeholder="Optional webhook validation secret…"
						bind:value={settings.git_webhook_secret}
						disabled={!canWrite}
						autocomplete="new-password"
					/>
					<p class="field-hint">
						Used to verify payload signatures from GitHub (as a secret key) or GitLab (in the <code>X-Gitlab-Token</code> header).
					</p>
				</div>

				{#if saveError}
					<div class="error-banner">
						<AlertCircle size={15} />
						<span>{saveError}</span>
					</div>
				{/if}
			</div>

			{#if canWrite}
				<div class="save-bar">
					<button type="submit" class="git-action-btn connect save-btn" disabled={saving}>
						{#if saving}
							<div class="btn-spinner"></div>Saving…
						{:else if saved}
							<Check size={14} />Saved!
						{:else}
							<Save size={14} />Save Secret
						{/if}
					</button>
				</div>
			{/if}
		</form>
	</section>

</div>

<!-- Add Provider Modal -->
{#if addModalOpen}
	<div class="modal-backdrop" onclick={() => addModalOpen = false} role="presentation"></div>
	<div class="modal-container">
		<div class="modal-header">
			<h3 class="modal-title">Connect Git Account</h3>
			<button class="modal-close" onclick={() => addModalOpen = false}>&times;</button>
		</div>
		<form onsubmit={addProvider}>
			<div class="modal-body">
				{#if errorMsg}
					<div class="error-banner" style="margin-bottom: 12px;">
						<AlertCircle size={14} />
						<span>{errorMsg}</span>
					</div>
				{/if}

				<div class="modal-fields">
					<div class="field">
						<span class="field-label">Account Nickname</span>
						<input
							class="field-input"
							type="text"
							placeholder="e.g. My Personal GitHub, Company GitLab"
							bind:value={newName}
						/>
					</div>

					<div class="field">
						<span class="field-label">Git Platform</span>
						<select class="field-input" bind:value={newType}>
							<option value="github">GitHub (github.com)</option>
							<option value="gitlab">GitLab (gitlab.com)</option>
							<option value="bitbucket">Bitbucket (bitbucket.org)</option>
						</select>
					</div>

					<div class="field">
						<span class="field-label">Connection Method</span>
						<div class="auth-type-selector">
							<label class="auth-type-option" class:active={newAuthType === 'pat'}>
								<input type="radio" name="auth_type" value="pat" bind:group={newAuthType} />
								<span>Personal Access Token</span>
							</label>
							<label class="auth-type-option" class:active={newAuthType === 'oauth'}>
								<input type="radio" name="auth_type" value="oauth" bind:group={newAuthType} />
								<span>OAuth Integration</span>
							</label>
						</div>
					</div>

					{#if newAuthType === 'pat'}
						<div class="field">
							<span class="field-label">Personal Access Token</span>
							<input
								class="field-input font-mono"
								type="password"
								placeholder="Paste access token here…"
								bind:value={newToken}
								autocomplete="off"
							/>
							<p class="field-hint">
								{#if newType === 'github'}
									Create a token at <a href="https://github.com/settings/tokens" target="_blank" rel="noopener noreferrer" style="color: var(--accent); text-decoration: underline;">github.com/settings/tokens</a> with <code>repo</code> scope.
								{:else if newType === 'gitlab'}
									Create a token at <a href="https://gitlab.com/-/profile/personal_access_tokens" target="_blank" rel="noopener noreferrer" style="color: var(--accent); text-decoration: underline;">gitlab.com/-/profile/personal_access_tokens</a> with <code>read_repository</code> scope.
								{:else}
									Create an App Password at <a href="https://bitbucket.org/account/settings/app-passwords" target="_blank" rel="noopener noreferrer" style="color: var(--accent); text-decoration: underline;">bitbucket.org</a> with <code>Repositories Read</code>.
								{/if}
							</p>
						</div>
					{/if}
				</div>
			</div>
			<div class="modal-footer">
				<button type="button" class="git-action-btn reconfigure" onclick={() => addModalOpen = false}>Cancel</button>
				<button type="submit" class="git-action-btn connect" disabled={adding}>
					{#if adding}
						<Loader2 size={12} class="spin" />Connecting…
					{:else if newAuthType === 'oauth'}
						Redirect to Connect
					{:else}
						Connect Account
					{/if}
				</button>
			</div>
		</form>
	</div>
{/if}

{/if}

<style>
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.8s linear infinite; }

	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }
	.spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }

	.providers-page { display: flex; flex-direction: column; gap: 20px; }

	.settings-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	.section-header {
		display: flex; gap: 14px; padding: 18px 20px; align-items: center;
		border-bottom: 1px solid var(--border); background: var(--bg-elevated);
	}
	.section-icon {
		width: 32px; height: 32px; border-radius: var(--radius-md);
		background: rgba(37,99,235,0.1); color: var(--accent);
		display: flex; align-items: center; justify-content: center; flex-shrink: 0; margin-top: 1px;
	}
	.webhook-icon { background: rgba(139,92,246,0.1); color: #8B5CF6; }
	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc  { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.git-provider-list { display: flex; flex-direction: column; }
	.git-provider-item { border-bottom: 1px solid var(--border); transition: background var(--transition-fast); }
	.git-provider-item:last-child { border-bottom: none; }
	.git-provider-item.connected { background: color-mix(in srgb, #22C55E 1%, transparent); }
	.git-provider-main { display: flex; align-items: center; gap: 14px; padding: 14px 20px; }
	.git-provider-logo { width: 36px; height: 36px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
	.git-provider-info { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
	.git-provider-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.git-provider-host { font-size: 12px; color: var(--text-dim); }
	.git-provider-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }

	.empty-state {
		padding: 30px; text-align: center; color: var(--text-muted); font-size: 13px;
	}

	.oauth-notice { padding: 10px 14px; border-radius: var(--radius-sm); font-size: 13px; font-weight: 500; margin: 12px 20px; }
	.oauth-notice.success { background: color-mix(in srgb, #22C55E 12%, transparent); color: #16A34A; border: 1px solid color-mix(in srgb, #22C55E 30%, transparent); }
	.oauth-notice.error   { background: color-mix(in srgb, #EF4444 10%, transparent); color: #DC2626; border: 1px solid color-mix(in srgb, #EF4444 30%, transparent); }

	.git-action-btn { font-size: 12px; font-weight: 500; padding: 5px 12px; border-radius: var(--radius-sm); cursor: pointer; transition: all var(--transition-fast); border: 1px solid transparent; display: inline-flex; align-items: center; gap: 5px; }
	.git-action-btn.connect    { background: var(--accent); color: white; border-color: var(--accent); }
	.git-action-btn.connect:hover:not(:disabled) { opacity: 0.85; }
	.git-action-btn.reconfigure { background: transparent; color: var(--text-muted); border-color: var(--border); }
	.git-action-btn.reconfigure:hover { border-color: var(--accent); color: var(--accent); }
	.git-action-btn.disconnect  { background: transparent; color: #EF4444; border-color: rgba(239,68,68,0.4); }
	.git-action-btn.disconnect:hover { background: rgba(239,68,68,0.08); }
	.git-action-btn:disabled { opacity: 0.5; cursor: default; }

	/* Modal styling */
	.modal-backdrop {
		position: fixed; top: 0; left: 0; right: 0; bottom: 0;
		background: rgba(0, 0, 0, 0.45); backdrop-filter: blur(2px);
		z-index: 100;
	}
	.modal-container {
		position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%);
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: var(--radius-lg); width: calc(100% - 32px); max-width: 460px;
		display: flex; flex-direction: column; z-index: 101;
		box-shadow: var(--shadow-xl); overflow: hidden;
	}
	.modal-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 14px 20px; border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.modal-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0; }
	.modal-close {
		background: none; border: none; font-size: 20px; color: var(--text-muted);
		cursor: pointer; padding: 0 4px; display: flex; align-items: center;
	}
	.modal-close:hover { color: var(--text-primary); }
	.modal-body { padding: 20px; }
	.modal-fields { display: flex; flex-direction: column; gap: 16px; }
	.modal-footer {
		display: flex; justify-content: flex-end; gap: 8px;
		padding: 12px 20px; border-top: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.auth-type-selector {
		display: flex; gap: 8px; margin-top: 4px;
	}
	.auth-type-option {
		flex: 1; display: flex; align-items: center; justify-content: center; gap: 6px;
		padding: 10px; border: 1px solid var(--border); border-radius: var(--radius-md);
		cursor: pointer; font-size: 12px; font-weight: 500; transition: all var(--transition-fast);
		color: var(--text-secondary);
	}
	.auth-type-option input { display: none; }
	.auth-type-option:hover { border-color: var(--accent); color: var(--text-primary); }
	.auth-type-option.active { border-color: var(--accent); background: rgba(37,99,235,0.05); color: var(--accent); }

	.webhook-form { display: flex; flex-direction: column; }
	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
	.field-input { background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px; font-family: var(--font-sans); padding: 8px 10px; outline: none; transition: border-color var(--transition-fast); width: 100%; box-sizing: border-box; }
	.field-input.font-mono { font-family: var(--font-mono); }
	.field-input:focus { border-color: var(--accent); }
	.field-input:disabled { opacity: 0.6; cursor: default; }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; margin: 2px 0 0; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }

	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	.save-bar { display: flex; justify-content: flex-end; padding: 0 20px 18px; }
	.save-btn { display: flex; align-items: center; gap: 6px; min-width: 140px; justify-content: center; }
	.btn-spinner { width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite; }

	@media (max-width: 639px) {
		.providers-page { gap: 16px; }
		.section-header { padding: 14px 16px; flex-wrap: wrap; gap: 10px; }
		.fields { padding: 14px 16px; }
		.git-provider-main { flex-wrap: wrap; padding: 12px 16px; gap: 10px; }
		.git-provider-actions { flex-wrap: wrap; width: 100%; }
		.save-bar { padding: 0 16px 14px; }
	}
</style>
