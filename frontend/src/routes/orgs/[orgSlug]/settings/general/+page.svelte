<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { page } from '$app/stores';
	import {
		Globe, GitBranch, Save, Check, AlertCircle, Key, Loader2,
		RefreshCw, Terminal, Zap, PackageOpen
	} from '@lucide/svelte';

	interface PlatformSettings {
		main_domain?: string;
		traefik_network?: string;
		traefik_entrypoint_http?: string;
		traefik_entrypoint_https?: string;
		traefik_cert_resolver?: string;
		git_github_token?: string;
		git_gitlab_token?: string;
		git_bitbucket_token?: string;
		git_webhook_secret?: string;
	}

	// ── Version info ────────────────────────────────────────────────────────────
	interface VersionInfo {
		current: string;
		git_sha: string;
		build_date: string;
		update_available: boolean;
		remote_sha: string | null;
	}
	let versionInfo    = $state<VersionInfo | null>(null);
	let loadingVersion = $state(false);
	let checkingUpdate = $state(false);

	function formatBuildDate(iso: string): string {
		if (!iso || iso === 'unknown') return '';
		try {
			return new Date(iso).toLocaleString('en-US', {
				year: 'numeric', month: 'short', day: 'numeric',
				hour: '2-digit', minute: '2-digit', timeZoneName: 'short'
			});
		} catch { return iso; }
	}

	async function checkForUpdates() {
		checkingUpdate = true;
		const res = await api.get<VersionInfo>('/admin/version?force=true');
		if (res.data) versionInfo = res.data;
		checkingUpdate = false;
	}

	// ── Update state ────────────────────────────────────────────────────────────
	type UpdateStatus = 'idle' | 'running' | 'done' | 'error' | 'disconnected';
	let updateStatus   = $state<UpdateStatus>('idle');
	let updateLog      = $state<string[]>([]);
	let updateLogEl    = $state<HTMLDivElement | null>(null);
	let updateSource: EventSource | null = null;

	function startUpdate() {
		if (updateStatus === 'running') return;
		updateLog = [];
		updateStatus = 'running';

		updateSource?.close();
		updateSource = new EventSource('/api/admin/update/stream');

		updateSource.onmessage = (e) => {
			if (!e.data?.trim()) return;
			updateLog = [...updateLog, e.data];
			if (updateLogEl) requestAnimationFrame(() => {
				if (updateLogEl) updateLogEl.scrollTop = updateLogEl.scrollHeight;
			});
		};

		updateSource.addEventListener('done', (e: MessageEvent) => {
			updateLog = [...updateLog, `✓ ${e.data}`];
			updateStatus = 'done';
			updateSource?.close();
			updateSource = null;
		});

		updateSource.addEventListener('error', (e: MessageEvent) => {
			if (e.data) {
				updateLog = [...updateLog, `✗ ${e.data}`];
				updateStatus = 'error';
				updateSource?.close();
				updateSource = null;
			}
		});

		// onerror fires when the SSE connection drops — expected when backend restarts
		updateSource.onerror = () => {
			if (updateStatus === 'running') {
				updateLog = [...updateLog, '⟳ Connection lost — services are restarting. Reload the page when ready.'];
				updateStatus = 'disconnected';
			}
			updateSource?.close();
			updateSource = null;
		};
	}

	function clearUpdateLog() {
		updateLog = [];
		updateStatus = 'idle';
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

	let orgId = $derived($orgStore.activeOrg?.id ?? '');

	let patInputFor = $state('');
	let patValues   = $state<Record<string, string>>({});
	let patSaving   = $state('');
	let settings    = $state<PlatformSettings>({});
	let loading     = $state(true);
	let saving      = $state(false);
	let saved       = $state(false);
	let saveError   = $state('');
	let oauthNotice = $state('');

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
		if (res.data) settings = res.data;
		patSaving = '';
		patInputFor = '';
		patValues[provider.id] = '';
	}

	async function disconnectProvider(provider: GitProviderDef) {
		const res = await api.put<PlatformSettings>('/settings', { [provider.tokenKey]: '' });
		if (res.data) settings = res.data;
	}

	function connectViaOAuth(provider: GitProviderDef) {
		const returnTo = encodeURIComponent(window.location.pathname);
		window.location.href = `/api/auth/oauth/${provider.id}?org_id=${orgId}&return_to=${returnTo}`;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true; saved = false; saveError = '';
		try {
			const res = await api.put<PlatformSettings>('/settings', settings);
			if (res.error) saveError = res.error.message;
			else { saved = true; setTimeout(() => (saved = false), 3000); }
		} finally { saving = false; }
	}

	onMount(async () => {
		const res = await api.get<PlatformSettings>('/settings');
		if (res.data) settings = res.data;
		loading = false;

		loadingVersion = true;
		const vRes = await api.get<VersionInfo>('/admin/version');
		if (vRes.data) versionInfo = vRes.data;
		loadingVersion = false;
	});
