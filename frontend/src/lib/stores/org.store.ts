import { writable, derived } from 'svelte/store';
import type { Organization, OrgMember } from '$lib/api/types';

interface OrgState {
	organizations: Organization[];
	activeOrg: Organization | null;
	members: OrgMember[];
	myMembership: OrgMember | null;
	isLoading: boolean;
}

const initialState: OrgState = {
	organizations: [],
	activeOrg: null,
	members: [],
	myMembership: null,
	isLoading: false
};

function createOrgStore() {
	const { subscribe, set, update } = writable<OrgState>(initialState);

	return {
		subscribe,

		setOrganizations(orgs: Organization[]) {
			update((state) => ({ ...state, organizations: orgs }));
		},

		setActiveOrg(org: Organization | null) {
			update((state) => ({ ...state, activeOrg: org, members: [], myMembership: null }));
		},

		setMembers(members: OrgMember[]) {
			update((state) => ({ ...state, members }));
		},

		setMyMembership(membership: OrgMember | null) {
			update((state) => ({ ...state, myMembership: membership }));
		},

		setLoading(loading: boolean) {
			update((state) => ({ ...state, isLoading: loading }));
		}
	};
}

export const orgStore = createOrgStore();
