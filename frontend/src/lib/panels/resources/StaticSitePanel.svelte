<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Service } from '$lib/api/types';
	import StaticSiteDetailPanel from '$lib/panels/StaticSiteDetailPanel.svelte';
	import GitAccountPickerPanel from './GitAccountPickerPanel.svelte';
	import GitRepoPickerPanel from './GitRepoPickerPanel.svelte';
	import GitBranchPickerPanel from './GitBranchPickerPanel.svelte';
	import { ChevronRight, Settings, Package } from '@lucide/svelte';
	import ArtifactoryPickerPanel from './ArtifactoryPickerPanel.svelte';
	import { Button, FormField, TextField, Select, Checkbox, RadioGroup, InlineAlert, Spinner, Divider, SectionLabel } from '$lib/components/ui';

	interface Props {
		projectId: string;
		orgId:     string;
		onCreated?: (service: Service) => void;
	}

	let { projectId, orgId, onCreated }: Props = $props();

	interface ConnectedAccount { id: string; label: string; host: string; token: string; provider_type: 'github' | 'gitlab' | 'bitbucket'; }

	const PROVIDER_COLORS: Record<string, string> = {
		github:    '#24292f',
		gitlab:    '#FC6D26',
		bitbucket: '#0052CC',
	};

	// ── Form base ──────────────────────────────────────────────────────────────
	let name       = $state('');
	let slug       = $state('');
	let slugEdited = $state(false);
	// Widened to string: RadioGroup binds a plain string ('git' | 'upload' | 'artifactory').
	let source     = $state<string>('git');

	// ── Artifactory source ─────────────────────────────────────────────────────
	type SelectedArtifact = { id: string; namespace_id: string; namespace_slug: string; repo: string; tag: string; kind: string };
	let selectedArtifact = $state<SelectedArtifact | null>(null);

	function openArtifactoryPicker() {
		uiStore.pushPanel({
			component: ArtifactoryPickerPanel,
			title: 'Pick Artifact',
			props: {
				kind: 'static_bundle',
				onSelect: (art: SelectedArtifact) => {
					selectedArtifact = art;
					if (!name) { name = art.repo; slug = deriveSlug(art.repo); }
					uiStore.popPanel();
				},
			},
		});
	}

	// ── Git source ─────────────────────────────────────────────────────────────
	let connectedAccounts  = $state<ConnectedAccount[]>([]);
	let accountsLoading    = $state(true);
	let selectedAccount    = $state<ConnectedAccount | null>(null);
	let selectedRepo       = $state<{ name: string; fullName: string; cloneUrl: string } | null>(null);
	let selectedBranch     = $state('main');

	// ── Build config ───────────────────────────────────────────────────────────
	// When true: no manual config — auto-detect or shipyard.json in repo
	let useShipyardJson = $state(true);
	let framework   = $state('custom');
	let buildCmd    = $state('bun run build');
	let outputDir   = $state('dist');
	let nodeVer     = $state('1');
	let installCmd  = $state('bun install');

	// Framework → sensible bun-based defaults
	const FRAMEWORK_PRESETS: Record<string, { install: string; build: string; output: string; ver: string }> = {
		sveltekit: { install: 'bun install', build: 'bun run build',      output: 'build',           ver: '1' },
		nextjs:    { install: 'bun install', build: 'bun run build',      output: 'out',             ver: '1' },
		nuxt:      { install: 'bun install', build: 'bunx nuxi generate', output: '.output/public',  ver: '1' },
		astro:     { install: 'bun install', build: 'bun run build',      output: 'dist',            ver: '1' },
		gatsby:    { install: 'bun install', build: 'bun run build',      output: 'public',          ver: '1' },
		vite:      { install: 'bun install', build: 'bun run build',      output: 'dist',            ver: '1' },
		bun:       { install: 'bun install', build: 'bun run build',      output: 'dist',            ver: '1' },
		hugo:      { install: '',            build: 'hugo',               output: 'public',          ver: '' },
		jekyll:    { install: 'bundle install', build: 'bundle exec jekyll build', output: '_site',  ver: '' },
	};

	const FRAMEWORKS = [
		{ value: 'custom',    label: 'Custom — enter commands manually' },
		{ value: 'bun',       label: 'Bun (generic JS/TS project)' },
		{ value: 'sveltekit', label: 'SvelteKit' },
		{ value: 'nextjs',    label: 'Next.js (static export)' },
		{ value: 'nuxt',      label: 'Nuxt (nuxi generate)' },
		{ value: 'astro',     label: 'Astro' },
		{ value: 'vite',      label: 'Vite' },
		{ value: 'gatsby',    label: 'Gatsby' },
		{ value: 'hugo',      label: 'Hugo' },
		{ value: 'jekyll',    label: 'Jekyll' },
	];

	function applyFrameworkPreset(f: string) {
		framework = f;
		const p = FRAMEWORK_PRESETS[f];
		if (p) {
			installCmd = p.install;
			buildCmd   = p.build;
			outputDir  = p.output;
			nodeVer    = p.ver;
		}
	}


	// ── Submission ─────────────────────────────────────────────────────────────
	let submitting = $state(false);
	let error      = $state('');

	function deriveSlug(n: string) {
		return n.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
	}

	$effect(() => {
		if (!slugEdited) slug = deriveSlug(name);
	});

	async function loadAccounts() {
		accountsLoading = true;
		const res = await api.listGitProviders(orgId);
		if (res.data) {
			connectedAccounts = res.data.map(p => ({
				id: p.id,
				label: p.name,
				host: p.provider_type === 'github' ? 'github.com' : p.provider_type === 'gitlab' ? 'gitlab.com' : 'bitbucket.org',
				token: p.token,
				provider_type: p.provider_type as any
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
					id: a.provider_type, // Picker expects 'github', 'gitlab', 'bitbucket' as id for icon resolving
					label: a.label,
					host: a.host,
					token: a.token,
				})),
				onSelect: (account: any) => {
					// Match the selected connectedAccount by token/host
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
				token:    selectedAccount.token,
				onSelect: (repo: { name: string; fullName: string; cloneUrl: string }) => {
					selectedRepo   = repo;
					selectedBranch = 'main';
					if (!name) { name = repo.name; slug = deriveSlug(repo.name); }
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
				provider:     selectedAccount.provider_type,
				token:        selectedAccount.token,
				repoFullName: selectedRepo.fullName,
				onSelect: (branch: string) => {
					selectedBranch = branch;
					uiStore.popPanel();
				},
			},
		});
	}

	function getFrameworkIcon(fw: string): string | null {
		if (fw === 'sveltekit') return 'sveltekit';
		if (fw === 'nextjs') return 'nextjs';
		if (fw === 'nuxt') return 'nuxtjs';
		if (fw === 'astro') return 'astro';
		if (fw === 'vite') return 'vite';
		if (fw === 'vue') return 'vue';
		if (fw === 'react') return 'react';
		if (fw === 'angular') return 'angular';
		if (fw === 'solid') return 'solid';
		return null;
	}

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		if (source === 'git' && !selectedRepo) {
			error = 'Please select a repository.';
			return;
		}
		if (source === 'artifactory' && !selectedArtifact) {
			error = 'Please select an artifact from the registry.';
			return;
		}
		error = '';
		submitting = true;

		const res = await api.post<Service>(`/projects/${projectId}/services`, {
			name,
			slug: slug || deriveSlug(name),
			type: 'static',
			icon: source === 'artifactory' ? 'package' : (source === 'git' && (useShipyardJson || framework === 'custom')) ? 'html5' : (getFrameworkIcon(framework) || 'html5'),
			...(source === 'git' && selectedRepo ? {
				git_repo_url: selectedRepo.cloneUrl,
				git_branch:   selectedBranch || 'main',
				git_provider_id: selectedAccount?.id ?? null,
			} : {}),
		});

		if (res.error) { error = res.error.message; submitting = false; return; }
		if (!res.data)  { uiStore.clearPanels(); return; }

		const svc = res.data;
		onCreated?.(svc);

		// Save config
		const isArtifact = source === 'artifactory';
		await api.updateStaticConfig(svc.id, {
			source: isArtifact ? 'artifact' : source,
			build_command:   (!isArtifact && source !== 'upload' && !useShipyardJson) ? buildCmd : '',
			output_dir:      (!isArtifact && source !== 'upload' && !useShipyardJson) ? outputDir : '',
			node_version:    (!isArtifact && source !== 'upload' && !useShipyardJson) ? nodeVer : '',
			install_command: (!isArtifact && source !== 'upload' && !useShipyardJson) ? installCmd : '',
			framework:       (!isArtifact && source !== 'upload' && !useShipyardJson) ? framework : 'auto',
			...(isArtifact && selectedArtifact ? {
				deploy_config: {
					artifact_namespace_id: selectedArtifact.namespace_id,
					artifact_repo:         selectedArtifact.repo,
					artifact_tag:          selectedArtifact.tag,
				},
			} : {}),
		} as any);

		submitting = false;
		uiStore.clearPanels();
		uiStore.pushPanel({
			key:       `static_site:${svc.id}`,
			component: StaticSiteDetailPanel,
			props:     { serviceId: svc.id, projectId, orgId },
			title:     svc.name,
		});
	}
