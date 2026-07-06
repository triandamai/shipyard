<script lang="ts">
	import { page } from '$app/stores';
	import { Settings2, Users, Server, Radio, Cpu, Container, KeyRound, Rocket, ShieldCheck, Database, Mail, GitBranch, Globe } from '@lucide/svelte';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { orgStore } from '$lib/stores/org.store';
	import { isAdminRole, isOwnerRole, can, perm } from '$lib/auth/permissions';

	let { children } = $props();

	let orgSlug = $derived($page.params.orgSlug ?? '');
	let currentPath = $derived($page.url.pathname);

	let myRole           = $derived($orgStore.myMembership?.role ?? null);
	let permissions      = $derived($orgStore.myMembership?.permissions ?? []);
	let isAdmin          = $derived(isAdminRole(myRole));
	let isOwner          = $derived(isOwnerRole(myRole));
	let membershipLoaded = $derived($orgStore.membershipLoaded);
	let orgId            = $derived($orgStore.activeOrg?.id ?? '');
	const SETTINGS_SUFFIXES = [
		'settings:read','settings:write','members:read','members:invite','members:manage',
		'infra:read','infra:write','docker:read','docker:write',
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

	const baseTabs = [
		{ label: 'General',     href: (slug: string) => `/orgs/${slug}/settings/general`,     icon: Settings2  },
		{ label: 'Providers',   href: (slug: string) => `/orgs/${slug}/settings/providers`,   icon: GitBranch  },
		{ label: 'Traefik',     href: (slug: string) => `/orgs/${slug}/settings/traefik`,     icon: Server     },
		{ label: 'Static',      href: (slug: string) => `/orgs/${slug}/settings/static`,      icon: Globe      },
		{ label: 'Members',     href: (slug: string) => `/orgs/${slug}/settings/members`,     icon: Users      },
		{ label: 'MQTT',        href: (slug: string) => `/orgs/${slug}/settings/mqtt`,        icon: Radio      },
		{ label: 'Infra',       href: (slug: string) => `/orgs/${slug}/settings/infra`,       icon: Cpu        },
		{ label: 'Docker',      href: (slug: string) => `/orgs/${slug}/settings/docker`,      icon: Container  },
		{ label: 'API Keys',    href: (slug: string) => `/orgs/${slug}/settings/api-keys`,    icon: KeyRound   },
		{ label: 'Deployments', href: (slug: string) => `/orgs/${slug}/settings/deployments`, icon: Rocket     },
		{ label: 'SMTP',        href: (slug: string) => `/orgs/${slug}/settings/smtp`,        icon: Mail       },
		{ label: 'Audit',       href: (slug: string) => `/orgs/${slug}/settings/audit`,       icon: ShieldCheck },
	];

	const ownerOnlyTabs = [
		{ label: 'Database',    href: (slug: string) => `/orgs/${slug}/settings/database`,     icon: Database   },
	];

	let tabs = $derived([
		...baseTabs,
		...(isOwner ? ownerOnlyTabs : []),
	]);

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
				<a
					class="tab-btn"
					class:active={isActive(href)}
					{href}
				>
					<tab.icon size={14} />
					{tab.label}
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
