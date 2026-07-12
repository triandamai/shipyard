import { writable } from 'svelte/store';
import type { TopologyNode, TopologyEdge, Topology } from '$lib/api/types';
import type { Node as FlowNode, Edge as FlowEdge } from '@xyflow/svelte';

interface TopologyState {
	nodes: TopologyNode[];
	edges: TopologyEdge[];
	/** SvelteFlow-formatted nodes ready to pass to <SvelteFlow>. */
	flowNodes: FlowNode[];
	/** SvelteFlow-formatted edges ready to pass to <SvelteFlow>. */
	flowEdges: FlowEdge[];
	isLoading: boolean;
}

const initialState: TopologyState = {
	nodes: [],
	edges: [],
	flowNodes: [],
	flowEdges: [],
	isLoading: false
};

function getStoredPosition(nodeId: string): { x: number; y: number } | null {
	if (typeof window === 'undefined') return null;
	try {
		const value = localStorage.getItem(nodeId);
		if (!value) return null;
		const parts = value.split(';');
		if (parts.length === 2) {
			const x = parseFloat(parts[0]);
			const y = parseFloat(parts[1]);
			if (!isNaN(x) && !isNaN(y)) {
				return { x, y };
			}
		}
	} catch (e) {
		console.error('Failed to read from localStorage:', e);
	}
	return null;
}

function setStoredPosition(nodeId: string, x: number, y: number) {
	if (typeof window === 'undefined') return;
	try {
		localStorage.setItem(nodeId, `${x};${y}`);
	} catch (e) {
		console.error('Failed to write to localStorage:', e);
	}
}

/** Convert a TopologyNode to a SvelteFlow Node, optionally merging a saved position. */
function toFlowNode(node: TopologyNode, position?: { x: number; y: number }): FlowNode {
	const stored = getStoredPosition(node.id);
	return {
		id: node.id,
		type: node.type,
		position: stored ?? position ?? { x: 0, y: 0 },
		data: node.data
	};
}

/** Convert a TopologyEdge to a SvelteFlow Edge. */
function toFlowEdge(edge: TopologyEdge): FlowEdge {
	const isEnvRef = edge.type === 'env_ref';
	return {
		id: edge.id,
		source: edge.source,
		target: edge.target,
		label: isEnvRef ? 'env ref' : edge.type,
		type: 'default',
		...(isEnvRef && {
			style: 'stroke: #7c3aed; stroke-dasharray: 6,3;',
			labelStyle: 'fill: #7c3aed; font-size: 9px; font-weight: 600;',
			labelBgStyle: 'fill: transparent;',
		}),
	};
}

// Column X positions — left to right:
//   domain → root_service → child_service → container/replica → network → volume → portal
const DOMAIN_X    = -300;
const ROOT_SVC_X  = 0;
const CHILD_SVC_X = 270;
const CONTAINER_X = 540;
const NETWORK_X   = 810;
const VOLUME_X    = 1080;
const PORTAL_X    = 1350;
const Y_STEP      = 150;   // vertical distance between sibling nodes
const Y_START     = 60;    // canvas top margin
const SVC_GAP     = 50;    // extra vertical padding between root-service slots

/**
 * Compute auto-layout positions for nodes that don't have a saved position.
 *
 * Column order (left → right):
 *   domain | root_service | child_service (compose) | container/replica | network | volume | portal
 *
 * Algorithm:
 *   Pass 0 — index all nodes into lookup maps
 *   Pass 1 — place root services (stacking vertically, slot-sized by their subtree)
 *   Pass 2 — place child services (compose children) grouped under their root
 *   Pass 3 — place containers relative to their direct parent (child or root service)
 *   Pass 4 — place domains relative to their root service
 *   Pass 5 — stack network / volume / portal nodes in their global columns
 */
