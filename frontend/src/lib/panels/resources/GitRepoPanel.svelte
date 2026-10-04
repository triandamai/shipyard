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
	import EnvManagerPanel from '$lib/panels/EnvManagerPanel.svelte';
	import { Button, FormField, TextField, Tabs, InlineAlert, Spinner, Divider } from '$lib/components/ui';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: (service: Service) => void;
		initialName?: string;
	}

	let { projectId, orgId, onCreated, initialName = '' }: Props = $props();

	interface ConnectedAccount { id: string; label: string; host: string; token: string; provider_type: 'github' | 'gitlab' | 'bitbucket'; }

	const PROVIDER_COLORS: Record<string, string> = {
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

	// Environment variables
	let envVars = $state<{ key: string; value: string; is_secret: boolean }[]>([]);

	// Build
	let buildType = $state<'auto' | 'dockerfile' | 'compose' | 'nixpack' | 'buildpack' | 'railpack'>('auto');
	let dockerfilePath = $state('./Dockerfile');

	let isSubmitting = $state(false);
	let submitError = $state('');

	function deriveSlug(n: string) {
		return n.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
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
						selectedRepo = null;
						selectedBranch = 'main';
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

	function openEnvManager() {
		uiStore.pushPanel({
			component: EnvManagerPanel,
			title: 'Manage Environment Variables',
			props: {
				projectId,
				serviceName: name || 'New Service',
				initialEnvs: envVars,
				onConfirm: (updated: typeof envVars) => { envVars = updated; },
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
		if (!selectedRepo) { submitError = 'Please select a repository.'; return; }
		submitError = '';
		isSubmitting = true;
		try {
			const res = await api.post<Service>(`/projects/${projectId}/services`, {
				name,
				slug: slug || deriveSlug(name),
				type: buildType === 'compose' ? 'docker_compose' : 'git',
				icon: (buildType === 'compose' || buildType === 'dockerfile') ? 'docker' : null,
				git_repo_url: selectedRepo.cloneUrl,
				git_branch: selectedBranch || 'main',
				git_provider_id: selectedAccount?.id ?? null,
				...(ports.length > 0 ? { ports } : {}),
			});

			if (res.error) { submitError = res.error.message; return; }
			if (!res.data) { uiStore.clearPanels(); return; }

			const serviceId = res.data.id;
			const gitEnvs = [
				{ key: '__GIT_REPO__',       value: selectedRepo.cloneUrl,                                      is_secret: false },
				{ key: '__GIT_BRANCH__',     value: selectedBranch || 'main',                                   is_secret: false },
				{ key: '__GIT_PROVIDER__',   value: selectedAccount?.provider_type ?? '',                       is_secret: false },
				{ key: '__BUILD_TYPE__',     value: buildType,                                                  is_secret: false },
				{ key: '__DOCKERFILE_PATH__', value: buildType === 'dockerfile' ? dockerfilePath.trim() : '',   is_secret: false },
			];
			for (const env of gitEnvs) {
				if (env.value) await api.post(`/projects/${projectId}/services/${serviceId}/env`, env);
			}

			for (const env of envVars) {
				if (env.key.trim()) {
					await api.post(`/projects/${projectId}/services/${serviceId}/env`, {
						key: env.key.trim(),
						value: env.value,
						is_secret: env.is_secret,
					});
				}
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
		<FormField label="Service Name" for="gr-name">
			<TextField id="gr-name" bind:value={name}
				oninput={() => (slug = deriveSlug(name))} placeholder="my-service" required />
		</FormField>
		<FormField label="Slug" for="gr-slug">
			<span class="mono-field">
				<TextField id="gr-slug" bind:value={slug} placeholder="my-service" required />
			</span>
		</FormField>

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

		<Divider margin="2px 0" />

		<!-- Port Mapping -->
		<FormField label="Port Mapping">
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
							<button type="button" class="chip-remove" aria-label="Remove port {p}" onclick={() => removePort(i)}><X size={10} /></button>
						</span>
					{/each}
				</div>
			{/if}
		</FormField>

		<!-- Networks -->
		<FormField label="Networks">
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
							<button type="button" class="chip-remove" aria-label="Remove network {net.name}" onclick={() => removeNetwork(net.id)}><X size={10} /></button>
						</span>
					{/each}
				</div>
			{/if}
		</FormField>

		<!-- Volume Mounts -->
		<FormField label="Volume Mounts" hint="Bind named volumes or host paths into the container">
			<VolumeMountList {projectId} bind:mounts={volumeMounts} />
		</FormField>

		<!-- Environment Variables -->
		<FormField label="Environment Variables">
			<button type="button" class="picker-btn" onclick={openEnvManager}>
				<Settings size={13} class="picker-icon" />
				<span class="picker-placeholder">
					{envVars.length > 0 ? `${envVars.length} variable${envVars.length === 1 ? '' : 's'} configured` : 'Configure environment variables…'}
				</span>
				<ChevronRight size={14} class="picker-chevron" />
			</button>
		</FormField>

		<Divider margin="2px 0" />

		<!-- Build type -->
		<FormField label="Build Type">
			<Tabs
				ariaLabel="Build type"
				value={buildType}
				onChange={(id) => (buildType = id as typeof buildType)}
				tabs={[
					{ id: 'auto',       label: 'Auto (SSR)' },
					{ id: 'dockerfile', label: 'Dockerfile' },
					{ id: 'compose',    label: 'Compose' },
					{ id: 'nixpack',    label: 'Nixpack' },
					{ id: 'buildpack',  label: 'Buildpack' },
					{ id: 'railpack',   label: 'Railpack' },
				]}
			/>
		</FormField>

		{#if buildType === 'dockerfile'}
			<FormField label="Dockerfile Path" for="gr-dockerfile" hint="Relative to the repository root">
				<span class="mono-field">
					<TextField id="gr-dockerfile" bind:value={dockerfilePath} placeholder="./Dockerfile" />
				</span>
			</FormField>
		{/if}

		{#if submitError}
			<div role="alert"><InlineAlert tone="error">{submitError}</InlineAlert></div>
		{/if}

		<Button type="submit" disabled={isSubmitting || !selectedRepo}>
			{#if isSubmitting}
				<Spinner size={12} tone="current" /> Creating…
			{:else}
				Add Git Service
			{/if}
		</Button>
	</form>
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 14px; }
	.font-mono { font-family: var(--font-mono); }
	.mono-field :global(input) { font-family: var(--font-mono); }

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
	.picker-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
	.picker-btn:disabled { opacity: 0.45; cursor: default; }
	.picker-btn.loading { cursor: default; }

	.picker-placeholder { color: var(--text-dim); flex: 1; }
	.picker-value { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.selected-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

	.picker-btn :global(.picker-icon)    { color: var(--text-dim); flex-shrink: 0; }
	.picker-btn :global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; margin-left: auto; }

	/* Chips */
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
	.chip-port {
		background: var(--bg-elevated); color: var(--text-secondary);
		border: 1px solid var(--border);
	}
	.chip-remove {
		background: none; border: none; cursor: pointer; padding: 1px;
		color: inherit; opacity: 0.6; display: flex; align-items: center; border-radius: 50%;
	}
	.chip-remove:hover { opacity: 1; }
	.chip-remove:focus-visible { outline: 2px solid var(--accent); opacity: 1; }
</style>
