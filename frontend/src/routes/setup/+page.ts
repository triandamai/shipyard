import type { PageLoad } from './$types';

export const load: PageLoad = async ({ fetch }) => {
	try {
		const res = await fetch('/api/setup/status');
		if (res.ok) {
			const json = await res.json();
			return { alreadySetup: json.data?.initialized === true };
		}
	} catch {
		// If the status check fails (backend unreachable), let the user proceed
	}
	return { alreadySetup: false };
};
