<script lang="ts">
	import { SvelteFlow, Controls, Background, MiniMap, type NodeTypes, type Node } from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';
	import { Plus } from '@lucide/svelte';

	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { topologyStore } from '$lib/stores/topology.store';
	import { projectStore } from '$lib/stores/project.store';
	import { orgStore } from '$lib/stores/org.store';
	import { uiStore } from '$lib/stores/ui.store';
	import { subscribeToTopology } from '$lib/mqtt/subscriptions';
	import { eventBus } from '$lib/mqtt/eventBus';
	import type { Topology, MqttPayload } from '$lib/api/types';

	import ServiceNode from '$lib/flows/ServiceNode.svelte';
	import NetworkNode from '$lib/flows/NetworkNode.svelte';
	import VolumeNode from '$lib/flows/VolumeNode.svelte';
	import DomainNode from '$lib/flows/DomainNode.svelte';
	import ContainerNode from '$lib/flows/ContainerNode.svelte';
	import ServiceDetailPanel from '$lib/panels/ServiceDetailPanel.svelte';
	import NetworkDetailPanel from '$lib/panels/NetworkDetailPanel.svelte';
	import VolumeDetailPanel from '$lib/panels/VolumeDetailPanel.svelte';
	import ContainerDetailPanel from '$lib/panels/ContainerDetailPanel.svelte';
	import DomainDetailPanel from '$lib/panels/DomainDetailPanel.svelte';
	import AddResourcePanel from '$lib/panels/AddResourcePanel.svelte';

	let orgSlug = $derived(page.params.orgSlug ?? '');
	let projectSlug = $derived(page.params.projectSlug ?? '');

	// Resolve UUIDs from slugs for API calls and MQTT topics
	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let projectId = $derived(
		$projectStore.projects.find((p) => p.slug === projectSlug)?.id ?? ''
	);

	let isLoading = $state(true);
	let error = $state<string | null>(null);

	// SvelteFlow requires $state.raw for both arrays — deep reactivity causes
	// internal mutation warnings and performance issues inside the flow library.
	let nodes = $state.raw<Node[]>([]);
	let edges = $state.raw<import('@xyflow/svelte').Edge[]>([]);

	// Keep local arrays in sync with topology store
	$effect(() => {
		nodes = $topologyStore.flowNodes;
	});
	$effect(() => {
		edges = $topologyStore.flowEdges;
	});

	const nodeTypes: NodeTypes = {
		service:   ServiceNode as any,
		network:   NetworkNode as any,
		volume:    VolumeNode as any,
		domain:    DomainNode as any,
		container: ContainerNode as any,
	};

	function handleNodeClick({ node }: { node: Node; event: MouseEvent | TouchEvent }) {
		if (node.type === 'service') {
			const serviceId = node.id.replace(/^svc_/, '');
			uiStore.pushPanel({
				component: ServiceDetailPanel,
				props: {
					serviceId,
					projectId,
					orgId,
					onDeleted: () => {
						uiStore.popPanel();
						syncTopology(orgId, projectId);
					}
				},
				title: (node.data?.name as string) || 'Service'
			});
		} else if (node.type === 'domain') {
			const domainId = node.id.replace(/^dom_/, '');
			const svcId    = ((node.data?.service_id as string) || '').replace(/^svc_/, '');
			uiStore.pushPanel({
				component: DomainDetailPanel,
				props: {
					domainId,
					serviceId: svcId,
					onDeleted: () => {
						uiStore.popPanel();
						syncTopology(orgId, projectId);
					},
				},
				title: (node.data?.hostname as string) || 'Domain'
			});
		} else if (node.type === 'network') {
			const networkId = node.id.replace(/^net_/, '');
			uiStore.pushPanel({
				component: NetworkDetailPanel,
				props: {
					networkId,
					projectId,
					onDeleted: () => syncTopology(orgId, projectId),
				},
				title: (node.data?.name as string) || 'Network'
			});
		} else if (node.type === 'volume') {
			const volumeId = node.id.replace(/^vol_/, '');
			uiStore.pushPanel({
				component: VolumeDetailPanel,
				props: {
					volumeId,
					projectId,
					onDeleted: () => syncTopology(orgId, projectId),
				},
				title: (node.data?.name as string) || 'Volume'
			});
		} else if (node.type === 'container') {
			const containerId = node.id.replace(/^ctr_/, '');
			// service_id in node.data uses the canvas node-ID format ("svc_{uuid}")
			// — strip the prefix to get the raw API service UUID.
			const svcId = ((node.data?.service_id as string) || '').replace(/^svc_/, '');
			uiStore.pushPanel({
				component: ContainerDetailPanel,
				props: { containerId, serviceId: svcId },
				title: `Replica #${node.data?.replica_index ?? ''}`
			});
		}
	}

	// ── Node position persistence ──────────────────────────────────────────────
	// Collect every node's current canvas position and debounce-save to the DB.
	// 800 ms after the last drag-stop, one PATCH request is sent.
	let _saveTimer: ReturnType<typeof setTimeout> | null = null;

	function handleNodeDragStop(_: { targetNode: Node | null; nodes: Node[]; event: MouseEvent | TouchEvent }) {
		if (!orgId || !projectId) return;
		if (_saveTimer) clearTimeout(_saveTimer);
		_saveTimer = setTimeout(async () => {
			const positions: Record<string, { x: number; y: number }> = {};
			for (const n of nodes) positions[n.id] = n.position;
			const res = await api.patchNodePositions(orgId, projectId, positions);
			if (!res.error) {
				projectStore.updateNodePositions(projectId, positions);
			}
		}, 800);
	}

	let unsubscribeTopology: (() => void) | null = null;

	function handleTopologyMqtt(payload: MqttPayload) {
		if (payload.event === 'service.deleted') {
			// Optimistically remove the deleted service node and its container replicas
			// immediately, then sync in the background to catch any dangling edges/nodes.
			const serviceId = payload.meta?.service_id as string | undefined;
			if (serviceId) {
				topologyStore.removeNode(`svc_${serviceId}`);
			}
			syncTopology(orgId, projectId);
			return;
		}
		if (payload.event === 'topology.changed') {
			// Silent background sync — no loading state, no node movement.
			syncTopology(orgId, projectId);
			return;
		}
	}

	/** Initial full load — shows the loading spinner, replaces all nodes. */
	async function loadTopology(oid: string, pid: string) {
		isLoading = true;
		error = null;
		const res = await api.get<Topology>(`/projects/${pid}/topology`);
		if (res.error) {
			error = res.error.message;
		} else if (res.data) {
			const proj = $projectStore.projects.find((p) => p.id === pid) ?? $projectStore.activeProject;
			topologyStore.loadForProject(res.data, proj?.node_positions ?? null);
		}
		isLoading = false;
	}

	/**
	 * Silent background sync triggered by MQTT events or resource creation.
	 * Merges fresh data into the canvas without hiding it or moving any nodes.
	 * Passes the current canvas positions so user-dragged positions are preserved
	 * even when they haven't been saved to the DB yet.
	 */
	async function syncTopology(oid: string, pid: string) {
		const res = await api.get<Topology>(`/projects/${pid}/topology`);
		if (res.data) {
			const canvasPositions: Record<string, { x: number; y: number }> = {};
			for (const n of nodes) canvasPositions[n.id] = n.position;
			topologyStore.mergeTopology(res.data, canvasPositions);
		}
	}

	// Set active project whenever the slug resolves in the store
	$effect(() => {
		const project = $projectStore.projects.find((p) => p.slug === projectSlug);
		if (project && project.id !== $projectStore.activeProject?.id) {
			projectStore.setActiveProject(project);
		}
	});

	// Load topology and wire up MQTT as soon as org + project UUIDs are both available.
	// This fires again whenever orgId or projectId changes (e.g. after layout finishes
	// loading on a hard refresh), so there's no race condition.
	$effect(() => {
		const oid = orgId;
		const pid = projectId;
		if (!oid || !pid) return;

		loadTopology(oid, pid);

		const sub = subscribeToTopology(oid, pid);
		const topoTopic = `platform/orgs/${oid}/projects/${pid}/topology`;
		eventBus.on(topoTopic, handleTopologyMqtt);

		// Service status events arrive separately (after topology.changed) and carry
		// the authoritative status/replica count.  Update the canvas node directly so
		// the service card reflects the real state without waiting for a full re-fetch.
		const svcStatusPrefix = `platform/orgs/${oid}/projects/${pid}/services/`;
		const handleServiceStatus = (topic: string, payload: MqttPayload) => {
			if (!topic.startsWith(svcStatusPrefix) || !topic.endsWith('/status')) return;
			const serviceId = topic.slice(svcStatusPrefix.length, -'/status'.length);
			const meta = payload.meta as Record<string, unknown> | undefined;
			if (meta?.status) {
				topologyStore.refreshNode(`svc_${serviceId}`, {
					data: { status: meta.status as string, replicas: (meta.replicas as number) ?? 0 }
				});
			}
		};
		eventBus.on('*', handleServiceStatus);

		return () => {
			sub?.();
			eventBus.off(topoTopic, handleTopologyMqtt);
			eventBus.off('*', handleServiceStatus);
			topologyStore.setLoading(false);
		};
	});
