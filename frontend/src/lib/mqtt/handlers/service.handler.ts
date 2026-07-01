import { eventBus } from '../eventBus';
import { serviceStore } from '$lib/stores/service.store';
import type { MqttPayload } from '$lib/api/types';

/**
 * Service status MQTT handler.
 * Updates service store when status changes arrive via MQTT.
 */
export function initServiceHandler() {
	eventBus.on('*', (topic, payload) => {
		if (typeof topic !== 'string') return;

		const statusMatch = topic.match(
			/platform\/orgs\/[^/]+\/projects\/[^/]+\/services\/([^/]+)\/status$/
		);
		if (statusMatch) {
			const serviceId = statusMatch[1];
			const p = payload as MqttPayload;
			const status = (p.meta as any)?.status ?? p.event;
			serviceStore.updateServiceStatus(serviceId, status);
		}
	});
}
