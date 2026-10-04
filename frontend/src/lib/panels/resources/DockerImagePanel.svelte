<script lang="ts">
	import { Trash2, Network, ChevronRight, X, Plug, Package, Eye, EyeOff } from '@lucide/svelte';
	import { Button, FormField, TextField, Checkbox, InlineAlert, Spinner, Tabs, SectionLabel } from '$lib/components/ui';
	import ArtifactoryPickerPanel from './ArtifactoryPickerPanel.svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Service, Network as NetworkType } from '$lib/api/types';
	import ServiceDetailPanel from '$lib/panels/ServiceDetailPanel.svelte';
	import NetworkPickerPanel from './NetworkPickerPanel.svelte';
	import PortMappingPanel from './PortMappingPanel.svelte';
	import VolumeMountList from '$lib/components/VolumeMountList.svelte';
	import type { VolumeMount } from '$lib/components/VolumeMountList.svelte';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: (service: Service) => void;
		initialName?: string;
		initialSlug?: string;
		initialImage?: string;
	}

	let {
		projectId,
		orgId,
		onCreated,
		initialName = '',
		initialSlug = '',
		initialImage = '',
	}: Props = $props();

	let name = $state(initialName);
	let slug = $state(initialSlug);
	let image = $state(initialImage);
	let registrySource = $state<'external' | 'shipyard'>('external');
	let registryUrl = $state('');
	let registryUser = $state('');
	let registryPass = $state('');
	let registryHostname = $state('');

	// Load registry hostname so we can pre-fill for Shipyard registry
	$effect(() => {
		if (orgId) {
			api.get(`/orgs/${orgId}/registry/info`).then(res => {
				registryHostname = res.data?.hostname ?? '';
			});
		}
	});

	type SelectedArtifact = { id: string; namespace_id: string; namespace_slug: string; repo: string; tag: string; kind: string };
	let selectedArtifact = $state<SelectedArtifact | null>(null);
	// Only meaningful for the Shipyard Artifactory source — auto-redeploy when a
	// new image is pushed to the bound repo:tag.
	let autoDeployOnPush = $state(false);

	function openShipyardPicker() {
		uiStore.pushPanel({
			component: ArtifactoryPickerPanel,
			title: 'Pick Docker Image',
			props: {
				kind: 'docker_image',
				onSelect: (art: SelectedArtifact) => {
					selectedArtifact = art;
					// Build the full image ref: registry.domain/org/project/repo:tag
					image = `${registryHostname}/${art.namespace_slug}/${art.repo}:${art.tag}`;
					if (!name) { name = art.repo; slug = deriveSlug(art.repo); }
					uiStore.popPanel();
				},
			},
		});
	}
	let ports = $state<string[]>([]);
	// Bound to a number TextField: may be a string or a number at runtime, '' when cleared.
	let replicas = $state<string | number>(1);
	let envs = $state<Array<{ key: string; value: string; is_secret: boolean }>>([]);

	// Network selection
	let selectedNetworks = $state<NetworkType[]>([]);
	// Volume mount bindings
	let volumeMounts = $state<VolumeMount[]>([]);

	let isSubmitting = $state(false);
	let submitError = $state('');

	function deriveSlug(n: string) {
		return n.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
	}

	function addEnv() { envs = [...envs, { key: '', value: '', is_secret: false }]; }
	function removeEnv(i: number) { envs = envs.filter((_, idx) => idx !== i); }
	function updateEnv(i: number, field: 'key' | 'value' | 'is_secret', val: string | boolean) {
		envs = envs.map((e, idx) => (idx === i ? { ...e, [field]: val } : e));
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
				onConfirm: (_ids: string[], items: NetworkType[]) => { selectedNetworks = items; },
			},
		});
	}

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		submitError = '';
		isSubmitting = true;
		try {
			const res = await api.post<Service>(`/projects/${projectId}/services`, {
				name,
				slug: slug || deriveSlug(name),
				type: 'docker',
				image,
				icon: 'docker',
				...(ports.length > 0 ? { ports } : {}),
				replicas: Math.max(1, Number(replicas) || 0),
			});

			if (res.error) { submitError = res.error.message; return; }
			if (!res.data)  { uiStore.clearPanels(); return; }

			const serviceId = res.data.id;

			// Env vars
			const allEnvs: Array<{ key: string; value: string; is_secret: boolean }> = [];
			if (registryUrl.trim())  allEnvs.push({ key: 'DOCKER_REGISTRY', value: registryUrl.trim(),  is_secret: false });
			if (registryUser.trim()) allEnvs.push({ key: 'DOCKER_USERNAME', value: registryUser.trim(), is_secret: false });
			if (registryPass.trim()) allEnvs.push({ key: 'DOCKER_PASSWORD', value: registryPass.trim(), is_secret: true });
			for (const env of envs) {
				if (env.key.trim()) allEnvs.push({ key: env.key.trim(), value: env.value, is_secret: env.is_secret });
			}
			if (allEnvs.length > 0) await api.bulkSetEnvs(serviceId, allEnvs);

			// Attach networks
			for (const net of selectedNetworks) {
				await api.attachNetwork(projectId, net.id, serviceId);
			}
			// Store volume mount specs as a JSON env var for the deploy layer
			const validMounts = volumeMounts.filter(m => m.source.trim() && m.target.trim());
			if (validMounts.length > 0) {
				await api.post(`/projects/${projectId}/services/${serviceId}/env`, {
					key: '__VOLUME_MOUNTS__',
					value: JSON.stringify(validMounts),
					is_secret: false,
				});
			}

			// Bind the service to the Shipyard registry repo:tag so deploys always
			// resolve the newest push of that tag. The checkbox only controls
			// whether a push also auto-triggers the deploy.
			if (registrySource === 'shipyard' && selectedArtifact) {
				await api.putArtifactSource(serviceId, {
					namespace_id: selectedArtifact.namespace_id,
					repo: selectedArtifact.repo,
					tag: selectedArtifact.tag,
					auto_deploy_on_push: autoDeployOnPush,
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
		<FormField label="Name" for="di-name">
			<TextField id="di-name" type="text" bind:value={name}
				oninput={() => (slug = deriveSlug(name))} placeholder="my-service" required />
		</FormField>
		<FormField label="Slug" for="di-slug">
			<div class="mono-field">
				<TextField id="di-slug" type="text" bind:value={slug} placeholder="my-service" required />
			</div>
		</FormField>
		<FormField label="Docker Image" for="di-image">
			<div class="mono-field">
				<TextField id="di-image" type="text" bind:value={image} placeholder="nginx:latest" required />
			</div>
		</FormField>

		<SectionLabel>Registry</SectionLabel>

		<Tabs
			ariaLabel="Registry source"
			tabs={[
				{ id: 'external', label: 'External / Docker Hub' },
				{ id: 'shipyard', label: 'Shipyard Artifactory', icon: Package },
			]}
			value={registrySource}
			onChange={(id) => {
				if (id === 'external') { registrySource = 'external'; selectedArtifact = null; }
				else { registrySource = 'shipyard'; }
			}}
		/>

		{#if registrySource === 'shipyard'}
			<FormField label="Select Image">
				<button type="button" class="picker-btn" onclick={openShipyardPicker}>
					{#if selectedArtifact}
						<Package size={13} class="picker-icon" />
						<span class="picker-value font-mono">{selectedArtifact.namespace_slug}/{selectedArtifact.repo}:{selectedArtifact.tag}</span>
					{:else}
						<span class="picker-placeholder">Browse Shipyard registry…</span>
					{/if}
					<ChevronRight size={13} class="picker-chevron" />
				</button>
				{#if registryHostname}
					<span class="field-hint">Registry: {registryHostname}</span>
				{/if}
			</FormField>

			<div class="auto-deploy">
				<Checkbox bind:checked={autoDeployOnPush} label="Auto-deploy on push" />
				<span class="checkbox-hint">
					Redeploy this service automatically when a new
					{#if selectedArtifact}<code class="font-mono">:{selectedArtifact.tag}</code>{:else}image{/if}
					is pushed to the registry.
				</span>
			</div>
		{:else}
			<FormField label="Registry URL (optional)" for="di-reg">
				<div class="mono-field">
					<TextField id="di-reg" type="text" bind:value={registryUrl} placeholder="registry-1.docker.io" />
				</div>
			</FormField>
			<div class="form-row">
				<div class="form-col">
					<FormField label="Username" for="di-user">
						<TextField id="di-user" type="text" bind:value={registryUser} placeholder="myuser" autocomplete="off" />
					</FormField>
				</div>
				<div class="form-col">
					<FormField label="Password / Token" for="di-pass">
						<div class="mono-field">
							<TextField id="di-pass" type="password" bind:value={registryPass} placeholder="••••••••" autocomplete="new-password" />
						</div>
					</FormField>
				</div>
			</div>
		{/if}

		<SectionLabel>Deployment</SectionLabel>

		<!-- Port Mapping -->
		<FormField label="Port Mapping">
			<button type="button" class="picker-btn" onclick={openPortMapping}>
				<Plug size={13} class="picker-icon" />
				<span class="picker-placeholder">
					{ports.length > 0 ? `${ports.length} port${ports.length === 1 ? '' : 's'} configured` : 'Add port mappings…'}
				</span>
				<ChevronRight size={13} class="picker-chevron" />
			</button>
			{#if ports.length > 0}
				<div class="chips">
					{#each ports as p, i (i)}
						<span class="chip chip-port">
							<span class="font-mono">{p}</span>
							<button type="button" class="chip-remove" aria-label="Remove port {p}" onclick={() => removePort(i)}><X size={10} /></button>
						</span>
					{/each}
				</div>
			{/if}
		</FormField>

		<FormField label="Replicas" for="di-replicas">
			<TextField id="di-replicas" type="number" min="1" max="20" bind:value={replicas as string} />
		</FormField>

		<!-- Networks -->
		<FormField label="Networks">
			<button type="button" class="picker-btn" onclick={openNetworkPicker}>
				<Network size={13} class="picker-icon" />
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
		<FormField label="Volume Mounts">
			<span class="form-hint">Bind named volumes or host paths into the container</span>
			<VolumeMountList {projectId} bind:mounts={volumeMounts} />
		</FormField>

		<div class="section-head">
			<SectionLabel>Environment Variables</SectionLabel>
			<Button variant="secondary" size="sm" onclick={addEnv}>+ Add</Button>
		</div>

		{#if envs.length > 0}
			<div class="env-list">
				{#each envs as env, i (i)}
					<div class="env-row">
						<div class="mono-field env-key">
							<TextField type="text" placeholder="KEY" aria-label="Variable name"
								value={env.key} oninput={(ev) => updateEnv(i, 'key', (ev.target as HTMLInputElement).value)} />
						</div>
						<div class="mono-field env-val">
							<TextField type={env.is_secret ? 'password' : 'text'} placeholder="value" aria-label="Variable value"
								value={env.value} oninput={(ev) => updateEnv(i, 'value', (ev.target as HTMLInputElement).value)} />
						</div>
						<Button
							variant={env.is_secret ? 'primary' : 'secondary'}
							size="icon"
							title={env.is_secret ? 'Secret' : 'Plain'}
							aria-label={env.is_secret ? 'Secret — click to make plain' : 'Plain — click to make secret'}
							onclick={() => updateEnv(i, 'is_secret', !env.is_secret)}
						>
							{#if env.is_secret}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
						</Button>
						<Button variant="danger-outline" size="icon" aria-label="Remove variable" onclick={() => removeEnv(i)}>
							<Trash2 size={12} />
						</Button>
					</div>
				{/each}
			</div>
		{/if}

		{#if submitError}
			<div role="alert"><InlineAlert tone="error">{submitError}</InlineAlert></div>
		{/if}

		<Button type="submit" disabled={isSubmitting}>
			{#if isSubmitting}<Spinner size={12} tone="current" /> Creating…
			{:else}Add Docker Service{/if}
		</Button>
	</form>
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 14px; }

	.font-mono { font-family: var(--font-mono); }
	.mono-field :global(input) { font-family: var(--font-mono); }
	.form-hint { font-size: 11px; color: var(--text-dim); line-height: 1.5; }
	.field-hint { font-size: 10px; color: var(--text-dim); }
	.form-row { display: flex; gap: 10px; }
	.form-col { flex: 1; min-width: 0; }

	.section-head { display: flex; align-items: center; justify-content: space-between; margin-top: 4px; }
	.section-head :global(.ui-section-label) { margin-bottom: 0; }

	.picker-value { flex: 1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.auto-deploy {
		display: flex; flex-direction: column; gap: 4px;
		padding: 8px 10px; background: var(--bg-elevated);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
	}
	.checkbox-hint { font-size: 11px; color: var(--text-dim); line-height: 1.4; padding-left: 24px; }
	.checkbox-hint code {
		font-family: var(--font-mono); font-size: 10px;
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
		border: 1px solid var(--border);
	}

	.picker-btn {
		display: flex; align-items: center; gap: 7px;
		padding: 8px 10px; background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px;
		font-family: var(--font-sans); cursor: pointer; text-align: left; width: 100%;
		transition: border-color var(--transition-fast);
	}
	.picker-btn:hover { border-color: var(--accent); }
	.picker-btn:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
	:global(.picker-icon) { color: var(--text-dim); flex-shrink: 0; }
	.picker-placeholder { flex: 1; color: var(--text-dim); font-size: 13px; }
	:global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; }

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
	.chip-remove:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; opacity: 1; }

	.env-list { display: flex; flex-direction: column; gap: 6px; }
	.env-row { display: flex; gap: 4px; align-items: center; }
	.env-key { width: 120px; flex-shrink: 0; }
	.env-val { flex: 1; min-width: 0; }
</style>
