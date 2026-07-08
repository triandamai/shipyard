<script lang="ts">
	import { SvelteFlow, Controls, Background, MiniMap, type NodeTypes, type Node } from '@xyflow/svelte';
	import '@xyflow/svelte/dist/style.css';
	import { Plus, ChevronDown, RefreshCw, Settings2, ShieldOff } from '@lucide/svelte';
	import { goto } from '$app/navigation';

	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { topologyStore } from '$lib/stores/topology.store';
	import { projectStore } from '$lib/stores/project.store';
	import { orgStore } from '$lib/stores/org.store';
	import { uiStore } from '$lib/stores/ui.store';
	import { subscribeToTopology } from '$lib/mqtt/subscriptions';
	import { eventBus } from '$lib/mqtt/eventBus';
	import type { Topology, MqttPayload } from '$lib/api/types';
	import { isAdminRole, hasProjectAccess, hasProjectEditAccess } from '$lib/auth/permissions';

	import ServiceNode from '$lib/flows/ServiceNode.svelte';
	import NetworkNode from '$lib/flows/NetworkNode.svelte';
	import VolumeNode from '$lib/flows/VolumeNode.svelte';
	import DomainNode from '$lib/flows/DomainNode.svelte';
	import ContainerNode from '$lib/flows/ContainerNode.svelte';
	import StaticSiteNode from '$lib/flows/StaticSiteNode.svelte';
	import PortalNode from '$lib/flows/PortalNode.svelte';
	import ServiceDetailPanel from '$lib/panels/ServiceDetailPanel.svelte';
	import NetworkDetailPanel from '$lib/panels/NetworkDetailPanel.svelte';
	import VolumeDetailPanel from '$lib/panels/VolumeDetailPanel.svelte';
	import ContainerDetailPanel from '$lib/panels/ContainerDetailPanel.svelte';
	import DomainDetailPanel from '$lib/panels/DomainDetailPanel.svelte';
	import StaticSiteDetailPanel from '$lib/panels/StaticSiteDetailPanel.svelte';
	import AddResourcePanel from '$lib/panels/AddResourcePanel.svelte';

	let orgSlug = $derived(page.params.orgSlug ?? '');
	let projectSlug = $derived(page.params.projectSlug ?? '');

	// Resolve UUIDs from slugs for API calls and MQTT topics
	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let projectId = $derived(
		$projectStore.projects.find((p) => p.slug === projectSlug)?.id ?? ''
	);

	// ── Permission gates ───────────────────────────────────────────────────────
	let myRole       = $derived($orgStore.myMembership?.role ?? null);
	let myPerms      = $derived($orgStore.myMembership?.permissions ?? []);
	let memberLoaded = $derived($orgStore.membershipLoaded ?? false);

	// Admins/owners always have full access.
	// Regular members need either an org-level projects:read permission or a
	// project-scoped shipyard:<orgId>:<projectId>:* permission.
	let canViewProject = $derived(hasProjectAccess(myRole, myPerms, orgId, projectId));
	let canEditProject = $derived(hasProjectEditAccess(myRole, myPerms, orgId, projectId));

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
		service:     ServiceNode as any,
		network:     NetworkNode as any,
		volume:      VolumeNode as any,
		domain:      DomainNode as any,
		container:   ContainerNode as any,
		static_site: StaticSiteNode as any,
		portal:      PortalNode as any,
	};

	function handleNodeClick({ node }: { node: Node; event: MouseEvent | TouchEvent }) {
		if (node.type === 'service') {
			const serviceId = node.id.replace(/^svc_/, '');
			uiStore.pushPanel({
				key: `service:${serviceId}`,
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
				key: `domain:${domainId}`,
				component: DomainDetailPanel,
				props: {
					domainId,
					serviceId: svcId,
					projectId,
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
				key: `network:${networkId}`,
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
				key: `volume:${volumeId}`,
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
				key: `container:${containerId}`,
				component: ContainerDetailPanel,
				props: { containerId, serviceId: svcId },
				title: `Replica #${node.data?.replica_index ?? ''}`
			});
		} else if (node.type === 'static_site') {
			const serviceId = node.id.replace(/^svc_/, '');
			uiStore.pushPanel({
				key: `static_site:${serviceId}`,
				component: StaticSiteDetailPanel,
				props: {
					serviceId,
					projectId,
					orgId,
					onDeployed: () => syncTopology(orgId, projectId),
					onDeleted:  () => syncTopology(orgId, projectId),
				},
				title: (node.data?.name as string) || 'Static Site'
			});
		} else if (node.type === 'portal') {
			// Portal nodes are read-only info — no panel needed
		}
	}

	// ── Node position persistence ──────────────────────────────────────────────
	// Collect every node's current canvas position and debounce-save to the DB.
	// 800 ms after the last drag-stop, one PATCH request is sent.
	let _saveTimer: ReturnType<typeof setTimeout> | null = null;

	function handleNodeDragStop(_: { targetNode: Node | null; nodes: Node[]; event: MouseEvent | TouchEvent }) {
		if (!orgId || !projectId || !canEditProject) return;
		if (_saveTimer) clearTimeout(_saveTimer);

		// Immediately update the store and LocalStorage for all dragged nodes
		for (const n of _.nodes) {
			topologyStore.updateNodePosition(n.id, n.position.x, n.position.y);
		}

		_saveTimer = setTimeout(async () => {
			const positions: Record<string, { x: number; y: number }> = {};
			for (const n of nodes) positions[n.id] = n.position;
			const res = await api.patchNodePositions(orgId, projectId, positions);
			if (!res.error) {
				projectStore.updateNodePositions(projectId, positions);
				for (const [id, pos] of Object.entries(positions)) {
					topologyStore.updateNodePosition(id, pos.x, pos.y);
				}
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

	// ── App bar state ──────────────────────────────────────────────────────────
	let projectMenuOpen = $state(false);
	let activeProjectName = $derived(
		$projectStore.projects.find((p) => p.slug === projectSlug)?.name ?? projectSlug
	);
	let allProjects = $derived($projectStore.projects);

	function closeProjectMenu() { projectMenuOpen = false; }

	function switchProject(slug: string) {
		closeProjectMenu();
		if (slug !== projectSlug) {
			import('$app/navigation').then(({ goto }) => goto(`/orgs/${orgSlug}/projects/${slug}`));
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

<!-- Canvas app bar -->
<div class="canvas-appbar">
	{#if projectMenuOpen}
		<div class="appbar-backdrop" onclick={closeProjectMenu} role="presentation"></div>
	{/if}

	<div class="project-switcher">
		<button
			class="project-btn"
			onclick={() => projectMenuOpen = !projectMenuOpen}
			aria-haspopup="true"
			aria-expanded={projectMenuOpen}
		>
			<span class="project-name">{activeProjectName}</span>
			<ChevronDown size={14} class={projectMenuOpen ? 'rotate-180' : ''} />
		</button>

		{#if projectMenuOpen}
			<div class="project-menu" role="menu">
				{#each allProjects as project}
					<button
						class="project-menu-item"
						class:active={project.slug === projectSlug}
						onclick={() => switchProject(project.slug)}
						role="menuitem"
					>
						{project.name}
					</button>
				{/each}
				{#if allProjects.length === 0}
					<div class="project-menu-empty">No projects</div>
				{/if}
			</div>
		{/if}
	</div>

	<button
		class="appbar-action"
		onclick={() => syncTopology(orgId, projectId)}
		title="Reload topology"
		aria-label="Reload topology"
	>
		<RefreshCw size={14} />
	</button>

	{#if canEditProject}
		<div class="appbar-divider"></div>

		<button
			class="appbar-add-btn"
			onclick={() => uiStore.pushPanel({
				component: AddResourcePanel,
				props: { projectId, orgId, onCreated: () => syncTopology(orgId, projectId) },
				title: 'Add Resource'
			})}
		>
			<Plus size={13} />
			Add Resource
		</button>

		<button
			class="appbar-action"
			onclick={() => goto(`/orgs/${orgSlug}/projects/${projectSlug}/settings`)}
			title="Project settings"
			aria-label="Project settings"
		>
			<Settings2 size={14} />
		</button>
	{/if}
</div>

<div class="canvas-wrapper">
	{#if memberLoaded && !canViewProject}
		<!-- Access denied — user has no project permissions -->
		<div class="access-denied">
			<ShieldOff size={36} class="access-denied-icon" />
			<h2 class="access-denied-title">Access Restricted</h2>
			<p class="access-denied-desc">
				You don't have permission to view this project.<br />
				Contact your organization admin to request access.
			</p>
			<button class="btn btn-secondary btn-sm" onclick={() => goto(`/orgs/${orgSlug}`)}>
				Back to overview
			</button>
		</div>
	{:else if isLoading}
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
			nodesDraggable={canEditProject}
			nodesConnectable={canEditProject}
			elementsSelectable={canEditProject}
			onnodeclick={handleNodeClick}
			onnodedragstop={handleNodeDragStop}
		>
			<Controls />
			<Background />
			<MiniMap
				style="background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-md);"
			/>
		</SvelteFlow>

		{#if !canEditProject && canViewProject}
			<div class="view-only-badge">View only</div>
		{/if}
	{/if}
</div>

<style>
	.canvas-wrapper {
		width: 100%;
		height: 100vh;
		position: relative;
		background: var(--bg-base);
	}

	/* ── Access denied ──────────────────────────────────────────────── */
	.access-denied {
		position: absolute;
		inset: 0;
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		background: var(--bg-base);
		color: var(--text-primary);
		text-align: center;
		padding: 32px;
	}
	:global(.access-denied-icon) { color: var(--text-dim); }
	.access-denied-title {
		font-size: 18px;
		font-weight: 600;
		margin: 0;
	}
	.access-denied-desc {
		font-size: 13px;
		color: var(--text-muted);
		line-height: 1.6;
		margin: 0;
	}

	/* ── View-only badge ────────────────────────────────────────────── */
	.view-only-badge {
		position: absolute;
		bottom: 16px;
		left: 50%;
		transform: translateX(-50%);
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 20px;
		padding: 4px 12px;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
		letter-spacing: 0.05em;
		text-transform: uppercase;
		pointer-events: none;
		z-index: 10;
	}

	/* Override @xyflow background and edge-label CSS vars with Shipyard design tokens.
	   xyflow only applies dark-mode defaults when its own .svelte-flow.dark class is set,
	   which Shipyard never adds — so we override the variables directly here. */
	.canvas-wrapper :global(.svelte-flow) {
		background: var(--bg-base);
		--xy-edge-label-background-color: var(--bg-surface);
		--xy-edge-label-color: var(--text-secondary);
	}

	/* Edge label pill — HTML div rendered via portal inside .svelte-flow */
	.canvas-wrapper :global(.svelte-flow__edge-label) {
		background: var(--bg-surface);
		color: var(--text-secondary);
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 1px 6px;
		font-size: 10px;
		font-family: var(--font-sans);
		font-weight: 500;
		white-space: nowrap;
		line-height: 1.6;
	}

	.canvas-wrapper :global(.svelte-flow__edge-path) {
		stroke: var(--border-hover);
	}
	.canvas-wrapper :global(.svelte-flow__edge.selected .svelte-flow__edge-path) {
		stroke: var(--accent);
	}

	/* Background dot/grid pattern respects theme */
	.canvas-wrapper :global(.svelte-flow__background pattern circle),
	.canvas-wrapper :global(.svelte-flow__background pattern rect) {
		fill: var(--border);
	}

	.canvas-wrapper :global(.svelte-flow__minimap) {
		border-radius: var(--radius-md);
	}

	@media (max-width: 640px) {
		.canvas-wrapper :global(.svelte-flow__minimap) {
			display: none;
		}
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

	.appbar-divider {
		width: 1px;
		height: 18px;
		background: var(--border);
		flex-shrink: 0;
	}

	.appbar-add-btn {
		display: flex;
		align-items: center;
		gap: 5px;
		padding: 5px 10px;
		background: var(--accent);
		color: #fff;
		border: none;
		border-radius: var(--radius-md);
		cursor: pointer;
		font-size: 12px;
		font-weight: 600;
		font-family: var(--font-sans);
		transition: background var(--transition-fast);
		white-space: nowrap;
	}

	.appbar-add-btn:hover { background: var(--accent-hover); }

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

	/* ── Canvas app bar ── */
	.canvas-appbar {
		position: fixed;
		top: 10px;
		left: 50%;
		transform: translateX(-50%);
		z-index: 20;
		display: flex;
		align-items: center;
		gap: 6px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 4px 6px 4px 4px;
		box-shadow: var(--shadow-md);
	}

	.appbar-backdrop {
		position: fixed;
		inset: 0;
		z-index: 100;
	}

	.project-switcher { position: relative; }

	.project-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 5px 10px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		cursor: pointer;
		font-size: 13px;
		font-weight: 600;
		color: var(--text-primary);
		transition: all var(--transition-fast);
	}

	.project-btn:hover {
		background: var(--bg-hover);
		border-color: var(--border-hover);
	}

	.project-name { max-width: 160px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	.project-menu {
		position: absolute;
		top: calc(100% + 6px);
		left: 0;
		min-width: 180px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-lg);
		z-index: 200;
		overflow: hidden;
		padding: 4px;
		display: flex;
		flex-direction: column;
		gap: 1px;
	}

	.project-menu-item {
		display: block;
		width: 100%;
		text-align: left;
		padding: 7px 10px;
		background: none;
		border: none;
		border-radius: var(--radius-sm);
		cursor: pointer;
		font-size: 13px;
		color: var(--text-secondary);
		transition: all var(--transition-fast);
	}

	.project-menu-item:hover { background: var(--bg-elevated); color: var(--text-primary); }
	.project-menu-item.active { color: var(--accent); font-weight: 600; background: var(--accent-muted); }

	.project-menu-empty {
		padding: 8px 10px;
		font-size: 12px;
		color: var(--text-muted);
	}

	.appbar-action {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		background: none;
		border: none;
		border-radius: var(--radius-sm);
		cursor: pointer;
		color: var(--text-muted);
		transition: all var(--transition-fast);
	}

	.appbar-action:hover { background: var(--bg-elevated); color: var(--text-primary); }

	:global(.rotate-180) { transform: rotate(180deg); transition: transform var(--transition-fast); }
</style>
