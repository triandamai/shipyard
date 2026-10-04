<script lang="ts">
	import { CheckCircle, Copy, RefreshCw, GitBranch, Tag } from '@lucide/svelte';
	import { Card, FormField, TextField, Select, Toggle, Button, InlineAlert, Spinner, Tabs, KeyValueList } from '$lib/components/ui';
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
	<Card padding="14px 16px">
		<div class="git-card">
			<div class="git-card-title">Linked Git Account</div>
			<p class="git-card-desc">Select the Git provider account used to clone and authenticate this repository.</p>

			<FormField label="Git Account" for="gss-provider">
				{#if loadingProviders}
					<Select id="gss-provider" value="" disabled options={[{ value: '', label: 'Loading accounts…' }]} />
				{:else}
					<Select
						id="gss-provider"
						bind:value={providerId}
						options={[
							{ value: '', label: providerDefaultLabel },
							...providers.map((p) => ({ value: p.id, label: `${p.name} (${p.provider_type.toUpperCase()})` })),
						]}
					/>
				{/if}
			</FormField>

			{#if providerError}   <div role="alert"><InlineAlert tone="error">{providerError}</InlineAlert></div> {/if}
			{#if providerSuccess} <div role="status"><InlineAlert tone="success">{providerSuccess}</InlineAlert></div> {/if}
			{#if providerWebhookStatus}
				<div role="status">
					<InlineAlert tone={providerWebhookStatus.ok ? 'success' : 'error'}>{providerWebhookStatus.message}</InlineAlert>
				</div>
			{/if}

			{#if onSaveProvider}
				<div class="git-save-row">
					<Button variant="primary" size="sm" onclick={onSaveProvider} disabled={providerSaving}>
						{#if providerSaving}<Spinner size={12} tone="current" /> Saving…{:else}Link Account{/if}
					</Button>
				</div>
			{/if}
		</div>
	</Card>

	<!-- ── Deploy-strategy card ────────────────────────────────────────────────── -->
	<Card padding="14px 16px">
		<div class="git-card">
			{#if showAutoDeployToggle}
				<div class="git-card-header">
					<div class="git-card-title">Auto-deploy</div>
					<Toggle bind:checked={autoDeploy} label="Auto-deploy" />
				</div>
				<p class="git-card-desc">
					Automatically trigger a deployment when Shipyard receives a webhook push event.
				</p>
			{:else}
				<div class="git-card-title">Deployment Strategy</div>
				<p class="git-card-desc">Configure which Git events trigger automatic deployments.</p>
			{/if}

			<FormField label="Strategy" for="gss-strategy">
				<Select
					id="gss-strategy"
					bind:value={strategy}
					disabled={deployDisabled}
					options={[
						{ value: 'push', label: 'Deploy on Branch Push' },
						{ value: 'tag', label: 'Deploy on Tag Push' },
						{ value: 'pull_request', label: 'Deploy on PR / MR Merge' },
					]}
				/>
			</FormField>

			{#if strategy === 'push'}
				<FormField label="Branch to watch" for="gss-branch" hint="Only pushes to this branch trigger a deployment.">
					<div class="mono-field">
						<TextField id="gss-branch" type="text" bind:value={branch} placeholder="main"
							disabled={deployDisabled} spellcheck="false" autocomplete="off">
							{#snippet icon()}<GitBranch size={13} />{/snippet}
						</TextField>
					</div>
				</FormField>
			{/if}

			{#if strategy === 'tag'}
				<FormField label="Tag pattern" for="gss-tag" hint="Deploy when a tag matching this glob is pushed (e.g. v*).">
					<div class="mono-field">
						<TextField id="gss-tag" type="text" bind:value={tagPattern} placeholder="v*"
							disabled={deployDisabled} spellcheck="false" autocomplete="off">
							{#snippet icon()}<Tag size={13} />{/snippet}
						</TextField>
					</div>
				</FormField>
			{/if}

			{#if strategy === 'pull_request'}
				<FormField label="Target branch (PR merge)" for="gss-pr-branch" hint="Deploy when a pull request is merged into this branch.">
					<div class="mono-field">
						<TextField id="gss-pr-branch" type="text"
							value={effectivePrBranch}
							oninput={(e) => setPrBranch((e.target as HTMLInputElement).value)}
							placeholder="main"
							disabled={deployDisabled} spellcheck="false" autocomplete="off">
							{#snippet icon()}<GitBranch size={13} />{/snippet}
						</TextField>
					</div>
				</FormField>
			{/if}

			{#if saveError}   <div role="alert"><InlineAlert tone="error">{saveError}</InlineAlert></div> {/if}
			{#if saveSuccess} <div role="status"><InlineAlert tone="success">{saveSuccess}</InlineAlert></div> {/if}

			<div class="git-save-row">
				<Button variant="primary" size="sm" onclick={onSave} disabled={saving}>
					{#if saving}<Spinner size={12} tone="current" /> Saving…
					{:else if saveOk}Saved
					{:else}Save{/if}
				</Button>
			</div>

			{#if strategyWebhookStatus}
				<div role="status">
					<InlineAlert tone={strategyWebhookStatus.ok ? 'success' : 'error'}>{strategyWebhookStatus.message}</InlineAlert>
				</div>
			{/if}
		</div>
	</Card>

	<!-- ── Webhook card ────────────────────────────────────────────────────────── -->
	{#if webhookUrl !== undefined || webhookLoading}
		<Card padding="14px 16px">
			<div class="git-card">
				<div class="git-card-header">
					<div class="git-card-title">Webhook URL</div>
					{#if showProviderTabs}
						<Tabs
							ariaLabel="Webhook provider"
							tabs={[{ id: 'github', label: 'Github' }, { id: 'gitlab', label: 'Gitlab' }, { id: 'gitea', label: 'Gitea' }]}
							bind:value={webhookProvider}
						/>
					{/if}
				</div>
				<p class="git-card-desc">
					Register this URL as a webhook in your repository with the <strong>push</strong> event.
					The token in the URL authenticates the request — no secret header needed.
				</p>

				{#if webhookLoading}
					<div class="webhook-loading"><Spinner size={10} /> Loading…</div>
				{:else}
					<div class="webhook-url-row">
						<div class="webhook-url mono-field">
							<TextField type="text" readonly aria-label="Webhook URL" value={webhookUrl ?? ''} />
						</div>
						{#if onCopyWebhook}
							<Button variant="secondary" size="sm" onclick={onCopyWebhook}
								disabled={!webhookUrl || isRotatingWebhook}>
								{#if webhookCopied}
									<CheckCircle size={13} /> Copied
								{:else}
									<Copy size={13} /> Copy
								{/if}
							</Button>
						{/if}
					</div>

					{#if onRotateWebhook}
						<div class="webhook-actions">
							{#if webhookRotateConfirm}
								<span class="webhook-rotate-confirm-text">Rotating invalidates the current URL. Continue?</span>
								<Button variant="danger" size="sm" onclick={onRotateWebhook} disabled={isRotatingWebhook}>
									{#if isRotatingWebhook}<Spinner size={10} tone="current" /> Rotating…{:else}Yes, rotate{/if}
								</Button>
								<Button variant="secondary" size="sm" onclick={() => webhookRotateConfirm = false}>Cancel</Button>
							{:else}
								<Button variant="secondary" size="sm" onclick={() => webhookRotateConfirm = true}>
									<RefreshCw size={11} /> Rotate URL
								</Button>
							{/if}
						</div>
					{/if}

					{#if autoWebhookInfo}
						<div role="status"><InlineAlert tone="info">{autoWebhookInfo}</InlineAlert></div>
					{/if}
				{/if}
			</div>
		</Card>
	{/if}

	<!-- ── Repo card (read-only) ──────────────────────────────────────────────── -->
	{#if repoUrl}
		<Card padding="14px 16px">
			<div class="git-card">
				<div class="git-card-title">Repository</div>
				<KeyValueList keyWidth="48px" items={[{ key: 'URL', value: repoUrl, mono: !repoIsLink }]}>
					{#snippet value()}
						{#if repoIsLink}
							<a class="git-repo-url" href={repoUrl} target="_blank" rel="noreferrer">{repoUrl}</a>
						{:else}
							<code class="git-repo-val">{repoUrl}</code>
						{/if}
					{/snippet}
				</KeyValueList>
			</div>
		</Card>
	{/if}

</div>

<style>
	.git-config-section { display: flex; flex-direction: column; gap: 12px; }
	.git-card { display: flex; flex-direction: column; gap: 10px; }
	.git-card-header { display: flex; align-items: center; justify-content: space-between; gap: 8px; flex-wrap: wrap; }
	.git-card-title { font-size: 13px; font-weight: 700; color: var(--text-primary); }
	.git-card-desc { font-size: 12px; color: var(--text-muted); line-height: 1.5; margin: 0; }
	.git-save-row { display: flex; align-items: center; gap: 8px; padding-top: 2px; }

	.mono-field :global(input) { font-family: var(--font-mono); font-size: 12px; }

	.webhook-url-row { display: flex; gap: 6px; align-items: center; }
	.webhook-url { flex: 1; min-width: 0; }
	.webhook-url :global(input) { font-size: 11px; text-overflow: ellipsis; }
	.webhook-loading { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-muted); }
	.webhook-actions { display: flex; align-items: center; gap: 6px; flex-wrap: wrap; }
	.webhook-rotate-confirm-text { font-size: 11px; color: var(--text-muted); flex-shrink: 0; }

	.git-repo-val { font-family: var(--font-mono); color: var(--text-primary); font-size: 11px; overflow-wrap: anywhere; }
	.git-repo-url { color: var(--accent); font-size: 12px; overflow-wrap: anywhere; }
	.git-repo-url:hover { text-decoration: underline; }
</style>
