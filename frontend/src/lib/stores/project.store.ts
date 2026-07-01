import { writable } from 'svelte/store';
import type { Project } from '$lib/api/types';

interface ProjectState {
	projects: Project[];
	activeProject: Project | null;
	isLoading: boolean;
}

const initialState: ProjectState = {
	projects: [],
	activeProject: null,
	isLoading: false
};

function createProjectStore() {
	const { subscribe, set, update } = writable<ProjectState>(initialState);

	return {
		subscribe,

		setProjects(projects: Project[]) {
			update((state) => ({ ...state, projects }));
		},

		setActiveProject(project: Project | null) {
			update((state) => ({ ...state, activeProject: project }));
		},

		addProject(project: Project) {
			update((state) => ({ ...state, projects: [...state.projects, project] }));
		},

		removeProject(id: string) {
			update((state) => ({
				...state,
				projects: state.projects.filter((p) => p.id !== id),
				activeProject: state.activeProject?.id === id ? null : state.activeProject
			}));
		},

		updateNodePositions(projectId: string, positions: Record<string, { x: number; y: number }>) {
			update((state) => ({
				...state,
				projects: state.projects.map((p) =>
					p.id === projectId ? { ...p, node_positions: positions } : p
				),
				activeProject:
					state.activeProject?.id === projectId
						? { ...state.activeProject, node_positions: positions }
						: state.activeProject
			}));
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		}
	};
}

export const projectStore = createProjectStore();
