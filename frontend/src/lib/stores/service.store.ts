import { writable, derived } from 'svelte/store';
import type { Service } from '$lib/api/types';

interface ServiceState {
	services: Service[];
	activeService: Service | null;
	isLoading: boolean;
}

const initialState: ServiceState = {
	services: [],
	activeService: null,
	isLoading: false
};

function createServiceStore() {
	const { subscribe, set, update } = writable<ServiceState>(initialState);

	return {
		subscribe,

		setServices(services: Service[]) {
			update((state) => ({ ...state, services }));
		},

		setActiveService(service: Service | null) {
			update((state) => ({ ...state, activeService: service }));
		},

		updateServiceStatus(serviceId: string, status: string, replicas?: number) {
			update((state) => ({
				...state,
				services: state.services.map((s) =>
					s.id === serviceId
						? { ...s, status, replicas: replicas ?? s.replicas }
						: s
				),
				activeService:
					state.activeService?.id === serviceId
						? { ...state.activeService, status, replicas: replicas ?? state.activeService.replicas }
						: state.activeService
			}));
		},

		addService(service: Service) {
			update((state) => ({ ...state, services: [...state.services, service] }));
		},

		removeService(id: string) {
			update((state) => ({
				...state,
				services: state.services.filter((s) => s.id !== id),
				activeService: state.activeService?.id === id ? null : state.activeService
			}));
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		}
	};
}

export const serviceStore = createServiceStore();
