import mqtt from 'mqtt';
import { mqttStore } from '$lib/stores/mqtt.store';
import { eventBus } from './eventBus';
import { getAuthToken } from '$lib/auth/cookies';
import type { MqttPayload } from '$lib/api/types';

let client: mqtt.MqttClient | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
let retryCount = 0;
const MAX_RETRY_DELAY_MS = 60_000;
const STORAGE_KEY = 'shipyard_mqtt_cid';

/** Return a stable client ID for this browser, persisted across page loads. */
function getClientId(): string {
    try {
        const stored = localStorage.getItem(STORAGE_KEY);
        if (stored) return stored;
        const id = `shipyard-ui-${Math.random().toString(36).substring(2, 9)}`;
        localStorage.setItem(STORAGE_KEY, id);
        return id;
    } catch {
        // localStorage unavailable (private browsing, etc.) — fall back to ephemeral
        return `shipyard-ui-${Math.random().toString(36).substring(2, 9)}`;
    }
}

interface MqttInitOptions {
	brokerUrl?: string;
	onEvent?: (topic: string, payload: MqttPayload) => void;
}

function scheduleReconnect(brokerUrl: string, options: MqttInitOptions) {
	if (reconnectTimer) return;
	// Exponential backoff: 3s, 6s, 12s … capped at 60s
	const delay = Math.min(3000 * Math.pow(2, retryCount), MAX_RETRY_DELAY_MS);
	retryCount++;
	reconnectTimer = setTimeout(() => {
		reconnectTimer = null;
		connectMqtt(brokerUrl, options);
	}, delay);
}

function connectMqtt(brokerUrl: string, options: MqttInitOptions) {
	// Destroy any existing client first
	if (client) {
		client.removeAllListeners();
		client.end(true);
		client = null;
	}

	const token = getAuthToken();
	client = mqtt.connect(brokerUrl, {
		clientId: getClientId(),
		clean: true,
		// Disable the library's built-in reconnect — we handle it ourselves
		reconnectPeriod: 0,
		connectTimeout: 5000,
		keepalive: 30,
		// Authenticate with the user's JWT token so rmqtt can verify the connection
		username: 'shipyard-web',
		password: token ?? undefined,
		wsOptions: {
			protocol: 'mqtt',
		},
	});

	client.on('connect', () => {
		retryCount = 0;
		mqttStore.setConnected(true);
	});

	client.on('close', () => {
		mqttStore.setConnected(false);
		scheduleReconnect(brokerUrl, options);
	});

	client.on('error', () => {
		// Errors are followed by 'close', which schedules reconnect
		mqttStore.setConnected(false);
	});

	client.on('message', (topic: string, message: Buffer) => {
		try {
			const payload: MqttPayload = JSON.parse(message.toString());
			eventBus.emit(topic, payload);
			options.onEvent?.(topic, payload);
		} catch {
			// Ignore malformed messages
		}
	});
}

/** Initialize the MQTT client. Connects directly to the broker's WebSocket port. */
export function initMqtt(options: MqttInitOptions = {}) {
	// RMQTT listens on port 8083 for WebSocket connections.
	// PUBLIC_MQTT_WS_URL lets you override the address in production
	// (e.g. wss://mqtt.yourdomain.com).
	const brokerUrl =
		options.brokerUrl ??
		import.meta.env.PUBLIC_MQTT_WS_URL ??
		(window.location.protocol === 'https:'
			? `wss://${window.location.hostname}/mqtt`
			: `ws://${window.location.hostname}:8083`);
	retryCount = 0;
	if (reconnectTimer) {
		clearTimeout(reconnectTimer);
		reconnectTimer = null;
	}
	connectMqtt(brokerUrl, options);
	return { getClient: () => client };
}

/** Subscribe to an MQTT topic. */
export function subscribeTopic(topic: string) {
	if (!client) return;
	client.subscribe(topic, { qos: 0 }, (err) => {
		if (!err) mqttStore.addSubscription(topic);
	});
}

/** Unsubscribe from an MQTT topic. */
export function unsubscribeTopic(topic: string) {
	if (!client) return;
	client.unsubscribe(topic, {}, (err) => {
		if (!err) mqttStore.removeSubscription(topic);
	});
}

/** Get the MQTT client instance. */
export function getMqttClient(): mqtt.MqttClient | null {
	return client;
}

/** Disconnect the MQTT client and stop reconnect attempts. */
export function disconnectMqtt() {
	if (reconnectTimer) {
		clearTimeout(reconnectTimer);
		reconnectTimer = null;
	}
	if (client) {
		client.end(true);
		client = null;
		mqttStore.setConnected(false);
	}
}
