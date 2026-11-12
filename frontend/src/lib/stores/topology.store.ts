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
export const STEP_X = 320;       // horizontal distance between connected stages
export const STEP_Y = 180;       // vertical distance between sibling rows
export const START_X = 80;       // default canvas left margin
export const START_Y = 80;       // default canvas top margin
export const COMPONENT_GAP = 70; // vertical gap between disconnected subgraphs
export const COLLISION_PAD_X = 40;
export const COLLISION_PAD_Y = 30;

const RESOURCE_TYPES = new Set(['domain', 'volume', 'network', 'portal']);

interface DirectedEdge {
	source: string;
	target: string;
}

/**
 * Normalizes dependency direction across explicit edges and implicit relationships:
 * "start with who depend to whom the less depend is the last"
 * An edge u -> v means "u depends on v":
 * u (dependent/consumer) is placed earlier (Layer L),
 * v (dependency/resource) is placed later (Layer >= L + 1).
 */
function normalizeEdges(
	nodes: TopologyNode[],
	edges: TopologyEdge[]
): DirectedEdge[] {
	const nodeTypeMap = new Map<string, string>();
	for (const n of nodes) {
		nodeTypeMap.set(n.id, n.type);
	}

	const edgeSet = new Set<string>();
	const result: DirectedEdge[] = [];

	function addDirected(src: string, tgt: string) {
		if (src === tgt) return;
		if (!nodeTypeMap.has(src) || !nodeTypeMap.has(tgt)) return;
		const key = `${src}->${tgt}`;
		if (!edgeSet.has(key)) {
			edgeSet.add(key);
			result.push({ source: src, target: tgt });
		}
	}

	// 1. Process explicit edges
	for (const e of edges) {
		const srcType = nodeTypeMap.get(e.source);
		const tgtType = nodeTypeMap.get(e.target);

		// If user dragged backwards from a resource to a service, normalize so service -> resource
		if (srcType && RESOURCE_TYPES.has(srcType) && tgtType && !RESOURCE_TYPES.has(tgtType)) {
			addDirected(e.target, e.source);
		} else {
			addDirected(e.source, e.target);
		}
	}

	// 2. Synthesize implicit edges from node metadata if missing
	for (const n of nodes) {
		const d = n.data as Record<string, unknown> | undefined;
		const serviceId = d?.service_id as string | undefined;
		if (serviceId && nodeTypeMap.has(serviceId)) {
			addDirected(serviceId, n.id);
		}
		const parentId = d?.service_parent_id as string | undefined;
		if (parentId && nodeTypeMap.has(parentId)) {
			addDirected(parentId, n.id);
		}
	}

	return result;
}

/**
 * Partitions nodes into weakly connected components so disconnected subgraphs
 * (e.g. independent service stacks) are placed in clean, non-overlapping vertical bands.
 */
function getConnectedComponents(
	nodes: TopologyNode[],
	directedEdges: DirectedEdge[]
): string[][] {
	const adj = new Map<string, Set<string>>();
	for (const n of nodes) {
		adj.set(n.id, new Set());
	}
	for (const e of directedEdges) {
		adj.get(e.source)?.add(e.target);
		adj.get(e.target)?.add(e.source);
	}

	const visited = new Set<string>();
	const components: string[][] = [];

	for (const n of nodes) {
		if (visited.has(n.id)) continue;
		const comp: string[] = [];
		const queue: string[] = [n.id];
		visited.add(n.id);

		while (queue.length > 0) {
			const curr = queue.shift()!;
			comp.push(curr);
			for (const neighbor of adj.get(curr) ?? []) {
				if (!visited.has(neighbor)) {
					visited.add(neighbor);
					queue.push(neighbor);
				}
			}
		}
		components.push(comp);
	}

	// Sort components: components with services first, then by node count descending
	components.sort((a, b) => {
		const aServices = a.filter(id => !RESOURCE_TYPES.has(nodes.find(n => n.id === id)?.type ?? '')).length;
		const bServices = b.filter(id => !RESOURCE_TYPES.has(nodes.find(n => n.id === id)?.type ?? '')).length;
		if (bServices !== aServices) return bServices - aServices;
		return b.length - a.length;
	});

	return components;
}

