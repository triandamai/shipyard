<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Service } from '$lib/api/types';
	import ServiceDetailPanel from '$lib/panels/ServiceDetailPanel.svelte';
	import GitAccountPickerPanel from './GitAccountPickerPanel.svelte';
	import GitRepoPickerPanel from './GitRepoPickerPanel.svelte';
	import GitBranchPickerPanel from './GitBranchPickerPanel.svelte';
	import { ChevronRight, Settings, Network as NetworkIcon, X, Plug } from '@lucide/svelte';
	import NetworkPickerPanel from './NetworkPickerPanel.svelte';
	import PortMappingPanel from './PortMappingPanel.svelte';
	import VolumeMountList from '$lib/components/VolumeMountList.svelte';
	import type { VolumeMount } from '$lib/components/VolumeMountList.svelte';
	import type { Network } from '$lib/api/types';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: (service: Service) => void;
		initialName?: string;
	}

	let { projectId, orgId, onCreated, initialName = '' }: Props = $props();

	type GitProvider = 'github' | 'gitlab' | 'bitbucket';
	interface ConnectedAccount { id: GitProvider; label: string; host: string; token: string; }

	const PROVIDER_COLORS: Record<GitProvider, string> = {
		github:    '#24292f',
		gitlab:    '#FC6D26',
		bitbucket: '#0052CC',
	};

	// Form state
	let name = $state(initialName);
	let slug = $state('');

	// Step 1 – account
	let connectedAccounts = $state<ConnectedAccount[]>([]);
	let accountsLoading = $state(true);
	let selectedAccount = $state<ConnectedAccount | null>(null);

	// Step 2 – repo
	let selectedRepo = $state<{ name: string; fullName: string; cloneUrl: string } | null>(null);

	// Step 3 – branch
	let selectedBranch = $state('main');

	// Port mapping
	let ports = $state<string[]>([]);

	// Network + Volume mounts
	let selectedNetworks = $state<Network[]>([]);
	let volumeMounts     = $state<VolumeMount[]>([]);

	// Build
	let buildType = $state<'dockerfile' | 'compose' | 'nixpack' | 'buildpack' | 'railpack'>('dockerfile');
	let dockerfilePath = $state('./Dockerfile');

	let isSubmitting = $state(false);
	let submitError = $state('');

	function deriveSlug(n: string) {
		return n.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
	}

	async function loadAccounts() {
		accountsLoading = true;
		const res = await api.get<Record<string, string>>('/settings');
		if (res.data) {
			const d = res.data;
			const accounts: ConnectedAccount[] = [];
			if (d.git_github_token)    accounts.push({ id: 'github',    label: 'GitHub',    host: 'github.com',    token: d.git_github_token });
			if (d.git_gitlab_token)    accounts.push({ id: 'gitlab',    label: 'GitLab',    host: 'gitlab.com',    token: d.git_gitlab_token });
			if (d.git_bitbucket_token) accounts.push({ id: 'bitbucket', label: 'Bitbucket', host: 'bitbucket.org', token: d.git_bitbucket_token });
			connectedAccounts = accounts;
			if (accounts.length === 1) selectedAccount = accounts[0];
		}
		accountsLoading = false;
	}

	onMount(loadAccounts);

	function openAccountPicker() {
		uiStore.pushPanel({
			component: GitAccountPickerPanel,
			title: 'Select Git Account',
			props: {
				accounts: connectedAccounts,
				onSelect: (account: ConnectedAccount) => {
					selectedAccount = account;
					selectedRepo = null;
					selectedBranch = 'main';
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
				provider: selectedAccount.id,
				token: selectedAccount.token,
				onSelect: (repo: { name: string; fullName: string; cloneUrl: string }) => {
					selectedRepo = repo;
					selectedBranch = 'main';
					if (!name) { name = repo.name; slug = deriveSlug(repo.name); }
					uiStore.popPanel();
				},
			},
		});
	}

	function removePort(i: number) { ports = ports.filter((_, idx) => idx !== i); }

	function openPortMapping() {
		uiStore.pushPanel({
			component: PortMappingPanel,
			title: 'Port Mapping',
			props: {
				initialPorts: ports,
				onConfirm: (updated: string[]) => { ports = updated; },
			},
		});
	}

	function removeNetwork(id: string) { selectedNetworks = selectedNetworks.filter(n => n.id !== id); }

	function openNetworkPicker() {
		uiStore.pushPanel({
			component: NetworkPickerPanel,
			title: 'Select Networks',
			props: {
				projectId,
				initialSelected: selectedNetworks.map(n => n.id),
				onConfirm: (_ids: string[], items: Network[]) => { selectedNetworks = items; },
			},
		});
	}

	function openBranchPicker() {
		if (!selectedAccount || !selectedRepo) return;
		uiStore.pushPanel({
			component: GitBranchPickerPanel,
			title: 'Select Branch',
			props: {
				provider: selectedAccount.id,
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
		if (!selectedRepo) { submitError = 'Please select a repository.'; return; }
		submitError = '';
		isSubmitting = true;
		try {
			const res = await api.post<Service>(`/projects/${projectId}/services`, {
				name,
				slug: slug || deriveSlug(name),
				type: buildType === 'compose' ? 'docker_compose' : 'git',
				git_repo_url: selectedRepo.cloneUrl,
				git_branch: selectedBranch || 'main',
				...(ports.length > 0 ? { ports } : {}),
			});

			if (res.error) { submitError = res.error.message; return; }
			if (!res.data) { uiStore.clearPanels(); return; }

			const serviceId = res.data.id;
			const gitEnvs = [
				{ key: '__GIT_REPO__',       value: selectedRepo.cloneUrl,                                      is_secret: false },
				{ key: '__GIT_BRANCH__',     value: selectedBranch || 'main',                                   is_secret: false },
				{ key: '__GIT_PROVIDER__',   value: selectedAccount?.id ?? '',                                  is_secret: false },
				{ key: '__BUILD_TYPE__',     value: buildType,                                                  is_secret: false },
				{ key: '__DOCKERFILE_PATH__', value: buildType === 'dockerfile' ? dockerfilePath.trim() : '',   is_secret: false },
			];
			for (const env of gitEnvs) {
				if (env.value) await api.post(`/projects/${projectId}/services/${serviceId}/env`, env);
			}

			for (const net of selectedNetworks) {
				await api.attachNetwork(projectId, net.id, serviceId);
			}
			const validMounts = volumeMounts.filter(m => m.source.trim() && m.target.trim());
			if (validMounts.length > 0) {
				await api.post(`/projects/${projectId}/services/${serviceId}/env`, {
					key: '__VOLUME_MOUNTS__',
					value: JSON.stringify(validMounts),
					is_secret: false,
				});
			}

			onCreated?.(res.data);
			uiStore.clearPanels();
			uiStore.pushPanel({ component: ServiceDetailPanel, props: { serviceId, projectId, orgId }, title: res.data.name });
		} finally {
			isSubmitting = false;
		}
	}
</script>

<div class="panel-wrap">
	<form class="form" onsubmit={handleSubmit}>

		<!-- Name -->
		<div class="form-group">
			<label class="form-label" for="gr-name">Service Name</label>
			<input id="gr-name" class="form-input" type="text" bind:value={name}
				oninput={() => (slug = deriveSlug(name))} placeholder="my-service" required />
		</div>
		<div class="form-group">
			<label class="form-label" for="gr-slug">Slug</label>
			<input id="gr-slug" class="form-input font-mono" type="text" bind:value={slug}
				placeholder="my-service" required />
		</div>

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
						<span class="selected-dot" style="background:{PROVIDER_COLORS[selectedAccount.id]}"></span>
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

		<div class="divider"></div>

		<!-- Port Mapping -->
		<div class="form-group">
			<label class="form-label">Port Mapping</label>
			<button type="button" class="picker-btn" onclick={openPortMapping}>
				<Plug size={13} class="picker-icon" />
				<span class="picker-placeholder">
					{ports.length > 0 ? `${ports.length} port${ports.length === 1 ? '' : 's'} configured` : 'Add port mappings…'}
				</span>
				<ChevronRight size={14} class="picker-chevron" />
			</button>
			{#if ports.length > 0}
				<div class="chips">
					{#each ports as p, i (i)}
						<span class="chip chip-port">
							<span class="picker-value font-mono">{p}</span>
							<button type="button" class="chip-remove" onclick={() => removePort(i)}><X size={10} /></button>
						</span>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Networks -->
		<div class="form-group">
			<label class="form-label">Networks</label>
			<button type="button" class="picker-btn" onclick={openNetworkPicker}>
				<NetworkIcon size={13} class="picker-icon" />
				<span class="picker-placeholder">Select networks…</span>
				<ChevronRight size={13} class="picker-chevron" />
			</button>
			{#if selectedNetworks.length > 0}
				<div class="chips">
					{#each selectedNetworks as net (net.id)}
						<span class="chip chip-blue">
							{net.name}
							<button type="button" class="chip-remove" onclick={() => removeNetwork(net.id)}><X size={10} /></button>
						</span>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Volume Mounts -->
		<div class="form-group">
			<label class="form-label">Volume Mounts</label>
			<span class="form-hint" style="margin-bottom:4px">Bind named volumes or host paths into the container</span>
			<VolumeMountList {projectId} bind:mounts={volumeMounts} />
		</div>

		<div class="divider"></div>

		<!-- Build type -->
		<div class="form-group">
			<label class="form-label">Build Type</label>
			<div class="build-types">
				{#each [
					{ id: 'dockerfile', label: 'Dockerfile' },
					{ id: 'compose',    label: 'Compose' },
					{ id: 'nixpack',    label: 'Nixpack' },
					{ id: 'buildpack',  label: 'Buildpack' },
					{ id: 'railpack',   label: 'Railpack' },
				] as bt (bt.id)}
					<button type="button" class="build-btn" class:active={buildType === bt.id}
						onclick={() => (buildType = bt.id as typeof buildType)}>
						{bt.label}
					</button>
				{/each}
			</div>
		</div>

		{#if buildType === 'dockerfile'}
			<div class="form-group">
				<label class="form-label" for="gr-dockerfile">Dockerfile Path</label>
				<input id="gr-dockerfile" class="form-input font-mono" type="text"
					bind:value={dockerfilePath} placeholder="./Dockerfile" />
				<span class="form-hint">Relative to the repository root</span>
			</div>
		{/if}

		{#if submitError}
			<div class="error-msg">{submitError}</div>
		{/if}

		<button class="btn btn-primary submit-btn" type="submit" disabled={isSubmitting || !selectedRepo}>
			{#if isSubmitting}
				<div class="btn-spinner"></div> Creating…
			{:else}
				Add Git Service
			{/if}
		</button>
	</form>
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 14px; }
	.form-group { display: flex; flex-direction: column; gap: 4px; }

	.form-label {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
		transition: color var(--transition-fast);
	}
	.form-label.dimmed { color: color-mix(in srgb, var(--text-dim) 50%, transparent); }

	.form-input {
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary);
		font-size: 13px; font-family: var(--font-sans); padding: 8px 10px;
		outline: none; transition: border-color var(--transition-fast);
	}
	.form-input:focus { border-color: var(--accent); }
	.font-mono { font-family: var(--font-mono); }
	.form-hint { font-size: 11px; color: var(--text-dim); }

	.divider { height: 1px; background: var(--border); margin: 2px 0; }

	/* Picker button — looks like an input but opens a sub-panel */
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

	.selected-dot {
		width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0;
	}

	:global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; margin-left: auto; }

	.mini-spinner {
		width: 12px; height: 12px; border: 2px solid var(--border);
		border-top-color: var(--accent); border-radius: 50%;
		animation: spin 0.7s linear infinite; flex-shrink: 0;
	}

	/* Picker + chips (shared with other panels) */
	.chips { display: flex; flex-wrap: wrap; gap: 5px; margin-top: 4px; }
	.chip {
		display: inline-flex; align-items: center; gap: 4px;
		padding: 2px 8px 2px 10px; border-radius: 99px;
		font-size: 11px; font-weight: 600; font-family: var(--font-mono);
	}
	.chip-blue {
		background: var(--accent-blue-muted); color: var(--accent-blue);
		border: 1px solid color-mix(in srgb, var(--accent-blue) 30%, transparent);
	}
	.chip-yellow {
		background: var(--accent-yellow-muted); color: var(--accent-yellow);
		border: 1px solid color-mix(in srgb, var(--accent-yellow) 30%, transparent);
	}
	.chip-port {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border);
	}
	.chip-remove {
		background: none; border: none; cursor: pointer; padding: 1px;
		color: inherit; opacity: 0.6; display: flex; align-items: center; border-radius: 50%;
	}
	.chip-remove:hover { opacity: 1; }

	:global(.picker-icon)   { color: var(--text-dim); flex-shrink: 0; }
	:global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; }

	.no-accounts-link {
		display: flex; align-items: center; gap: 6px;
		font-size: 12px; color: var(--accent); text-decoration: none;
		padding: 8px 10px; background: color-mix(in srgb, var(--accent) 6%, transparent);
		border: 1px dashed color-mix(in srgb, var(--accent) 40%, transparent);
		border-radius: var(--radius-sm);
	}
	.no-accounts-link:hover { text-decoration: underline; }

	.build-types { display: flex; flex-wrap: wrap; gap: 6px; }

	.build-btn {
		padding: 5px 12px; font-size: 12px; font-weight: 500;
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		background: var(--bg-elevated); color: var(--text-secondary);
		cursor: pointer; transition: all var(--transition-fast);
	}
	.build-btn:hover { border-color: var(--accent); color: var(--accent); }
	.build-btn.active {
		background: color-mix(in srgb, var(--accent) 15%, transparent);
		border-color: var(--accent); color: var(--accent); font-weight: 600;
	}

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

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
