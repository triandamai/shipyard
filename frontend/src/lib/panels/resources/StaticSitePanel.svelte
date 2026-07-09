<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Service } from '$lib/api/types';
	import StaticSiteDetailPanel from '$lib/panels/StaticSiteDetailPanel.svelte';
	import GitAccountPickerPanel from './GitAccountPickerPanel.svelte';
	import GitRepoPickerPanel from './GitRepoPickerPanel.svelte';
	import GitBranchPickerPanel from './GitBranchPickerPanel.svelte';
	import { ChevronRight, Settings } from '@lucide/svelte';

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
	let source     = $state<'git' | 'upload'>('git');

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
		error = '';
		submitting = true;

		const res = await api.post<Service>(`/projects/${projectId}/services`, {
			name,
			slug: slug || deriveSlug(name),
			type: 'static',
			icon: source === 'git' && (useShipyardJson || framework === 'custom') ? null : getFrameworkIcon(framework),
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
		await api.updateStaticConfig(svc.id, {
			source,
			build_command:   source === 'upload' || useShipyardJson ? '' : buildCmd,
			output_dir:      source === 'upload' || useShipyardJson ? '' : outputDir,
			node_version:    source === 'upload' || useShipyardJson ? '' : nodeVer,
			install_command: source === 'upload' || useShipyardJson ? '' : installCmd,
			framework:       source === 'upload' || useShipyardJson ? 'auto' : framework,
		});

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
		<div class="form-group">
			<label class="form-label">Site name <span class="required">*</span></label>
			<input class="form-input" bind:value={name} placeholder="my-site" required />
		</div>
		<div class="form-group">
			<label class="form-label">Slug</label>
			<input
				class="form-input mono"
				bind:value={slug}
				placeholder="auto-derived from name"
				oninput={(e) => {
					const v = (e.target as HTMLInputElement).value;
					slugEdited = v.length > 0;
				}}
			/>
		</div>
	</div>

	<div class="divider"></div>

	<!-- Source toggle -->
	<div class="form-section">
		<div class="section-title">Source</div>
		<div class="source-options">
			<label class="source-opt" class:active={source === 'git'}>
				<input type="radio" name="source" value="git" bind:group={source} />
				<span class="opt-label">Git — build from source</span>
			</label>
			<label class="source-opt" class:active={source === 'upload'}>
				<input type="radio" name="source" value="upload" bind:group={source} />
				<span class="opt-label">Upload — pre-built files</span>
			</label>
		</div>
	</div>

	{#if source === 'git'}
		<div class="divider"></div>

		<!-- Step 1: Git account -->
		<div class="form-section">
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
						<span class="picker-value mono">{selectedRepo.fullName}</span>
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
					<span class="{selectedBranch ? 'picker-value mono' : 'picker-placeholder'}">
						{selectedBranch || (selectedRepo ? 'Select branch…' : 'Select repository first')}
					</span>
					<ChevronRight size={14} class="picker-chevron" />
				</button>
			</div>
		</div>

		<div class="divider"></div>

		<!-- Build config -->
		<div class="form-section">
			<div class="section-title">Build Config</div>

			<!-- shipyard.json checkbox -->
			<label class="toggle-row">
				<input type="checkbox" bind:checked={useShipyardJson} />
				<div class="toggle-text">
					<span class="toggle-label">Use <code>shipyard.json</code> or auto-detect</span>
					<span class="toggle-hint">
						{#if useShipyardJson}
							Shipyard will read <code>shipyard.json</code> from your repo, or auto-detect the framework if absent.
						{:else}
							Manually specify build settings below. These are saved as the default and can still be overridden by <code>shipyard.json</code>.
						{/if}
					</span>
				</div>
			</label>

			{#if !useShipyardJson}
				<div class="build-fields">
					<div class="form-group">
						<label class="form-label">Framework</label>
						<select
							class="form-select"
							value={framework}
							onchange={(e) => applyFrameworkPreset((e.target as HTMLSelectElement).value)}
						>
							{#each FRAMEWORKS as f (f.value)}
								<option value={f.value}>{f.label}</option>
							{/each}
						</select>
						<span class="field-hint">Selecting a framework fills in the commands below.</span>
					</div>
					<div class="form-row">
						<div class="form-group">
							<label class="form-label">Bun / Node version</label>
							<input class="form-input" bind:value={nodeVer} placeholder="1" />
						</div>
						<div class="form-group">
							<label class="form-label">Output dir</label>
							<input class="form-input mono" bind:value={outputDir} placeholder="dist" />
						</div>
					</div>
					<div class="form-group">
						<label class="form-label">Install command</label>
						<input class="form-input mono" bind:value={installCmd} placeholder="bun install" />
					</div>
					<div class="form-group">
						<label class="form-label">Build command</label>
						<input class="form-input mono" bind:value={buildCmd} placeholder="bun run build" />
					</div>
				</div>
			{/if}
		</div>
	{/if}

	{#if error}
		<div class="form-error">{error}</div>
	{/if}

	<div class="form-actions">
		<button
			type="submit"
			class="btn-primary"
			disabled={submitting || !name.trim() || (source === 'git' && !selectedRepo)}
		>
			{#if submitting}
				Creating…
			{:else}
				Create Static Site
			{/if}
		</button>
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

	.section-title {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.divider { height: 1px; background: var(--border); }

	.form-group { display: flex; flex-direction: column; gap: 4px; }

	.form-row {
		display: grid;
		grid-template-columns: 1fr 1fr;
		gap: 10px;
	}

	.form-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		transition: color var(--transition-fast);
	}
	.form-label.dimmed { color: color-mix(in srgb, var(--text-dim) 50%, transparent); }

	.required { color: var(--accent-red, #ef4444); }

	.form-input, .form-select {
		padding: 8px 10px;
		font-size: 13px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		color: var(--text-primary);
		outline: none;
		font-family: var(--font-sans);
		transition: border-color var(--transition-fast);
	}
	.form-input:focus, .form-select:focus { border-color: var(--accent); }
	.form-input.mono, .mono { font-family: var(--font-mono); }

	/* Source radio */
	.source-options { display: flex; flex-direction: column; gap: 6px; }

	.source-opt {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.source-opt input[type="radio"] { cursor: pointer; }
	.source-opt.active {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 6%, transparent);
	}
	.opt-label { font-size: 12px; font-weight: 500; color: var(--text-primary); }

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
	.picker-btn:disabled { opacity: 0.45; cursor: default; }
	.picker-btn.loading { cursor: default; }

	.picker-placeholder { color: var(--text-dim); flex: 1; }
	.picker-value { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.selected-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

	:global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; margin-left: auto; }

	.mini-spinner {
		width: 12px;
		height: 12px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
		flex-shrink: 0;
	}

	.no-accounts-link {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 12px;
		color: var(--accent);
		text-decoration: none;
		padding: 8px 10px;
		background: color-mix(in srgb, var(--accent) 6%, transparent);
		border: 1px dashed color-mix(in srgb, var(--accent) 40%, transparent);
		border-radius: var(--radius-sm);
	}
	.no-accounts-link:hover { text-decoration: underline; }

	/* shipyard.json toggle row */
	.toggle-row {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: all var(--transition-fast);
		background: var(--bg-elevated);
	}
	.toggle-row:has(input:checked) {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 5%, transparent);
	}
	.toggle-row input[type="checkbox"] { margin-top: 2px; flex-shrink: 0; cursor: pointer; }

	.toggle-text { display: flex; flex-direction: column; gap: 3px; }

	.toggle-label {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-primary);
	}
	.toggle-label code {
		font-family: var(--font-mono);
		font-size: 11px;
		background: var(--bg-base);
		padding: 1px 4px;
		border-radius: 3px;
		border: 1px solid var(--border);
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

	.field-hint {
		font-size: 10px;
		color: var(--text-dim);
		margin-top: 2px;
	}

	.optional { color: var(--text-dim); font-weight: 400; text-transform: none; letter-spacing: 0; }

	.build-fields {
		display: flex;
		flex-direction: column;
		gap: 10px;
		padding: 12px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
	}

	.form-error {
		font-size: 12px;
		color: var(--accent-red, #ef4444);
		padding: 8px 10px;
		background: color-mix(in srgb, #ef4444 8%, transparent);
		border-radius: var(--radius-sm);
	}

	.form-actions { padding-top: 4px; }

	.btn-primary {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 8px 18px;
		font-size: 13px;
		font-weight: 600;
		background: var(--accent);
		color: white;
		border: none;
		border-radius: var(--radius-sm);
		cursor: pointer;
		transition: opacity var(--transition-fast);
	}
	.btn-primary:hover:not(:disabled) { opacity: 0.88; }
	.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
