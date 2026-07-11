import { writable } from 'svelte/store';
import type { OrgBilling, ComputeNode } from '$lib/api/types';
import { api } from '$lib/api/client';

interface BillingState {
	billing: OrgBilling | null;
	nodes: ComputeNode[];
	loading: boolean;
	error: string | null;
}

const initialState: BillingState = {
	billing: null,
	nodes: [],
	loading: false,
	error: null,
};

function createBillingStore() {
	const { subscribe, set, update } = writable<BillingState>(initialState);

	return {
		subscribe,

		async loadBilling(orgId: string) {
			if (!orgId) return;
			update((s) => ({ ...s, loading: true, error: null }));
			const res = await api.getBilling(orgId);
			if (res.data) {
				update((s) => ({ ...s, billing: res.data, loading: false }));
			} else {
				update((s) => ({ ...s, error: res.error?.message ?? 'Failed to load billing', loading: false }));
			}
		},

		async loadNodes(orgId: string) {
			if (!orgId) return;
			const res = await api.listNodes(orgId);
			if (res.data) {
				update((s) => ({ ...s, nodes: res.data ?? [] }));
			}
		},

		async refreshNodes(orgId: string) {
			if (!orgId) return;
			const res = await api.listNodes(orgId);
			if (res.data) {
				update((s) => ({ ...s, nodes: res.data ?? [] }));
			}
		},

		reset() {
			set(initialState);
		},
	};
}

export const billingStore = createBillingStore();