function autoLayoutPositions(
	nodes: TopologyNode[],
	positions: Record<string, { x: number; y: number }>
): Record<string, { x: number; y: number }> {
	const result: Record<string, { x: number; y: number }> = { ...positions };
	for (const n of nodes) {
		const stored = getStoredPosition(n.id);
		if (stored) {
			result[n.id] = stored;
		}
	}

	// --- Pass 0: build lookup maps ---
	const childSvcsByParent: Record<string, string[]> = {}; // parent node-id → [child svc node-ids]
	const containersBySvc:   Record<string, string[]> = {}; // svc node-id   → [container node-ids]
	const domainsBySvc:      Record<string, string[]> = {}; // svc node-id   → [domain node-ids]
	const rootSvcIds:        string[] = [];
	const childSvcIdSet = new Set<string>();
	const networkIds:    string[] = [];
	const volumeIds:     string[] = [];
	const portalIds:     string[] = [];

	for (const n of nodes) {
		const d = n.data as Record<string, unknown>;
		if (n.type === 'service' || n.type === 'static_site' || n.type === 'edge_function') {
			const parentId = d?.service_parent_id as string | undefined;
			if (parentId) {
				// parentId is already in "svc_{uuid}" format (matches node id)
				(childSvcsByParent[parentId] ??= []).push(n.id);
				childSvcIdSet.add(n.id);
			} else {
				rootSvcIds.push(n.id);
			}
		} else if (n.type === 'container') {
			const svcId = (d?.service_id as string) ?? '';
			(containersBySvc[svcId] ??= []).push(n.id);
		} else if (n.type === 'domain') {
			const svcId = (d?.service_id as string) ?? '';
			(domainsBySvc[svcId] ??= []).push(n.id);
		} else if (n.type === 'network') {
			networkIds.push(n.id);
		} else if (n.type === 'volume') {
			volumeIds.push(n.id);
		} else if (n.type === 'portal') {
			portalIds.push(n.id);
		}
	}

	// Sub-slot height for a single child service (sized by its container count).
	function childSubSlot(childId: string): number {
		return Math.max(1, (containersBySvc[childId] ?? []).length) * Y_STEP;
	}

	// Total slot height for a root service.
	function rootSlotHeight(rootId: string): number {
		const children = childSvcsByParent[rootId] ?? [];
		if (children.length > 0) {
			// Sum of all child sub-slots
			return children.reduce((sum, cid) => sum + childSubSlot(cid), 0) + SVC_GAP;
		}
		// No compose children — size by own containers or domains
		const nc = (containersBySvc[rootId] ?? []).length;
		const nd = (domainsBySvc[rootId]    ?? []).length;
		return Math.max(1, nc, nd) * Y_STEP + SVC_GAP;
	}

	// --- Pass 1: assign Y positions to root service nodes ---
	let nextRootY = Y_START;
	for (const id of rootSvcIds) {
		if (result[id]) {
			nextRootY = Math.max(nextRootY, result[id].y + rootSlotHeight(id));
		}
	}
	const rootY: Record<string, number> = {};
	for (const id of rootSvcIds) {
		if (!result[id]) {
			result[id] = { x: ROOT_SVC_X, y: nextRootY };
			nextRootY += rootSlotHeight(id);
		}
		rootY[id] = result[id].y;
	}

	// --- Pass 2: place child (compose) services under their root ---
	const childY: Record<string, number> = {};
	for (const rootId of rootSvcIds) {
		const children = childSvcsByParent[rootId] ?? [];
		let offsetY = rootY[rootId] ?? Y_START;
		for (const childId of children) {
			if (!result[childId]) {
				result[childId] = { x: CHILD_SVC_X, y: offsetY };
			}
			childY[childId] = result[childId].y;
			offsetY += childSubSlot(childId);
		}
	}

	// --- Pass 3: place containers next to their direct parent service ---
	// Containers of compose child services
	for (const childId of childSvcIdSet) {
		const ctrs = containersBySvc[childId] ?? [];
		const baseY = childY[childId] ?? Y_START;
		for (let i = 0; i < ctrs.length; i++) {
			if (!result[ctrs[i]]) {
				result[ctrs[i]] = { x: CONTAINER_X, y: baseY + i * Y_STEP };
			}
		}
	}
	// Containers of root services that have no compose children
	for (const rootId of rootSvcIds) {
		if ((childSvcsByParent[rootId] ?? []).length > 0) continue;
		const ctrs = containersBySvc[rootId] ?? [];
		const baseY = rootY[rootId] ?? Y_START;
		for (let i = 0; i < ctrs.length; i++) {
			if (!result[ctrs[i]]) {
				result[ctrs[i]] = { x: CONTAINER_X, y: baseY + i * Y_STEP };
			}
		}
	}

	// --- Pass 4: place domains to the left of their root service ---
	for (const rootId of rootSvcIds) {
		const doms = domainsBySvc[rootId] ?? [];
		const baseY = rootY[rootId] ?? Y_START;
		for (let i = 0; i < doms.length; i++) {
			if (!result[doms[i]]) {
				result[doms[i]] = { x: DOMAIN_X, y: baseY + i * Y_STEP };
			}
		}
	}

	// --- Pass 5: stack network, volume, and portal columns ---
	const placedNetMaxY = networkIds
		.filter((id) => result[id])
		.reduce((max, id) => Math.max(max, result[id].y), -Infinity);
	let nextNetY = placedNetMaxY === -Infinity ? Y_START : placedNetMaxY + Y_STEP;
	for (const id of networkIds) {
		if (result[id]) continue;
		result[id] = { x: NETWORK_X, y: nextNetY };
		nextNetY += Y_STEP;
	}

	const placedVolMaxY = volumeIds
		.filter((id) => result[id])
		.reduce((max, id) => Math.max(max, result[id].y), -Infinity);
	let nextVolY = placedVolMaxY === -Infinity ? Y_START : placedVolMaxY + Y_STEP;
	for (const id of volumeIds) {
		if (result[id]) continue;
		result[id] = { x: VOLUME_X, y: nextVolY };
		nextVolY += Y_STEP;
	}

	const placedPortalMaxY = portalIds
		.filter((id) => result[id])
		.reduce((max, id) => Math.max(max, result[id].y), -Infinity);
	let nextPortalY = placedPortalMaxY === -Infinity ? Y_START : placedPortalMaxY + Y_STEP;
	for (const id of portalIds) {
		if (result[id]) continue;
		result[id] = { x: PORTAL_X, y: nextPortalY };
		nextPortalY += Y_STEP;
	}

	return result;
}