/**
 * Assigns topological layers to nodes in a connected component using longest path layering:
 * 1. Breaks cycles via DFS back-edge detection.
 * 2. Assigns Layer 0 to root callers (in-degree 0).
 * 3. Assigns Layer(v) = max(Layer(u) + 1) for all u -> v.
 * Leaf dependencies (databases, volumes, domains, networks, terminal sinks) naturally
 * end up at the furthest/last layers.
 */
function assignComponentLayers(
	nodeIds: string[],
	directedEdges: DirectedEdge[]
): { layers: string[][]; dagEdges: DirectedEdge[] } {
	const nodeSet = new Set(nodeIds);
	const compEdges = directedEdges.filter(e => nodeSet.has(e.source) && nodeSet.has(e.target));

	// 1. Cycle breaking using DFS
	const outAdj = new Map<string, string[]>();
	for (const id of nodeIds) outAdj.set(id, []);
	for (const e of compEdges) {
		outAdj.get(e.source)?.push(e.target);
	}

	const visited = new Set<string>();
	const onStack = new Set<string>();
	const dagEdges: DirectedEdge[] = [];

	function dfs(u: string) {
		visited.add(u);
		onStack.add(u);
		for (const v of outAdj.get(u) ?? []) {
			if (onStack.has(v)) {
				// Back-edge forming cycle! Ignore for layering
				continue;
			}
			dagEdges.push({ source: u, target: v });
			if (!visited.has(v)) {
				dfs(v);
			}
		}
		onStack.delete(u);
	}

	for (const id of nodeIds) {
		if (!visited.has(id)) {
			dfs(id);
		}
	}

	// 2. In-degree calculation on dagEdges
	const inDegree = new Map<string, number>();
	const dagOut = new Map<string, string[]>();
	for (const id of nodeIds) {
		inDegree.set(id, 0);
		dagOut.set(id, []);
	}
	for (const e of dagEdges) {
		inDegree.set(e.target, (inDegree.get(e.target) ?? 0) + 1);
		dagOut.get(e.source)?.push(e.target);
	}

	// 3. Longest-path layering
	// Root nodes: in-degree 0 (nodes that depend on others, but nothing depends on them)
	const roots = nodeIds.filter(id => inDegree.get(id) === 0);
	const layerMap = new Map<string, number>();
	for (const r of (roots.length > 0 ? roots : [nodeIds[0]])) {
		layerMap.set(r, 0);
	}

	const queue = roots.length > 0 ? [...roots] : [nodeIds[0]];
	const processed = new Set<string>(queue);

	while (queue.length > 0) {
		const u = queue.shift()!;
		const uLayer = layerMap.get(u) ?? 0;

		for (const v of dagOut.get(u) ?? []) {
			const currentVLayer = layerMap.get(v) ?? 0;
			const newVLayer = Math.max(currentVLayer, uLayer + 1);
			layerMap.set(v, newVLayer);

			inDegree.set(v, (inDegree.get(v) ?? 1) - 1);
			if (inDegree.get(v)! <= 0 && !processed.has(v)) {
				processed.add(v);
				queue.push(v);
			}
		}
	}

	// Fallback for any unreachable nodes
	for (const id of nodeIds) {
		if (!layerMap.has(id)) {
			layerMap.set(id, 0);
		}
	}

	// Group nodes into layers
	const maxLayer = Math.max(...Array.from(layerMap.values()), 0);
	const layers: string[][] = Array.from({ length: maxLayer + 1 }, () => []);
	for (const id of nodeIds) {
		const l = layerMap.get(id) ?? 0;
		layers[l].push(id);
	}

	return { layers, dagEdges };
}

/**
 * Crossing reduction via Barycentric heuristic sweeps:
 * Alternates forward sweeps (sorting by predecessor positions) and backward sweeps
 * (sorting by successor positions) to untangle crossed edges.
 */
