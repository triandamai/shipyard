<script lang="ts">
	import { CheckCircle, Copy, RefreshCw } from '@lucide/svelte';
	import type { GitProvider } from '$lib/api/types';

	interface WebhookStatus { ok: boolean; message: string }

	interface Props {
		// ── Provider card ──────────────────────────────────────────────────────────
		providers:            GitProvider[];
		loadingProviders?:    boolean;
		providerId:           string;
		providerDefaultLabel?: string;
		/** If supplied, a separate save button appears in the provider card. */
		onSaveProvider?:      () => void;
		providerSaving?:      boolean;
		providerError?:       string;
		providerSuccess?:     string;
		/** Status message shown inside the provider card (e.g. auto-register result). */
		providerWebhookStatus?: WebhookStatus | null;

		// ── Deploy-strategy card ───────────────────────────────────────────────────
		/** When false the auto-deploy toggle is hidden (StaticSite panel). */
		showAutoDeployToggle?: boolean;
		autoDeploy?:           boolean;
		strategy:              string;
		branch:                string;
		tagPattern?:           string;
		/**
		 * Branch used in the pull_request strategy.
		 * Falls back to `branch` when not provided (StaticSite / EdgeFn behaviour).
		 */
		prBranch?:             string;
		/** Disables all strategy fields (e.g. when auto-deploy is off). */
		deployDisabled?:       boolean;
		onSave:                () => void;
		saving?:               boolean;
		saveOk?:               boolean;
		saveError?:            string;
		saveSuccess?:          string;
		/** Status shown below the save button in the strategy card. */
		strategyWebhookStatus?: WebhookStatus | null;

		// ── Webhook card ───────────────────────────────────────────────────────────
		/** The URL string to display. Caller computes it (handles both token and static forms). */
		webhookUrl?:          string;
		webhookLoading?:      boolean;
		/** Show github/gitlab/gitea provider tabs (not needed for EdgeFn static URLs). */
		showProviderTabs?:    boolean;
		webhookProvider?:     string;
		webhookCopied?:       boolean;
		onCopyWebhook?:       () => void;
		/** If provided, a Rotate URL button is shown. */
		onRotateWebhook?:     () => void;
		webhookRotateConfirm?: boolean;
		isRotatingWebhook?:   boolean;
		/** Optional info message below the URL row (e.g. auto-register note). */
		autoWebhookInfo?:     string;

		// ── Repo card (optional, read-only) ────────────────────────────────────────
		repoUrl?:    string;
		repoIsLink?: boolean;
	}

	let {
		providers,
		loadingProviders     = false,
		providerId           = $bindable(),
		providerDefaultLabel = 'No account linked',
		onSaveProvider,
		providerSaving       = false,
		providerError        = '',
		providerSuccess      = '',
		providerWebhookStatus = null,

		showAutoDeployToggle = true,
		autoDeploy           = $bindable(true),
		strategy             = $bindable('push'),
		branch               = $bindable('main'),
		tagPattern           = $bindable(''),
		prBranch             = $bindable(undefined),
		deployDisabled       = false,
		onSave,
		saving               = false,
		saveOk               = false,
		saveError            = '',
		saveSuccess          = '',
		strategyWebhookStatus = null,

		webhookUrl,
		webhookLoading       = false,
		showProviderTabs     = true,
		webhookProvider      = $bindable('github'),
		webhookCopied        = false,
		onCopyWebhook,
		onRotateWebhook,
		webhookRotateConfirm = $bindable(false),
		isRotatingWebhook    = false,
		autoWebhookInfo,

		repoUrl,
		repoIsLink = false,
	}: Props = $props();

	// PR strategy branch — uses dedicated prBranch if bound, otherwise falls back to branch
	let effectivePrBranch = $derived(prBranch ?? branch);
	function setPrBranch(v: string) {
		if (prBranch !== undefined) prBranch = v;
		else branch = v;
	}
</script>