function buildFlowNodes(
	nodes: TopologyNode[],
	positions: Record<string, { x: number; y: number }> = {}
): FlowNode[] {
	const resolved = autoLayoutPositions(nodes, positions);
	return nodes.map((n) => toFlowNode(n, resolved[n.id]));
}

function buildFlowEdges(edges: TopologyEdge[]): FlowEdge[] {
	return edges.map(toFlowEdge);
}

function createTopologyStore() {
	const { subscribe, set, update } = writable<TopologyState>(initialState);

	return {
		subscribe,

		setTopology(
			nodes: TopologyNode[],
			edges: TopologyEdge[],
			positions: Record<string, { x: number; y: number }> = {}
		) {
			for (const [id, p] of Object.entries(positions)) {
				setStoredPosition(id, p.x, p.y);
			}
			update((state) => ({
				...state,
				nodes,
				edges,
				flowNodes: buildFlowNodes(nodes, positions),
				flowEdges: buildFlowEdges(edges)
			}));
		},

		/**
		 * Load topology from the API response and convert to SvelteFlow format.
		 * Pass node_positions from the project to restore user-arranged layout.
		 * Use only on initial load — for MQTT-triggered updates use mergeTopology().
		 */
		loadForProject(
			topology: Topology,
			positions: Record<string, { x: number; y: number }> | null = null
		) {
			const pos = positions ?? {};
			for (const [id, p] of Object.entries(pos)) {
				setStoredPosition(id, p.x, p.y);
			}
			update((state) => ({
				...state,
				nodes: topology.nodes,
				edges: topology.edges,
				flowNodes: buildFlowNodes(topology.nodes, pos),
				flowEdges: buildFlowEdges(topology.edges),
			}));
		},

		/**
		 * Merge a fresh topology response into the canvas without moving any nodes.
		 *
		 * Existing nodes keep their current canvas positions.
		 * New nodes (e.g. a freshly spawned container) are auto-laid out.
		 * Nodes that no longer appear in the response are removed.
		 * Edges are always replaced (they carry no position state).
		 *
		 * Pass `canvasPositions` (harvested from the bound `nodes` variable in the
		 * page) to use the most up-to-date positions, which may differ from the
		 * store when the user has dragged nodes without saving.
		 */
		mergeTopology(topology: Topology, canvasPositions?: Record<string, { x: number; y: number }>) {
			if (canvasPositions) {
				for (const [id, p] of Object.entries(canvasPositions)) {
					setStoredPosition(id, p.x, p.y);
				}
			}
			update((state) => {
				// Start from store positions then override with live canvas positions
				// (the canvas binding is more up-to-date after user drags).
				const livePositions: Record<string, { x: number; y: number }> = {};
				for (const fn of state.flowNodes) {
					livePositions[fn.id] = fn.position;
				}
				if (canvasPositions) {
					Object.assign(livePositions, canvasPositions);
				}

				return {
					...state,
					nodes: topology.nodes,
					edges: topology.edges,
					flowNodes: buildFlowNodes(topology.nodes, livePositions),
					flowEdges: buildFlowEdges(topology.edges),
				};
			});
		},

		updateNodePosition(nodeId: string, x: number, y: number) {
			setStoredPosition(nodeId, x, y);
			update((state) => {
				const flowNodes = state.flowNodes.map((fn) =>
					fn.id === nodeId ? { ...fn, position: { x, y } } : fn
				);
				return { ...state, flowNodes };
			});
		},

		/** Refresh a single node's data (e.g., status change from MQTT). */
		refreshNode(nodeId: string, data: Partial<TopologyNode>) {
			update((state) => {
				const nodes = state.nodes.map((n) =>
					n.id === nodeId ? { ...n, data: { ...n.data, ...(data.data ?? {}) } } : n
				);
				const flowNodes = state.flowNodes.map((fn) =>
					fn.id === nodeId
						? { ...fn, data: { ...fn.data, ...(data.data ?? {}) } }
						: fn
				);
				return { ...state, nodes, flowNodes };
			});
		},

		/** Add a node to the topology. */
		addNode(node: TopologyNode, position?: { x: number; y: number }) {
			update((state) => ({
				...state,
				nodes: [...state.nodes, node],
				flowNodes: [...state.flowNodes, toFlowNode(node, position)]
			}));
		},

		/** Remove a node and its edges. */
		removeNode(nodeId: string) {
			update((state) => ({
				...state,
				nodes: state.nodes.filter((n) => n.id !== nodeId),
				edges: state.edges.filter((e) => e.source !== nodeId && e.target !== nodeId),
				flowNodes: state.flowNodes.filter((fn) => fn.id !== nodeId),
				flowEdges: state.flowEdges.filter(
					(fe) => fe.source !== nodeId && fe.target !== nodeId
				)
			}));
		},

		/** Add an edge. */
		addEdge(edge: TopologyEdge) {
			update((state) => ({
				...state,
				edges: [...state.edges, edge],
				flowEdges: [...state.flowEdges, toFlowEdge(edge)]
			}));
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		}
	};
}

export const topologyStore = createTopologyStore();
