<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { projectStore } from '$lib/stores/project.store';
	import { topologyStore } from '$lib/stores/topology.store';
	import { toastStore } from '$lib/stores/toast.store';
	import { api } from '$lib/api/client';
	import type { Service, Project, TopologyNode } from '$lib/api/types';
	import { FormField, Select, Button, InlineAlert, Spinner } from '$lib/components/ui';
	import { Compass, Sparkles, ArrowRight, ExternalLink } from '@lucide/svelte';

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
		// Load local services for current project
		const locRes = await api.getServices(projectId);
		if (locRes.data) {
			localServices = locRes.data;
		}

		// Pre-select first destination project if available
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

	async function handleCreatePortal(e: SubmitEvent) {
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

			const portalId = `portal_${destinationProjectId}_${destService ? destService.id : 'proj'}_${Date.now()}`;
			const portalNode: TopologyNode = {
				id: portalId,
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

			topologyStore.addNode(portalNode);

			if (localServiceId) {
				topologyStore.addEdge({
					id: `e_portal_${localServiceId}_${portalId}`,
					source: `svc_${localServiceId}`,
					target: portalId,
					type: 'env_ref'
				});
			}

			toastStore.add({
				type: 'success',
				title: 'Portal Created',
				message: `Teleport portal to ${destProject?.name ?? 'destination'} opened!`
			});

			onCreated?.();
			uiStore.clearPanels();
		} catch (err: any) {
			submitError = err?.message || 'Failed to create portal';
		} finally {
			isSubmitting = false;
		}
	}
</script>

<div class="panel-wrap">
	<div class="portal-hero">
		<div class="portal-badge">
			<Compass size={14} class="portal-spin" />
			<span>Cross-Project Teleportation</span>
		</div>
		<p class="panel-desc">
			Create a teleportation portal to connect services across projects.
			Clicking the portal on canvas teleports you directly to the destination project.
		</p>
	</div>

	<form class="form" onsubmit={handleCreatePortal}>
		{#if availableProjects.length === 0}
			<InlineAlert tone="info">
				No other projects found in this organization. Create another project first to establish a cross-project portal.
			</InlineAlert>
		{:else}
			<FormField label="Destination Project" for="portal-dest-proj">
				<Select
					id="portal-dest-proj"
					bind:value={destinationProjectId}
					options={availableProjects.map((p: Project) => ({
						value: p.id,
						label: `${p.name} (${p.slug})`
					}))}
				/>
			</FormField>

			<FormField label="Target Service (optional)" for="portal-dest-svc">
				{#if isLoadingRemote}
					<div class="loading-state">
						<Spinner size={12} />
						<span>Loading destination services…</span>
					</div>
				{:else}
					<Select
						id="portal-dest-svc"
						bind:value={destinationServiceId}
						options={[
							{ value: '', label: 'Entire Project (General Teleport)' },
							...remoteServices.map((s) => ({
								value: s.id,
								label: `${s.name} (${s.slug})`
							}))
						]}
					/>
				{/if}
			</FormField>

			{#if localServices.length > 0}
				<FormField label="Connect From Local Service (optional)" for="portal-local-svc">
					<Select
						id="portal-local-svc"
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
					<Spinner size={12} tone="current" /> Opening Portal…
				{:else}
					<Sparkles size={13} />
					Open Portal
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

	.portal-hero {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 12px;
		border-radius: var(--radius-md);
		background: radial-gradient(circle at 10% 20%, color-mix(in srgb, var(--accent-purple) 18%, transparent), var(--bg-elevated));
		border: 1px solid color-mix(in srgb, var(--accent-purple) 30%, var(--border));
	}

	.portal-badge {
		display: inline-flex;
		align-items: center;
		gap: 5px;
		font-size: 10px;
		font-weight: 700;
		color: var(--accent-purple);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	:global(.portal-spin) {
		animation: portal-spin 8s linear infinite;
	}

	@keyframes portal-spin {
		from { transform: rotate(0deg); }
		to { transform: rotate(360deg); }
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