function orderLayersCrossingReduction(
	layers: string[][],
	nodesMap: Map<string, TopologyNode>,
	dagEdges: DirectedEdge[]
): string[][] {
	if (layers.length <= 1) return layers;

	const predecessors = new Map<string, string[]>();
	const successors = new Map<string, string[]>();
	for (const l of layers) {
		for (const id of l) {
			predecessors.set(id, []);
			successors.set(id, []);
		}
	}
	for (const e of dagEdges) {
		predecessors.get(e.target)?.push(e.source);
		successors.get(e.source)?.push(e.target);
	}

	const order = layers.map(layer => [...layer]);
	const getPos = (id: string, layerIdx: number): number => {
		const idx = order[layerIdx].indexOf(id);
		return idx >= 0 ? idx : 0;
	};

	// 4 Sweeps (forward, backward, forward, backward)
	for (let iter = 0; iter < 4; iter++) {
		// Forward sweep: Layer 1 to M
		for (let k = 1; k < order.length; k++) {
			const layer = order[k];
			const barycenters = new Map<string, number>();

			for (const id of layer) {
				const preds = predecessors.get(id) ?? [];
				const validPreds = preds.filter(p => {
					return order.some((l, lIdx) => lIdx < k && l.includes(p));
				});

				if (validPreds.length > 0) {
					let sum = 0;
					for (const p of validPreds) {
						for (let prevK = 0; prevK < k; prevK++) {
							if (order[prevK].includes(p)) {
								sum += getPos(p, prevK);
								break;
							}
						}
					}
					barycenters.set(id, sum / validPreds.length);
				} else {
					barycenters.set(id, getPos(id, k));
				}
			}

			layer.sort((a, b) => {
				const bA = barycenters.get(a) ?? 0;
				const bB = barycenters.get(b) ?? 0;
				if (Math.abs(bA - bB) > 0.001) {
					return bA - bB;
				}
				// Deterministic tie-breaker: domains before volumes, then by ID
				const typeA = nodesMap.get(a)?.type ?? '';
				const typeB = nodesMap.get(b)?.type ?? '';
				if (typeA === 'domain' && typeB !== 'domain') return -1;
				if (typeB === 'domain' && typeA !== 'domain') return 1;
				if (typeA === 'volume' && typeB !== 'volume') return -1;
				if (typeB === 'volume' && typeA !== 'volume') return 1;
				return a.localeCompare(b);
			});
		}

		// Backward sweep: Layer M-1 down to 0
		for (let k = order.length - 2; k >= 0; k--) {
			const layer = order[k];
			const barycenters = new Map<string, number>();

			for (const id of layer) {
				const succs = successors.get(id) ?? [];
				const validSuccs = succs.filter(s => {
					return order.some((l, lIdx) => lIdx > k && l.includes(s));
				});

				if (validSuccs.length > 0) {
					let sum = 0;
					for (const s of validSuccs) {
						for (let nextK = k + 1; nextK < order.length; nextK++) {
							if (order[nextK].includes(s)) {
								sum += getPos(s, nextK);
								break;
							}
						}
					}
					barycenters.set(id, sum / validSuccs.length);
				} else {
					barycenters.set(id, getPos(id, k));
				}
			}

			layer.sort((a, b) => {
				const bA = barycenters.get(a) ?? 0;
				const bB = barycenters.get(b) ?? 0;
				if (Math.abs(bA - bB) > 0.001) {
					return bA - bB;
				}
				return a.localeCompare(b);
			});
		}
	}

	return order;
}

/**
 * Assigns X and Y coordinates with parent centering, symmetric sibling fanning,
 * and strict collision clearance.
 */
