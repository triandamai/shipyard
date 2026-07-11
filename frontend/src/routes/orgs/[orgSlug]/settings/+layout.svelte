<script lang="ts">
	import { page } from '$app/stores';
	import { Settings2, Users, KeyRound, Rocket, ShieldCheck, GitBranch } from '@lucide/svelte';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { orgStore } from '$lib/stores/org.store';
	import { isAdminRole, can, perm } from '$lib/auth/permissions';

	let { children } = $props();

	let orgSlug = $derived($page.params.orgSlug ?? '');
	let currentPath = $derived($page.url.pathname);

	let myRole           = $derived($orgStore.myMembership?.role ?? null);
	let permissions      = $derived($orgStore.myMembership?.permissions ?? []);
	let isAdmin          = $derived(isAdminRole(myRole));
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let orgId            = $derived($orgStore.activeOrg?.id ?? '');
	const SETTINGS_SUFFIXES = [
		'settings:read','settings:write','members:read','members:invite','members:manage',
		'providers:read','providers:write',
		'infra:read','infra:write','static:read',
		'docker:read','docker:write',
		'deployments:read','deployments:write','smtp:read','smtp:write',
		'audit:read','keys:read','keys:write','system:update',
	];
	// Allow access if admin/owner OR if the member has any settings-area permission.
	let canViewSettings = $derived(
		isAdmin ||
		permissions.some(p =>
			p.startsWith(`shipyard:${orgId}:`) &&
			SETTINGS_SUFFIXES.some(suffix => p.endsWith(`:${suffix}`))
		)
	);

	type TabBadge = 'admin' | null;

	const tabs: { label: string; href: (slug: string) => string; icon: typeof Settings2; badge: TabBadge }[] = [
		{ label: 'General',     href: (slug: string) => `/orgs/${slug}/settings/general`,     icon: Settings2,   badge: null    },
		{ label: 'Providers',   href: (slug: string) => `/orgs/${slug}/settings/providers`,   icon: GitBranch,   badge: 'admin' },
		{ label: 'Members',     href: (slug: string) => `/orgs/${slug}/settings/members`,     icon: Users,       badge: 'admin' },
		{ label: 'API Keys',    href: (slug: string) => `/orgs/${slug}/settings/api-keys`,    icon: KeyRound,    badge: 'admin' },
		{ label: 'Deployments', href: (slug: string) => `/orgs/${slug}/settings/deployments`, icon: Rocket,      badge: null    },
		{ label: 'Audit',       href: (slug: string) => `/orgs/${slug}/settings/audit`,       icon: ShieldCheck, badge: null    },
	];

	function isActive(tabHref: string) {
		return currentPath === tabHref || currentPath.startsWith(tabHref + '/');
	}
</script>

<div class="settings-layout">
	<div class="settings-header">
		<div class="page-header">
			<h1 class="page-title">Settings</h1>
			<p class="page-subtitle">Organization configuration and team management</p>
		</div>
		<nav class="tab-bar">
			{#each tabs as tab}
				{@const href = tab.href(orgSlug)}
				{@const restricted = tab.badge === 'admin' && !isAdmin}
				<a
					class="tab-btn"
					class:active={isActive(href)}
					class:tab-restricted={restricted}
					{href}
					title={restricted ? 'Admin or Owner only' : undefined}
				>
					<tab.icon size={14} />
					{tab.label}
					{#if tab.badge === 'admin' && !isAdmin}
						<span class="tab-badge tab-badge--admin">Admin</span>
					{/if}
				</a>
			{/each}
		</nav>
	</div>

	{#if !membershipLoaded || canViewSettings}
		<div class="settings-content">
			{@render children()}
		</div>
	{/if}
</div>

<PermissionDeniedDialog
	open={membershipLoaded && !!orgId && !canViewSettings}
	message="You need settings permission to view organization settings."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

<style>
	.settings-layout {
		height: 100%;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	.settings-header {
		flex-shrink: 0;
		padding: 28px 32px 0;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.page-header { display: flex; flex-direction: column; gap: 2px; }
	.page-title { font-size: 22px; font-weight: 700; color: var(--text-primary); letter-spacing: -0.02em; margin: 0; }
	.page-subtitle { font-size: 13px; color: var(--text-muted); margin: 0; }

	.tab-bar {
		display: flex;
		gap: 2px;
		border-bottom: 1px solid var(--border);
		overflow-x: auto;
		overflow-y: hidden;
		flex-wrap: nowrap;
		-webkit-overflow-scrolling: touch;
		scrollbar-width: none;
	}
	.tab-bar::-webkit-scrollbar { display: none; }

	.tab-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 9px 14px;
		font-size: 13px;
		font-weight: 500;
		font-family: var(--font-sans);
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--text-muted);
		cursor: pointer;
		margin-bottom: -1px;
		text-decoration: none;
		white-space: nowrap;
		transition: color var(--transition-fast), border-color var(--transition-fast);
	}
	.tab-btn:hover { color: var(--text-primary); }
	.tab-btn.active { color: var(--accent); border-bottom-color: var(--accent); }

	/* Restricted tabs: dim but still navigable — server will enforce permissions */
	.tab-btn.tab-restricted { opacity: 0.55; }
	.tab-btn.tab-restricted:hover { opacity: 0.8; }

	/* Role / platform badge pills inside a tab */
	.tab-badge {
		font-size: 9px;
		font-weight: 700;
		padding: 1px 5px;
		border-radius: 999px;
		letter-spacing: 0.04em;
		text-transform: uppercase;
		flex-shrink: 0;
		line-height: 1.6;
	}
	.tab-badge--admin {
		background: rgba(99,102,241,0.1);
		color: #6366F1;
		border: 1px solid rgba(99,102,241,0.25);
	}

	.settings-content {
		flex: 1;
		overflow-y: auto;
		padding: 24px 32px 32px;
	}

	@media (max-width: 639px) {
		.settings-header { padding: 16px 16px 0; }
		.settings-content { padding: 16px 16px 80px; }
	}

</style>
