<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import type { Template, Service } from '$lib/api/types';
	import DockerImagePanel from './DockerImagePanel.svelte';
	import DatabasePanel from './DatabasePanel.svelte';
	import GitRepoPanel from './GitRepoPanel.svelte';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: (service: Service) => void;
	}

	let { projectId, orgId, onCreated }: Props = $props();

	let templates = $state<Template[]>([]);
	let loading = $state(true);

	onMount(async () => {
		const res = await api.getTemplates();
		if (res.data) templates = res.data;
		loading = false;
	});

	function openTemplate(tpl: Template) {
		if (tpl.type === 'database') {
			uiStore.pushPanel({
				component: DatabasePanel,
				props: { projectId, orgId, onCreated, initialName: tpl.name },
				title: tpl.name,
			});
		} else if (tpl.type === 'git') {
			uiStore.pushPanel({
				component: GitRepoPanel,
				props: { projectId, orgId, onCreated, initialName: tpl.name },
				title: tpl.name,
			});
		} else {
			uiStore.pushPanel({
				component: DockerImagePanel,
				props: {
					projectId,
					orgId,
					onCreated,
					initialName: tpl.name,
					initialSlug: tpl.id,
					initialImage: tpl.image ?? '',
				},
				title: tpl.name,
			});
		}
	}
</script>

<div class="panel-wrap">
	{#if loading}
		<div class="loading-row">
			<div class="spinner"></div> Loading templates…
		</div>
	{:else if templates.length === 0}
		<p class="empty">No templates available.</p>
	{:else}
		<p class="hint">Select a template to pre-fill service settings.</p>
		<div class="template-list">
			{#each templates as tpl (tpl.id)}
				<button class="template-row" onclick={() => openTemplate(tpl)}>
					<span class="template-name">{tpl.name}</span>
					{#if tpl.description}<span class="template-desc">{tpl.description}</span>{/if}
					{#if tpl.image}<code class="template-image">{tpl.image}</code>{/if}
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; display: flex; flex-direction: column; gap: 12px; }

	.hint { font-size: 13px; color: var(--text-muted); margin: 0; }
	.empty { font-size: 13px; color: var(--text-dim); margin: 0; }

	.loading-row { display: flex; align-items: center; gap: 8px; color: var(--text-muted); font-size: 13px; }

	.spinner {
		width: 14px; height: 14px; border: 2px solid var(--border);
		border-top-color: var(--accent); border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	.template-list { display: flex; flex-direction: column; gap: 4px; }

	.template-row {
		display: flex; flex-direction: column; gap: 2px;
		padding: 10px 12px; background: var(--bg-elevated);
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		cursor: pointer; text-align: left;
		transition: border-color var(--transition-fast);
	}
	.template-row:hover {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 8%, var(--bg-elevated));
	}

	.template-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
	.template-desc { font-size: 11px; color: var(--text-muted); }

	.template-image {
		font-size: 10px; font-family: var(--font-mono); color: var(--text-dim);
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
		margin-top: 2px; display: inline-block; width: fit-content;
	}

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