function assignCoordinates(
	orderedLayers: string[][],
	nodesMap: Map<string, TopologyNode>,
	dagEdges: DirectedEdge[],
	startY: number
): { positions: Record<string, { x: number; y: number }>; maxY: number } {
	const positions: Record<string, { x: number; y: number }> = {};
	let currentMaxY = startY;

	const predsMap = new Map<string, string[]>();
	for (const e of dagEdges) {
		const list = predsMap.get(e.target) ?? [];
		list.push(e.source);
		predsMap.set(e.target, list);
	}

	for (let k = 0; k < orderedLayers.length; k++) {
		const layer = orderedLayers[k];
		const x = START_X + k * STEP_X;

		if (k === 0) {
			for (let i = 0; i < layer.length; i++) {
				const y = startY + i * STEP_Y;
				positions[layer[i]] = { x, y };
				if (y > currentMaxY) currentMaxY = y;
			}
		} else {
			// Find siblings connected to the same single parent for symmetric fanning
			const parentChildren = new Map<string, string[]>();
			for (const id of layer) {
				const preds = predsMap.get(id) ?? [];
				if (preds.length === 1 && positions[preds[0]]) {
					const list = parentChildren.get(preds[0]) ?? [];
					list.push(id);
					parentChildren.set(preds[0], list);
				}
			}

			const idealYs: number[] = [];
			for (let i = 0; i < layer.length; i++) {
				const id = layer[i];
				const preds = (predsMap.get(id) ?? []).filter(p => positions[p]);

				if (preds.length === 1 && parentChildren.get(preds[0])) {
					const siblings = parentChildren.get(preds[0])!;
					const sibIdx = siblings.indexOf(id);
					const parentY = positions[preds[0]].y;
					const fanOffset = (sibIdx - (siblings.length - 1) / 2) * STEP_Y;
					idealYs.push(parentY + fanOffset);
				} else if (preds.length > 0) {
					const avg = preds.reduce((acc, p) => acc + positions[p].y, 0) / preds.length;
					idealYs.push(avg);
				} else {
					idealYs.push(startY + i * STEP_Y);
				}
			}

			// Forward pass: resolve collisions and enforce minimum STEP_Y distance
			const assignedYs: number[] = [];
			for (let i = 0; i < layer.length; i++) {
				let y = idealYs[i];
				if (i === 0) {
					y = Math.max(startY, y);
				} else {
					y = Math.max(y, assignedYs[i - 1] + STEP_Y);
				}
				assignedYs.push(y);
			}

			// Backward relaxation: pull back up towards ideal Y where space allows
			for (let i = layer.length - 2; i >= 0; i--) {
				if (assignedYs[i] > idealYs[i]) {
					const maxAllowed = assignedYs[i + 1] - STEP_Y;
					const minAllowed = startY + i * STEP_Y;
					assignedYs[i] = Math.max(minAllowed, Math.min(maxAllowed, idealYs[i]));
				}
			}

			for (let i = 0; i < layer.length; i++) {
				const y = assignedYs[i];
				positions[layer[i]] = { x, y };
				if (y > currentMaxY) currentMaxY = y;
			}
		}
	}

	// Backward centering pass: center parent nodes relative to their downstream children
	for (let k = orderedLayers.length - 2; k >= 0; k--) {
		const layer = orderedLayers[k];
		const nextLayer = orderedLayers[k + 1];

		for (let i = 0; i < layer.length; i++) {
			const parentId = layer[i];
			const children = nextLayer.filter(childId => (predsMap.get(childId) ?? []).includes(parentId));

			if (children.length > 0) {
				const childrenMidY = children.reduce((acc, c) => acc + positions[c].y, 0) / children.length;

				const minY = i === 0 ? startY : positions[layer[i - 1]].y + STEP_Y;
				const maxY = i === layer.length - 1 ? Infinity : positions[layer[i + 1]].y - STEP_Y;

				const targetY = Math.max(minY, Math.min(maxY, childrenMidY));
				positions[parentId].y = targetY;
				if (targetY > currentMaxY) currentMaxY = targetY;
			}
		}
	}

	return { positions, maxY: currentMaxY };
}

/**
 * Computes a Sugiyama-style layered DAG topology layout with crossing reduction:
 * - "start with who depend to whom the less depend is the last"
 * - In-degree 0 services / callers start at Layer 0 (left).
 * - Intermediate services follow downstream layers.
 * - Out-degree 0 leaf resources (volumes, domains, networks, terminal sinks) are placed at the last layers.
 * - Barycentric crossing reduction untangles intersecting connection lines.
 * - Sibling nodes symmetrically fan out around parents with zero overlap.
 */
