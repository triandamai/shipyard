import { writable, derived, get } from 'svelte/store';
import type { Container, ContainerStatus } from '$lib/api/types';

// Keyed by service_id → array of containers (sorted by replica_index)
type ContainerMap = Record<string, Container[]>;

interface ContainerState {
	containers: ContainerMap;
	isLoading: boolean;
}

const initialState: ContainerState = {
	containers: {},
	isLoading: false
};

function createContainerStore() {
	const store = writable<ContainerState>(initialState);
	const { subscribe, set, update } = store;

	return {
		subscribe,

		/** Fetch and set containers for a service (from API response). */
		loadForService(serviceId: string, containers: Container[]) {
			update((state) => ({
				...state,
				containers: {
					...state.containers,
					[serviceId]: containers.sort((a, b) => a.replica_index - b.replica_index)
				}
			}));
		},

		/** Handle full container list refresh from MQTT. */
		handleMqttFullUpdate(serviceId: string, containers: Container[]) {
			update((state) => ({
				...state,
				containers: {
					...state.containers,
					[serviceId]: containers.sort((a, b) => a.replica_index - b.replica_index)
				}
			}));
		},

		/** Handle single container status delta from MQTT. */
		handleMqttStatusUpdate(
			serviceId: string,
			containerId: string,
			statusUpdate: {
				status: ContainerStatus;
				status_message?: string | null;
				finished_at?: string | null;
				exit_code?: number | null;
			}
		) {
			update((state) => {
				const existing = state.containers[serviceId] || [];
				const updated = existing.map((c) =>
					c.id === containerId ? { ...c, ...statusUpdate } : c
				);
				return {
					...state,
					containers: { ...state.containers, [serviceId]: updated }
				};
			});
		},

		/** Get containers for a specific service. */
		getForService(serviceId: string): Container[] {
			return get(store).containers[serviceId] || [];
		},

		/** Get count of running containers for a service. */
		getRunningCount(serviceId: string): number {
			const containers = get(store).containers[serviceId] || [];
			return containers.filter((c) => c.status === 'running').length;
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		}
	};
}

export const containerStore = createContainerStore();
