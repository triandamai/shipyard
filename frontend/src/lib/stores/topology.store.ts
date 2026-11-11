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

// ── Local Storage Cache ──────────────────────────────────────────────────────
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
		localStorage.setItem(nodeId, `${Math.round(x)};${Math.round(y)}`);
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
	const isConnection = edge.type === 'connection';
	const isDomain = edge.type === 'domain';
	const isVolume = edge.type === 'volume';
	return {
		id: edge.id,
		source: edge.source,
		target: edge.target,
		label: isEnvRef ? 'env ref' : edge.type === 'compose_child' ? 'stack' : edge.type,
		type: 'smoothstep',
		animated: isConnection || isEnvRef,
		...(isEnvRef && {
			style: 'stroke: var(--accent-purple); stroke-dasharray: 6,3;',
			labelStyle: 'fill: var(--accent-purple); font-size: 9px; font-weight: 600;',
			labelBgStyle: 'fill: transparent;',
		}),
		...(isDomain && {
			style: 'stroke: var(--accent-green); stroke-width: 1.5;',
			labelStyle: 'fill: var(--accent-green); font-size: 9px; font-weight: 600;',
			labelBgStyle: 'fill: transparent;',
		}),
		...(isVolume && {
			style: 'stroke: var(--accent-yellow); stroke-width: 1.5;',
			labelStyle: 'fill: var(--accent-yellow); font-size: 9px; font-weight: 600;',
			labelBgStyle: 'fill: transparent;',
		}),
		...(isConnection && {
			style: 'stroke: var(--accent); stroke-width: 2;',
			labelStyle: 'fill: var(--accent); font-size: 9px; font-weight: 600;',
			labelBgStyle: 'fill: transparent;',
		}),
	};
}

// ── Layout & Collision Constants ─────────────────────────────────────────────
export const NODE_WIDTH = 250;
export const NODE_HEIGHT = 140;
export const STEP_X = 300;       // horizontal distance between connected stages
export const STEP_Y = 170;       // vertical distance between sibling rows
export const START_X = 80;       // default canvas left margin
export const START_Y = 80;       // default canvas top margin
export const COLLISION_PAD_X = 40;
export const COLLISION_PAD_Y = 30;

/**
 * Check if a candidate bounding box collides with any occupied node position.
 */
function isSlotOccupied(
	x: number,
	y: number,
	occupied: Record<string, { x: number; y: number }>,
	excludeNodeId?: string
): boolean {
	const minX = x - (NODE_WIDTH + COLLISION_PAD_X);
	const maxX = x + (NODE_WIDTH + COLLISION_PAD_X);
	const minY = y - (NODE_HEIGHT + COLLISION_PAD_Y);
	const maxY = y + (NODE_HEIGHT + COLLISION_PAD_Y);

	for (const [id, pos] of Object.entries(occupied)) {
		if (excludeNodeId && id === excludeNodeId) continue;
		if (pos.x > minX && pos.x < maxX && pos.y > minY && pos.y < maxY) {
			return true;
		}
	}
	return false;
}

/**
 * Find the closest collision-free slot near an anchor position.
 * Tests candidates in a progressive radial/staggered pattern around the preferred offset.
 */
export function findNearbyFreeSlot(
	anchor: { x: number; y: number },
	occupied: Record<string, { x: number; y: number }>,
	preferredOffset: { dx: number; dy: number } = { dx: STEP_X, dy: 0 },
	excludeNodeId?: string
): { x: number; y: number } {
	const baseX = anchor.x + preferredOffset.dx;
	const baseY = anchor.y + preferredOffset.dy;

	// Candidate offsets relative to baseX, baseY
	const candidateOffsets: Array<{ dx: number; dy: number }> = [
		{ dx: 0, dy: 0 },
		{ dx: 0, dy: STEP_Y },
		{ dx: 0, dy: -STEP_Y },
		{ dx: 0, dy: 2 * STEP_Y },
		{ dx: 0, dy: -2 * STEP_Y },
		{ dx: STEP_X, dy: 0 },
		{ dx: STEP_X, dy: STEP_Y },
		{ dx: STEP_X, dy: -STEP_Y },
		{ dx: -STEP_X, dy: 0 },
		{ dx: 0, dy: 3 * STEP_Y },
		{ dx: STEP_X, dy: 2 * STEP_Y },
		{ dx: 2 * STEP_X, dy: 0 }
	];

	for (const off of candidateOffsets) {
		const candX = Math.max(START_X, baseX + off.dx);
		const candY = Math.max(START_Y, baseY + off.dy);
		if (!isSlotOccupied(candX, candY, occupied, excludeNodeId)) {
			return { x: candX, y: candY };
		}
	}

	// Fallback: spiral search outwards until a free slot is located
	for (let ring = 1; ring <= 10; ring++) {
		for (let row = -ring; row <= ring; row++) {
			for (let col = 0; col <= ring; col++) {
				const candX = Math.max(START_X, anchor.x + col * STEP_X);
				const candY = Math.max(START_Y, anchor.y + row * STEP_Y);
				if (!isSlotOccupied(candX, candY, occupied, excludeNodeId)) {
					return { x: candX, y: candY };
				}
			}
		}
	}

	return { x: baseX, y: baseY };
}

