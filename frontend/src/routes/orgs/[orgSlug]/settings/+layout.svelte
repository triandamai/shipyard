<script lang="ts">
	import { page } from '$app/stores';
	import { Settings2, Users, KeyRound, Rocket, ShieldCheck, GitBranch, Cloud } from '@lucide/svelte';
	import PageHeader from '$lib/components/ui/PageHeader.svelte';
	import Tabs from '$lib/components/ui/Tabs.svelte';
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
		{ label: 'Cloudflare',  href: (slug: string) => `/orgs/${slug}/settings/cloudflare`,  icon: Cloud,       badge: 'admin' },
		{ label: 'Members',     href: (slug: string) => `/orgs/${slug}/settings/members`,     icon: Users,       badge: 'admin' },
		{ label: 'API Keys',    href: (slug: string) => `/orgs/${slug}/settings/api-keys`,    icon: KeyRound,    badge: 'admin' },
		{ label: 'Deployments', href: (slug: string) => `/orgs/${slug}/settings/deployments`, icon: Rocket,      badge: null    },
		{ label: 'Audit',       href: (slug: string) => `/orgs/${slug}/settings/audit`,       icon: ShieldCheck, badge: null    },
	];

	function isActive(tabHref: string) {
		return currentPath === tabHref || currentPath.startsWith(tabHref + '/');
	}

	let tabItems = $derived(
		tabs.map((tab) => ({
			id: tab.label,
			label: tab.label,
			href: tab.href(orgSlug),
			icon: tab.icon,
			// Same as before: the Admin pill only shows to non-admins on admin-only tabs.
			badge: tab.badge === 'admin' && !isAdmin ? 'Admin' : undefined,
		}))
	);
	let activeTab = $derived(tabItems.find((t) => isActive(t.href))?.id ?? '');
</script>

<div class="settings-layout">
	<div class="settings-header">
		<PageHeader
			title="Settings"
			subtitle="Organization configuration and team management"
		/>
		<Tabs tabs={tabItems} value={activeTab} ariaLabel="Settings sections" />
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

	.settings-header :global(.ui-page-header) { margin-bottom: 0; }

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
