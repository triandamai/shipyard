<script lang="ts">
	import { Code2 } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { uiStore } from '$lib/stores/ui.store';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated: () => void;
	}

	let { projectId, onCreated }: Props = $props();

	let name = $state('');
	let slug = $state('');
	let template = $state<'node' | 'python' | 'static'>('node');
	let isCreating = $state(false);
	let error = $state<string | null>(null);

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
		<div class="template-grid">
			<button class="template-card" class:selected={template === 'node'} onclick={() => (template = 'node')}>
				<Code2 size={16} /> Node
			</button>
			<button class="template-card" class:selected={template === 'python'} onclick={() => (template = 'python')}>
				<Code2 size={16} /> Python
			</button>
			<button class="template-card" class:selected={template === 'static'} onclick={() => (template = 'static')}>
				<Code2 size={16} /> Static
			</button>
		</div>
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
	.template-grid {
		display: flex;
		gap: 8px;
	}
	.template-card {
		flex: 1;
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
	.error {
		color: var(--accent-red);
		font-size: 12px;
	}
</style>
