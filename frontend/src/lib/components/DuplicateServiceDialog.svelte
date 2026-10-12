<script lang="ts">
	import { Copy } from '@lucide/svelte';
	import { Modal, Button, FormField, TextField, Select, InlineAlert } from '$lib/components/ui';
	import { api, type DuplicateServiceResult } from '$lib/api/client';
	import type { Project, MqttPayload } from '$lib/api/types';
	import { eventBus } from '$lib/mqtt/eventBus';

	interface Props {
		open: boolean;
		orgId: string;
		projectId: string;
		service: { id: string; name: string; slug: string };
		onDuplicated?: (result: DuplicateServiceResult) => void;
	}

	let { open = $bindable(), orgId, projectId, service, onDuplicated }: Props = $props();

	let name = $state('');
	let slug = $state('');
	let slugTouched = $state(false);
	let targetProject = $state('');
	let projects = $state<Project[]>([]);
	let error = $state('');
	let busy = $state(false);

	function slugify(s: string): string {
		return s
			.toLowerCase()
			.replace(/[^a-z0-9]+/g, '-')
			.replace(/^-+|-+$/g, '')
			.slice(0, 63);
	}

	// Reset the form each time the dialog opens.
	$effect(() => {
		if (!open) return;
		name = `${service.name} copy`;
		slug = slugify(`${service.slug}-copy`);
		slugTouched = false;
		targetProject = projectId;
		error = '';
		api.getProjects(orgId).then((res) => {
			projects = res.data ?? [];
		});
	});

	$effect(() => {
		if (!slugTouched) slug = slugify(name);
	});

	let projectOptions = $derived(
		projects.length
			? projects.map((p) => ({ value: p.id, label: p.id === projectId ? `${p.name} (this project)` : p.name }))
			: [{ value: projectId, label: 'This project' }]
	);
	let slugValid = $derived(/^[a-z0-9][a-z0-9-]{0,62}$/.test(slug));
	let otherProject = $derived(targetProject !== projectId);

	async function submit() {
		if (busy || !name.trim() || !slugValid) return;
		busy = true;
		error = '';
		const res = await api.duplicateService(projectId, service.id, {
			name: name.trim(),
			slug,
			target_project_id: otherProject ? targetProject : undefined
		});
		busy = false;
		if (res.error || !res.data) {
			error = res.error?.message ?? 'Duplicate failed';
			return;
		}
		open = false;
		// The server also publishes this over MQTT; emitting it locally keeps the
		// canvas in sync when the broker connection is down.
		const payload: MqttPayload = {
			event: 'topology.changed',
			timestamp: new Date().toISOString(),
			meta: { reason: 'service_duplicated', service_id: res.data.service_id }
		};
		eventBus.emit(`platform/orgs/${orgId}/projects/${res.data.project_id}/topology`, payload);
		onDuplicated?.(res.data);
	}
</script>

<Modal bind:open title={`Duplicate ${service.name}`}>
	<form
		class="dup-form"
		onsubmit={(e) => {
			e.preventDefault();
			submit();
		}}
	>
		<FormField label="Name" for="dup-name">
			<TextField id="dup-name" bind:value={name} autocomplete="off" />
		</FormField>
		<FormField
			label="Slug"
			for="dup-slug"
			hint="Lowercase letters, digits and dashes. Must be unique in the project."
			error={slug && !slugValid ? 'Use a-z, 0-9 and -' : undefined}
		>
			<TextField id="dup-slug" bind:value={slug} oninput={() => (slugTouched = true)} autocomplete="off" />
		</FormField>
		<FormField label="Project" for="dup-project">
			<Select id="dup-project" bind:value={targetProject} options={projectOptions} />
		</FormField>

		<ul class="dup-notes">
			<li>Copies the image or git settings, ports, resources, env vars and volume mounts.</li>
			<li>Volumes start empty, domains aren't copied, and the copy stays stopped until you deploy it.</li>
			{#if otherProject}
				<li>Networks of this project aren't available in the other project, so they're not attached.</li>
			{/if}
		</ul>

		{#if error}
			<InlineAlert tone="error">{error}</InlineAlert>
		{/if}
	</form>

	{#snippet footer()}
		<Button variant="secondary" size="sm" onclick={() => (open = false)} disabled={busy}>Cancel</Button>
		<Button variant="primary" size="sm" onclick={submit} disabled={busy || !name.trim() || !slugValid}>
			<Copy size={12} />
			{busy ? 'Duplicating…' : 'Duplicate'}
		</Button>
	{/snippet}
</Modal>

<style>
	.dup-form {
		display: flex;
		flex-direction: column;
		gap: 12px;
		min-width: min(420px, 100%);
	}
	.dup-notes {
		margin: 0;
		padding-left: 18px;
		font-size: 12px;
		line-height: 1.5;
		color: var(--text-muted);
	}
</style>
