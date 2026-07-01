<script lang="ts">
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Service, ImportComposeResponse } from '$lib/api/types';
	import {
		Trash2, Eye, EyeOff, Plus, AlertTriangle, CheckCircle2,
		Server, Network, ChevronRight, Star, ExternalLink, Code2, LayoutList,
		Info, CircleAlert, CircleCheck
	} from '@lucide/svelte';
	import CodeEditor from '$lib/components/CodeEditor.svelte';
	import ServiceDetailPanel from '$lib/panels/ServiceDetailPanel.svelte';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: (service: Service) => void;
	}

	let { projectId, orgId, onCreated }: Props = $props();

	// ── Root service identity ──────────────────────────────────────────
	let rootName = $state('');
	let rootSlug = $state('');
	let slugEdited = $state(false);

	function onRootNameInput(e: Event) {
		const val = (e.target as HTMLInputElement).value;
		rootName = val;
		if (!slugEdited) {
			rootSlug = val.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '');
		}
	}

	function onRootSlugInput(e: Event) {
		rootSlug = (e.target as HTMLInputElement).value;
		slugEdited = true;
	}

	// ── Editor state ───────────────────────────────────────────────────
	const DEFAULT_COMPOSE = `services:
  web:
    image: nginx:latest
    environment:
      - APP_ENV=production

  api:
    image: node:18-alpine
    environment:
      - NODE_ENV=production
      - PORT=3000

networks:
  default:
    driver: overlay
`;

	let composeYaml = $state(DEFAULT_COMPOSE);

	// ── Parsed preview ─────────────────────────────────────────────────
	interface ParsedService { name: string; image: string }
	interface ParsedNetwork { name: string; external: boolean; driver: string }

	let parsedServices = $derived(parseServices(composeYaml));
	let parsedNetworks = $derived(parseNetworks(composeYaml));

	function parseServices(yaml: string): ParsedService[] {
		const lines = yaml.split('\n');
		const result: ParsedService[] = [];
		let inServices = false;
		let currentSvc = '';

		for (const line of lines) {
			const trimmed = line.trimEnd();
			if (/^services\s*:/.test(trimmed)) { inServices = true; continue; }
			if (inServices) {
				if (/^[a-zA-Z]/.test(trimmed) && trimmed.includes(':')) { inServices = false; continue; }
				const svcMatch = trimmed.match(/^  ([a-zA-Z][a-zA-Z0-9_-]*):\s*$/);
				if (svcMatch) { currentSvc = svcMatch[1]; result.push({ name: currentSvc, image: '' }); continue; }
				const imgMatch = trimmed.match(/^    image:\s*['"]?([^\s'"]+)['"]?/);
				if (imgMatch && currentSvc) {
					const last = result.find(s => s.name === currentSvc);
					if (last) last.image = imgMatch[1];
				}
			}
		}
		return result;
	}

	function parseNetworks(yaml: string): ParsedNetwork[] {
		const lines = yaml.split('\n');
		const result: ParsedNetwork[] = [];
		let inNetworks = false;
		let currentNet = '';

		for (const line of lines) {
			const trimmed = line.trimEnd();
			if (/^networks\s*:/.test(trimmed)) { inNetworks = true; continue; }
			if (inNetworks) {
				if (/^[a-zA-Z]/.test(trimmed) && trimmed.includes(':') && !trimmed.startsWith(' ')) { inNetworks = false; continue; }
				const netMatch = trimmed.match(/^  ([a-zA-Z][a-zA-Z0-9_-]*):\s*$/);
				if (netMatch) { currentNet = netMatch[1]; result.push({ name: currentNet, external: false, driver: '' }); continue; }
				if (currentNet) {
					const last = result.find(n => n.name === currentNet);
					if (last) {
						const extMatch = trimmed.match(/^    external:\s*true/);
						if (extMatch) last.external = true;
						const drvMatch = trimmed.match(/^    driver:\s*['"]?([^\s'"#]+)['"]?/);
						if (drvMatch) last.driver = drvMatch[1];
					}
				}
			}
		}
		return result;
	}

	type NetCompat = 'ok' | 'warn';
	interface NetInfo { compat: NetCompat; note: string }

	function networkInfo(driver: string): NetInfo {
		const d = driver.trim() || 'bridge';
		if (d === 'overlay') return {
			compat: 'ok',
			note: 'Multi-host — Shipyard auto-adds attachable: true for compose stacks.'
		};
		if (d === 'host') return {
			compat: 'warn',
			note: 'Shares host network namespace — port conflicts are runtime errors.'
		};
		if (d === 'none') return {
			compat: 'warn',
			note: 'No networking — containers are isolated from each other.'
		};
		// bridge (default) + unknown drivers
		if (d === 'bridge') return {
			compat: 'warn',
			note: 'Single-node only — containers won\'t reach across Swarm nodes.'
		};
		return {
			compat: 'warn',
			note: `Unknown driver "${d}" — may not deploy in Swarm mode.`
		};
	}

	let netCompatWarnings = $derived(
		parsedNetworks
			.filter(n => !n.external)
			.map(n => ({ ...n, info: networkInfo(n.driver) }))
			.filter(n => n.info.compat !== 'ok')
	);
	let hasCompatIssue = $derived(netCompatWarnings.length > 0);

	// ── Editor / Preview tab ──────────────────────────────────────────
	type EditorTab = 'editor' | 'preview';
	let activeEditorTab = $state<EditorTab>('editor');

	// ── Global env overrides ───────────────────────────────────────────
	let globalEnvs = $state<Array<{ key: string; value: string; is_secret: boolean }>>([]);

	function addEnv() { globalEnvs = [...globalEnvs, { key: '', value: '', is_secret: false }]; }
	function removeEnv(i: number) { globalEnvs = globalEnvs.filter((_, idx) => idx !== i); }
	function updateEnv(i: number, field: 'key' | 'value' | 'is_secret', val: string | boolean) {
		globalEnvs = globalEnvs.map((e, idx) => idx === i ? { ...e, [field]: val } : e);
	}

	// ── Submit ─────────────────────────────────────────────────────────
	let submitting = $state(false);
	let submitError = $state('');
	let result = $state<(ImportComposeResponse & { services: Service[]; rootService: Service | null }) | null>(null);

	async function handleImport() {
		if (!composeYaml.trim()) return;
		if (!rootName.trim()) { submitError = 'Stack name is required'; return; }
		if (!rootSlug.trim())  { submitError = 'Stack slug is required'; return; }

		submitting = true;
		submitError = '';
		try {
			const res = await api.importCompose(projectId, composeYaml, rootName.trim(), rootSlug.trim());
			if (res.error) { submitError = res.error.message; return; }
			if (!res.data) return;

			const svcsRes = await api.getServices(projectId);
			const allServices = svcsRes.data ?? [];

			const rootSvc = allServices.find(s => s.id === res.data!.root_service_id) ?? null;
			const childServices = allServices.filter(s => res.data!.service_ids.includes(s.id));

			// Apply global env overrides to all child services
			const validEnvs = globalEnvs.filter(e => e.key.trim());
			if (validEnvs.length > 0) {
				await Promise.all(childServices.map(s => api.bulkSetEnvs(s.id, validEnvs)));
			}

			result = { ...res.data, services: childServices, rootService: rootSvc };

			if (rootSvc) onCreated?.(rootSvc);
		} finally {
			submitting = false;
		}
	}

	function openService(svc: Service) {
		uiStore.pushPanel({
			component: ServiceDetailPanel,
			props: { serviceId: svc.id, projectId, orgId },
			title: svc.name,
		});
	}

	function done() { uiStore.clearPanels(); }
