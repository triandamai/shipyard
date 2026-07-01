import { writable } from 'svelte/store';

export type ToastType = 'info' | 'success' | 'error' | 'warning';

export interface Toast {
	id: string;
	type: ToastType;
	title: string;
	message?: string;
}

function createToastStore() {
	const { subscribe, update } = writable<Toast[]>([]);

	function add(toast: Omit<Toast, 'id'>, duration = 4000) {
		const id = `${Date.now()}-${Math.random()}`;
		update((toasts) => [...toasts, { ...toast, id }]);
		if (duration > 0) setTimeout(() => remove(id), duration);
		return id;
	}

	function remove(id: string) {
		update((toasts) => toasts.filter((t) => t.id !== id));
	}

	return { subscribe, add, remove };
}

export const toastStore = createToastStore();
