<script lang="ts">
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Service, ImportComposeResponse } from '$lib/api/types';
	import {
		Trash2, Eye, EyeOff, Plus, AlertTriangle, CheckCircle2,
		Server, Network, ChevronRight, Star, ExternalLink, Code2, LayoutList,
		Info
	} from '@lucide/svelte';
	import { Button, Badge, Card, ListRow, Tabs, FormField, TextField, InlineAlert, Spinner, EmptyState, SectionLabel } from '$lib/components/ui';
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
			<Card padding="4px 14px">
				<ListRow
					iconTone="green"
					title="Import complete"
					meta="{result.services_created} service{result.services_created === 1 ? '' : 's'} · {result.networks_created} network{result.networks_created === 1 ? '' : 's'} created"
				>
					{#snippet icon()}<CheckCircle2 size={16} />{/snippet}
				</ListRow>
			</Card>

			{#if result.warnings.length > 0}
				<section class="result-section">
					<SectionLabel><AlertTriangle size={12} /> Warnings</SectionLabel>
					<ul class="plain-list">
						{#each result.warnings as w}
							<li><InlineAlert tone="warning">{w}</InlineAlert></li>
						{/each}
					</ul>
				</section>
			{/if}

			<section class="result-section">
				<SectionLabel><Server size={12} /> Services created</SectionLabel>
				<ul class="plain-list">
					<!-- Root service first -->
					{#if result.rootService}
						{@const svc = result.rootService}
						<li>
							<Card padding="8px 12px">
								<div class="service-result-item">
									<div class="svc-result-info">
										<span class="svc-result-name">{svc.name}</span>
										<Badge tone="blue"><Star size={9} /> Root</Badge>
										<span class="svc-result-image">{svc.type}</span>
									</div>
									<Button variant="ghost" size="sm" onclick={() => openService(svc)}>
										<ExternalLink size={12} /> View
									</Button>
								</div>
							</Card>
						</li>
					{/if}
					<!-- Child services -->
					{#each result.services as svc (svc.id)}
						<li class="service-result-child">
							<Card padding="8px 12px">
								<div class="service-result-item">
									<div class="svc-result-info">
										<span class="child-indent">↳</span>
										<span class="svc-result-name">{svc.name}</span>
										<span class="svc-result-image">{svc.image}</span>
									</div>
									<Button variant="ghost" size="sm" onclick={() => openService(svc)}>
										<ExternalLink size={12} /> View
									</Button>
								</div>
							</Card>
						</li>
					{/each}
				</ul>
			</section>

			<div class="done-row"><Button onclick={done}>Done</Button></div>
		</div>

	{:else}
		<!-- ── Editor view ──────────────────────────────────────────────── -->
		<form class="form" onsubmit={(e) => { e.preventDefault(); handleImport(); }}>

			<!-- Root stack identity -->
			<div class="form-section">
				<SectionLabel>Stack Identity</SectionLabel>
				<p class="section-hint">A parent service is created with this name — the compose services become its children.</p>
				<div class="identity-row">
					<div class="field-group">
						<FormField label="Stack name" for="root-name">
							<TextField
								id="root-name"
								type="text"
								placeholder="My Stack"
								value={rootName}
								oninput={onRootNameInput}
								required
							/>
						</FormField>
					</div>
					<div class="field-group mono-field">
						<FormField label="Slug" for="root-slug">
							<TextField
								id="root-slug"
								type="text"
								placeholder="my-stack"
								value={rootSlug}
								oninput={onRootSlugInput}
								pattern="[a-z0-9-]+"
								title="Lowercase letters, numbers, and hyphens only"
								required
							/>
						</FormField>
					</div>
				</div>
			</div>

			<!-- Compose editor + preview tabs -->
			<div class="editor-block">
				<Tabs
					bind:value={activeEditorTab}
					ariaLabel="Compose view"
					tabs={[
						{ id: 'editor', label: 'Editor', icon: Code2 },
						{ id: 'preview', label: 'Preview', icon: LayoutList, badge: parsedServices.length > 0 ? String(parsedServices.length + parsedNetworks.length) : undefined },
					]}
				/>

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
								<EmptyState message="No services detected yet." sub="Switch to the Editor tab and paste your compose file.">
									{#snippet icon()}<LayoutList size={28} />{/snippet}
								</EmptyState>
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
														<Badge tone="neutral">external · skipped</Badge>
													{:else}
														<Badge tone={info.compat === 'ok' ? 'green' : 'yellow'}>{net.driver || 'bridge'}</Badge>
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
												<Badge tone="green">overlay</Badge>
												<span>Multi-host. Containers on any Swarm node can reach each other. Shipyard adds <code>attachable: true</code> automatically so compose stacks can join.</span>
											</div>
											<div class="net-guide-row">
												<Badge tone="yellow">bridge</Badge>
												<span>Single-node only. Default for standalone compose. Works fine when all containers are on one machine; won't span Swarm nodes.</span>
											</div>
											<div class="net-guide-row">
												<Badge tone="yellow">host</Badge>
												<span>Container shares the host's network stack. No port mapping needed, but host port conflicts become runtime errors.</span>
											</div>
											<div class="net-guide-row">
												<Badge tone="yellow">none</Badge>
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
					<SectionLabel>Environment Overrides</SectionLabel>
					<Button variant="secondary" size="sm" onclick={addEnv}>
						<Plus size={11} /> Add
					</Button>
				</div>
				<p class="section-hint" style="margin-bottom: 8px">
					Extra env vars injected into <strong>all</strong> created services, in addition to those in the compose file.
				</p>

				{#if globalEnvs.length > 0}
					<div class="env-list">
						{#each globalEnvs as env, i (i)}
							<div class="env-row">
								<div class="mono-field env-key">
									<TextField
										type="text"
										placeholder="KEY"
										aria-label="Variable name"
										value={env.key}
										oninput={(e) => updateEnv(i, 'key', (e.target as HTMLInputElement).value)}
									/>
								</div>
								<div class="mono-field env-val">
									<TextField
										type={env.is_secret ? 'password' : 'text'}
										placeholder="value"
										aria-label="Variable value"
										value={env.value}
										oninput={(e) => updateEnv(i, 'value', (e.target as HTMLInputElement).value)}
									/>
								</div>
								<Button
									variant={env.is_secret ? 'primary' : 'secondary'}
									size="icon"
									title={env.is_secret ? 'Secret — click to reveal' : 'Plain — click to hide'}
									aria-label={env.is_secret ? 'Secret — click to reveal' : 'Plain — click to hide'}
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
				{:else}
					<div class="env-empty">No overrides — compose environment: fields will be used as-is.</div>
				{/if}
			</div>

			{#if hasCompatIssue}
				<InlineAlert tone="warning">
					<div class="compat-banner">
						<AlertTriangle size={14} />
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
				</InlineAlert>
			{/if}

			{#if submitError}
				<div role="alert"><InlineAlert tone="error">{submitError}</InlineAlert></div>
			{/if}

			<Button type="submit" disabled={submitting || !composeYaml.trim() || !rootName.trim() || !rootSlug.trim()}>
				{#if submitting}
					<Spinner size={12} tone="current" /> Importing…
				{:else}
					<ChevronRight size={14} /> Import Compose
				{/if}
			</Button>
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

	.mono-field :global(input) { font-family: var(--font-mono); }

	/* ── Sections ── */
	.form-section {
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.form-section :global(.ui-section-label) { margin-bottom: 0; }

	.section-label-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.section-hint {
		font-size: 11px;
		color: var(--text-dim);
		margin: 0;
	}

	/* ── Stack identity inputs ── */
	.identity-row {
		display: flex;
		gap: 10px;
	}

	.field-group {
		flex: 1;
		min-width: 0;
	}

	/* ── Editor block (tabs + content) ── */
	.editor-block {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.editor-block :global(.ui-tabs) {
		background: var(--bg-elevated);
		flex-shrink: 0;
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

	.preview-group { display: flex; flex-direction: column; gap: 6px; }

	.preview-group-title {
		display: flex;
		align-items: center;
		gap: 5px;
		font-size: 10px;
		font-weight: 700;
		color: var(--text-dim);
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

	.net-note {
		font-size: 10px;
		color: var(--text-dim);
		flex: 1;
		line-height: 1.4;
	}

	/* ── Env ── */
	.env-list { display: flex; flex-direction: column; gap: 6px; }

	.env-row { display: flex; align-items: center; gap: 5px; }

	.env-key { width: 120px; flex-shrink: 0; }
	.env-val { flex: 1; min-width: 0; }

	.env-empty {
		font-size: 11px;
		color: var(--text-dim);
		padding: 10px;
		border: 1px dashed var(--border);
		border-radius: var(--radius-sm);
		text-align: center;
	}

	/* ── Result view ── */
	.result-view {
		display: flex;
		flex-direction: column;
		gap: 18px;
		flex: 1;
	}

	.result-section { display: flex; flex-direction: column; }
	.result-section :global(.ui-section-label) { display: flex; align-items: center; gap: 5px; margin-bottom: 8px; }

	.plain-list {
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
	}

	.service-result-child { margin-left: 16px; }

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

	.done-row { margin-top: auto; padding-top: 4px; display: flex; flex-direction: column; }
	.done-row :global(.ui-btn) { width: 100%; }

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
		background: color-mix(in srgb, var(--accent-yellow) 15%, transparent);
		padding: 0 3px;
		border-radius: 3px;
	}
	.compat-note { font-size: 11px; margin: 0; }
</style>
