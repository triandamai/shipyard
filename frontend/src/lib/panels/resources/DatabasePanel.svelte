<script lang="ts">
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Service, Network } from '$lib/api/types';
	import ServiceDetailPanel from '$lib/panels/ServiceDetailPanel.svelte';
	import NetworkPickerPanel from './NetworkPickerPanel.svelte';
	import PortMappingPanel from './PortMappingPanel.svelte';
	import VolumeMountList from '$lib/components/VolumeMountList.svelte';
	import type { VolumeMount } from '$lib/components/VolumeMountList.svelte';
	import { Network as NetworkIcon, ChevronRight, X, Plug } from '@lucide/svelte';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: (service: Service) => void;
		initialName?: string;
	}

	let { projectId, orgId, onCreated, initialName = '' }: Props = $props();

	const DB_DEFAULTS: Record<string, { image: string; defaultPort: string; userKey: string; passKey: string; dbKey: string }> = {
		postgres: { image: 'postgres:16',    defaultPort: '5432',   userKey: 'POSTGRES_USER',              passKey: 'POSTGRES_PASSWORD',         dbKey: 'POSTGRES_DB' },
		mysql:    { image: 'mysql:8.0',      defaultPort: '3306',   userKey: 'MYSQL_USER',                 passKey: 'MYSQL_PASSWORD',            dbKey: 'MYSQL_DATABASE' },
		redis:    { image: 'redis:7-alpine', defaultPort: '6379',   userKey: '',                           passKey: 'REDIS_PASSWORD',            dbKey: '' },
		mongodb:  { image: 'mongo:7',        defaultPort: '27017',  userKey: 'MONGO_INITDB_ROOT_USERNAME', passKey: 'MONGO_INITDB_ROOT_PASSWORD', dbKey: 'MONGO_INITDB_DATABASE' },
	};

	let name = $state(initialName);
	let slug = $state('');
	let engine = $state('postgres');
	let image = $state(DB_DEFAULTS.postgres.image);
	let dbName = $state('mydb');
	let dbUser = $state('admin');
	let dbPassword = $state(generatePassword());
	let ports = $state<string[]>([DB_DEFAULTS.postgres.defaultPort]);

	let selectedNetworks = $state<Network[]>([]);
	let volumeMounts     = $state<VolumeMount[]>([]);

	let isSubmitting = $state(false);
	let submitError  = $state('');

	function generatePassword(): string {
		const chars = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789';
		return Array.from({ length: 20 }, () => chars[Math.floor(Math.random() * chars.length)]).join('');
	}

	function deriveSlug(n: string) {
		return n.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
	}

	function onEngineChange() {
		const def = DB_DEFAULTS[engine];
		if (def) { image = def.image; ports = [def.defaultPort]; }
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

	function getDbIcon(eng: string): string {
		if (eng === 'postgres') return 'postgresql';
		if (eng === 'mysql') return 'mysql';
		if (eng === 'redis') return 'redis';
		if (eng === 'mongodb') return 'mongodb';
		return 'database';
	}

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		submitError = '';
		isSubmitting = true;
		try {
			const res = await api.post<Service>(`/projects/${projectId}/services`, {
				name,
				slug: slug || deriveSlug(name),
				type: 'database',
				image,
				icon: getDbIcon(engine),
				...(ports.length > 0 ? { ports } : {}),
			});

			if (res.error) { submitError = res.error.message; return; }
			if (!res.data)  { uiStore.clearPanels(); return; }

			const serviceId = res.data.id;
			const def = DB_DEFAULTS[engine];
			const envs: Array<{ key: string; value: string; is_secret: boolean }> = [];
			if (def.dbKey && dbName)   envs.push({ key: def.dbKey,  value: dbName,     is_secret: false });
			if (def.userKey && dbUser) envs.push({ key: def.userKey, value: dbUser,     is_secret: false });
			if (dbPassword)            envs.push({ key: def.passKey, value: dbPassword, is_secret: true });
			if (engine === 'mysql' && dbPassword) {
				envs.push({ key: 'MYSQL_ROOT_PASSWORD', value: dbPassword, is_secret: true });
			}
			if (envs.length > 0) await api.bulkSetEnvs(serviceId, envs);

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
		<div class="form-group">
			<label class="form-label" for="db-name">Name</label>
			<input id="db-name" class="form-input" type="text" bind:value={name}
				oninput={() => (slug = deriveSlug(name))} placeholder="my-database" required />
		</div>
		<div class="form-group">
			<label class="form-label" for="db-slug">Slug</label>
			<input id="db-slug" class="form-input font-mono" type="text" bind:value={slug}
				placeholder="my-database" required />
		</div>

		<div class="form-group">
			<label class="form-label" for="db-engine">Engine</label>
			<select id="db-engine" class="form-select" bind:value={engine} onchange={onEngineChange}>
				<option value="postgres">PostgreSQL</option>
				<option value="mysql">MySQL</option>
				<option value="redis">Redis</option>
				<option value="mongodb">MongoDB</option>
			</select>
		</div>

		<div class="form-group">
			<label class="form-label" for="db-image">Docker Image</label>
			<input id="db-image" class="form-input font-mono" type="text" bind:value={image}
				placeholder="postgres:16" />
			<span class="form-hint">Override to pin a specific version</span>
		</div>

		{#if engine !== 'redis'}
			<div class="form-group">
				<label class="form-label" for="db-dbname">Database Name</label>
				<input id="db-dbname" class="form-input" type="text" bind:value={dbName} placeholder="mydb" />
				<span class="form-hint">Env: <code class="mono">{DB_DEFAULTS[engine]?.dbKey}</code></span>
			</div>
		{/if}

		{#if DB_DEFAULTS[engine]?.userKey}
			<div class="form-group">
				<label class="form-label" for="db-user">Username</label>
				<input id="db-user" class="form-input" type="text" bind:value={dbUser} placeholder="admin" />
				<span class="form-hint">Env: <code class="mono">{DB_DEFAULTS[engine]?.userKey}</code></span>
			</div>
		{/if}

		<div class="form-group">
			<label class="form-label" for="db-pass">Password</label>
			<div class="pass-row">
				<input id="db-pass" class="form-input font-mono" type="text" bind:value={dbPassword} />
				<button type="button" class="regen-btn" onclick={() => (dbPassword = generatePassword())} title="Regenerate">↺</button>
			</div>
			<span class="form-hint">Env: <code class="mono">{DB_DEFAULTS[engine]?.passKey}</code> (stored as secret)</span>
		</div>

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

		{#if submitError}
			<div class="error-msg">{submitError}</div>
		{/if}

		<button class="btn btn-primary submit-btn" type="submit" disabled={isSubmitting}>
			{#if isSubmitting}<div class="btn-spinner"></div> Creating…
			{:else}Add Database{/if}
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

	.form-input, .form-select {
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary);
		font-size: 13px; font-family: var(--font-sans); padding: 8px 10px;
		outline: none; transition: border-color var(--transition-fast);
	}
	.form-input:focus, .form-select:focus { border-color: var(--accent); }
	.font-mono { font-family: var(--font-mono); }
	.form-hint { font-size: 11px; color: var(--text-dim); }

	.mono {
		font-family: var(--font-mono); font-size: 10px;
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
	}

	.pass-row { display: flex; gap: 6px; }
	.pass-row .form-input { flex: 1; }

	.regen-btn {
		width: 34px; height: 34px; background: var(--bg-elevated);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		cursor: pointer; color: var(--text-muted); font-size: 16px;
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0; transition: all var(--transition-fast);
	}
	.regen-btn:hover { border-color: var(--accent); color: var(--accent); }

	.picker-btn {
		display: flex; align-items: center; gap: 7px;
		padding: 8px 10px; background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary); font-size: 13px;
		font-family: var(--font-sans); cursor: pointer; text-align: left; width: 100%;
		transition: border-color var(--transition-fast);
	}
	.picker-btn:hover { border-color: var(--accent); }
	:global(.picker-icon)   { color: var(--text-dim); flex-shrink: 0; }
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