</script>

<form class="form-body" onsubmit={handleSubmit}>

	<!-- Name + Slug -->
	<div class="form-section">
		<FormField label="Site name *" for="ss-name">
			<TextField id="ss-name" bind:value={name} placeholder="my-site" required />
		</FormField>
		<FormField label="Slug" for="ss-slug">
			<span class="mono-field">
				<TextField
					id="ss-slug"
					bind:value={slug}
					placeholder="auto-derived from name"
					oninput={(e) => {
						const v = (e.target as HTMLInputElement).value;
						slugEdited = v.length > 0;
					}}
				/>
			</span>
		</FormField>
	</div>

	<Divider margin="0" />

	<!-- Source toggle -->
	<div class="form-section">
		<FormField label="Source">
			<RadioGroup
				name="source"
				bind:value={source}
				options={[
					{ value: 'git', label: 'Git — build from source' },
					{ value: 'upload', label: 'Upload — pre-built files' },
					{ value: 'artifactory', label: 'Shipyard Artifactory — deploy from registry' },
				]}
			/>
		</FormField>
	</div>

	{#if source === 'git'}
		<Divider margin="0" />

		<!-- Step 1: Git account -->
		<div class="form-section">
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
						<span class="picker-value mono">{selectedRepo.fullName}</span>
					{:else}
						<span class="picker-placeholder">{selectedAccount ? 'Select repository…' : 'Select account first'}</span>
					{/if}
					<ChevronRight size={14} class="picker-chevron" />
				</button>
			</FormField>

			<!-- Step 3: Branch -->
			<FormField label="Step 3 — Branch">
				<button type="button" class="picker-btn" disabled={!selectedRepo} onclick={openBranchPicker}>
					<span class={selectedBranch ? 'picker-value mono' : 'picker-placeholder'}>
						{selectedBranch || (selectedRepo ? 'Select branch…' : 'Select repository first')}
					</span>
					<ChevronRight size={14} class="picker-chevron" />
				</button>
			</FormField>
		</div>

		<Divider margin="0" />

		<!-- Build config -->
		<div class="form-section">
			<SectionLabel>Build Config</SectionLabel>

			<!-- shipyard.json checkbox -->
			<div class="toggle-box" class:on={useShipyardJson}>
				<Checkbox bind:checked={useShipyardJson} label="Use shipyard.json or auto-detect" />
				<span class="toggle-hint">
					{#if useShipyardJson}
						Shipyard will read <code>shipyard.json</code> from your repo, or auto-detect the framework if absent.
					{:else}
						Manually specify build settings below. These are saved as the default and can still be overridden by <code>shipyard.json</code>.
					{/if}
				</span>
			</div>

			{#if !useShipyardJson}
				<div class="build-fields">
					<FormField label="Framework" for="ss-framework" hint="Selecting a framework fills in the commands below.">
						<Select
							id="ss-framework"
							value={framework}
							options={FRAMEWORKS}
							onchange={(e) => applyFrameworkPreset((e.target as HTMLSelectElement).value)}
						/>
					</FormField>
					<div class="form-row">
						<FormField label="Bun / Node version" for="ss-nodever">
							<TextField id="ss-nodever" bind:value={nodeVer} placeholder="1" />
						</FormField>
						<FormField label="Output dir" for="ss-outdir">
							<span class="mono-field">
								<TextField id="ss-outdir" bind:value={outputDir} placeholder="dist" />
							</span>
						</FormField>
					</div>
					<FormField label="Install command" for="ss-install">
						<span class="mono-field">
							<TextField id="ss-install" bind:value={installCmd} placeholder="bun install" />
						</span>
					</FormField>
					<FormField label="Build command" for="ss-build">
						<span class="mono-field">
							<TextField id="ss-build" bind:value={buildCmd} placeholder="bun run build" />
						</span>
					</FormField>
				</div>
			{/if}
		</div>
	{/if}

	{#if source === 'artifactory'}
		<Divider margin="0" />
		<div class="form-section">
			<FormField label="Artifact">
				<button type="button" class="picker-btn" onclick={openArtifactoryPicker}>
					{#if selectedArtifact}
						<Package size={13} class="picker-icon" />
						<span class="picker-value mono">{selectedArtifact.namespace_slug}/{selectedArtifact.repo}:{selectedArtifact.tag}</span>
					{:else}
						<span class="picker-placeholder">Select from Shipyard registry…</span>
					{/if}
					<ChevronRight size={14} class="picker-chevron" />
				</button>
			</FormField>
		</div>
	{/if}

	{#if error}
		<div role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{/if}

	<div class="form-actions">
		<Button
			type="submit"
			disabled={submitting || !name.trim() || (source === 'git' && !selectedRepo) || (source === 'artifactory' && !selectedArtifact)}
		>
			{#if submitting}
				Creating…
			{:else}
				Create Static Site
			{/if}
		</Button>
	</div>
</form>

<style>
	.form-body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		height: 100%;
		overflow-y: auto;
	}

	.form-section { display: flex; flex-direction: column; gap: 10px; }

	.form-row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
	}

	.mono, .mono-field :global(input) { font-family: var(--font-mono); }

	/* Picker button */
	.picker-btn {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 8px 10px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		cursor: pointer;
		text-align: left;
		width: 100%;
		min-height: 36px;
		transition: border-color var(--transition-fast), opacity var(--transition-fast);
	}
	.picker-btn:hover:not(:disabled) { border-color: var(--accent); }
	.picker-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
	.picker-btn:disabled { opacity: 0.45; cursor: default; }
	.picker-btn.loading { cursor: default; }

	.picker-placeholder { color: var(--text-dim); flex: 1; }
	.picker-value { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.selected-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

	.picker-btn :global(.picker-icon) { color: var(--text-dim); flex-shrink: 0; }
	.picker-btn :global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; margin-left: auto; }

	/* shipyard.json checkbox box */
	.toggle-box {
		display: flex;
		flex-direction: column;
		gap: 6px;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		transition: border-color var(--transition-fast), background var(--transition-fast);
	}
	.toggle-box.on {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 5%, transparent);
	}

	.toggle-hint {
		font-size: 11px;
		color: var(--text-muted);
		line-height: 1.5;
	}
	.toggle-hint code {
		font-family: var(--font-mono);
		font-size: 10px;
		background: var(--bg-base);
		padding: 0 3px;
		border-radius: 3px;
	}

	.build-fields {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
	}

	.form-actions { padding-top: 4px; }
</style>
