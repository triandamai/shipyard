<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { ChevronRight, Settings, CheckCircle, Zap, Package } from '@lucide/svelte';
	import GitAccountPickerPanel from './GitAccountPickerPanel.svelte';
	import GitRepoPickerPanel from './GitRepoPickerPanel.svelte';
	import GitBranchPickerPanel from './GitBranchPickerPanel.svelte';
	import ArtifactoryPickerPanel from './ArtifactoryPickerPanel.svelte';
	import { Button, FormField, RadioGroup, InlineAlert, Spinner, Divider } from '$lib/components/ui';

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

	// Widened to string: RadioGroup binds a plain string ('git' | 'artifactory').
	let source = $state<string>('git');

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
			<Button variant="secondary" onclick={() => uiStore.clearPanels()}>Close</Button>
		</div>
	{:else}
		<form class="form" onsubmit={handleSubmit}>
			<InlineAlert tone="info">
				<div class="hint-box">
					<Zap size={13} />
					<span>
						Shipyard detects functions in <span class="mono">functions/</span> or from
						<span class="mono">shipyard.json</span>. No Dockerfile needed.
					</span>
				</div>
			</InlineAlert>

			<Divider margin="2px 0" />

			<!-- Source selection -->
			<FormField label="Source">
				<RadioGroup
					name="ef-source"
					bind:value={source}
					options={[
						{ value: 'git', label: 'Git repository' },
						{ value: 'artifactory', label: 'Shipyard Artifactory' },
					]}
				/>
			</FormField>

			{#if source === 'artifactory'}
				<FormField label="Artifact">
					<button type="button" class="picker-btn" onclick={openArtifactoryPicker}>
						{#if selectedArtifact}
							<Package size={13} class="picker-icon" />
							<span class="picker-value font-mono">{selectedArtifact.namespace_slug}/{selectedArtifact.repo}:{selectedArtifact.tag}</span>
						{:else}
							<span class="picker-placeholder">Select from Shipyard registry…</span>
						{/if}
						<ChevronRight size={14} class="picker-chevron" />
					</button>
				</FormField>
			{/if}

			{#if source === 'git'}
			<Divider margin="2px 0" />

			<!-- Step 1: Account -->
			<FormField label="Step 1 — Git Account">
				{#if accountsLoading}
					<div class="picker-btn loading">
						<Spinner size={12} />
						<span>Loading accounts…</span>
					</div>
				{:else if connectedAccounts.length === 0}
					<Button variant="secondary" href="/orgs/{orgId}/settings">
						<Settings size={13} />
						No Git providers connected — click to open Settings
					</Button>
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
			</FormField>

			<!-- Step 2: Repository -->
			<FormField label="Step 2 — Repository">
				<button type="button" class="picker-btn" disabled={!selectedAccount} onclick={openRepoPicker}>
					{#if selectedRepo}
						<span class="picker-value font-mono">{selectedRepo.fullName}</span>
					{:else}
						<span class="picker-placeholder">{selectedAccount ? 'Select repository…' : 'Select account first'}</span>
					{/if}
					<ChevronRight size={14} class="picker-chevron" />
				</button>
			</FormField>

			<!-- Step 3: Branch -->
			<FormField label="Step 3 — Branch">
				<button type="button" class="picker-btn" disabled={!selectedRepo} onclick={openBranchPicker}>
					{#if selectedBranch}
						<span class="picker-value font-mono">{selectedBranch}</span>
					{:else}
						<span class="picker-placeholder">{selectedRepo ? 'Select branch…' : 'Select repository first'}</span>
					{/if}
					<ChevronRight size={14} class="picker-chevron" />
				</button>
			</FormField>

			{/if}<!-- end {#if source === 'git'} -->

			{#if submitError}
				<div role="alert"><InlineAlert tone="error">{submitError}</InlineAlert></div>
			{/if}

			<Button type="submit"
				disabled={isSubmitting || (source === 'git' && !selectedRepo) || (source === 'artifactory' && !selectedArtifact)}
			>
				{#if isSubmitting}
					<Spinner size={12} tone="current" /> Deploying…
				{:else}
					Deploy Edge Functions
				{/if}
			</Button>
		</form>
	{/if}
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 14px; }

	.hint-box { display: flex; align-items: flex-start; gap: 8px; }
	.hint-box :global(svg) { flex-shrink: 0; margin-top: 2px; }
	.mono { font-family: var(--font-mono); font-size: 11px; }

	.picker-btn {
		display: flex; align-items: center; gap: 8px;
		padding: 8px 10px; background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px;
		font-family: var(--font-sans); cursor: pointer; text-align: left; width: 100%;
		transition: border-color var(--transition-fast), opacity var(--transition-fast);
		min-height: 36px;
	}
	.picker-btn:hover:not(:disabled) { border-color: var(--accent); }
	.picker-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
	.picker-btn:disabled { opacity: 0.45; cursor: default; }
	.picker-btn.loading { cursor: default; }

	.picker-placeholder { color: var(--text-dim); flex: 1; }
	.picker-value { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.font-mono { font-family: var(--font-mono); }

	.selected-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
	:global(.picker-icon) { color: var(--text-dim); flex-shrink: 0; }
	:global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; margin-left: auto; }

	/* Success state */
	.success-wrap {
		display: flex; flex-direction: column; align-items: center;
		gap: 12px; padding: 32px 16px; text-align: center;
	}
	.success-icon { color: var(--accent-green); }
	.success-title { font-size: 16px; font-weight: 700; color: var(--text-primary); margin: 0; }
	.success-sub { font-size: 13px; color: var(--text-dim); line-height: 1.6; margin: 0; max-width: 280px; }
</style>