export function computeDependencyLayeredLayout(
	nodes: TopologyNode[],
	edges: TopologyEdge[]
): Record<string, { x: number; y: number }> {
	if (nodes.length === 0) return {};

	const nodesMap = new Map<string, TopologyNode>();
	for (const n of nodes) nodesMap.set(n.id, n);

	const directedEdges = normalizeEdges(nodes, edges);
	const components = getConnectedComponents(nodes, directedEdges);

	const allPositions: Record<string, { x: number; y: number }> = {};
	let currentStartY = START_Y;

	for (const compNodeIds of components) {
		const { layers, dagEdges } = assignComponentLayers(compNodeIds, directedEdges);
		const orderedLayers = orderLayersCrossingReduction(layers, nodesMap, dagEdges);
		const { positions, maxY } = assignCoordinates(orderedLayers, nodesMap, dagEdges, currentStartY);

		Object.assign(allPositions, positions);
		currentStartY = maxY + STEP_Y + COMPONENT_GAP;
	}

	return allPositions;
}

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
 * - Respects explicitly stored positions from DB or canvas.
 * - When all nodes lack positions, computes full dependency-layered layout with crossing reduction.
 * - When incremental nodes lack positions, positions them nearby according to dependency edges.
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

	// 2. Identify nodes that still need positioning
	const unpositioned = nodes.filter((n) => !result[n.id]);
	if (unpositioned.length === 0) {
		return result;
	}

	// If all nodes need positioning, compute the full dependency-layered DAG layout
	if (unpositioned.length === nodes.length) {
		const computed = computeDependencyLayeredLayout(nodes, edges);
		for (const [id, pos] of Object.entries(computed)) {
			result[id] = pos;
			setStoredPosition(id, pos.x, pos.y);
		}
		return result;
	}

	// Build adjacency maps for fast relationship lookup
	const connectedTo: Record<string, string[]> = {};
	for (const e of edges) {
		(connectedTo[e.source] ??= []).push(e.target);
		(connectedTo[e.target] ??= []).push(e.source);
	}

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
			const existingSiblings = Object.entries(result).filter(([_, p]) => {
				return Math.abs(p.x - (anchor.pos.x + STEP_X)) < 50;
			}).length;

			const staggerSign = existingSiblings % 2 === 1 ? 1 : -1;
			const staggerMult = Math.ceil(existingSiblings / 2);
			const staggerDy = existingSiblings === 0 ? 0 : staggerSign * staggerMult * STEP_Y;

			const preferredOffset = {
				dx: STEP_X,
				dy: staggerDy
			};
			const freeSlot = findNearbyFreeSlot(anchor.pos, result, preferredOffset, n.id);
			result[n.id] = freeSlot;
			setStoredPosition(n.id, freeSlot.x, freeSlot.y);
		} else {
			remaining.push(n);
		}
	}

	// 4. Place remaining unanchored nodes in a new row below existing nodes
	for (const n of remaining) {
		const placedEntries = Object.entries(result);
		if (placedEntries.length > 0) {
			const maxY = Math.max(...placedEntries.map(([_, p]) => p.y));
			const freeSlot = findNearbyFreeSlot({ x: START_X, y: maxY }, result, { dx: 0, dy: STEP_Y }, n.id);
			result[n.id] = freeSlot;
			setStoredPosition(n.id, freeSlot.x, freeSlot.y);
		} else {
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

				const staggerSign = existingConnections % 2 === 1 ? 1 : -1;
				const staggerMult = Math.ceil(existingConnections / 2);
				const staggerDy = existingConnections === 0 ? 0 : staggerSign * staggerMult * STEP_Y;

				const preferredOffset = {
					dx: STEP_X,
					dy: staggerDy
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
		 * Re-layout all nodes neatly on demand using the dependency-layered DAG layout.
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
				newPositions = computeDependencyLayeredLayout(state.nodes, state.edges);
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

					const d = node.data as Record<string, unknown> | undefined;
					const parentId = (d?.service_parent_id || d?.service_id) as string | undefined;
					const parentNode = parentId ? state.flowNodes.find((fn) => fn.id === parentId) : null;

					if (parentNode) {
						resolvedPos = findNearbyFreeSlot(parentNode.position, occupied, { dx: STEP_X, dy: 0 }, node.id);
					} else if (state.flowNodes.length > 0) {
						const maxPlacedY = Math.max(...state.flowNodes.map(fn => fn.position.y));
						resolvedPos = findNearbyFreeSlot({ x: START_X, y: maxPlacedY }, occupied, { dx: 0, dy: STEP_Y }, node.id);
					} else {
						resolvedPos = { x: START_X, y: START_Y };
					}
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
