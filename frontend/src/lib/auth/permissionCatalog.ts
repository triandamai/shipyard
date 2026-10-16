import { PERMISSION_GROUPS } from '$lib/api/types';

/** One thing a permission can allow, e.g. "Read". `id` is what gets stored. */
export interface PermAction {
	id: string;
	label: string;
	description?: string;
}

/** A searchable resource ("Deployments") with the actions that can be granted on it. */
export interface PermResource {
	key: string;
	label: string;
	description?: string;
	actions: PermAction[];
}

const ACTION_LABELS: Record<string, string> = {
	read: 'Read',
	view: 'View',
	write: 'Write',
	manage: 'Manage',
	invite: 'Invite',
	deploy: 'Deploy',
	admin: 'Admin'
};

// Display order inside a resource: the weakest action first.
const ACTION_ORDER = ['view', 'read', 'invite', 'deploy', 'write', 'manage', 'admin'];

function titleCase(s: string): string {
	return s.charAt(0).toUpperCase() + s.slice(1);
}

function actionLabel(verb: string): string {
	return ACTION_LABELS[verb] ?? titleCase(verb);
}

function sortActions(actions: PermAction[]): PermAction[] {
	const rank = (a: PermAction) => {
		const i = ACTION_ORDER.indexOf(a.id.split(':').pop() ?? '');
		return i === -1 ? ACTION_ORDER.length : i;
	};
	return [...actions].sort((a, b) => rank(a) - rank(b));
}

/**
 * Builds a catalog from flat ids like `settings:read`: everything before the
 * last `:` is the resource, the last part is the action.
 */
export function catalogFromIds(
	groups: { label: string; perms: { id: string; description?: string }[] }[]
): PermResource[] {
	const out: PermResource[] = [];
	for (const g of groups) {
		const byRes = new Map<string, PermAction[]>();
		for (const p of g.perms) {
			const cut = p.id.lastIndexOf(':');
			const res = p.id.slice(0, cut);
			const verb = p.id.slice(cut + 1);
			const list = byRes.get(res) ?? [];
			list.push({ id: p.id, label: actionLabel(verb), description: p.description });
			byRes.set(res, list);
		}
		for (const [res, actions] of byRes) {
			const multi = byRes.size > 1;
			out.push({
				key: res,
				label: multi ? `${g.label} · ${titleCase(res.split(':').pop() ?? res)}` : g.label,
				actions: sortActions(actions)
			});
		}
	}
	return out;
}

/** Organization-level permissions (ids are the suffix after `shipyard:<orgId>:`). */
export function orgPermissionCatalog(): PermResource[] {
	return catalogFromIds(
		PERMISSION_GROUPS.map((g) => ({
			label: g.group,
			perms: g.permissions.map((p) => ({ id: p.id, description: p.description }))
		}))
	);
}

/** Scopes an org API key can carry. */
export function apiKeyScopeCatalog(): PermResource[] {
	return [
		{
			key: 'access',
			label: 'Projects, services and deployments',
			actions: [
				{ id: 'read', label: 'Read', description: 'View projects, services, and deployments' },
				{ id: 'deploy', label: 'Deploy', description: 'Trigger deployments' },
				{ id: 'write', label: 'Write', description: 'Create and update services' },
				{ id: 'admin', label: 'Admin', description: 'Manage API keys and org settings' }
			]
		},
		{
			key: 'registry',
			label: 'Registry',
			actions: [
				{ id: 'registry:view', label: 'View', description: 'Pull images and browse artifacts' },
				{ id: 'registry:manage', label: 'Manage', description: 'Push and delete artifacts' }
			]
		}
	];
}

/** Human label for a stored permission id, for chips. */
export function permissionLabel(catalog: PermResource[], id: string): string {
	for (const r of catalog) {
		const a = r.actions.find((x) => x.id === id);
		if (a) return `${r.label}: ${a.label}`;
	}
	return id;
}
