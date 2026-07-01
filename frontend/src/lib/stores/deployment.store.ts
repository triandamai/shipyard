import { writable } from 'svelte/store';
import type { Deployment, DeploymentStep, DeploymentLog } from '$lib/api/types';

interface DeploymentState {
	deployments: Deployment[];
	activeDeployment: Deployment | null;
	steps: DeploymentStep[];
	isLoading: boolean;
}

const initialState: DeploymentState = {
	deployments: [],
	activeDeployment: null,
	steps: [],
	isLoading: false
};

function createDeploymentStore() {
	const { subscribe, set, update } = writable<DeploymentState>(initialState);

	return {
		subscribe,

		setDeployments(deployments: Deployment[]) {
			update((state) => ({ ...state, deployments }));
		},

		setActiveDeployment(deployment: Deployment | null) {
			update((state) => ({ ...state, activeDeployment: deployment, steps: [] }));
		},

		setSteps(steps: DeploymentStep[]) {
			update((state) => ({
				...state,
				steps: steps.sort((a, b) => a.order_index - b.order_index)
			}));
		},

		updateDeploymentStatus(deploymentId: string, status: Deployment['status']) {
			update((state) => ({
				...state,
				deployments: state.deployments.map((d) =>
					d.id === deploymentId ? { ...d, status } : d
				),
				activeDeployment:
					state.activeDeployment?.id === deploymentId
						? { ...state.activeDeployment, status }
						: state.activeDeployment
			}));
		},

		updateStepStatus(stepId: string, status: DeploymentStep['status']) {
			update((state) => ({
				...state,
				steps: state.steps.map((s) =>
					s.id === stepId ? { ...s, status } : s
				)
			}));
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		}
	};
}

export const deploymentStore = createDeploymentStore();