<div class="git-config-section">

	<!-- ── Provider card ──────────────────────────────────────────────────────── -->
	<div class="git-card">
		<div class="git-card-title">Linked Git Account</div>
		<p class="git-card-desc">Select the Git provider account used to clone and authenticate this repository.</p>

		<div class="git-field">
			<label class="git-label" for="gss-provider">Git Account</label>
			{#if loadingProviders}
				<div class="git-select placeholder">Loading accounts…</div>
			{:else}
				<select id="gss-provider" class="git-select" bind:value={providerId}>
					<option value="">{providerDefaultLabel}</option>
					{#each providers as p (p.id)}
						<option value={p.id}>{p.name} ({p.provider_type.toUpperCase()})</option>
					{/each}
				</select>
			{/if}
		</div>

		{#if providerError}   <p class="git-error">{providerError}</p>   {/if}
		{#if providerSuccess} <p class="git-ok">{providerSuccess}</p>     {/if}
		{#if providerWebhookStatus}
			<div class="webhook-status {providerWebhookStatus.ok ? 'success' : 'error'}">
				{providerWebhookStatus.message}
			</div>
		{/if}

		{#if onSaveProvider}
			<div class="git-save-row">
				<button class="btn btn-primary btn-sm" onclick={onSaveProvider} disabled={providerSaving}>
					{#if providerSaving}<span class="spinner-xs"></span> Saving…{:else}Link Account{/if}
				</button>
			</div>
		{/if}
	</div>

	<!-- ── Deploy-strategy card ────────────────────────────────────────────────── -->
	<div class="git-card">
		{#if showAutoDeployToggle}
			<div class="git-card-header">
				<div class="git-card-title">Auto-deploy</div>
				<label class="toggle-switch">
					<input type="checkbox" bind:checked={autoDeploy} />
					<span class="toggle-track"></span>
				</label>
			</div>
			<p class="git-card-desc">
				Automatically trigger a deployment when Shipyard receives a webhook push event.
			</p>
		{:else}
			<div class="git-card-title">Deployment Strategy</div>
			<p class="git-card-desc">Configure which Git events trigger automatic deployments.</p>
		{/if}

		<div class="git-field">
			<label class="git-label" for="gss-strategy">Strategy</label>
			<select id="gss-strategy" class="git-select" bind:value={strategy} disabled={deployDisabled}>
				<option value="push">Deploy on Branch Push</option>
				<option value="tag">Deploy on Tag Push</option>
				<option value="pull_request">Deploy on PR / MR Merge</option>
			</select>
		</div>

		{#if strategy === 'push'}
			<div class="git-field">
				<label class="git-label" for="gss-branch">Branch to watch</label>
				<div class="git-branch-row">
					<span class="git-branch-icon">⎇</span>
					<input id="gss-branch" class="git-branch-input" type="text"
						bind:value={branch} placeholder="main"
						disabled={deployDisabled} spellcheck="false" autocomplete="off" />
				</div>
				<p class="git-hint">Only pushes to this branch trigger a deployment.</p>
			</div>
		{/if}

		{#if strategy === 'tag'}
			<div class="git-field">
				<label class="git-label" for="gss-tag">Tag pattern</label>
				<div class="git-branch-row">
					<span class="git-branch-icon">🏷</span>
					<input id="gss-tag" class="git-branch-input" type="text"
						bind:value={tagPattern} placeholder="v*"
						disabled={deployDisabled} spellcheck="false" autocomplete="off" />
				</div>
				<p class="git-hint">Deploy when a tag matching this glob is pushed (e.g. <code>v*</code>).</p>
			</div>
		{/if}

		{#if strategy === 'pull_request'}
			<div class="git-field">
				<label class="git-label" for="gss-pr-branch">Target branch (PR merge)</label>
				<div class="git-branch-row">
					<span class="git-branch-icon">⎇</span>
					<input id="gss-pr-branch" class="git-branch-input" type="text"
						value={effectivePrBranch}
						oninput={(e) => setPrBranch((e.target as HTMLInputElement).value)}
						placeholder="main"
						disabled={deployDisabled} spellcheck="false" autocomplete="off" />
				</div>
				<p class="git-hint">Deploy when a pull request is merged into this branch.</p>
			</div>
		{/if}

		{#if saveError}   <p class="git-error">{saveError}</p>   {/if}
		{#if saveSuccess} <p class="git-ok">{saveSuccess}</p>     {/if}

		<div class="git-save-row">
			<button class="btn btn-primary btn-sm" onclick={onSave} disabled={saving}>
				{#if saving}<span class="spinner-xs"></span> Saving…
				{:else if saveOk}Saved
				{:else}Save{/if}
			</button>
		</div>

		{#if strategyWebhookStatus}
			<div class="webhook-status {strategyWebhookStatus.ok ? 'success' : 'error'}" style="margin-top:4px">
				{strategyWebhookStatus.message}
			</div>
		{/if}
	</div>

	<!-- ── Webhook card ────────────────────────────────────────────────────────── -->
	{#if webhookUrl !== undefined || webhookLoading}
		<div class="git-card">
			<div class="git-card-header">
				<div class="git-card-title">Webhook URL</div>
				{#if showProviderTabs}
					<div class="webhook-provider-tabs">
						{#each (['github', 'gitlab', 'gitea'] as const) as p}
							<button class:active={webhookProvider === p} onclick={() => webhookProvider = p}>
								{p.charAt(0).toUpperCase() + p.slice(1)}
							</button>
						{/each}
					</div>
				{/if}
			</div>
			<p class="git-card-desc">
				Register this URL as a webhook in your repository with the <strong>push</strong> event.
				The token in the URL authenticates the request — no secret header needed.
			</p>

			{#if webhookLoading}
				<div class="webhook-loading"><div class="spinner-xs-inline"></div> Loading…</div>
			{:else}
				<div class="webhook-url-row">
					<input class="webhook-url-input" readonly value={webhookUrl ?? ''} />
					{#if onCopyWebhook}
						<button class="webhook-copy-btn" onclick={onCopyWebhook}
							disabled={!webhookUrl || isRotatingWebhook}>
							{#if webhookCopied}
								<CheckCircle size={13} /> Copied
							{:else}
								<Copy size={13} /> Copy
							{/if}
						</button>
					{/if}
				</div>

				{#if onRotateWebhook}
					<div class="webhook-actions">
						{#if webhookRotateConfirm}
							<span class="webhook-rotate-confirm-text">Rotating invalidates the current URL. Continue?</span>
							<button class="webhook-rotate-btn danger" onclick={onRotateWebhook} disabled={isRotatingWebhook}>
								{#if isRotatingWebhook}<div class="spinner-xs-inline"></div> Rotating…{:else}Yes, rotate{/if}
							</button>
							<button class="webhook-rotate-btn" onclick={() => webhookRotateConfirm = false}>Cancel</button>
						{:else}
							<button class="webhook-rotate-btn" onclick={() => webhookRotateConfirm = true}>
								<RefreshCw size={11} /> Rotate URL
							</button>
						{/if}
					</div>
				{/if}

				{#if autoWebhookInfo}
					<div class="webhook-status info">{autoWebhookInfo}</div>
				{/if}
			{/if}
		</div>
	{/if}

	<!-- ── Repo card (read-only) ──────────────────────────────────────────────── -->
	{#if repoUrl}
		<div class="git-card">
			<div class="git-card-title">Repository</div>
			<div class="git-repo-info">
				<span class="git-repo-label">URL</span>
				{#if repoIsLink}
					<a class="git-repo-url" href={repoUrl} target="_blank" rel="noreferrer">{repoUrl}</a>
				{:else}
					<code class="git-repo-val mono">{repoUrl}</code>
				{/if}
			</div>
		</div>
	{/if}

</div>

<style>
	/* ── Layout ── */
	.git-config-section { display: flex; flex-direction: column; gap: 12px; }

	.git-card {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 14px 16px;
		display: flex; flex-direction: column; gap: 10px;
	}

	.git-card-header {
		display: flex; align-items: center; justify-content: space-between;
	}

	.git-card-title {
		font-size: 13px; font-weight: 700; color: var(--text-primary);
	}

	.git-card-desc {
		font-size: 12px; color: var(--text-muted); line-height: 1.5; margin: 0;
	}

	/* ── Form elements ── */
	.git-field { display: flex; flex-direction: column; gap: 5px; }

	.git-label {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
	}

	.git-select {
		width: 100%; padding: 7px 10px;
		border-radius: var(--radius-sm); border: 1px solid var(--border);
		background: var(--bg-surface); color: var(--text-primary);
		font-size: 12px; outline: none; cursor: pointer;
		transition: border-color var(--transition-fast);
	}
	.git-select:focus  { border-color: var(--accent); }
	.git-select:disabled { opacity: 0.5; cursor: not-allowed; }
	.git-select.placeholder { opacity: 0.6; pointer-events: none; cursor: default; }

	.git-branch-row {
		display: flex; align-items: center;
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		background: var(--bg-surface); overflow: hidden;
		transition: border-color var(--transition-fast);
	}
	.git-branch-row:focus-within { border-color: var(--accent); }

	.git-branch-icon {
		padding: 0 8px; font-size: 13px; color: var(--text-dim);
		background: var(--bg-elevated); border-right: 1px solid var(--border);
		display: flex; align-items: center; height: 32px; flex-shrink: 0;
	}

	.git-branch-input {
		flex: 1; padding: 6px 10px; border: none; outline: none;
		background: transparent; color: var(--text-primary);
		font-size: 12px; font-family: var(--font-mono);
	}
	.git-branch-input:disabled { opacity: 0.5; }

	.git-hint { font-size: 11px; color: var(--text-dim); margin: 0; line-height: 1.4; }
	.git-hint code {
		font-family: var(--font-mono); font-size: 10px;
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
		border: 1px solid var(--border);
	}

	.git-save-row { display: flex; align-items: center; gap: 8px; padding-top: 2px; }
	.git-error { font-size: 11px; color: #ef4444; margin: 0; }
	.git-ok    { font-size: 11px; color: #22c55e;  margin: 0; }

	/* ── Toggle ── */
	.toggle-switch { display: flex; align-items: center; cursor: pointer; flex-shrink: 0; }
	.toggle-switch input { display: none; }
	.toggle-track {
		width: 32px; height: 18px; border-radius: 9px;
		background: var(--border); position: relative;
		transition: background var(--transition-fast);
	}
	.toggle-track::after {
		content: ''; position: absolute; top: 2px; left: 2px;
		width: 14px; height: 14px; border-radius: 50%;
		background: white; transition: transform var(--transition-fast);
		box-shadow: 0 1px 3px rgba(0,0,0,0.3);
	}
	.toggle-switch input:checked + .toggle-track { background: var(--accent); }
	.toggle-switch input:checked + .toggle-track::after { transform: translateX(14px); }

	/* ── Webhook ── */
	.webhook-provider-tabs {
		display: flex; gap: 2px;
	}
	.webhook-provider-tabs button {
		font-size: 11px; font-weight: 500;
		padding: 3px 8px; border-radius: 4px;
		border: 1px solid transparent;
		background: none; color: var(--text-muted); cursor: pointer;
		transition: all var(--transition-fast);
	}
	.webhook-provider-tabs button:hover { color: var(--text-primary); }
	.webhook-provider-tabs button.active {
		background: var(--bg-surface); border-color: var(--border);
		color: var(--text-primary); font-weight: 600;
	}

	.webhook-url-row { display: flex; gap: 6px; align-items: center; }
	.webhook-url-input {
		flex: 1; font-family: var(--font-mono); font-size: 11px;
		color: var(--text-muted); background: var(--bg-base);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		padding: 6px 8px; outline: none; min-width: 0;
		overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
	}
	.webhook-copy-btn {
		display: flex; align-items: center; gap: 4px;
		font-size: 11px; font-weight: 500; font-family: var(--font-sans);
		padding: 5px 10px; border-radius: var(--radius-sm);
		border: 1px solid var(--border); background: var(--bg-elevated);
		color: var(--text-muted); cursor: pointer; white-space: nowrap; flex-shrink: 0;
		transition: all var(--transition-fast);
	}
	.webhook-copy-btn:hover:not(:disabled) { border-color: var(--accent); color: var(--accent); }
	.webhook-copy-btn:disabled { opacity: 0.4; cursor: not-allowed; }

	.webhook-loading {
		display: flex; align-items: center; gap: 6px;
		font-size: 12px; color: var(--text-muted);
	}

	.webhook-actions {
		display: flex; align-items: center; gap: 6px; flex-wrap: wrap;
	}
	.webhook-rotate-btn {
		display: inline-flex; align-items: center; gap: 4px;
		font-size: 11px; font-weight: 500; font-family: var(--font-sans);
		padding: 4px 9px; border-radius: 5px;
		border: 1px solid var(--border); background: var(--bg-elevated);
		color: var(--text-muted); cursor: pointer;
		transition: all var(--transition-fast);
	}
	.webhook-rotate-btn:hover:not(:disabled) { color: var(--text-primary); border-color: var(--border-hover); }
	.webhook-rotate-btn.danger:hover:not(:disabled) { color: #ef4444; border-color: #ef4444; background: rgba(239,68,68,0.08); }
	.webhook-rotate-btn:disabled { opacity: 0.4; cursor: not-allowed; }
	.webhook-rotate-confirm-text {
		font-size: 11px; color: var(--text-muted); flex-shrink: 0;
	}

	.webhook-status {
		font-size: 11px; font-weight: 500;
		padding: 5px 8px; border-radius: 5px;
		border: 1px solid;
	}
	.webhook-status.success { color: #22c55e; background: rgba(34,197,94,0.08);  border-color: rgba(34,197,94,0.2); }
	.webhook-status.error   { color: #ef4444; background: rgba(239,68,68,0.08); border-color: rgba(239,68,68,0.2); }
	.webhook-status.info    { color: var(--text-muted); background: var(--bg-base); border-color: var(--border); }

	/* ── Repo ── */
	.git-repo-info {
		display: flex; align-items: center; gap: 10px; font-size: 12px;
	}
	.git-repo-label { color: var(--text-muted); width: 36px; flex-shrink: 0; font-size: 11px; }
	.git-repo-val   { color: var(--text-primary); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
	.git-repo-url   { color: var(--accent); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; }
	.git-repo-url:hover { text-decoration: underline; }
	.mono { font-family: var(--font-mono); }

	/* ── Buttons ── */
	.btn {
		display: inline-flex; align-items: center; gap: 6px;
		font-size: 12px; font-weight: 600; font-family: var(--font-sans);
		border-radius: var(--radius-sm); cursor: pointer;
		transition: all var(--transition-fast); border: none;
		padding: 7px 14px;
	}
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-primary { background: var(--accent); color: white; }
	.btn-primary:hover:not(:disabled) { opacity: 0.88; }
	.btn-sm { padding: 5px 10px; font-size: 11px; }

	/* ── Spinners ── */
	.spinner-xs {
		display: inline-block; width: 12px; height: 12px;
		border: 2px solid rgba(255,255,255,0.4); border-top-color: white;
		border-radius: 50%; animation: spin 0.7s linear infinite;
	}
	.spinner-xs-inline {
		display: inline-block; width: 10px; height: 10px;
		border: 1.5px solid var(--border); border-top-color: var(--accent);
		border-radius: 50%; animation: spin 0.7s linear infinite;
	}
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
