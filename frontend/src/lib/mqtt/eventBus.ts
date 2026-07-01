import mitt from 'mitt';
import type { MqttPayload } from '$lib/api/types';

/** Typed event bus using mitt. Topic string → MqttPayload. */
type Events = Record<string, MqttPayload>;

export const eventBus = mitt<Events>();

/**
 * Subscribe to a wildcard topic pattern and call handler for matching events.
 * Useful for subscribing to all events under a prefix.
 */
export function onTopicPattern(
	pattern: string,
	handler: (topic: string, payload: MqttPayload) => void
) {
	eventBus.on('*', (topic, payload) => {
		if (typeof topic === 'string' && topic.startsWith(pattern)) {
			handler(topic, payload as MqttPayload);
		}
	});
}
