import { eventBus } from '../eventBus';
import { logStore, type LogLine } from '$lib/stores/log.store';
import type { MqttPayload } from '$lib/api/types';

/**
 * Log MQTT handler.
 * Appends real-time log lines from container log topics to the log store ring buffer.
 */
export function initLogHandler() {
	eventBus.on('*', (topic, payload) => {
		if (typeof topic !== 'string') return;

		const logMatch = topic.match(
			/platform\/orgs\/[^/]+\/projects\/[^/]+\/services\/[^/]+\/logs\/([^/]+)$/
		);
		if (logMatch) {
			const replicaId = logMatch[1];
			const p = payload as MqttPayload;

			const line: LogLine = {
				id: `${Date.now()}-${Math.random().toString(36).substring(2, 8)}`,
				timestamp: p.timestamp,
				level: p.level ?? 'info',
				message: p.message ?? '',
				replicaId
			};

			logStore.append(line);
		}
	});
}
