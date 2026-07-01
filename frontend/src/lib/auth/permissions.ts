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
		'service:delete': 'app:project:service:delete',
		'service:deploy': 'app:project:service:deploy',
		'service:write': 'app:project:service:write',
		'env:write': 'app:project:service:write',
		'domain:write': 'app:project:domain:write',
		'volume:write': 'app:project:volume:write',
		'network:write': 'app:project:network:write',
		'member:manage': 'app:org:members:manage',
		'member:invite': 'app:org:members:invite',
		'settings:write': 'app:org:settings:write'
	};

	return permissions.includes(permMap[action] ?? '');
}
