<script lang="ts">
	import { Container, GitBranch, Database, FileCode, HardDrive, Network, LayoutTemplate, Globe, Zap } from '@lucide/svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import type { Service } from '$lib/api/types';
	import DockerImagePanel from './resources/DockerImagePanel.svelte';
	import GitRepoPanel from './resources/GitRepoPanel.svelte';
	import DatabasePanel from './resources/DatabasePanel.svelte';
	import DockerComposePanel from './resources/DockerComposePanel.svelte';
	import VolumePanel from './resources/VolumePanel.svelte';
	import NetworkPanel from './resources/NetworkPanel.svelte';
	import TemplatePanel from './resources/TemplatePanel.svelte';
	import StaticSitePanel from './resources/StaticSitePanel.svelte';
	import EdgeFunctionPanel from './resources/EdgeFunctionPanel.svelte';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: (service: Service) => void;
	}

	let { projectId, orgId, onCreated }: Props = $props();

	type ResourceType = 'docker' | 'git' | 'database' | 'compose' | 'static' | 'volume' | 'network' | 'template' | 'edge-function';

	const PANELS: Record<ResourceType, { component: any; title: string }> = {
		docker:          { component: DockerImagePanel,  title: 'Docker Image' },
		git:             { component: GitRepoPanel,       title: 'Git Repository' },
		database:        { component: DatabasePanel,      title: 'Database' },
		compose:         { component: DockerComposePanel, title: 'Docker Compose' },
		static:          { component: StaticSitePanel,    title: 'Static Site' },
		volume:          { component: VolumePanel,        title: 'Volume' },
		network:         { component: NetworkPanel,       title: 'Network' },
		template:        { component: TemplatePanel,      title: 'Templates' },
		'edge-function': { component: EdgeFunctionPanel,  title: 'Edge Function' },
	};

	const resourceTypes: { id: ResourceType; label: string; description: string; icon: any }[] = [
		{ id: 'docker',          label: 'Docker Image',   description: 'Deploy any Docker image',            icon: Container },
		{ id: 'git',             label: 'Git Repo',        description: 'Build & deploy from source',         icon: GitBranch },
		{ id: 'database',        label: 'Database',         description: 'PostgreSQL, MySQL, Redis, MongoDB',  icon: Database },
		{ id: 'compose',         label: 'Docker Compose',   description: 'Import a docker-compose.yml',        icon: FileCode },
		{ id: 'static',          label: 'Static Site',      description: 'Host HTML/CSS/JS via shared nginx', icon: Globe },
		{ id: 'edge-function',   label: 'Edge Function',    description: 'Serverless functions from a Git repo', icon: Zap },
		{ id: 'volume',          label: 'Volume',            description: 'Persistent storage volume',          icon: HardDrive },
		{ id: 'network',         label: 'Network',           description: 'Attach services to a network',       icon: Network },
		{ id: 'template',        label: 'Template',          description: 'Start from a pre-built template',    icon: LayoutTemplate },
	];

	function open(type: ResourceType) {
		const { component, title } = PANELS[type];
		uiStore.pushPanel({ component, props: { projectId, orgId, onCreated }, title });
	}
</script>

<div class="add-panel">
	<p class="panel-desc">Choose the type of resource to add to this project.</p>

	<div class="resource-grid">
		{#each resourceTypes as rt (rt.id)}
			<button class="resource-card" onclick={() => open(rt.id)}>
				<div class="card-icon"><rt.icon size={20} /></div>
				<div class="card-text">
					<span class="card-label">{rt.label}</span>
					<span class="card-desc">{rt.description}</span>
				</div>
			</button>
		{/each}
	</div>
</div>

<style>
	.add-panel {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		height: 100%;
		overflow-y: auto;
	}

	.panel-desc { font-size: 13px; color: var(--text-muted); margin: 0; }

	.resource-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }

	.resource-card {
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 8px;
		padding: 14px 12px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		cursor: pointer;
		transition: all var(--transition-fast);
		text-align: left;
	}

	.resource-card:hover {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 8%, var(--bg-elevated));
	}

	.card-icon { color: var(--accent); }

	.card-text { display: flex; flex-direction: column; gap: 2px; }
	.card-label { font-size: 13px; font-weight: 600; color: var(--text-primary); }
	.card-desc  { font-size: 11px; color: var(--text-muted); line-height: 1.4; }
</style>
