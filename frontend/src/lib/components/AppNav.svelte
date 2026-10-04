<script lang="ts">
	import { Home, FolderOpen, Settings, Package, PanelLeftClose, PanelLeftOpen, LogOut, User, RefreshCw, ExternalLink, Moon, Sun, Command, CreditCard, ShieldAlert } from '@lucide/svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { NavRail, Dropdown, Button } from '$lib/components/ui';
	import { uiStore } from '$lib/stores/ui.store';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { versionStore } from '$lib/stores/version.store';
	import { toastStore } from '$lib/stores/toast.store';
	import { api } from '$lib/api/client';
	import { clearAuthCookies } from '$lib/auth/cookies';
	import { can, perm } from '$lib/auth/permissions';

	interface Props {
		orgSlug: string;
	}

	let { orgSlug }: Props = $props();

	let collapsed = $derived($uiStore.sidebarCollapsed);

	function isActive(href: string, exact = false): boolean {
		if (exact) return page.url.pathname === href;
		return page.url.pathname === href || page.url.pathname.startsWith(href + '/');
	}

	let groups = $derived([
		{ key: 'home',     icon: Home,       label: 'Home',     href: `/orgs/${orgSlug}`,          active: isActive(`/orgs/${orgSlug}`, true) },
		{ key: 'projects', icon: FolderOpen, label: 'Projects', href: `/orgs/${orgSlug}/projects`, active: isActive(`/orgs/${orgSlug}/projects`) },
		{ key: 'registry', icon: Package,    label: 'Registry', href: `/orgs/${orgSlug}/registry`, active: isActive(`/orgs/${orgSlug}/registry`) },
		{ key: 'settings', icon: Settings,   label: 'Settings', href: `/orgs/${orgSlug}/settings`, active: isActive(`/orgs/${orgSlug}/settings`) }
	]);

	let userEmail    = $derived($authStore.user?.email ?? '');
	let isSuperadmin = $derived($authStore.user?.is_superadmin === true);
	let myRole       = $derived($orgStore.myMembership?.role ?? '');
	let myPerms      = $derived($orgStore.myMembership?.permissions ?? []);
	let orgId        = $derived($orgStore.activeOrg?.id ?? '');
	let initials     = $derived(userEmail ? userEmail.slice(0, 1).toUpperCase() : 'U');
	let canUpdate    = $derived(can(myRole as import('$lib/api/types').MemberRole, myPerms, perm(orgId, 'system', 'update')));
	let hasUpdate    = $derived($versionStore.info?.update_available ?? false);
	let updating     = $derived($versionStore.updating);

	const ROLE_LABELS: Record<string, string> = {
		owner: 'Owner', admin: 'Admin', member: 'Member', viewer: 'Viewer'
	};

	async function logout() {
		// Invalidate the HttpOnly refresh token on the server first.
		await api.logout();
		clearAuthCookies();
		authStore.logout();
		api.setToken(null);
		// Hard navigation so the root layout re-reads cookies from scratch.
		window.location.href = '/login';
	}

	let isDark = $state(false);

	$effect(() => {
		isDark = localStorage.getItem('shipyard_theme') === 'dark';
		document.documentElement.setAttribute('data-theme', isDark ? 'dark' : '');
	});

	function toggleTheme() {
		isDark = !isDark;
		localStorage.setItem('shipyard_theme', isDark ? 'dark' : 'light');
		document.documentElement.setAttribute('data-theme', isDark ? 'dark' : '');
	}

	async function runUpdate() {
		if (updating) return;
		versionStore.setUpdating(true);
		toastStore.add({ type: 'info', title: 'Update started', message: 'Pulling latest images…' });
		const res = await api.triggerUpdate();
		if (res.error) {
			versionStore.setUpdateError(res.error.message);
			toastStore.add({ type: 'error', title: 'Update failed', message: res.error.message });
		} else {
			versionStore.clearUpdate();
			// Invalidate local version info so next check reflects new version.
			versionStore.setInfo(null);
			toastStore.add({ type: 'success', title: 'Update complete', message: res.data?.message ?? 'Restart services to apply.' });
		}
	}
</script>

