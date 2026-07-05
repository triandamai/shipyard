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

// ── Permission string builders ────────────────────────────────────────────────

/** Build an org-level permission string: `shipyard:<orgId>:<resource>:<action>` */
export const perm = (orgId: string, resource: string, action: string): string =>
	`shipyard:${orgId}:${resource}:${action}`;

/** Build a project-level permission string: `shipyard:<orgId>:<projectId>:<resource>:<action>` */
export const permProject = (orgId: string, projectId: string, resource: string, action: string): string =>
	`shipyard:${orgId}:${projectId}:${resource}:${action}`;

// ── Generic permission check ──────────────────────────────────────────────────

/**
 * Gate for a specific exact permission string.
 * Owners and admins bypass all checks.
 * Members/viewers need the string explicitly in their permissions list.
 */
export function can(
	role: MemberRole | null | undefined,
	permissions: string[],
	permission: string
): boolean {
	if (!role) return false;
	if (role === 'owner' || role === 'admin') return true;
	return permissions.includes(permission);
}

// ── Project-scoped helpers ────────────────────────────────────────────────────

/**
 * True if the user can VIEW a specific project.
 * Accepts:
 *   - admin/owner role (bypass)
 *   - org-level `shipyard:<orgId>:projects:read` or `projects:write`
 *   - any project-scoped `shipyard:<orgId>:<projectId>:*` permission
 */
export function hasProjectAccess(
	role: MemberRole | null | undefined,
	permissions: string[],
	orgId: string,
	projectId: string
): boolean {
	if (!role) return false;
	if (role === 'owner' || role === 'admin') return true;
	if (
		permissions.includes(perm(orgId, 'projects', 'read')) ||
		permissions.includes(perm(orgId, 'projects', 'write'))
	) return true;
	if (!projectId) return false;
	const prefix = `shipyard:${orgId}:${projectId}:`;
	return permissions.some(p => p.startsWith(prefix));
}

/**
 * True if the user can EDIT (write/manage) a specific project.
 * Accepts:
 *   - admin/owner role (bypass)
 *   - org-level `shipyard:<orgId>:projects:write`
 *   - project-scoped `shipyard:<orgId>:<projectId>:project:manage`
 */
export function hasProjectEditAccess(
	role: MemberRole | null | undefined,
	permissions: string[],
	orgId: string,
	projectId: string
): boolean {
	if (!role) return false;
	if (role === 'owner' || role === 'admin') return true;
	if (permissions.includes(perm(orgId, 'projects', 'write'))) return true;
	if (!projectId) return false;
	return permissions.includes(permProject(orgId, projectId, 'project', 'manage'));
}

/**
 * True if the user can DEPLOY services in a specific project.
 */
export function hasServiceDeployAccess(
	role: MemberRole | null | undefined,
	permissions: string[],
	orgId: string,
	projectId: string
): boolean {
	if (!role) return false;
	if (role === 'owner' || role === 'admin') return true;
	if (!projectId) return false;
	return permissions.includes(permProject(orgId, projectId, 'service', 'deploy'));
}
