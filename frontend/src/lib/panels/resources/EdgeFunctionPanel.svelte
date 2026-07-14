<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { ChevronRight, Settings, CheckCircle, Zap, Package } from '@lucide/svelte';
	import GitAccountPickerPanel from './GitAccountPickerPanel.svelte';
	import GitRepoPickerPanel from './GitRepoPickerPanel.svelte';
	import GitBranchPickerPanel from './GitBranchPickerPanel.svelte';
	import ArtifactoryPickerPanel from './ArtifactoryPickerPanel.svelte';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: () => void;
	}

	let { projectId, orgId, onCreated }: Props = $props();

	interface ConnectedAccount {
		id: string;
		label: string;
		host: string;
		token: string;
		provider_type: 'github' | 'gitlab' | 'bitbucket';
	}

	const PROVIDER_COLORS: Record<string, string> = {
		github:    '#24292f',
		gitlab:    '#FC6D26',
		bitbucket: '#0052CC',
	};

	let source = $state<'git' | 'artifactory'>('git');

	let connectedAccounts = $state<ConnectedAccount[]>([]);
	let accountsLoading   = $state(true);
	let selectedAccount   = $state<ConnectedAccount | null>(null);
	let selectedRepo      = $state<{ name: string; fullName: string; cloneUrl: string } | null>(null);
	let selectedBranch    = $state('main');

	type SelectedArtifact = { id: string; namespace_id: string; namespace_slug: string; repo: string; tag: string; kind: string };
	let selectedArtifact = $state<SelectedArtifact | null>(null);

	let isSubmitting = $state(false);
	let submitError  = $state('');
	let created      = $state(false);
	let createdGroupId = $state('');

	function openArtifactoryPicker() {
		uiStore.pushPanel({
			component: ArtifactoryPickerPanel,
			title: 'Pick Edge Function Artifact',
			props: {
				kind: 'edge_function',
				onSelect: (art: SelectedArtifact) => {
					selectedArtifact = art;
					uiStore.popPanel();
				},
			},
		});
	}

	async function loadAccounts() {
		accountsLoading = true;
		const res = await api.listGitProviders(orgId);
		if (res.data) {
			connectedAccounts = res.data.map(p => ({
				id: p.id,
				label: p.name,
				host: p.provider_type === 'github' ? 'github.com' : p.provider_type === 'gitlab' ? 'gitlab.com' : 'bitbucket.org',
				token: p.token,
				provider_type: p.provider_type as any,
			}));
			if (connectedAccounts.length === 1) selectedAccount = connectedAccounts[0];
		}
		accountsLoading = false;
	}

	onMount(loadAccounts);

	function openAccountPicker() {
		uiStore.pushPanel({
			component: GitAccountPickerPanel,
			title: 'Select Git Account',
			props: {
				accounts: connectedAccounts.map(a => ({
					id: a.provider_type,
					label: a.label,
					host: a.host,
					token: a.token,
				})),
				onSelect: (account: any) => {
					const matched = connectedAccounts.find(a => a.token === account.token);
					if (matched) {
						selectedAccount = matched;
						selectedRepo    = null;
						selectedBranch  = 'main';
					}
					uiStore.popPanel();
				},
			},
		});
	}

	function openRepoPicker() {
		if (!selectedAccount) return;
		uiStore.pushPanel({
			component: GitRepoPickerPanel,
			title: 'Select Repository',
			props: {
				provider: selectedAccount.provider_type,
				token: selectedAccount.token,
				onSelect: (repo: { name: string; fullName: string; cloneUrl: string }) => {
					selectedRepo   = repo;
					selectedBranch = 'main';
					uiStore.popPanel();
				},
			},
		});
	}

	function openBranchPicker() {
		if (!selectedAccount || !selectedRepo) return;
		uiStore.pushPanel({
			component: GitBranchPickerPanel,
			title: 'Select Branch',
			props: {
				provider: selectedAccount.provider_type,
				token: selectedAccount.token,
				repoFullName: selectedRepo.fullName,
				onSelect: (branch: string) => {
					selectedBranch = branch;
					uiStore.popPanel();
				},
			},
		});
	}

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		if (source === 'git' && (!selectedAccount || !selectedRepo)) {
			submitError = 'Please select an account and repository.';
			return;
		}
		if (source === 'artifactory' && !selectedArtifact) {
			submitError = 'Please select an artifact from the registry.';
			return;
		}
		submitError  = '';
		isSubmitting = true;
		try {
			const body = source === 'artifactory'
				? {
					provider:              'artifact',
					project_id:            projectId || null,
					artifact_namespace_id: selectedArtifact!.namespace_id,
					artifact_repo:         selectedArtifact!.repo,
					artifact_tag:          selectedArtifact!.tag,
				}
				: {
					provider:        selectedAccount!.provider_type,
					repo_url:        selectedRepo!.cloneUrl,
					branch:          selectedBranch || 'main',
					project_id:      projectId || null,
					git_provider_id: selectedAccount!.id,
				};
			const res = await api.post<{ id: string }>(`/orgs/${orgId}/edge-functions/groups`, body);
			if (res.error) { submitError = res.error.message; return; }
			createdGroupId = res.data?.id ?? '';
			created = true;
			onCreated?.();
		} finally {
			isSubmitting = false;
		}
	}
