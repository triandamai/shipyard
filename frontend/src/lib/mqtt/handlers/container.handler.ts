import { eventBus } from '../eventBus';
import { containerStore } from '$lib/stores/container.store';
import { serviceStore } from '$lib/stores/service.store';
import { topologyStore } from '$lib/stores/topology.store';
import type { MqttPayload, Container } from '$lib/api/types';

/**
 * Container MQTT handler.
 *
 * Subscribes to:
 * - .../containers — full container list refresh
 * - .../containers/{id}/status — single container status delta
 * - .../replicas/count — running replica count badge
 */
export function initContainerHandler() {
	eventBus.on('*', (topic, payload) => {
		if (typeof topic !== 'string') return;
		const p = payload as MqttPayload;

		// Full container list refresh
		const fullListMatch = topic.match(
			/platform\/orgs\/[^/]+\/projects\/[^/]+\/services\/([^/]+)\/containers$/
		);
		if (fullListMatch) {
			const serviceId = fullListMatch[1];
			const containers = (p.meta as unknown as Container[]) || [];
			containerStore.handleMqttFullUpdate(serviceId, containers);
			topologyStore.refreshNode(serviceId, {});
			return;
		}

		// Single container status
		const statusMatch = topic.match(
			/platform\/orgs\/[^/]+\/projects\/[^/]+\/services\/([^/]+)\/containers\/([^/]+)\/status$/
		);
		if (statusMatch) {
			const serviceId = statusMatch[1];
			const containerId = statusMatch[2];
			containerStore.handleMqttStatusUpdate(serviceId, containerId, p.meta as any);
			topologyStore.refreshNode(serviceId, {});
			return;
		}

		// Replica count
		const replicaMatch = topic.match(
			/platform\/orgs\/[^/]+\/projects\/[^/]+\/services\/([^/]+)\/replicas\/count$/
		);
		if (replicaMatch) {
			const serviceId = replicaMatch[1];
			const count = (p.meta as any)?.count ?? 0;
			serviceStore.updateServiceStatus(serviceId, 'running', count);
			return;
		}
	});
}