</script>

{#if loading}
	<div class="loading">
		<div class="spinner"></div>
		<span>Loading settings…</span>
	</div>
{:else}
	<form class="settings-form" onsubmit={save}>

		<!-- Main Domain -->
		<section class="settings-section">
			<div class="section-header">
				<div class="section-icon"><Globe size={16} /></div>
				<div>
					<h2 class="section-title">Main Domain</h2>
					<p class="section-desc">Base domain for all deployed services (e.g. <code>example.com</code>). Services get subdomains like <code>my-service.example.com</code>.</p>
				</div>
			</div>
			<div class="fields">
				<div class="field">
					<label class="field-label" for="main-domain">Base Domain</label>
					<input id="main-domain" class="field-input" type="text" bind:value={settings.main_domain} placeholder="example.com" />
					<span class="field-hint">Leave blank to use manual domain assignments per service.</span>
				</div>
			</div>
		</section>

		<!-- Git Integration -->
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
						</div>
						{#if showPat}
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

			<div class="fields" style="border-top: 1px solid var(--border); padding-top: 16px;">
				<div class="field">
					<label class="field-label" for="webhook-secret">Webhook Secret</label>
					<input
						id="webhook-secret"
						class="field-input font-mono"
						type="password"
						bind:value={settings.git_webhook_secret}
						placeholder="Used to verify push event signatures"
						autocomplete="off"
					/>
					<span class="field-hint">Set the same value in your GitHub/GitLab webhook configuration.</span>
				</div>
			</div>
			<div class="webhook-url-card">
				<span class="webhook-url-label">Incoming Webhook URL</span>
				<code class="webhook-url">{window.location.origin}/api/webhooks/github/:service_id/:token</code>
				<span class="field-hint">Replace <code>:service_id</code> and <code>:token</code> from the service detail panel.</span>
			</div>
		</section>

		{#if saveError}
			<div class="error-banner"><AlertCircle size={14} />{saveError}</div>
		{/if}

		<div class="save-bar">
			<button class="btn btn-primary save-btn" type="submit" disabled={saving}>
				{#if saving}<div class="btn-spinner"></div>Saving…
				{:else if saved}<Check size={14} />Saved
				{:else}<Save size={14} />Save Settings
				{/if}
			</button>
		</div>
	</form>

	<!-- Platform Update -->
	<section class="settings-section update-section">
		<div class="section-header">
			<div class="section-icon update-icon"><PackageOpen size={16} /></div>
			<div>
				<h2 class="section-title">Platform Update</h2>
				<p class="section-desc">
					Pull the latest Docker images from the registry and restart all Shipyard services.
					The connection will drop briefly while the backend restarts — that's expected.
				</p>
			</div>
		</div>

		<!-- Version info bar -->
		<div class="version-info-bar">
			{#if loadingVersion}
				<span class="version-loading">Checking version…</span>
			{:else if versionInfo}
				<div class="version-chip">
					<span class="version-label">Running</span>
					<code class="version-sha">{versionInfo.git_sha}</code>
					{#if versionInfo.build_date && versionInfo.build_date !== 'unknown'}
						<span class="version-date">{formatBuildDate(versionInfo.build_date)}</span>
					{/if}
				</div>
				{#if versionInfo.update_available && versionInfo.remote_sha}
					<span class="version-badge update-avail">Update available → <code>{versionInfo.remote_sha}</code></span>
				{:else}
					<span class="version-badge up-to-date">Up to date</span>
				{/if}
				<button class="refresh-version-btn" disabled={checkingUpdate} onclick={checkForUpdates}>
					{#if checkingUpdate}<Loader2 size={11} class="spin" />Checking…{:else}<RefreshCw size={11} />Check{/if}
				</button>
			{/if}
		</div>

		<div class="update-body">
			<div class="update-actions">
				<button
					class="btn btn-update"
					disabled={updateStatus === 'running'}
					onclick={startUpdate}
				>
					{#if updateStatus === 'running'}
						<Loader2 size={14} class="spin-icon" />Running update…
					{:else}
						<RefreshCw size={14} />Pull &amp; Restart
					{/if}
				</button>

				{#if updateStatus === 'done'}
					<span class="update-badge done"><Check size={11} />Done</span>
				{:else if updateStatus === 'error'}
					<span class="update-badge error"><AlertCircle size={11} />Failed</span>
				{:else if updateStatus === 'disconnected'}
					<span class="update-badge restarting"><Zap size={11} />Restarting…</span>
				{/if}

				{#if updateLog.length > 0 && updateStatus !== 'running'}
					<button class="clear-log-btn" onclick={clearUpdateLog}>Clear</button>
				{/if}
			</div>

			{#if updateLog.length > 0}
				<div class="update-log" bind:this={updateLogEl}>
					<div class="update-log-header">
						<Terminal size={11} />
						<span>Update output</span>
					</div>
					{#each updateLog as line, i (i)}
						<div class="update-log-line" class:log-done={line.startsWith('✓')} class:log-error={line.startsWith('✗')} class:log-restart={line.startsWith('⟳')}>
							{line}
						</div>
					{/each}
					{#if updateStatus === 'running'}
						<div class="update-log-cursor">▊</div>
					{/if}
				</div>
			{/if}

			{#if updateStatus === 'disconnected'}
				<div class="update-reconnect-hint">
					Services are restarting. Reload this page in a few seconds to confirm the update completed.
					<button class="btn btn-sm btn-secondary" onclick={() => window.location.reload()}>
						<RefreshCw size={12} />Reload now
					</button>
				</div>
			{/if}
		</div>
	</section>
{/if}

<style>
	.loading { display: flex; align-items: center; gap: 10px; color: var(--text-muted); font-size: 13px; padding: 40px 0; }
	.spinner { width: 18px; height: 18px; border: 2px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.7s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.8s linear infinite; }

	.settings-form { display: flex; flex-direction: column; gap: 20px; }

	.settings-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	.section-header {
		display: flex;
		gap: 14px;
		padding: 18px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.section-icon {
		width: 32px; height: 32px;
		border-radius: var(--radius-md);
		background: rgba(37, 99, 235, 0.1);
		color: var(--accent);
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0;
		margin-top: 1px;
	}

	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	.fields { display: flex; flex-direction: column; gap: 16px; padding: 18px 20px; }
	.fields-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 16px; padding: 18px 20px; }
	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }

	.field-input {
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		padding: 8px 10px;
		outline: none;
		transition: border-color var(--transition-fast);
	}
	.field-input.font-mono { font-family: var(--font-mono); }
	.field-input:focus { border-color: var(--accent); }
	.field-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; }
	.field-hint code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 10px; }

	code { font-family: var(--font-mono); background: var(--bg-elevated); padding: 1px 4px; border-radius: 3px; font-size: 12px; }

	.webhook-url-card { display: flex; flex-direction: column; gap: 6px; margin: 0 20px 18px; padding: 12px 14px; background: var(--bg-base); border: 1px solid var(--border); border-radius: var(--radius-md); }
	.webhook-url-label { font-size: 11px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.06em; }
	.webhook-url { font-family: var(--font-mono); font-size: 12px; color: var(--text-secondary); word-break: break-all; }

	.error-banner { display: flex; align-items: center; gap: 8px; padding: 10px 14px; background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.25); border-radius: var(--radius-md); color: #EF4444; font-size: 13px; }

	.save-bar { display: flex; justify-content: flex-end; padding: 4px 0 8px; }
	.save-btn { display: flex; align-items: center; gap: 6px; min-width: 140px; justify-content: center; }
	.btn-spinner { width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3); border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite; }

	/* Git providers */
	.git-provider-list { display: flex; flex-direction: column; border-top: 1px solid var(--border); }
	.git-provider-item { border-bottom: 1px solid var(--border); transition: background var(--transition-fast); }
	.git-provider-item.connected { background: color-mix(in srgb, #22C55E 3%, transparent); }
	.git-provider-main { display: flex; align-items: center; gap: 14px; padding: 14px 20px; }
	.git-provider-logo { width: 36px; height: 36px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
	.git-provider-info { display: flex; flex-direction: column; gap: 2px; flex: 1; min-width: 0; }
	.git-provider-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.git-provider-host { font-size: 12px; color: var(--text-dim); font-family: var(--font-mono); }
	.git-provider-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
	.git-status-badge { font-size: 11px; font-weight: 500; white-space: nowrap; }
	.git-status-badge.connected { color: #22C55E; }
	.git-status-badge.disconnected { color: var(--text-dim); }

	.oauth-notice { padding: 10px 14px; border-radius: var(--radius-sm); font-size: 13px; font-weight: 500; margin-bottom: 12px; }
	.oauth-notice.success { background: color-mix(in srgb, #22C55E 12%, transparent); color: #16A34A; border: 1px solid color-mix(in srgb, #22C55E 30%, transparent); }
	.oauth-notice.error { background: color-mix(in srgb, #EF4444 10%, transparent); color: #DC2626; border: 1px solid color-mix(in srgb, #EF4444 30%, transparent); }

	.git-action-btn { font-size: 12px; font-weight: 500; padding: 5px 12px; border-radius: var(--radius-sm); cursor: pointer; transition: all var(--transition-fast); border: 1px solid transparent; display: inline-flex; align-items: center; gap: 5px; }
	.git-action-btn.connect { background: var(--accent); color: white; border-color: var(--accent); }
	.git-action-btn.connect:hover:not(:disabled) { opacity: 0.85; }
	.git-action-btn.reconfigure { background: transparent; color: var(--text-muted); border-color: var(--border); }
	.git-action-btn.reconfigure:hover { border-color: var(--accent); color: var(--accent); }
	.git-action-btn.disconnect { background: transparent; color: #EF4444; border-color: rgba(239,68,68,0.4); }
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

	/* ── Platform Update ── */
	.update-section { margin-top: 8px; }
	.update-icon { background: rgba(139, 92, 246, 0.12); color: #8B5CF6; }

	.version-info-bar {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-base);
		flex-wrap: wrap;
		min-height: 42px;
	}
	.version-chip { display: flex; align-items: center; gap: 6px; }
	.version-label { font-size: 10px; font-weight: 600; color: var(--text-dim); text-transform: uppercase; letter-spacing: 0.07em; }
	.version-sha {
		font-family: var(--font-mono);
		font-size: 12px;
		background: var(--bg-elevated);
		color: var(--text-primary);
		padding: 2px 7px;
		border-radius: 4px;
		border: 1px solid var(--border);
	}
	.version-date { font-size: 11px; color: var(--text-dim); }
	.version-loading { font-size: 12px; color: var(--text-dim); }
	.version-badge {
		font-size: 11px;
		font-weight: 600;
		padding: 2px 9px;
		border-radius: 99px;
	}
	.version-badge code { font-family: var(--font-mono); font-size: 11px; }
	.version-badge.update-avail { background: rgba(245,158,11,0.12); color: #D97706; border: 1px solid rgba(245,158,11,0.3); }
	.version-badge.up-to-date   { background: rgba(16,185,129,0.10); color: #10B981; border: 1px solid rgba(16,185,129,0.25); }
	.refresh-version-btn {
		display: inline-flex;
		align-items: center;
		gap: 4px;
		margin-left: auto;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-muted);
		font-size: 11px;
		font-family: var(--font-sans);
		padding: 3px 9px;
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.refresh-version-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.refresh-version-btn:disabled { opacity: 0.5; cursor: default; }

	.update-body {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 18px 20px;
	}

	.update-actions {
		display: flex;
		align-items: center;
		gap: 10px;
		flex-wrap: wrap;
	}

	.btn-update {
		display: inline-flex;
		align-items: center;
		gap: 7px;
		padding: 8px 16px;
		background: #8B5CF6;
		color: white;
		border: none;
		border-radius: var(--radius-sm);
		font-size: 13px;
		font-weight: 500;
		font-family: var(--font-sans);
		cursor: pointer;
		transition: opacity var(--transition-fast), background var(--transition-fast);
	}
	.btn-update:hover:not(:disabled) { background: #7C3AED; }
	.btn-update:disabled { opacity: 0.55; cursor: default; }

	:global(.spin-icon) { animation: spin 0.8s linear infinite; }

	.update-badge {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		padding: 3px 10px;
		border-radius: 99px;
		font-size: 11px;
		font-weight: 600;
	}
	.update-badge.done       { background: rgba(16,185,129,0.12); color: #10B981; border: 1px solid rgba(16,185,129,0.3); }
	.update-badge.error      { background: rgba(239,68,68,0.10);  color: #EF4444; border: 1px solid rgba(239,68,68,0.3); }
	.update-badge.restarting { background: rgba(245,158,11,0.12); color: #F59E0B; border: 1px solid rgba(245,158,11,0.3); }

	.clear-log-btn {
		margin-left: auto;
		background: transparent;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-muted);
		font-size: 11px;
		padding: 3px 10px;
		cursor: pointer;
		font-family: var(--font-sans);
		transition: all var(--transition-fast);
	}
	.clear-log-btn:hover { border-color: var(--accent); color: var(--accent); }

	.update-log {
		background: #0d1117;
		border: 1px solid #30363d;
		border-radius: var(--radius-md);
		overflow-y: auto;
		max-height: 340px;
		font-family: var(--font-mono);
		font-size: 12px;
	}

	.update-log-header {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 7px 12px;
		border-bottom: 1px solid #21262d;
		color: #8b949e;
		font-size: 11px;
		font-family: var(--font-sans);
	}

	.update-log-line {
		padding: 2px 14px;
		color: #e6edf3;
		white-space: pre-wrap;
		word-break: break-all;
		line-height: 1.6;
	}
	.update-log-line.log-done    { color: #3fb950; }
	.update-log-line.log-error   { color: #f85149; }
	.update-log-line.log-restart { color: #d29922; }

	.update-log-cursor {
		padding: 2px 14px 6px;
		color: #e6edf3;
		animation: blink 1s step-end infinite;
	}
	@keyframes blink { 0%, 100% { opacity: 1; } 50% { opacity: 0; } }

	.update-reconnect-hint {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 10px 14px;
		background: rgba(245,158,11,0.08);
		border: 1px solid rgba(245,158,11,0.25);
		border-radius: var(--radius-md);
		color: #D97706;
		font-size: 12px;
		flex-wrap: wrap;
	}

	/* ── Responsive ── */
	@media (max-width: 639px) {
		.settings-form { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.fields { padding: 14px 16px; }
		.fields-grid { grid-template-columns: 1fr; padding: 14px 16px; }
		.git-provider-main { flex-wrap: wrap; padding: 12px 16px; gap: 10px; }
		.git-provider-actions { flex-wrap: wrap; width: 100%; }
		.git-status-badge { display: none; }
		.pat-input-area { padding: 12px 16px; }
		.pat-input-row { flex-wrap: wrap; }
		.webhook-url-card { margin: 0 16px 14px; }
		.save-bar { padding: 0 0 4px; }
	}
</style>
