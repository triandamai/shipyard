import type { MemberRole } from '$lib/api/types';

const ADMIN_ROLES: MemberRole[] = ['owner', 'admin'];

/** True if role is owner or admin. */
export function isAdminRole(role: MemberRole | null | undefined): boolean {
	return role === 'owner' || role === 'admin';
}

/** True only for owner. */
export function isOwnerRole(role: MemberRole | null | undefined): boolean {
	return role === 'owner';
}

/**
 * Gate for a specific action.
 * Owners and admins pass everything.
 * Members/viewers need an explicit permission string.
 */
export function can(
	role: MemberRole | null | undefined,
	permissions: string[],
	action:
		| 'project:read'
		| 'project:write'
		| 'project:delete'
		| 'service:delete'
		| 'service:deploy'
		| 'service:write'
		| 'env:write'
		| 'domain:write'
		| 'volume:write'
		| 'network:write'
		| 'member:manage'
		| 'member:invite'
		| 'settings:write'
): boolean {
	if (!role) return false;
	if (role === 'owner' || role === 'admin') return true;

	const permMap: Record<string, string> = {
		'project:read':   'app:project:read',
		'project:write':  'app:project:write',
		'project:delete': 'app:project:delete',
		'service:delete': 'app:project:service:delete',
		'service:deploy': 'app:project:service:deploy',
		'service:write':  'app:project:service:write',
		'env:write':      'app:project:service:write',
		'domain:write':   'app:project:domain:write',
		'volume:write':   'app:project:volume:write',
		'network:write':  'app:project:network:write',
		'member:manage':  'app:org:members:manage',
		'member:invite':  'app:org:members:invite',
		'settings:write': 'app:org:settings:write'
	};

	return permissions.includes(permMap[action] ?? '');
}

/**
 * True if the user can view a specific project.
 * Accepts org-level project:read/write OR a project-scoped permission (orgs:<id>:view/deploy/manage).
 */
export function hasProjectAccess(
	role: MemberRole | null | undefined,
	permissions: string[],
	projectId: string
): boolean {
	if (!role) return false;
	if (role === 'owner' || role === 'admin') return true;
	if (permissions.includes('app:project:read') || permissions.includes('app:project:write')) return true;
	if (!projectId) return false;
	return permissions.some(p => p.startsWith(`orgs:${projectId}:`));
}

/**
 * True if the user can edit (write/manage) a specific project.
 * Accepts org-level project:write OR a project-scoped manage permission (orgs:<id>:manage).
 */
export function hasProjectEditAccess(
	role: MemberRole | null | undefined,
	permissions: string[],
	projectId: string
): boolean {
	if (!role) return false;
	if (role === 'owner' || role === 'admin') return true;
	if (permissions.includes('app:project:write')) return true;
	if (!projectId) return false;
	return permissions.includes(`orgs:${projectId}:manage`);
}
