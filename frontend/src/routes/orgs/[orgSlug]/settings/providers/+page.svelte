<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, perm, isAdminRole } from '$lib/auth/permissions';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { GitBranch, Key, Loader2, Save, Check, AlertCircle } from '@lucide/svelte';

	interface PlatformSettings {
		git_github_token?: string;
		git_gitlab_token?: string;
		git_bitbucket_token?: string;
		git_webhook_secret?: string;
	}

	type GitProviderId = 'github' | 'gitlab' | 'bitbucket';
	interface GitProviderDef {
		id: GitProviderId;
		label: string;
		host: string;
		tokenKey: keyof PlatformSettings;
		color: string;
		patHint: string;
	}

	const GIT_PROVIDERS: GitProviderDef[] = [
		{ id: 'github',    label: 'GitHub',    host: 'github.com',    tokenKey: 'git_github_token',    color: '#24292f', patHint: 'Create a Personal Access Token at github.com/settings/tokens with repo scope' },
		{ id: 'gitlab',    label: 'GitLab',    host: 'gitlab.com',    tokenKey: 'git_gitlab_token',    color: '#FC6D26', patHint: 'Create a Personal Access Token at gitlab.com/-/profile/personal_access_tokens with read_repository scope' },
		{ id: 'bitbucket', label: 'Bitbucket', host: 'bitbucket.org', tokenKey: 'git_bitbucket_token', color: '#0052CC', patHint: 'Create an App Password at bitbucket.org/account/settings/app-passwords with Repositories Read permission' },
	];

	let orgId        = $derived($orgStore.activeOrg?.id ?? '');
	let myRole       = $derived($orgStore.myMembership?.role ?? null);
	let myPerms      = $derived($orgStore.myMembership?.permissions ?? []);
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let isAdmin      = $derived(isAdminRole(myRole));

	let canRead  = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'providers', 'read'))  || can(myRole, myPerms, perm(orgId, 'settings', 'read')));
	let canWrite = $derived(isAdmin || can(myRole, myPerms, perm(orgId, 'providers', 'write')) || can(myRole, myPerms, perm(orgId, 'settings', 'write')));

	let settings    = $state<PlatformSettings>({});
	let loading     = $state(true);
	let patInputFor = $state('');
	let patValues   = $state<Record<string, string>>({});
	let patSaving   = $state('');
	let oauthNotice = $state('');
	let saveError   = $state('');
	let saved       = $state(false);
	let saving      = $state(false);

	import { page } from '$app/stores';
	$effect(() => {
		const connected = $page.url.searchParams.get('git_connected');
		const error     = $page.url.searchParams.get('git_error');
		if (connected) oauthNotice = `✓ ${connected} connected successfully`;
		if (error)     oauthNotice = `✗ Connection failed: ${error}`;
	});

	function togglePatInput(provider: GitProviderDef) {
		patInputFor = patInputFor === provider.id ? '' : provider.id;
		if (!patValues[provider.id]) patValues[provider.id] = '';
	}

	async function savePat(provider: GitProviderDef) {
		const token = patValues[provider.id]?.trim();
		if (!token) return;
		patSaving = provider.id;
		const res = await api.put<PlatformSettings>('/settings', { [provider.tokenKey]: token });
		if (res.data) settings = { ...settings, ...res.data };
		patSaving = '';
		patInputFor = '';
		patValues[provider.id] = '';
	}

	async function disconnectProvider(provider: GitProviderDef) {
		const res = await api.put<PlatformSettings>('/settings', { [provider.tokenKey]: '' });
		if (res.data) settings = { ...settings, ...res.data };
	}

	function connectViaOAuth(provider: GitProviderDef) {
		const returnTo = encodeURIComponent(window.location.pathname);
		window.location.href = `/api/auth/oauth/${provider.id}?org_id=${orgId}&return_to=${returnTo}`;
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
			<div>
				<h2 class="section-title">Git Providers</h2>
				<p class="section-desc">Connect Git providers to deploy from private repositories.</p>
			</div>
		</div>

		{#if oauthNotice}
			<div class="oauth-notice" class:success={oauthNotice.startsWith('✓')} class:error={oauthNotice.startsWith('✗')}>
				{oauthNotice}
			</div>
		{/if}

		<div class="git-provider-list">
			{#each GIT_PROVIDERS as provider (provider.id)}
				{@const connected = !!(settings[provider.tokenKey])}
				{@const showPat = patInputFor === provider.id}
				<div class="git-provider-item" class:connected>
					<div class="git-provider-main">
						<div class="git-provider-logo" style="color:{provider.color}">
							{#if provider.id === 'github'}
								<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M12 0C5.37 0 0 5.37 0 12c0 5.31 3.435 9.795 8.205 11.385.6.105.825-.255.825-.57 0-.285-.015-1.23-.015-2.235-3.015.555-3.795-.735-4.035-1.41-.135-.345-.72-1.41-1.23-1.695-.42-.225-1.02-.78-.015-.795.945-.015 1.62.87 1.845 1.23 1.08 1.815 2.805 1.305 3.495.99.105-.78.42-1.305.765-1.605-2.67-.3-5.46-1.335-5.46-5.925 0-1.305.465-2.385 1.23-3.225-.12-.3-.54-1.53.12-3.18 0 0 1.005-.315 3.3 1.23.96-.27 1.98-.405 3-.405s2.04.135 3 .405c2.295-1.56 3.3-1.23 3.3-1.23.66 1.65.24 2.88.12 3.18.765.84 1.23 1.905 1.23 3.225 0 4.605-2.805 5.625-5.475 5.925.435.375.81 1.095.81 2.22 0 1.605-.015 2.895-.015 3.3 0 .315.225.69.825.57A12.02 12.02 0 0 0 24 12c0-6.63-5.37-12-12-12z"/></svg>
							{:else if provider.id === 'gitlab'}
								<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M22.65 14.39L12 22.13 1.35 14.39a.84.84 0 0 1-.3-.94l1.22-3.78 2.44-7.51A.42.42 0 0 1 4.82 2a.43.43 0 0 1 .58 0 .42.42 0 0 1 .11.18l2.44 7.49h8.1l2.44-7.49a.42.42 0 0 1 .11-.18.43.43 0 0 1 .58 0 .42.42 0 0 1 .11.18l2.44 7.51L23 13.45a.84.84 0 0 1-.35.94z"/></svg>
							{:else}
								<svg viewBox="0 0 24 24" fill="currentColor" width="22" height="22"><path d="M.778 12C.778 5.773 5.772.778 12 .778c6.228 0 11.222 4.995 11.222 11.222 0 6.228-4.994 11.222-11.222 11.222C5.772 23.222.778 18.228.778 12zm11.907-6.258c-1.99 0-3.597 1.608-3.597 3.597 0 1.99 1.608 3.598 3.597 3.598s3.598-1.609 3.598-3.598c0-1.99-1.609-3.597-3.598-3.597zm-5.73 10.03c.598-1.806 2.286-3.116 4.283-3.116h2.895c1.997 0 3.685 1.31 4.283 3.116H6.955z"/></svg>
							{/if}
						</div>
						<div class="git-provider-info">
							<span class="git-provider-name">{provider.label}</span>
							<span class="git-provider-host">{provider.host}</span>
						</div>
						{#if canWrite}
							<div class="git-provider-actions">
								{#if connected}
									<span class="git-status-badge connected">● Connected</span>
									<button type="button" class="git-action-btn reconfigure" onclick={() => togglePatInput(provider)}>{showPat ? 'Cancel' : 'Update Token'}</button>
									<button type="button" class="git-action-btn disconnect" onclick={() => disconnectProvider(provider)}>Disconnect</button>
								{:else}
									<span class="git-status-badge disconnected">○ Not connected</span>
									<button type="button" class="git-action-btn connect" onclick={() => togglePatInput(provider)}>{showPat ? 'Cancel' : 'Connect'}</button>
								{/if}
							</div>
						{:else}
							<div class="git-provider-actions">
								<span class="git-status-badge {connected ? 'connected' : 'disconnected'}">{connected ? '● Connected' : '○ Not connected'}</span>
							</div>
						{/if}
					</div>
					{#if showPat && canWrite}
						<div class="pat-input-area">
							<div class="pat-input-row">
								<div class="pat-input-icon"><Key size={13} /></div>
								<input
									class="field-input font-mono pat-input"
									type="password"
									placeholder="Paste your Personal Access Token…"
									bind:value={patValues[provider.id]}
									autocomplete="off"
								/>
								<button
									type="button"
									class="git-action-btn connect"
									disabled={!patValues[provider.id]?.trim() || patSaving === provider.id}
									onclick={() => savePat(provider)}
								>
									{#if patSaving === provider.id}
										<Loader2 size={12} class="spin" />Saving…
									{:else}
										Save Token
									{/if}
								</button>
							</div>
							<p class="pat-hint">{provider.patHint}</p>
							<p class="pat-oauth-link">
								Have an OAuth app configured?
								<button type="button" class="link-btn" onclick={() => connectViaOAuth(provider)}>Connect via OAuth instead →</button>
							</p>
						</div>
					{/if}
				</div>
			{/each}
		</div>
	</section>

	<!-- Webhook -->
	<section class="settings-section">
		<div class="section-header">
			<div class="section-icon webhook-icon"><Key size={16} /></div>
			<div>
				<h2 class="section-title">Webhook Configuration</h2>
				<p class="section-desc">Configure push webhook signatures and find the incoming webhook URL.</p>
			</div>
		</div>

		<form class="webhook-form" onsubmit={saveWebhookSecret}>
			<div class="fields">
				<div class="field">
					<label class="field-label" for="webhook-secret">Webhook Secret</label>
					<input
						id="webhook-secret"
						class="field-input font-mono"
						type="password"
						bind:value={settings.git_webhook_secret}
						placeholder="Used to verify push event signatures"
						autocomplete="off"
						disabled={!canWrite}
					/>
					<span class="field-hint">Set the same value in your GitHub/GitLab webhook configuration.</span>
				</div>
			</div>

			<div class="webhook-url-card">
				<span class="webhook-url-label">Incoming Webhook URL</span>
				<code class="webhook-url">{typeof window !== 'undefined' ? window.location.origin : ''}/api/webhooks/github/:service_id/:token</code>
				<span class="field-hint">Replace <code>:service_id</code> and <code>:token</code> from the service detail panel.</span>
			</div>

			{#if saveError}
				<div class="error-banner" style="margin: 0 20px 16px;"><AlertCircle size={14} />{saveError}</div>
			{/if}

			{#if canWrite}
				<div class="save-bar">
					<button class="btn btn-primary save-btn" type="submit" disabled={saving}>
						{#if saving}<div class="btn-spinner"></div>Saving…
						{:else if saved}<Check size={14} />Saved
						{:else}<Save size={14} />Save Secret
						{/if}
					</button>
				</div>
			{/if}
		</form>
	</section>

</div>
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
		display: flex; gap: 14px; padding: 18px 20px;
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
	.git-provider-item.connected { background: color-mix(in srgb, #22C55E 3%, transparent); }
	.git-provider-main { display: flex; align-items: center; gap: 14px; padding: 14px 20px; }
	.git-provider-logo { width: 36px; height: 36px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
	.git-provider-info { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
	.git-provider-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.git-provider-host { font-size: 12px; color: var(--text-dim); font-family: var(--font-mono); }
	.git-provider-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
	.git-status-badge { font-size: 11px; font-weight: 500; white-space: nowrap; }
	.git-status-badge.connected    { color: #22C55E; }
	.git-status-badge.disconnected { color: var(--text-dim); }

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

	.pat-input-area { display: flex; flex-direction: column; gap: 8px; padding: 12px 20px 14px; background: var(--bg-base); border-top: 1px solid var(--border); }
	.pat-input-row { display: flex; align-items: center; gap: 8px; }
	.pat-input-icon { color: var(--text-dim); display: flex; align-items: center; flex-shrink: 0; }
	.pat-input { flex: 1; min-width: 0; }
	.pat-hint { font-size: 11px; color: var(--text-dim); margin: 0; line-height: 1.4; }
	.pat-oauth-link { font-size: 11px; color: var(--text-dim); margin: 0; }
	.link-btn { background: none; border: none; padding: 0; cursor: pointer; color: var(--accent); font-size: 11px; font-family: inherit; }
	.link-btn:hover { text-decoration: underline; }

	.webhook-form { display: flex; flex-direction: column; }
	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
	.field-input { background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px; font-family: var(--font-sans); padding: 8px 10px; outline: none; transition: border-color var(--transition-fast); }
	.field-input.font-mono { font-family: var(--font-mono); }
	.field-input:focus { border-color: var(--accent); }
	.field-input:disabled { opacity: 0.6; cursor: default; }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }
	code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 12px; }

	.webhook-url-card { display: flex; flex-direction: column; gap: 6px; margin: 0 20px 18px; padding: 12px 14px; background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--radius-md); }
	.webhook-url-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
	.webhook-url { font-family: var(--font-mono); font-size: 12px; color: var(--text-secondary); word-break: break-all; }

	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	.save-bar { display: flex; justify-content: flex-end; padding: 0 20px 18px; }
	.save-btn { display: flex; align-items: center; gap: 6px; min-width: 140px; justify-content: center; }
	.btn-spinner { width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite; }

	@media (max-width: 639px) {
		.providers-page { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.fields { padding: 14px 16px; }
		.git-provider-main { flex-wrap: wrap; padding: 12px 16px; gap: 10px; }
		.git-provider-actions { flex-wrap: wrap; width: 100%; }
		.git-status-badge { display: none; }
		.pat-input-area { padding: 12px 16px; }
		.pat-input-row { flex-wrap: wrap; }
		.webhook-url-card { margin: 0 16px 14px; }
		.save-bar { padding: 0 16px 14px; }
	}
</style>
