import { subscribeTopic } from '../client';
import { eventBus } from '../eventBus';
import { alertsStore, type SpikeAlert } from '$lib/stores/alerts.store.svelte';
import type { MqttPayload } from '$lib/api/types';

const SPIKE_TOPIC = 'platform/alerts/spike';

export function initAlertsHandler() {
	subscribeTopic(SPIKE_TOPIC);

	eventBus.on('*', (topic, payload) => {
		if (topic !== SPIKE_TOPIC) return;
		const p = payload as MqttPayload;
		if (p.event !== 'resource.spike' || !p.meta) return;

		const m = p.meta as Record<string, unknown>;
		alertsStore.add({
			metric:       m.metric as SpikeAlert['metric'],
			value:        m.value as number,
			threshold:    m.threshold as number,
			container_id: (m.container_id as string | null) ?? null,
			node_id:      m.node_id as string,
			ts:           m.ts as number,
		});
	});
}