<NavRail {groups} phoneBar>
	{#snippet footer()}
		<span class="app-nav-hide-phone">
			<Button variant="ghost" size="icon" aria-label="Open command palette" onclick={() => uiStore.openCommandPalette()}>
				<Command size={16} />
			</Button>
		</span>
		<Button variant="ghost" size="icon" aria-label={isDark ? 'Switch to light mode' : 'Switch to dark mode'} onclick={toggleTheme}>
			{#if isDark}<Sun size={16} />{:else}<Moon size={16} />{/if}
		</Button>
		<span class="app-nav-hide-phone">
			<Button variant="ghost" size="icon" aria-label={collapsed ? 'Show panel' : 'Hide panel'} onclick={() => uiStore.toggleSidebar()}>
				{#if collapsed}<PanelLeftOpen size={18} />{:else}<PanelLeftClose size={18} />{/if}
			</Button>
		</span>
		<div class="app-nav-account">
			<Dropdown placement="right-end" triggerLabel={hasUpdate ? 'Account, update available' : 'Account'}>
				{#snippet trigger()}
					<span class="app-nav-avatar">
						{initials}
						{#if hasUpdate}<span class="app-nav-update-dot" aria-label="Update available"></span>{/if}
					</span>
				{/snippet}
				{#snippet menu(close)}
					<div class="ui-dropdown-header app-nav-menu-header">
						<span class="app-nav-avatar app-nav-avatar--lg">{initials}</span>
						<span class="app-nav-menu-info">
							<span class="app-nav-menu-email" title={userEmail}>{userEmail}</span>
							{#if myRole}<span class="app-nav-menu-role">{ROLE_LABELS[myRole] ?? myRole}</span>{/if}
						</span>
					</div>
					<div class="ui-dropdown-sep"></div>
					{#if hasUpdate && canUpdate && $versionStore.info}
						<div class="ui-dropdown-header app-nav-update">
							<span class="app-nav-update-label">Update available</span>
							<span class="app-nav-update-version">v{$versionStore.info.latest}</span>
						</div>
						{#if $versionStore.info.release_url}
							<a
								class="ui-dropdown-item"
								role="menuitem"
								href={$versionStore.info.release_url}
								target="_blank"
								rel="noopener noreferrer"
								onclick={close}
							>
								<ExternalLink size={13} /> Release notes
							</a>
						{/if}
						<button class="ui-dropdown-item" role="menuitem" disabled={updating} onclick={() => { close(); runUpdate(); }}>
							<RefreshCw size={13} /> {updating ? 'Updating…' : 'Update now'}
						</button>
						<div class="ui-dropdown-sep"></div>
					{/if}
					<button class="ui-dropdown-item" role="menuitem" onclick={() => { close(); goto(`/orgs/${orgSlug}/profile`); }}>
						<User size={13} /> Profile settings
					</button>
					<button class="ui-dropdown-item" role="menuitem" onclick={() => { close(); goto(`/orgs/${orgSlug}/billing`); }}>
						<CreditCard size={13} /> Billing & Plan
					</button>
					<div class="ui-dropdown-sep"></div>
					{#if isSuperadmin}
						<a class="ui-dropdown-item" role="menuitem" href="/admin" onclick={close}>
							<ShieldAlert size={13} /> Admin Panel
						</a>
						<div class="ui-dropdown-sep"></div>
					{/if}
					<button class="ui-dropdown-item ui-dropdown-item--danger" role="menuitem" onclick={() => { close(); logout(); }}>
						<LogOut size={13} /> Sign out
					</button>
				{/snippet}
			</Dropdown>
		</div>
	{/snippet}
</NavRail>

<style>
	.app-nav-account { margin-top: 6px; }
	.app-nav-avatar {
		position: relative;
		width: 32px;
		height: 32px;
		border-radius: 50%;
		background: var(--accent-muted);
		color: var(--accent);
		font-size: 12px;
		font-weight: 700;
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
	.app-nav-avatar--lg { width: 34px; height: 34px; }
	.app-nav-update-dot {
		position: absolute;
		top: 0;
		right: 0;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--accent-yellow);
		border: 2px solid var(--bg-surface);
	}
	.app-nav-menu-header { display: flex; align-items: center; gap: 10px; min-width: 220px; }
	.app-nav-menu-info { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
	.app-nav-menu-email { font-size: 12.5px; font-weight: 600; color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 170px; }
	.app-nav-menu-role { font-size: 11px; color: var(--text-muted); }
	.app-nav-update { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	.app-nav-update-label { font-size: 12px; font-weight: 600; color: var(--accent-yellow); }
	.app-nav-update-version { font-size: 11px; font-family: var(--font-mono); color: var(--text-muted); }

	@media (max-width: 639px) {
		.app-nav-hide-phone { display: none; }
		.app-nav-account { margin-top: 0; }
		.app-nav-account :global(.ui-dropdown-menu) {
			left: auto;
			right: 0;
			bottom: calc(100% + 10px);
			top: auto;
		}
	}
</style>