/**
 * Neighbor-aware auto-layout algorithm:
 * - Respects all existing positions from DB or canvas.
 * - When a new node is added:
 *   1. Identifies if the node is connected to an anchor node via edges or parent relation.
 *   2. Places the node nearby the anchor node using collision-free slot search.
 *   3. If unconnected, places it nearby the existing node cluster instead of a distant column.
 * - When no positions exist at all (clean canvas), lays out in a cohesive topology flow.
 */
function autoLayoutPositions(
	nodes: TopologyNode[],
	positions: Record<string, { x: number; y: number }>,
	edges: TopologyEdge[] = []
): Record<string, { x: number; y: number }> {
	const result: Record<string, { x: number; y: number }> = {};

	// 1. First recover explicitly stored/canvas positions
	for (const n of nodes) {
		const stored = getStoredPosition(n.id);
		if (stored) {
			result[n.id] = stored;
		} else if (positions[n.id]) {
			result[n.id] = positions[n.id];
		}
	}

	// Build adjacency maps for fast relationship lookup
	const connectedTo: Record<string, string[]> = {};
	for (const e of edges) {
		(connectedTo[e.source] ??= []).push(e.target);
		(connectedTo[e.target] ??= []).push(e.source);
	}

	// 2. Identify nodes that still need positioning
	const unpositioned = nodes.filter((n) => !result[n.id]);
	if (unpositioned.length === 0) {
		return result;
	}

	// Helper to find placed anchor for a given node
	function findAnchorForNode(n: TopologyNode): { anchorId: string; pos: { x: number; y: number } } | null {
		const d = n.data as Record<string, unknown> | undefined;
		const parentId = d?.service_parent_id as string | undefined;
		if (parentId && result[parentId]) {
			return { anchorId: parentId, pos: result[parentId] };
		}

		const serviceId = d?.service_id as string | undefined;
		if (serviceId && result[serviceId]) {
			return { anchorId: serviceId, pos: result[serviceId] };
		}

		// Check direct connections via edges
		const neighbors = connectedTo[n.id] ?? [];
		for (const neighborId of neighbors) {
			if (result[neighborId]) {
				return { anchorId: neighborId, pos: result[neighborId] };
			}
		}

		return null;
	}

	// 3. Place connected unpositioned nodes first
	const remaining: TopologyNode[] = [];
	for (const n of unpositioned) {
		const anchor = findAnchorForNode(n);
		if (anchor) {
			// Count how many nodes are already anchored to this node to stagger nicely
			const existingSiblings = Object.entries(result).filter(([id, p]) => {
				return Math.abs(p.x - (anchor.pos.x + STEP_X)) < 50;
			}).length;

			const preferredOffset = {
				dx: STEP_X,
				dy: existingSiblings * STEP_Y
			};
			const freeSlot = findNearbyFreeSlot(anchor.pos, result, preferredOffset, n.id);
			result[n.id] = freeSlot;
			setStoredPosition(n.id, freeSlot.x, freeSlot.y);
		} else {
			remaining.push(n);
		}
	}

	// 4. Place remaining unanchored nodes nearby the existing cluster
	for (const n of remaining) {
		const placedEntries = Object.entries(result);
		if (placedEntries.length > 0) {
			// Find reference node: prefer root services or the last placed node
			const rootServices = nodes.filter(
				(item) => result[item.id] && (item.type === 'service' || item.type === 'static_site' || item.type === 'sandbox_app')
			);
			const refNode = rootServices.length > 0 ? rootServices[rootServices.length - 1] : nodes.find((item) => result[item.id]);
			const anchorPos = refNode && result[refNode.id] ? result[refNode.id] : placedEntries[placedEntries.length - 1][1];

			const freeSlot = findNearbyFreeSlot(anchorPos, result, { dx: 0, dy: STEP_Y }, n.id);
			result[n.id] = freeSlot;
			setStoredPosition(n.id, freeSlot.x, freeSlot.y);
		} else {
			// Empty canvas starting position
			const initSlot = { x: START_X, y: START_Y };
			result[n.id] = initSlot;
			setStoredPosition(n.id, initSlot.x, initSlot.y);
		}
	}

	return result;
}

