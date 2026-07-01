import { writable } from 'svelte/store';

interface MqttState {
	connected: boolean;
	error: string | null;
	subscriptions: string[];
}

const initialState: MqttState = {
	connected: false,
	error: null,
	subscriptions: []
};

function createMqttStore() {
	const { subscribe, set, update } = writable<MqttState>(initialState);

	return {
		subscribe,

		setConnected(connected: boolean) {
			update((state) => ({ ...state, connected, error: connected ? null : state.error }));
		},

		setError(error: string | null) {
			update((state) => ({ ...state, error }));
		},

		addSubscription(topic: string) {
			update((state) => ({
				...state,
				subscriptions: state.subscriptions.includes(topic)
					? state.subscriptions
					: [...state.subscriptions, topic]
			}));
		},

		removeSubscription(topic: string) {
			update((state) => ({
				...state,
				subscriptions: state.subscriptions.filter((t) => t !== topic)
			}));
		}
	};
}

export const mqttStore = createMqttStore();
