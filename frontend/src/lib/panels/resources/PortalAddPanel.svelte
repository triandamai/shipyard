<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { projectStore } from '$lib/stores/project.store';
	import { topologyStore } from '$lib/stores/topology.store';
	import { toastStore } from '$lib/stores/toast.store';
	import { api } from '$lib/api/client';
	import type { Service, Project, TopologyNode } from '$lib/api/types';
	import { FormField, Select, Button, InlineAlert, Spinner } from '$lib/components/ui';
	import { FolderGit2 } from '@lucide/svelte';

	interface Props {
		projectId: string;
		orgId: string;
		onCreated?: () => void;
	}

	let { projectId, orgId, onCreated }: Props = $props();

	let destinationProjectId = $state('');
	let destinationServiceId = $state('');
	let localServiceId = $state('');

	let availableProjects = $derived(
		$projectStore.projects.filter((p: Project) => p.id !== projectId)
	);

	let remoteServices = $state<Service[]>([]);
	let localServices = $state<Service[]>([]);
	let isLoadingRemote = $state(false);
	let isSubmitting = $state(false);
	let submitError = $state('');

	onMount(async () => {
		const locRes = await api.getServices(projectId);
		if (locRes.data) {
			localServices = locRes.data;
		}

		if (availableProjects.length > 0 && !destinationProjectId) {
			destinationProjectId = availableProjects[0].id;
		}
	});

	$effect(() => {
		const destId = destinationProjectId;
		if (!destId) {
			remoteServices = [];
			return;
		}

		let cancelled = false;
		isLoadingRemote = true;
		destinationServiceId = '';

		api.getServices(destId).then((res) => {
			if (cancelled) return;
			isLoadingRemote = false;
			remoteServices = res.data ?? [];
		});

		return () => {
			cancelled = true;
		};
	});

	async function handleCreateLink(e: SubmitEvent) {
		e.preventDefault();
		submitError = '';

		if (!destinationProjectId) {
			submitError = 'Please select a destination project';
			return;
		}

		isSubmitting = true;
		try {
			const destProject = $projectStore.projects.find((p: Project) => p.id === destinationProjectId);
			const destService = remoteServices.find((s) => s.id === destinationServiceId);

			const nodeId = `portal_${destinationProjectId}_${destService ? destService.id : 'proj'}_${Date.now()}`;
			const linkNode: TopologyNode = {
				id: nodeId,
				type: 'portal',
				data: {
					resource_type: destService ? 'service' : 'project',
					resource_id: destService ? destService.id : destinationProjectId,
					resource_name: destService ? destService.name : (destProject?.name ?? 'Project Gateway'),
					source_project_id: destinationProjectId,
					source_project_name: destProject?.name ?? 'Unknown Project',
					source_project_slug: destProject?.slug ?? '',
				}
			};

			topologyStore.addNode(linkNode);

			if (localServiceId) {
				topologyStore.addEdge({
					id: `e_link_${localServiceId}_${nodeId}`,
					source: `svc_${localServiceId}`,
					target: nodeId,
					type: 'env_ref'
				});
			}

			toastStore.add({
				type: 'success',
				title: 'Project Linked',
				message: `Link to ${destProject?.name ?? 'destination project'} added to canvas.`
			});

			onCreated?.();
			uiStore.clearPanels();
		} catch (err: any) {
			submitError = err?.message || 'Failed to link project';
		} finally {
			isSubmitting = false;
		}
	}
</script>

<div class="panel-wrap">
	<div class="panel-hero">
		<div class="panel-badge">
			<FolderGit2 size={13} />
			<span>Cross-Project Connection</span>
		</div>
		<p class="panel-desc">
			Link services across projects in this organization. The linked project card displays destination details and opens a confirmation dialog when clicked.
		</p>
	</div>

	<form class="form" onsubmit={handleCreateLink}>
		{#if availableProjects.length === 0}
			<InlineAlert tone="info">
				No other projects found in this organization. Create another project first to establish a cross-project connection.
			</InlineAlert>
		{:else}
			<FormField label="Destination Project" for="link-dest-proj">
				<Select
					id="link-dest-proj"
					bind:value={destinationProjectId}
					options={availableProjects.map((p: Project) => ({
						value: p.id,
						label: `${p.name} (${p.slug})`
					}))}
				/>
			</FormField>

			<FormField label="Target Service (optional)" for="link-dest-svc">
				{#if isLoadingRemote}
					<div class="loading-state">
						<Spinner size={12} />
						<span>Loading destination services…</span>
					</div>
				{:else}
					<Select
						id="link-dest-svc"
						bind:value={destinationServiceId}
						options={[
							{ value: '', label: 'Entire Project (General Link)' },
							...remoteServices.map((s) => ({
								value: s.id,
								label: `${s.name} (${s.slug})`
							}))
						]}
					/>
				{/if}
			</FormField>

			{#if localServices.length > 0}
				<FormField label="Connect From Local Service (optional)" for="link-local-svc">
					<Select
						id="link-local-svc"
						bind:value={localServiceId}
						options={[
							{ value: '', label: 'None (Place standalone on canvas)' },
							...localServices.map((s) => ({
								value: s.id,
								label: s.name
							}))
						]}
					/>
				</FormField>
			{/if}
		{/if}

		{#if submitError}
			<div role="alert"><InlineAlert tone="error">{submitError}</InlineAlert></div>
		{/if}

		<div class="submit-row">
			<Button
				variant="primary"
				type="submit"
				disabled={isSubmitting || availableProjects.length === 0}
			>
				{#if isSubmitting}
					<Spinner size={12} tone="current" /> Linking Project…
				{:else}
					Link Project
				{/if}
			</Button>
		</div>
	</form>
</div>

<style>
	.panel-wrap {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.panel-hero {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		border-radius: var(--radius-md);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
	}

	.panel-badge {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		font-size: 10.5px;
		font-weight: 600;
		color: var(--text-secondary);
	}

	.panel-desc {
		font-size: 12px;
		color: var(--text-muted);
		margin: 0;
		line-height: 1.4;
	}

	.form {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.loading-state {
		display: flex;
		align-items: center;
		gap: 6px;
		font-size: 11px;
		color: var(--text-dim);
		padding: 6px 0;
	}

	.submit-row {
		display: flex;
		justify-content: flex-end;
		margin-top: 6px;
	}
</style>
