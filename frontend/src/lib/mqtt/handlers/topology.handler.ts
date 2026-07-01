import { eventBus } from '../eventBus';
import { topologyStore } from '$lib/stores/topology.store';
import type { MqttPayload, TopologyNode, TopologyEdge } from '$lib/api/types';

/**
 * Topology MQTT handler.
 * Updates the topology store when topology changes arrive.
 */
export function initTopologyHandler() {
	eventBus.on('*', (topic, payload) => {
		if (typeof topic !== 'string') return;

		const topoMatch = topic.match(
			/platform\/orgs\/[^/]+\/projects\/[^/]+\/topology$/
		);
		if (topoMatch) {
			const p = payload as MqttPayload;
			const meta = p.meta as any;
			if (meta?.nodes && meta?.edges) {
				topologyStore.setTopology(
					meta.nodes as TopologyNode[],
					meta.edges as TopologyEdge[]
				);
			}
		}
	});
}
