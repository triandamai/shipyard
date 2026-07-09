<script lang="ts">
	import { Trash2, Network, ChevronRight, X, Plug } from '@lucide/svelte';
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
	let registryUrl = $state('');
	let registryUser = $state('');
	let registryPass = $state('');
	let ports = $state<string[]>([]);
	let replicas = $state(1);
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
				replicas: Math.max(1, replicas),
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
		<div class="form-group">
			<label class="form-label" for="di-name">Name</label>
			<input id="di-name" class="form-input" type="text" bind:value={name}
				oninput={() => (slug = deriveSlug(name))} placeholder="my-service" required />
		</div>
		<div class="form-group">
			<label class="form-label" for="di-slug">Slug</label>
			<input id="di-slug" class="form-input font-mono" type="text" bind:value={slug}
				placeholder="my-service" required />
		</div>
		<div class="form-group">
			<label class="form-label" for="di-image">Docker Image</label>
			<input id="di-image" class="form-input font-mono" type="text" bind:value={image}
				placeholder="nginx:latest" required />
		</div>

		<div class="section-title">Registry (optional)</div>

		<div class="form-group">
			<label class="form-label" for="di-reg">Registry URL</label>
			<input id="di-reg" class="form-input font-mono" type="text" bind:value={registryUrl}
				placeholder="registry-1.docker.io" />
		</div>
		<div class="form-row">
			<div class="form-group" style="flex:1">
				<label class="form-label" for="di-user">Username</label>
				<input id="di-user" class="form-input" type="text" bind:value={registryUser}
					placeholder="myuser" autocomplete="off" />
			</div>
			<div class="form-group" style="flex:1">
				<label class="form-label" for="di-pass">Password / Token</label>
				<input id="di-pass" class="form-input font-mono" type="password" bind:value={registryPass}
					placeholder="••••••••" autocomplete="new-password" />
			</div>
		</div>

		<div class="section-title">Deployment</div>

		<!-- Port Mapping -->
		<div class="form-group">
			<label class="form-label">Port Mapping</label>
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
							<button type="button" class="chip-remove" onclick={() => removePort(i)}><X size={10} /></button>
						</span>
					{/each}
				</div>
			{/if}
		</div>

		<div class="form-group" style="width:100%">
			<label class="form-label" for="di-replicas">Replicas</label>
			<input id="di-replicas" class="form-input" type="number" min="1" max="20" bind:value={replicas} />
		</div>

		<!-- Networks -->
		<div class="form-group">
			<label class="form-label">Networks</label>
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

		<div class="section-title">
			Environment Variables
			<button type="button" class="add-env-btn" onclick={addEnv}>+ Add</button>
		</div>

		{#if envs.length > 0}
			<div class="env-list">
				{#each envs as env, i (i)}
					<div class="env-row">
						<input class="form-input font-mono env-key" type="text" placeholder="KEY"
							value={env.key} oninput={(ev) => updateEnv(i, 'key', (ev.target as HTMLInputElement).value)} />
						<input class="form-input font-mono env-val"
							type={env.is_secret ? 'password' : 'text'} placeholder="value"
							value={env.value} oninput={(ev) => updateEnv(i, 'value', (ev.target as HTMLInputElement).value)} />
						<button type="button" class="env-secret-btn" class:active={env.is_secret}
							title={env.is_secret ? 'Secret' : 'Plain'}
							onclick={() => updateEnv(i, 'is_secret', !env.is_secret)}>
							{env.is_secret ? '🔒' : '👁'}
						</button>
						<button type="button" class="env-del-btn" onclick={() => removeEnv(i)}>
							<Trash2 size={12} />
						</button>
					</div>
				{/each}
			</div>
		{/if}

		{#if submitError}
			<div class="error-msg">{submitError}</div>
		{/if}

		<button class="btn btn-primary submit-btn" type="submit" disabled={isSubmitting}>
			{#if isSubmitting}<div class="btn-spinner"></div> Creating…
			{:else}Add Docker Service{/if}
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
	}

	.form-input {
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary);
		font-size: 13px; font-family: var(--font-sans); padding: 8px 10px;
		outline: none; transition: border-color var(--transition-fast);
	}
	.form-input:focus { border-color: var(--accent); }
	.font-mono { font-family: var(--font-mono); }
	.form-hint { font-size: 11px; color: var(--text-dim); }
	.form-row { display: flex; gap: 10px; }

	.section-title {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
		border-bottom: 1px solid var(--border); padding-bottom: 4px;
		margin-top: 4px; display: flex; align-items: center; justify-content: space-between;
	}

	/* Picker button */
	.picker-btn {
		display: flex; align-items: center; gap: 7px;
		padding: 8px 10px; background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px;
		font-family: var(--font-sans); cursor: pointer; text-align: left; width: 100%;
		transition: border-color var(--transition-fast);
	}
	.picker-btn:hover { border-color: var(--accent); }
	:global(.picker-icon) { color: var(--text-dim); flex-shrink: 0; }
	.picker-placeholder { flex: 1; color: var(--text-dim); font-size: 13px; }
	:global(.picker-chevron) { color: var(--text-dim); flex-shrink: 0; }

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
		color: inherit; opacity: 0.6; display: flex; align-items: center;
		border-radius: 50%;
	}
	.chip-remove:hover { opacity: 1; }

	/* Env list */
	.add-env-btn {
		background: transparent; border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-muted);
		font-size: 11px; cursor: pointer; padding: 2px 8px;
		transition: all var(--transition-fast);
	}
	.add-env-btn:hover { border-color: var(--accent); color: var(--accent); }

	.env-list { display: flex; flex-direction: column; gap: 6px; }
	.env-row { display: flex; gap: 4px; align-items: center; }
	.env-key { width: 120px; flex-shrink: 0; font-size: 12px; padding: 6px 8px; }
	.env-val  { flex: 1; font-size: 12px; padding: 6px 8px; }

	.env-secret-btn {
		width: 28px; height: 28px; flex-shrink: 0; background: var(--bg-elevated);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		cursor: pointer; font-size: 12px; display: flex; align-items: center; justify-content: center;
	}
	.env-secret-btn.active { border-color: var(--accent-yellow, #F59E0B); }

	.env-del-btn {
		width: 28px; height: 28px; flex-shrink: 0; background: transparent;
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		cursor: pointer; color: var(--text-dim); display: flex; align-items: center; justify-content: center;
		transition: all var(--transition-fast);
	}
	.env-del-btn:hover { border-color: var(--accent-red); color: var(--accent-red); }

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
