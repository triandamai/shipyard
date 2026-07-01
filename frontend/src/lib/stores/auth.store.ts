import { writable } from 'svelte/store';
import type { User, AuthTokens, Organization, MemberRole } from '$lib/api/types';

interface AuthState {
	user: User | null;
	token: string | null;
	isAuthenticated: boolean;
	isLoading: boolean;
	sessionExpired: boolean;
	forbidden: boolean;
}

const initialState: AuthState = {
	user: null,
	token: null,
	isAuthenticated: false,
	isLoading: true,
	sessionExpired: false,
	forbidden: false
};

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
