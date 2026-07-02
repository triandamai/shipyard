<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { Settings2, Users, Server, Radio, Cpu } from '@lucide/svelte';
	import { orgStore } from '$lib/stores/org.store';
	import { isAdminRole } from '$lib/auth/permissions';

	let { children } = $props();

	let orgSlug = $derived($page.params.orgSlug);
	let currentPath = $derived($page.url.pathname);

	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let isAdmin  = $derived(isAdminRole(myRole));

	// Redirect non-admins away from settings (members and general write ops)
	$effect(() => {
		if (myRole && !isAdmin) {
			goto(`/orgs/${orgSlug}`);
		}
	});

	const tabs = [
		{ label: 'General',  href: (slug: string) => `/orgs/${slug}/settings/general`, icon: Settings2 },
		{ label: 'Traefik',  href: (slug: string) => `/orgs/${slug}/settings/traefik`,  icon: Server },
		{ label: 'Members',  href: (slug: string) => `/orgs/${slug}/settings/members`, icon: Users },
		{ label: 'MQTT',     href: (slug: string) => `/orgs/${slug}/settings/mqtt`,    icon: Radio },
		{ label: 'Infra',    href: (slug: string) => `/orgs/${slug}/settings/infra`,   icon: Cpu },
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

	<div class="settings-content">
		{@render children()}
	</div>
</div>

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
	}

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
		.settings-content { padding: 16px 16px 24px; }
		.tab-bar { overflow-x: auto; overflow-y: hidden; flex-wrap: nowrap; -webkit-overflow-scrolling: touch; scrollbar-width: none; }
		.tab-bar::-webkit-scrollbar { display: none; }
		.tab-btn { white-space: nowrap; }
	}
</style>
