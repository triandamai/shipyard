import { writable, derived } from 'svelte/store';
import type { User, AuthTokens, Organization, MemberRole } from '$lib/api/types';

interface AuthState {
	user: User | null;
	token: string | null;
	permissions: string[];
	isAuthenticated: boolean;
	isLoading: boolean;
	sessionExpired: boolean;
	forbidden: boolean;
}

const initialState: AuthState = {
	user: null,
	token: null,
	permissions: [],
	isAuthenticated: false,
	isLoading: true,
	sessionExpired: false,
	forbidden: false
};

/**
 * Check whether a permission list grants a specific permission.
 * Supports suffix wildcards: `"shipyard:settings:*"` matches `"shipyard:settings:infra:view"`.
 */
export function matchesPermission(permissions: string[], required: string): boolean {
	return permissions.some((p) => {
		if (p === required) return true;
		if (p.endsWith('*')) {
			const prefix = p.slice(0, -1);
			return required.startsWith(prefix);
		}
		return false;
	});
}

function createAuthStore() {
	const { subscribe, set, update } = writable<AuthState>(initialState);

	return {
		subscribe,

		restoreToken(token: string) {
			update((state) => ({
				...state,
				token,
				isLoading: true
			}));
		},

		setUser(user: User, tokens: AuthTokens) {
			update((state) => ({
				...state,
				user,
				token: tokens.access_token,
				permissions: user.permissions ?? [],
				isAuthenticated: true,
				isLoading: false
			}));
		},

		logout() {
			set({ ...initialState, isLoading: false });
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		},

		updateAccessToken(newToken: string) {
			update((state) => ({ ...state, token: newToken }));
		},

		markSessionExpired() {
			update((state) => ({ ...state, sessionExpired: true }));
		},

		markForbidden() {
			update((state) => ({ ...state, forbidden: true }));
		},

		clearForbidden() {
			update((state) => ({ ...state, forbidden: false }));
		}
	};
}

export const authStore = createAuthStore();

/**
 * Reactive permission checker derived from the auth store.
 * Usage: `$can('shipyard:settings:infra:view')`
 */
export const can = derived(authStore, ($auth) => (permission: string) =>
	matchesPermission($auth.permissions, permission)
);
