import { writable } from 'svelte/store';
import type { SvelteComponent } from 'svelte';

interface PanelEntry {
	id: string;
	key?: string;
	component: any; // Svelte component constructor
	props: Record<string, unknown>;
	title: string;
}

interface UIState {
	sidebarCollapsed: boolean;
	panelStack: PanelEntry[];
}

const initialState: UIState = {
	sidebarCollapsed: true,
	panelStack: []
};

let panelIdCounter = 0;

function createUIStore() {
	const { subscribe, set, update } = writable<UIState>(initialState);

	return {
		subscribe,

		toggleSidebar() {
			update((state) => ({ ...state, sidebarCollapsed: !state.sidebarCollapsed }));
		},

		/** Push a new slide panel onto the stack.
		 *  If `key` is provided and a panel with the same key already exists,
		 *  that panel is moved to the top instead of creating a duplicate.
		 */
		pushPanel(entry: { component: any; props?: Record<string, unknown>; title: string; key?: string }) {
			const id = `panel-${++panelIdCounter}`;
			update((state) => {
				if (entry.key) {
					const existing = state.panelStack.find((p) => p.key === entry.key);
					if (existing) {
						// Move existing panel to the top of the stack
						return {
							...state,
							panelStack: [
								...state.panelStack.filter((p) => p.key !== entry.key),
								existing
							]
						};
					}
				}
				return {
					...state,
					panelStack: [
						...state.panelStack,
						{
							id,
							key: entry.key,
							component: entry.component,
							props: entry.props ?? {},
							title: entry.title
						}
					]
				};
			});
			return id;
		},

		/** Pop the top panel off the stack. */
		popPanel() {
			update((state) => ({
				...state,
				panelStack: state.panelStack.slice(0, -1)
			}));
		},

		/** Clear all panels. */
		clearPanels() {
			update((state) => ({ ...state, panelStack: [] }));
		},

		/** Remove a specific panel by id. */
		removePanel(id: string) {
			update((state) => ({
				...state,
				panelStack: state.panelStack.filter((p) => p.id !== id)
			}));
		}
	};
}

export const uiStore = createUIStore();
