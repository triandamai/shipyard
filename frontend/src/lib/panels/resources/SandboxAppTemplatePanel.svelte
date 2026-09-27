<script lang="ts">
	import { Code2, FileCode, Globe, Terminal, Atom, Layers, Flame, Triangle, Box, Rocket } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { uiStore } from '$lib/stores/ui.store';
	import type { SandboxTemplate } from '$lib/api/types';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated: () => void;
	}

	let { projectId, onCreated }: Props = $props();

	let name = $state('');
	let slug = $state('');
	let template = $state<SandboxTemplate>('node');
	let variant = $state<'js' | 'ts'>('js');
	let isCreating = $state(false);
	let error = $state<string | null>(null);

	// Frameworks whose official tooling offers a genuine JS/TS choice — Nuxt
	// is deliberately excluded (its scaffolder defaults to TypeScript with no
	// real plain-JS mode in current tooling), so it has no toggle at all.
	const TS_CAPABLE: Partial<Record<SandboxTemplate, SandboxTemplate>> = {
		react: 'react-ts',
		vue: 'vue-ts',
		sveltekit: 'sveltekit-ts',
		next: 'next-ts',
		astro: 'astro-ts'
	};

	function selectTemplate(base: SandboxTemplate) {
		variant = 'js';
		template = base;
	}

	function setVariant(v: 'js' | 'ts') {
		variant = v;
		const base = (Object.entries(TS_CAPABLE).find(([, ts]) => ts === template)?.[0] as SandboxTemplate) ?? template;
		template = v === 'ts' ? (TS_CAPABLE[base] ?? base) : base;
	}

	let selectedBase = $derived(
		(Object.entries(TS_CAPABLE).find(([, ts]) => ts === template)?.[0] as SandboxTemplate) ?? template
	);

	function slugify(value: string): string {
		return value.toLowerCase().trim().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
	}

	function onNameInput(value: string) {
		name = value;
		slug = slugify(value);
	}

	async function create() {
		if (!name || !slug) {
			error = 'Name is required';
			return;
		}
		isCreating = true;
		error = null;
		const res = await api.createSandboxApp(projectId, { name, slug, template });
		isCreating = false;
		if (res.data) {
			onCreated();
			uiStore.popPanel();
		} else {
			error = res.error?.message ?? 'Failed to create app';
		}
	}
</script>

<div class="template-panel">
	<label class="field">
		<span>Name</span>
		<input class="input" value={name} oninput={(e) => onNameInput(e.currentTarget.value)} placeholder="my-app" />
	</label>

	<label class="field">
		<span>Slug</span>
		<input class="input" value={slug} oninput={(e) => (slug = e.currentTarget.value)} />
	</label>

	<div class="field">
		<span>Template</span>
		<div class="template-group-label">Basic</div>
		<div class="template-grid">
			<button class="template-card" class:selected={template === 'node'} onclick={() => selectTemplate('node')}>
				<Code2 size={16} /> Node
			</button>
			<button class="template-card" class:selected={template === 'python'} onclick={() => selectTemplate('python')}>
				<FileCode size={16} /> Python
			</button>
			<button class="template-card" class:selected={template === 'static'} onclick={() => selectTemplate('static')}>
				<Globe size={16} /> Static
			</button>
			<button class="template-card" class:selected={template === 'custom'} onclick={() => selectTemplate('custom')}>
				<Terminal size={16} /> Custom
			</button>
		</div>
		{#if template === 'custom'}
			<p class="template-hint">Starts blank with a starter <code>shipyard.json.example</code>. Scaffold your own project via the Terminal tab, then Stop and Start it from the app's canvas panel to apply it.</p>
		{/if}

		<div class="template-group-label">Frameworks</div>
		<div class="template-grid framework-grid">
			<button class="template-card" class:selected={selectedBase === 'react'} onclick={() => selectTemplate('react')}>
				<Atom size={16} /> React
			</button>
			<button class="template-card" class:selected={selectedBase === 'vue'} onclick={() => selectTemplate('vue')}>
				<Layers size={16} /> Vue
			</button>
			<button class="template-card" class:selected={selectedBase === 'sveltekit'} onclick={() => selectTemplate('sveltekit')}>
				<Flame size={16} /> SvelteKit
			</button>
			<button class="template-card" class:selected={selectedBase === 'next'} onclick={() => selectTemplate('next')}>
				<Triangle size={16} /> Next.js
			</button>
			<button class="template-card" class:selected={selectedBase === 'nuxt'} onclick={() => selectTemplate('nuxt')}>
				<Box size={16} /> Nuxt
			</button>
			<button class="template-card" class:selected={selectedBase === 'astro'} onclick={() => selectTemplate('astro')}>
				<Rocket size={16} /> Astro
			</button>
		</div>
		{#if TS_CAPABLE[selectedBase]}
			<div class="variant-toggle">
				<button class="variant-btn" class:active={variant === 'js'} onclick={() => setVariant('js')}>JavaScript</button>
				<button class="variant-btn" class:active={variant === 'ts'} onclick={() => setVariant('ts')}>TypeScript</button>
			</div>
		{/if}
	</div>

	{#if error}
		<p class="error">{error}</p>
	{/if}

	<button class="btn btn-primary" onclick={create} disabled={isCreating}>
		{isCreating ? 'Creating…' : 'Create App'}
	</button>
</div>

<style>
	.template-panel {
		display: flex;
		flex-direction: column;
		gap: 12px;
		padding: 16px;
	}
	.field {
		display: flex;
		flex-direction: column;
		gap: 4px;
		font-size: 12px;
		color: var(--text-secondary);
	}
	.template-group-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		margin-top: 8px;
	}
	.template-grid {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.template-card {
		flex: 1 1 90px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		background: var(--bg-surface);
		cursor: pointer;
		font-size: 12px;
		color: var(--text-secondary);
	}
	.template-card.selected {
		border-color: var(--accent);
		color: var(--accent);
	}
	.template-hint {
		font-size: 11px;
		color: var(--text-dim);
		margin: 2px 0 0;
		line-height: 1.4;
	}
	.template-hint code {
		font-family: var(--font-mono);
		background: var(--bg-elevated);
		padding: 1px 4px;
		border-radius: 3px;
	}
	.variant-toggle {
		display: flex;
		gap: 4px;
		margin-top: 4px;
	}
	.variant-btn {
		flex: 1;
		padding: 6px 10px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm, 4px);
		background: var(--bg-surface);
		color: var(--text-secondary);
		font-size: 12px;
		cursor: pointer;
	}
	.variant-btn.active {
		border-color: var(--accent);
		color: var(--accent);
	}
	.error {
		color: var(--accent-red);
		font-size: 12px;
	}
</style>
