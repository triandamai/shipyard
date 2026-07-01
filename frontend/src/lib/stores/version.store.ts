import { writable } from 'svelte/store';

export interface VersionInfo {
	current: string;
	latest: string;
	update_available: boolean;
	release_url: string;
	release_notes: string | null;
}

interface VersionState {
	info: VersionInfo | null;
	checking: boolean;
	updating: boolean;
	updateError: string | null;
}

function createVersionStore() {
	const { subscribe, update } = writable<VersionState>({
		info: null,
		checking: false,
		updating: false,
		updateError: null,
	});

	return {
		subscribe,
		setChecking(checking: boolean) {
			update((s) => ({ ...s, checking }));
		},
		setInfo(info: VersionInfo | null) {
			update((s) => ({ ...s, info, checking: false }));
		},
		setUpdating(updating: boolean) {
			update((s) => ({ ...s, updating, updateError: null }));
		},
		setUpdateError(err: string | null) {
			update((s) => ({ ...s, updating: false, updateError: err }));
		},
		clearUpdate() {
			update((s) => ({ ...s, updating: false, updateError: null }));
		},
	};
}

export const versionStore = createVersionStore();