</script>

<div class="panel-wrap">
	{#if result}
		<!-- ── Result view ──────────────────────────────────────────────── -->
		<div class="result-view">
			<div class="result-hero">
				<div class="result-icon success"><CheckCircle2 size={22} /></div>
				<div>
					<p class="result-title">Import complete</p>
					<p class="result-sub">
						{result.services_created} service{result.services_created === 1 ? '' : 's'} ·
						{result.networks_created} network{result.networks_created === 1 ? '' : 's'} created
					</p>
				</div>
			</div>

			{#if result.warnings.length > 0}
				<section class="result-section">
					<div class="section-label"><AlertTriangle size={12} /> Warnings</div>
					<ul class="warning-list">
						{#each result.warnings as w}
							<li class="warning-item">{w}</li>
						{/each}
					</ul>
				</section>
			{/if}

			<section class="result-section">
				<div class="section-label"><Server size={12} /> Services created</div>
				<ul class="service-result-list">
					<!-- Root service first -->
					{#if result.rootService}
						{@const svc = result.rootService}
						<li class="service-result-item">
							<div class="svc-result-info">
								<span class="svc-result-name">{svc.name}</span>
								<span class="root-badge"><Star size={9} /> Root</span>
								<span class="svc-result-image">{svc.type}</span>
							</div>
							<button class="btn btn-ghost btn-sm view-btn" onclick={() => openService(svc)}>
								<ExternalLink size={12} /> View
							</button>
						</li>
					{/if}
					<!-- Child services -->
					{#each result.services as svc (svc.id)}
						<li class="service-result-item service-result-child">
							<div class="svc-result-info">
								<span class="child-indent">↳</span>
								<span class="svc-result-name">{svc.name}</span>
								<span class="svc-result-image">{svc.image}</span>
							</div>
							<button class="btn btn-ghost btn-sm view-btn" onclick={() => openService(svc)}>
								<ExternalLink size={12} /> View
							</button>
						</li>
					{/each}
				</ul>
			</section>

			<button class="btn btn-primary done-btn" onclick={done}>Done</button>
		</div>

	{:else}
		<!-- ── Editor view ──────────────────────────────────────────────── -->
		<form class="form" onsubmit={(e) => { e.preventDefault(); handleImport(); }}>

			<!-- Root stack identity -->
			<div class="form-section">
				<div class="section-label">Stack Identity</div>
				<p class="section-hint">A parent service is created with this name — the compose services become its children.</p>
				<div class="identity-row">
					<div class="field-group">
						<label class="field-label" for="root-name">Stack name</label>
						<input
							id="root-name"
							class="field-input"
							type="text"
							placeholder="My Stack"
							value={rootName}
							oninput={onRootNameInput}
							required
						/>
					</div>
					<div class="field-group">
						<label class="field-label" for="root-slug">Slug</label>
						<input
							id="root-slug"
							class="field-input font-mono"
							type="text"
							placeholder="my-stack"
							value={rootSlug}
							oninput={onRootSlugInput}
							pattern="[a-z0-9-]+"
							title="Lowercase letters, numbers, and hyphens only"
							required
						/>
					</div>
				</div>
			</div>

			<!-- Compose editor + preview tabs -->
			<div class="editor-block">
				<!-- Tab bar -->
				<div class="editor-tabbar">
					<button
						type="button"
						class="editor-tab"
						class:active={activeEditorTab === 'editor'}
						onclick={() => (activeEditorTab = 'editor')}
					>
						<Code2 size={12} /> Editor
					</button>
					<button
						type="button"
						class="editor-tab"
						class:active={activeEditorTab === 'preview'}
						onclick={() => (activeEditorTab = 'preview')}
					>
						<LayoutList size={12} /> Preview
						{#if parsedServices.length > 0}
							<span class="tab-count">{parsedServices.length + parsedNetworks.length}</span>
						{/if}
					</button>
				</div>

				<!-- Tab content -->
				<div class="editor-tab-body">
					{#if activeEditorTab === 'editor'}
						<CodeEditor
							value={composeYaml}
							height="100%"
							onChange={(v) => (composeYaml = v)}
						/>
					{:else}
						<!-- Preview pane -->
						<div class="preview-pane">
							{#if parsedServices.length === 0 && parsedNetworks.length === 0}
								<div class="preview-empty">
									<LayoutList size={28} />
									<p>No services detected yet.</p>
									<span>Switch to the Editor tab and paste your compose file.</span>
								</div>
							{:else}
								{#if parsedServices.length > 0}
									<div class="preview-group">
										<div class="preview-group-title"><Server size={11} /> Child services</div>
										<ul class="preview-list">
											{#each parsedServices as svc (svc.name)}
												<li class="preview-item">
													<div class="preview-item-icon"><Server size={12} /></div>
													<span class="preview-name">{svc.name}</span>
													<span class="preview-image">{svc.image || 'image not set'}</span>
												</li>
											{/each}
										</ul>
									</div>
								{/if}

								{#if parsedNetworks.length > 0}
									<div class="preview-group">
										<div class="preview-group-title"><Network size={11} /> Networks</div>
										<ul class="preview-list">
											{#each parsedNetworks as net (net.name)}
												{@const info = networkInfo(net.driver)}
												<li class="preview-item" class:preview-item-muted={net.external}>
													<div class="preview-item-icon"><Network size={12} /></div>
													<span class="preview-name">{net.name}</span>
													{#if net.external}
														<span class="ext-chip">external · skipped</span>
													{:else}
														<span class="driver-chip driver-chip-{info.compat}">{net.driver || 'bridge'}</span>
														<span class="net-note">{info.note}</span>
													{/if}
												</li>
											{/each}
										</ul>
									</div>

									<!-- Network types reference -->
									<details class="net-guide">
										<summary class="net-guide-summary"><Info size={11} /> Network driver guide</summary>
										<div class="net-guide-body">
											<div class="net-guide-row">
												<span class="driver-chip driver-chip-ok">overlay</span>
												<span>Multi-host. Containers on any Swarm node can reach each other. Shipyard adds <code>attachable: true</code> automatically so compose stacks can join.</span>
											</div>
											<div class="net-guide-row">
												<span class="driver-chip driver-chip-warn">bridge</span>
												<span>Single-node only. Default for standalone compose. Works fine when all containers are on one machine; won't span Swarm nodes.</span>
											</div>
											<div class="net-guide-row">
												<span class="driver-chip driver-chip-warn">host</span>
												<span>Container shares the host's network stack. No port mapping needed, but host port conflicts become runtime errors.</span>
											</div>
											<div class="net-guide-row">
												<span class="driver-chip driver-chip-warn">none</span>
												<span>No network at all. Container is fully isolated — useful for batch jobs that need no connectivity.</span>
											</div>
										</div>
									</details>
								{/if}
							{/if}
						</div>
					{/if}
				</div>
			</div>

			<!-- Global env overrides -->
			<div class="form-section">
				<div class="section-label-row">
					<span class="section-label">Environment Overrides</span>
					<button type="button" class="add-btn" onclick={addEnv}>
						<Plus size={11} /> Add
					</button>
				</div>
				<p class="section-hint" style="margin-bottom: 8px">
					Extra env vars injected into <strong>all</strong> created services, in addition to those in the compose file.
				</p>

				{#if globalEnvs.length > 0}
					<div class="env-list">
						{#each globalEnvs as env, i (i)}
							<div class="env-row">
								<input
									class="env-input font-mono env-key"
									type="text"
									placeholder="KEY"
									value={env.key}
									oninput={(e) => updateEnv(i, 'key', (e.target as HTMLInputElement).value)}
								/>
								<input
									class="env-input font-mono env-val"
									type={env.is_secret ? 'password' : 'text'}
									placeholder="value"
									value={env.value}
									oninput={(e) => updateEnv(i, 'value', (e.target as HTMLInputElement).value)}
								/>
								<button
									type="button"
									class="env-icon-btn"
									class:secret-active={env.is_secret}
									title={env.is_secret ? 'Secret — click to reveal' : 'Plain — click to hide'}
									onclick={() => updateEnv(i, 'is_secret', !env.is_secret)}
								>
									{#if env.is_secret}<EyeOff size={12} />{:else}<Eye size={12} />{/if}
								</button>
								<button type="button" class="env-icon-btn del" onclick={() => removeEnv(i)}>
									<Trash2 size={12} />
								</button>
							</div>
						{/each}
					</div>
				{:else}
					<div class="env-empty">No overrides — compose environment: fields will be used as-is.</div>
				{/if}
			</div>

			{#if hasCompatIssue}
				<div class="compat-banner">
					<CircleAlert size={14} />
					<div>
						<strong>Network compatibility warning</strong>
						<ul class="compat-list">
							{#each netCompatWarnings as w}
								<li><code>{w.name}</code> ({w.driver || 'bridge'}) — {w.info.note}</li>
							{/each}
						</ul>
						<p class="compat-note">Shipyard will attempt deployment, but non-overlay networks may fail in Swarm mode. Consider switching to <strong>overlay</strong>.</p>
					</div>
				</div>
			{/if}

			{#if submitError}
				<div class="error-msg"><AlertTriangle size={13} />{submitError}</div>
			{/if}

			<button
				class="btn btn-primary submit-btn"
				type="submit"
				disabled={submitting || !composeYaml.trim() || !rootName.trim() || !rootSlug.trim()}
			>
				{#if submitting}
					<div class="btn-spinner"></div> Importing…
				{:else}
					<ChevronRight size={14} /> Import Compose
				{/if}
			</button>
		</form>
	{/if}
</div>

<style>
	.panel-wrap {
		height: 100%;
		overflow-y: auto;
		padding: 16px;
		display: flex;
		flex-direction: column;
	}

	.form {
		display: flex;
		flex-direction: column;
		gap: 18px;
		flex: 1;
	}

	/* ── Sections ── */
	.form-section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}

	.section-label-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.section-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.section-hint {
		font-size: 11px;
		color: var(--text-dim);
	}

	/* ── Stack identity inputs ── */
	.identity-row {
		display: flex;
		gap: 10px;
	}

	.field-group {
		display: flex;
		flex-direction: column;
		gap: 4px;
		flex: 1;
		min-width: 0;
	}

	.field-label {
		font-size: 11px;
		font-weight: 500;
		color: var(--text-muted);
	}

	.field-input {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		padding: 7px 10px;
		outline: none;
		width: 100%;
		transition: border-color var(--transition-fast);
	}

	.field-input:focus { border-color: var(--accent); }

	/* ── Editor block (tabs + content) ── */
	.editor-block {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.editor-tabbar {
		display: flex;
		gap: 0;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.editor-tab {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 8px 14px;
		font-size: 12px;
		font-weight: 500;
		font-family: var(--font-sans);
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--text-dim);
		cursor: pointer;
		margin-bottom: -1px;
		transition: color var(--transition-fast), border-color var(--transition-fast);
		white-space: nowrap;
	}

	.editor-tab:hover { color: var(--text-primary); }
	.editor-tab.active { color: var(--accent); border-bottom-color: var(--accent); }

	.tab-count {
		font-size: 10px;
		font-weight: 700;
		padding: 1px 5px;
		border-radius: 99px;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent);
		border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
	}

	.editor-tab-body {
		height: 380px;
		overflow: hidden;
		display: flex;
		flex-direction: column;
	}

	/* strip CodeEditor's own border since editor-block provides it */
	.editor-tab-body :global(.editor-wrap) {
		border: none;
		border-radius: 0;
		height: 100%;
	}

	/* ── Preview pane ── */
	.preview-pane {
		flex: 1;
		overflow-y: auto;
		padding: 14px;
		display: flex;
		flex-direction: column;
		gap: 14px;
		background: var(--bg-base);
	}

	.preview-empty {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 8px;
		color: var(--text-dim);
		text-align: center;
		padding: 32px;
	}

	.preview-empty p { font-size: 14px; font-weight: 600; color: var(--text-muted); margin: 4px 0 0; }
	.preview-empty span { font-size: 12px; }

	.preview-group { display: flex; flex-direction: column; gap: 6px; }

	.preview-group-title {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 10px;
		font-weight: 700;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.08em;
	}

	.preview-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 3px; }

	.preview-item {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 6px 8px;
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.preview-item.preview-item-muted { opacity: 0.5; }

	.preview-item-icon { color: var(--text-dim); display: flex; flex-shrink: 0; }

	.preview-name {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-primary);
		font-family: var(--font-mono);
		min-width: 80px;
	}

	.preview-image {
		flex: 1;
		font-size: 11px;
		color: var(--text-dim);
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.ext-chip {
		font-size: 9px;
		font-weight: 600;
		padding: 1px 6px;
		border-radius: 99px;
		background: var(--bg-surface);
		color: var(--text-dim);
		border: 1px solid var(--border);
	}

	/* ── Env ── */
	.add-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		font-weight: 500;
		padding: 3px 8px;
		border-radius: var(--radius-sm);
		background: transparent;
		border: 1px solid var(--border);
		color: var(--text-muted);
		cursor: pointer;
		transition: all var(--transition-fast);
	}
	.add-btn:hover { border-color: var(--accent); color: var(--accent); }

	.env-list { display: flex; flex-direction: column; gap: 6px; }

	.env-row { display: flex; align-items: center; gap: 5px; }

	.env-input {
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 12px;
		padding: 6px 8px;
		outline: none;
		transition: border-color var(--transition-fast);
	}
	.env-input:focus { border-color: var(--accent); }
	.env-key { width: 120px; flex-shrink: 0; }
	.env-val { flex: 1; min-width: 0; }
	.font-mono { font-family: var(--font-mono); }

	.env-icon-btn {
		width: 28px;
		height: 28px;
		flex-shrink: 0;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-dim);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all var(--transition-fast);
	}
	.env-icon-btn:hover { color: var(--text-primary); border-color: var(--border-hover); }
	.env-icon-btn.secret-active { border-color: var(--accent); color: var(--accent); }
	.env-icon-btn.del:hover { border-color: var(--accent-red); color: var(--accent-red); }

	.env-empty {
		font-size: 11px;
		color: var(--text-dim);
		padding: 10px;
		border: 1px dashed var(--border);
		border-radius: var(--radius-sm);
		text-align: center;
	}

	/* ── Submit ── */
	.submit-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		justify-content: center;
		margin-top: 4px;
	}

	.btn-spinner {
		width: 12px;
		height: 12px;
		border: 2px solid rgba(255, 255, 255, 0.3);
		border-top-color: white;
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	@keyframes spin { to { transform: rotate(360deg); } }

	.error-msg {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 12px;
		color: var(--accent-red);
		padding: 8px 10px;
		background: color-mix(in srgb, var(--accent-red) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent-red) 30%, transparent);
		border-radius: var(--radius-sm);
	}

	/* ── Result view ── */
	.result-view {
		display: flex;
		flex-direction: column;
		gap: 18px;
		flex: 1;
	}

	.result-hero {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		padding: 14px;
		background: color-mix(in srgb, var(--accent-green, #22C55E) 6%, var(--bg-elevated));
		border: 1px solid color-mix(in srgb, var(--accent-green, #22C55E) 25%, transparent);
		border-radius: var(--radius-md);
	}

	.result-icon {
		width: 36px;
		height: 36px;
		border-radius: var(--radius-md);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.result-icon.success { background: color-mix(in srgb, #22C55E 15%, transparent); color: #22C55E; }

	.result-title { font-size: 14px; font-weight: 700; color: var(--text-primary); margin: 0 0 3px; }
	.result-sub   { font-size: 12px; color: var(--text-muted); margin: 0; }

	.result-section { display: flex; flex-direction: column; gap: 8px; }

	.warning-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.warning-item {
		display: flex;
		align-items: flex-start;
		gap: 6px;
		font-size: 11px;
		color: var(--text-muted);
		padding: 6px 10px;
		background: color-mix(in srgb, #F59E0B 8%, transparent);
		border: 1px solid color-mix(in srgb, #F59E0B 25%, transparent);
		border-radius: var(--radius-sm);
	}

	.service-result-list {
		list-style: none;
		margin: 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.service-result-item {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
		padding: 8px 12px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
	}

	.service-result-child {
		margin-left: 16px;
		border-color: color-mix(in srgb, var(--border) 60%, transparent);
	}

	.child-indent {
		font-size: 12px;
		color: var(--text-dim);
		flex-shrink: 0;
	}

	.svc-result-info {
		display: flex;
		align-items: center;
		gap: 8px;
		min-width: 0;
		flex: 1;
	}

	.svc-result-name {
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
		font-family: var(--font-mono);
	}

	.svc-result-image {
		font-size: 11px;
		color: var(--text-dim);
		font-family: var(--font-mono);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.root-badge {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		font-size: 9px;
		font-weight: 700;
		text-transform: uppercase;
		letter-spacing: 0.06em;
		padding: 1px 6px;
		border-radius: 99px;
		background: color-mix(in srgb, var(--accent) 12%, transparent);
		color: var(--accent);
		border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
		flex-shrink: 0;
	}

	.view-btn {
		display: flex;
		align-items: center;
		gap: 4px;
		flex-shrink: 0;
	}

	.done-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		margin-top: auto;
		padding-top: 4px;
	}

	/* ── Network driver badges ── */
	.driver-chip {
		font-size: 9px;
		font-weight: 700;
		padding: 1px 6px;
		border-radius: 99px;
		border: 1px solid transparent;
		flex-shrink: 0;
		font-family: var(--font-mono);
	}
	.driver-chip-ok {
		background: color-mix(in srgb, #22C55E 12%, transparent);
		color: #16A34A;
		border-color: color-mix(in srgb, #22C55E 30%, transparent);
	}
	.driver-chip-warn {
		background: color-mix(in srgb, #F59E0B 12%, transparent);
		color: #B45309;
		border-color: color-mix(in srgb, #F59E0B 30%, transparent);
	}

	.net-note {
		font-size: 10px;
		color: var(--text-dim);
		flex: 1;
		line-height: 1.4;
	}

	/* ── Network driver guide (collapsible) ── */
	.net-guide {
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		overflow: hidden;
	}
	.net-guide-summary {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 7px 10px;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
		cursor: pointer;
		list-style: none;
		user-select: none;
	}
	.net-guide-summary::-webkit-details-marker { display: none; }
	.net-guide[open] .net-guide-summary { border-bottom: 1px solid var(--border); }
	.net-guide-body {
		padding: 8px 10px 10px;
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.net-guide-row {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		font-size: 11px;
		color: var(--text-muted);
		line-height: 1.5;
	}
	.net-guide-row code {
		font-family: var(--font-mono);
		font-size: 10px;
		background: var(--bg-surface);
		padding: 0 3px;
		border-radius: 3px;
	}

	/* ── Pre-create compat warning banner ── */
	.compat-banner {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		padding: 10px 12px;
		background: color-mix(in srgb, #F59E0B 8%, transparent);
		border: 1px solid color-mix(in srgb, #F59E0B 30%, transparent);
		border-radius: var(--radius-sm);
		color: #92400E;
	}
	.compat-banner :global(svg) { flex-shrink: 0; margin-top: 1px; }
	.compat-banner strong { font-size: 12px; font-weight: 700; display: block; margin-bottom: 4px; }
	.compat-list {
		margin: 0 0 6px 0;
		padding-left: 16px;
		display: flex;
		flex-direction: column;
		gap: 2px;
	}
	.compat-list li { font-size: 11px; line-height: 1.4; }
	.compat-list code {
		font-family: var(--font-mono);
		font-size: 10px;
		background: color-mix(in srgb, #F59E0B 15%, transparent);
		padding: 0 3px;
		border-radius: 3px;
	}
	.compat-note { font-size: 11px; margin: 0; }
</style>