function buildFlowNodes(
	nodes: TopologyNode[],
	positions: Record<string, { x: number; y: number }> = {},
	edges: TopologyEdge[] = []
): FlowNode[] {
	const resolved = autoLayoutPositions(nodes, positions, edges);
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
				flowNodes: buildFlowNodes(nodes, positions, edges),
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
				flowNodes: buildFlowNodes(topology.nodes, pos, topology.edges),
				flowEdges: buildFlowEdges(topology.edges),
			}));
		},

		/**
		 * Merge a fresh topology response into the canvas.
		 *
		 * Existing nodes keep their current canvas positions.
		 * New nodes (e.g. freshly created service/network/volume) are auto-positioned
		 * nearby their connected or nearest nodes.
		 * Edges are updated with smooth transitions.
		 */
		mergeTopology(topology: Topology, canvasPositions?: Record<string, { x: number; y: number }>) {
			if (canvasPositions) {
				for (const [id, p] of Object.entries(canvasPositions)) {
					setStoredPosition(id, p.x, p.y);
				}
			}
			update((state) => {
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
					flowNodes: buildFlowNodes(topology.nodes, livePositions, topology.edges),
					flowEdges: buildFlowEdges(topology.edges),
				};
			});
		},

		/**
		 * Auto-position target node to be nearby source node when connected.
		 * Resolves collision against existing nodes and updates the store & storage.
		 * Returns the updated positions map so it can be persisted to backend.
		 */
		autoPositionConnected(
			sourceId: string,
			targetId: string
		): Record<string, { x: number; y: number }> | null {
			let updatedPositions: Record<string, { x: number; y: number }> | null = null;

			update((state) => {
				const sourceNode = state.flowNodes.find((n) => n.id === sourceId);
				const targetNode = state.flowNodes.find((n) => n.id === targetId);
				if (!sourceNode || !targetNode) return state;

				const occupied: Record<string, { x: number; y: number }> = {};
				for (const fn of state.flowNodes) {
					occupied[fn.id] = fn.position;
				}

				// Find existing targets connected to source to stagger vertically
				const existingConnections = state.flowEdges.filter(
					(e) => (e.source === sourceId && e.target !== targetId) || (e.target === sourceId && e.source !== targetId)
				).length;

				const preferredOffset = {
					dx: STEP_X,
					dy: existingConnections * STEP_Y
				};

				const newSlot = findNearbyFreeSlot(sourceNode.position, occupied, preferredOffset, targetId);
				setStoredPosition(targetId, newSlot.x, newSlot.y);

				const flowNodes = state.flowNodes.map((fn) =>
					fn.id === targetId ? { ...fn, position: newSlot } : fn
				);

				updatedPositions = {};
				for (const fn of flowNodes) {
					updatedPositions[fn.id] = fn.position;
				}

				return {
					...state,
					flowNodes
				};
			});

			return updatedPositions;
		},

		/**
		 * Re-layout all nodes neatly on demand (Auto Layout / Align action).
		 */
		recomputeLayout(): Record<string, { x: number; y: number }> {
			let newPositions: Record<string, { x: number; y: number }> = {};
			update((state) => {
				// Clear local storage positions for this project's nodes to trigger clean re-layout
				for (const n of state.nodes) {
					if (typeof window !== 'undefined') {
						localStorage.removeItem(n.id);
					}
				}
				newPositions = autoLayoutPositions(state.nodes, {}, state.edges);
				for (const [id, p] of Object.entries(newPositions)) {
					setStoredPosition(id, p.x, p.y);
				}
				return {
					...state,
					flowNodes: state.nodes.map((n) => toFlowNode(n, newPositions[n.id]))
				};
			});
			return newPositions;
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

		/** Add a node to the topology, auto-positioning nearby if position is omitted. */
		addNode(node: TopologyNode, position?: { x: number; y: number }) {
			update((state) => {
				let resolvedPos = position;
				if (!resolvedPos) {
					const occupied: Record<string, { x: number; y: number }> = {};
					for (const fn of state.flowNodes) occupied[fn.id] = fn.position;

					const refNode = state.flowNodes[state.flowNodes.length - 1];
					const anchor = refNode ? refNode.position : { x: START_X, y: START_Y };
					resolvedPos = findNearbyFreeSlot(anchor, occupied, { dx: 0, dy: STEP_Y }, node.id);
				}

				setStoredPosition(node.id, resolvedPos.x, resolvedPos.y);
				return {
					...state,
					nodes: [...state.nodes, node],
					flowNodes: [...state.flowNodes, toFlowNode(node, resolvedPos)]
				};
			});
		},

		/** Remove a node and its edges. */
		removeNode(nodeId: string) {
			if (typeof window !== 'undefined') {
				try { localStorage.removeItem(nodeId); } catch {}
			}
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
			update((state) => {
				const exists = state.edges.some(
					(e) => (e.source === edge.source && e.target === edge.target) ||
					       (e.source === edge.target && e.target === edge.source)
				);
				if (exists) return state;

				return {
					...state,
					edges: [...state.edges, edge],
					flowEdges: [...state.flowEdges, toFlowEdge(edge)]
				};
			});
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		}
	};
}

export const topologyStore = createTopologyStore();