</script>

<div class="canvas-wrapper">
	{#if isLoading}
		<div class="canvas-loading">
			<div class="spinner"></div>
			<span>Loading topology…</span>
		</div>
	{:else if error}
		<div class="canvas-error">
			<span>Failed to load topology: {error}</span>
			<button
				class="btn btn-secondary btn-sm"
				onclick={() => loadTopology(orgId, projectId)}
			>
				Retry
			</button>
		</div>
	{:else}
		<SvelteFlow
			bind:nodes
			bind:edges
			{nodeTypes}
			fitView
			onnodeclick={handleNodeClick}
			onnodedragstop={handleNodeDragStop}
		>
			<Controls />
			<Background />
			<MiniMap
				style="background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-md);"
			/>
		</SvelteFlow>

		<!-- Floating add resource button -->
		<button
			class="add-resource-btn btn btn-primary"
			aria-label="Add resource"
			onclick={() => uiStore.pushPanel({
				component: AddResourcePanel,
				props: { projectId, orgId, onCreated: () => syncTopology(orgId, projectId) },
				title: 'Add Resource'
			})}
		>
			<Plus size={16} />
			Add Resource
		</button>
	{/if}
</div>

<style>
	.canvas-wrapper {
		width: 100%;
		height: 100vh;
		position: relative;
		background: var(--bg-base);
	}

	/* Override @xyflow background to match design tokens */
	.canvas-wrapper :global(.svelte-flow) {
		background: var(--bg-base);
	}

	.canvas-wrapper :global(.svelte-flow__minimap) {
		border-radius: var(--radius-md);
	}

	.canvas-wrapper :global(.svelte-flow__controls) {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}

	.canvas-wrapper :global(.svelte-flow__controls-button) {
		background: transparent;
		border-bottom: 1px solid var(--border);
		color: var(--text-muted);
	}

	.canvas-wrapper :global(.svelte-flow__controls-button:hover) {
		background: var(--bg-elevated);
		color: var(--text-primary);
	}

	.canvas-wrapper :global(.svelte-flow__controls-button:last-child) {
		border-bottom: none;
	}

	.add-resource-btn {
		position: absolute;
		bottom: 24px;
		right: 24px;
		z-index: 10;
		box-shadow: var(--shadow-md);
	}

	.canvas-loading,
	.canvas-error {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		height: 100%;
		color: var(--text-muted);
		font-size: 13px;
	}

	.spinner {
		width: 32px;
		height: 32px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