</script>

<div class="panel-wrap">
	{#if created}
		<div class="success-wrap">
			<div class="success-icon"><CheckCircle size={40} /></div>
			<p class="success-title">Edge functions deployed!</p>
			<p class="success-sub">
				Functions detected in <span class="mono">{selectedRepo?.fullName}</span> are now
				active. Shipyard will redeploy on every push to
				<span class="mono">{selectedBranch}</span>.
			</p>
			<button class="btn btn-secondary" onclick={() => uiStore.clearPanels()}>Close</button>
		</div>
	{:else}
		<form class="form" onsubmit={handleSubmit}>
			<div class="form-hint-box">
				<Zap size={13} />
				<span>
					Shipyard detects functions in <span class="mono">functions/</span> or from
					<span class="mono">shipyard.json</span>. No Dockerfile needed.
				</span>
			</div>

			<div class="divider"></div>

			<!-- Source selection -->
			<div class="form-group">
				<label class="form-label">Source</label>
				<div class="source-options">
					<label class="source-opt" class:active={source === 'git'}>
						<input type="radio" name="ef-source" value="git" bind:group={source} />
						<span class="opt-label">Git repository</span>
					</label>
					<label class="source-opt" class:active={source === 'artifactory'}>
						<input type="radio" name="ef-source" value="artifactory" bind:group={source} />
						<span class="opt-label">Shipyard Artifactory</span>
					</label>
				</div>
			</div>

			{#if source === 'artifactory'}
				<div class="form-group">
					<label class="form-label">Artifact</label>
					<button type="button" class="picker-btn" onclick={openArtifactoryPicker}>
						{#if selectedArtifact}
							<Package size={13} style="color:#f59e0b;flex-shrink:0" />
							<span class="picker-value font-mono">{selectedArtifact.namespace_slug}/{selectedArtifact.repo}:{selectedArtifact.tag}</span>
						{:else}
							<span class="picker-placeholder">Select from Shipyard registry…</span>
						{/if}
						<ChevronRight size={14} class="picker-chevron" />
					</button>
				</div>
			{/if}

			{#if source === 'git'}
			<div class="divider"></div>

			<!-- Step 1: Account -->
			<div class="form-group">
				<label class="form-label">Step 1 — Git Account</label>
				{#if accountsLoading}
					<div class="picker-btn loading">
						<div class="mini-spinner"></div>
						<span>Loading accounts…</span>
					</div>
				{:else if connectedAccounts.length === 0}
					<a class="no-accounts-link" href="/orgs/{orgId}/settings">
						<Settings size={13} />
						No Git providers connected — click to open Settings
					</a>
				{:else}
					<button type="button" class="picker-btn" onclick={openAccountPicker}>
						{#if selectedAccount}
							<span class="selected-dot" style="background:{PROVIDER_COLORS[selectedAccount.provider_type]}"></span>
							<span class="picker-value">{selectedAccount.label} — {selectedAccount.host}</span>
						{:else}
							<span class="picker-placeholder">Select account…</span>
						{/if}
						<ChevronRight size={14} class="picker-chevron" />
					</button>
				{/if}
			</div>

			<!-- Step 2: Repository -->
			<div class="form-group">
				<label class="form-label" class:dimmed={!selectedAccount}>Step 2 — Repository</label>
				<button type="button" class="picker-btn" disabled={!selectedAccount} onclick={openRepoPicker}>
					{#if selectedRepo}
						<span class="picker-value font-mono">{selectedRepo.fullName}</span>
					{:else}
						<span class="picker-placeholder">{selectedAccount ? 'Select repository…' : 'Select account first'}</span>
					{/if}
					<ChevronRight size={14} class="picker-chevron" />
				</button>
			</div>

			<!-- Step 3: Branch -->
			<div class="form-group">
				<label class="form-label" class:dimmed={!selectedRepo}>Step 3 — Branch</label>
				<button type="button" class="picker-btn" disabled={!selectedRepo} onclick={openBranchPicker}>
					{#if selectedBranch}
						<span class="picker-value font-mono">{selectedBranch}</span>
					{:else}
						<span class="picker-placeholder">{selectedRepo ? 'Select branch…' : 'Select repository first'}</span>
					{/if}
					<ChevronRight size={14} class="picker-chevron" />
				</button>
			</div>

			{/if}<!-- end {#if source === 'git'} -->

			{#if submitError}
				<div class="error-msg">{submitError}</div>
			{/if}

			<button class="btn btn-primary submit-btn" type="submit"
				disabled={isSubmitting || (source === 'git' && !selectedRepo) || (source === 'artifactory' && !selectedArtifact)}
			>
				{#if isSubmitting}
					<div class="btn-spinner"></div> Deploying…
				{:else}
					Deploy Edge Functions
				{/if}
			</button>
		</form>
	{/if}
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 14px; }
	.form-group { display: flex; flex-direction: column; gap: 4px; }

	.form-hint-box {
		display: flex; align-items: flex-start; gap: 8px;
		font-size: 12px; color: var(--text-dim); line-height: 1.5;
		padding: 10px 12px;
		background: color-mix(in srgb, var(--accent) 6%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
		border-radius: var(--radius-sm);
	}
	.form-hint-box :global(svg) { flex-shrink: 0; margin-top: 1px; color: var(--accent); }
	.mono { font-family: var(--font-mono); font-size: 11px; }

	.form-label {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
		transition: color var(--transition-fast);
	}
	.form-label.dimmed { color: color-mix(in srgb, var(--text-dim) 50%, transparent); }

	.divider { height: 1px; background: var(--border); margin: 2px 0; }

	.picker-btn {
		display: flex; align-items: center; gap: 8px;
		padding: 8px 10px; background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px;
		font-family: var(--font-sans); cursor: pointer; text-align: left; width: 100%;
		transition: border-color var(--transition-fast), opacity var(--transition-fast);
		min-height: 36px;
	}
	.picker-btn:hover:not(:disabled) { border-color: var(--accent); }
	.picker-btn:disabled { opacity: 0.45; cursor: default; }
	.picker-btn.loading { cursor: default; }

	.picker-placeholder { color: var(--text-dim); flex: 1; }
	.picker-value { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.font-mono { font-family: var(--font-mono); }

	.selected-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
	:global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; margin-left: auto; }

	.mini-spinner {
		width: 12px; height: 12px; border: 2px solid var(--border);
		border-top-color: var(--accent); border-radius: 50%;
		animation: spin 0.7s linear infinite; flex-shrink: 0;
	}

	.no-accounts-link {
		display: flex; align-items: center; gap: 6px;
		font-size: 12px; color: var(--accent); text-decoration: none;
		padding: 8px 10px; background: color-mix(in srgb, var(--accent) 6%, transparent);
		border: 1px dashed color-mix(in srgb, var(--accent) 40%, transparent);
		border-radius: var(--radius-sm);
	}
	.no-accounts-link:hover { text-decoration: underline; }

	.source-options { display: flex; flex-direction: column; gap: 6px; }
	.source-opt {
		display: flex; align-items: center; gap: 8px;
		padding: 9px 11px; border: 1px solid var(--border); border-radius: var(--radius-sm);
		cursor: pointer; font-size: 12px; font-weight: 500; color: var(--text-primary);
		transition: all var(--transition-fast);
	}
	.source-opt input[type="radio"] { cursor: pointer; }
	.source-opt.active {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 6%, transparent);
	}
	.opt-label { font-size: 12px; font-weight: 500; }

	.error-msg {
		font-size: 12px; color: var(--accent-red); padding: 8px 10px;
		background: color-mix(in srgb, var(--accent-red) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent-red) 30%, transparent);
		border-radius: var(--radius-sm);
	}

	.submit-btn { margin-top: 4px; display: flex; align-items: center; gap: 6px; justify-content: center; }
	.btn-spinner {
		width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3);
		border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite;
	}

	/* Success state */
	.success-wrap {
		display: flex; flex-direction: column; align-items: center;
		gap: 12px; padding: 32px 16px; text-align: center;
	}
	.success-icon { color: var(--accent-green); }
	.success-title { font-size: 16px; font-weight: 700; color: var(--text-primary); margin: 0; }
	.success-sub { font-size: 13px; color: var(--text-dim); line-height: 1.6; margin: 0; max-width: 280px; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
