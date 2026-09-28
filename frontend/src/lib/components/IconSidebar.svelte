<script lang="ts">
	import { Home, FolderOpen, Settings, Package, Anchor, PanelLeftClose, PanelLeftOpen, LogOut, User, RefreshCw, ExternalLink, Moon, Sun, Command, CreditCard, ShieldAlert } from '@lucide/svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
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

	interface NavItem {
		icon: typeof Home;
		label: string;
		href: string;
		exact?: boolean;
	}

	let navItems = $derived<NavItem[]>([
		{ icon: Home,       label: 'Home',      href: `/orgs/${orgSlug}`,          exact: true },
		{ icon: FolderOpen, label: 'Projects',  href: `/orgs/${orgSlug}/projects`              },
		{ icon: Package,    label: 'Registry',  href: `/orgs/${orgSlug}/registry`              },
		{ icon: Settings,   label: 'Settings',  href: `/orgs/${orgSlug}/settings`              }
	]);

	function isActive(item: NavItem): boolean {
		if (item.exact) return page.url.pathname === item.href;
		return page.url.pathname === item.href || page.url.pathname.startsWith(item.href + '/');
	}

	// ── Avatar menu ──────────────────────────────────────────────────────────────
	let menuOpen = $state(false);

	let userEmail     = $derived($authStore.user?.email ?? '');
	let isSuperadmin  = $derived($authStore.user?.is_superadmin === true);
	let myRole        = $derived($orgStore.myMembership?.role ?? '');
	let myPerms   = $derived($orgStore.myMembership?.permissions ?? []);
	let orgId     = $derived($orgStore.activeOrg?.id ?? '');
	let initials  = $derived(userEmail ? userEmail.slice(0, 1).toUpperCase() : 'U');
	let canUpdate = $derived(can(myRole as import('$lib/api/types').MemberRole, myPerms, perm(orgId, 'system', 'update')));
	let hasUpdate = $derived($versionStore.info?.update_available ?? false);
	let updating   = $derived($versionStore.updating);

	const ROLE_LABELS: Record<string, string> = {
		owner: 'Owner', admin: 'Admin', member: 'Member', viewer: 'Viewer'
	};

	function toggleMenu() { menuOpen = !menuOpen; }
	function closeMenu()  { menuOpen = false; }

	function openProfile() {
		closeMenu();
		goto(`/orgs/${orgSlug}/profile`);
	}

	function openBilling() {
		closeMenu();
		goto(`/orgs/${orgSlug}/billing`);
	}

	async function logout() {
		closeMenu();
		// Invalidate the HttpOnly refresh token on the server first.
		await api.logout();
		clearAuthCookies();
		authStore.logout();
		api.setToken(null);
		// Hard navigation so the root layout re-reads cookies from scratch.
		window.location.href = '/login';
	}

	// ── Dark theme toggle ────────────────────────────────────────────────────────
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
		closeMenu();
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

<aside class="icon-sidebar">
	{#if menuOpen}
		<div class="menu-backdrop" onclick={closeMenu} role="presentation"></div>
	{/if}

	<!-- Logo -->
	<div class="logo-slot">
		<div class="logo-icon" title="Shipyard">
			<Anchor size={20} />
		</div>
	</div>

	<!-- Nav icons -->
	<nav class="nav-icons">
		{#each navItems as item}
			<a
				href={item.href}
				class="nav-icon-btn"
				class:active={isActive(item)}
				title={item.label}
				aria-label={item.label}
			>
				<item.icon size={20} />
				<span class="tooltip">{item.label}</span>
			</a>
		{/each}
	</nav>

	<!-- Bottom: toggle + avatar -->
	<div class="bottom-slot">
		<button
			class="nav-icon-btn toggle-btn palette-btn"
			onclick={() => uiStore.openCommandPalette()}
			title="Command palette"
			aria-label="Open command palette"
		>
			<Command size={16} />
			<span class="tooltip">Command palette <span class="tooltip-kbd">⌘K</span></span>
		</button>

		<button
			class="nav-icon-btn toggle-btn"
			onclick={toggleTheme}
			title={isDark ? 'Light mode' : 'Dark mode'}
			aria-label={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
		>
			{#if isDark}
				<Sun size={16} />
			{:else}
				<Moon size={16} />
			{/if}
			<span class="tooltip">{isDark ? 'Light mode' : 'Dark mode'}</span>
		</button>

		<button
			class="nav-icon-btn toggle-btn"
			onclick={() => uiStore.toggleSidebar()}
			title={collapsed ? 'Show panel' : 'Hide panel'}
			aria-label={collapsed ? 'Show panel' : 'Hide panel'}
		>
			{#if collapsed}
				<PanelLeftOpen size={18} />
			{:else}
				<PanelLeftClose size={18} />
			{/if}
			<span class="tooltip">{collapsed ? 'Show panel' : 'Hide panel'}</span>
		</button>

		<div class="avatar-slot">
			<button
				class="avatar-btn"
				class:active={menuOpen}
				onclick={toggleMenu}
				title="Account"
				aria-label="Account"
				aria-haspopup="true"
				aria-expanded={menuOpen}
			>
				<span class="avatar-initials">{initials}</span>
				{#if hasUpdate}
					<span class="update-badge" aria-label="Update available"></span>
				{/if}
			</button>

			{#if menuOpen}
				<div class="avatar-menu" role="menu">
					<div class="menu-header">
						<div class="menu-avatar">{initials}</div>
						<div class="menu-info">
							<span class="menu-email" title={userEmail}>{userEmail}</span>
							{#if myRole}
								<span class="menu-role">{ROLE_LABELS[myRole] ?? myRole}</span>
							{/if}
						</div>
					</div>

					<div class="menu-divider"></div>

					{#if hasUpdate && canUpdate && $versionStore.info}
						<div class="menu-update">
							<div class="update-info">
								<span class="update-label">Update available</span>
								<span class="update-version">v{$versionStore.info.latest}</span>
							</div>
							{#if $versionStore.info.release_url}
								<a
									href={$versionStore.info.release_url}
									target="_blank"
									rel="noopener noreferrer"
									class="update-link"
									onclick={closeMenu}
									aria-label="View release notes"
								>
									<ExternalLink size={11} />
								</a>
							{/if}
							<button
								class="menu-item menu-item--update"
								onclick={runUpdate}
								disabled={updating}
								role="menuitem"
							>
								<RefreshCw size={13} class={updating ? 'spin' : ''} />
								{updating ? 'Updating…' : 'Update now'}
							</button>
						</div>
						<div class="menu-divider"></div>
					{/if}

					<button class="menu-item" onclick={openProfile} role="menuitem">
						<User size={13} />
						Profile settings
					</button>

					<button class="menu-item" onclick={openBilling} role="menuitem">
						<CreditCard size={13} />
						Billing & Plan
					</button>

					<div class="menu-divider"></div>

					{#if isSuperadmin}
						<a href="/admin" class="menu-item menu-item--superadmin" role="menuitem" onclick={closeMenu}>
							<ShieldAlert size={13} />
							Admin Panel
						</a>
						<div class="menu-divider"></div>
					{/if}

					<button class="menu-item menu-item--danger" onclick={logout} role="menuitem">
						<LogOut size={13} />
						Sign out
					</button>
				</div>
			{/if}
		</div>
	</div>
</aside>

<style>
	.icon-sidebar {
		width: var(--sidebar-width, 52px);
		flex-shrink: 0;
		background: var(--bg-surface);
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		align-items: center;
		height: 100vh;
		position: fixed;
		left: 0;
		top: 0;
		z-index: 50;
	}

	.logo-slot {
		height: 52px;
		display: flex;
		align-items: center;
		justify-content: center;
		border-bottom: 1px solid var(--border);
		width: 100%;
	}

	.logo-icon {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 32px;
		height: 32px;
		border-radius: var(--radius-md);
		background: var(--accent-muted);
		color: var(--accent);
		cursor: default;
	}

	.nav-icons {
		flex: 1;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 2px;
		padding: 10px 0;
	}

	.nav-icon-btn {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 36px;
		height: 36px;
		border-radius: var(--radius-md);
		color: var(--text-muted);
		transition: all var(--transition-fast);
		text-decoration: none;
		background: transparent;
		border: none;
		cursor: pointer;
	}

	.nav-icon-btn:hover {
		background: var(--bg-hover);
		color: var(--text-secondary);
	}

	.nav-icon-btn.active {
		background: var(--accent-muted);
		color: var(--accent);
		box-shadow: inset 2px 0 0 var(--accent);
	}

	.tooltip {
		position: absolute;
		left: calc(100% + 10px);
		top: 50%;
		transform: translateY(-50%);
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text-primary);
		font-size: 12px;
		font-weight: 500;
		padding: 5px 10px;
		border-radius: var(--radius-md);
		white-space: nowrap;
		pointer-events: none;
		opacity: 0;
		transition: opacity var(--transition-fast);
		box-shadow: var(--shadow-lg);
		z-index: 100;
	}

	.nav-icon-btn:hover .tooltip {
		opacity: 1;
	}

	.bottom-slot {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 8px 0 10px;
		border-top: 1px solid var(--border);
		width: 100%;
	}

	.toggle-btn { opacity: 0.6; }
	.toggle-btn:hover { opacity: 1; }

	.palette-btn {
		opacity: 0.75;
	}
	.palette-btn:hover {
		opacity: 1;
		background: var(--accent-muted);
		color: var(--accent);
	}

	.tooltip-kbd {
		display: inline-block;
		font-size: 9px;
		background: var(--bg-hover);
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0 4px;
		margin-left: 4px;
		font-family: var(--font-mono);
		color: var(--text-dim);
		vertical-align: middle;
	}

	/* ── Avatar ── */
	.avatar-slot {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		width: 100%;
	}

	.avatar-btn {
		width: 30px;
		height: 30px;
		border-radius: 50%;
		background: var(--accent-muted);
		border: 1.5px solid color-mix(in srgb, var(--accent) 30%, transparent);
		cursor: pointer;
		display: flex;
		align-items: center;
		justify-content: center;
		transition: all var(--transition-fast);
	}

	.avatar-btn:hover,
	.avatar-btn.active {
		background: var(--accent);
		border-color: var(--accent);
	}

	.avatar-initials {
		font-size: 11px;
		font-weight: 700;
		color: var(--accent);
		letter-spacing: 0.02em;
		pointer-events: none;
	}

	.avatar-btn:hover .avatar-initials,
	.avatar-btn.active .avatar-initials {
		color: white;
	}

	/* ── Dropdown menu ── */
	.menu-backdrop {
		position: fixed;
		inset: 0;
		z-index: 200;
	}

	.avatar-menu {
		position: fixed;
		left: 56px;
		bottom: 10px;
		width: 220px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg, 10px);
		box-shadow: var(--shadow-lg);
		z-index: 300;
		overflow: hidden;
	}

	.menu-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 12px 14px;
	}

	.menu-avatar {
		width: 32px;
		height: 32px;
		border-radius: 50%;
		background: var(--accent-muted);
		border: 1.5px solid color-mix(in srgb, var(--accent) 40%, transparent);
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 12px;
		font-weight: 700;
		color: var(--accent);
		flex-shrink: 0;
	}

	.menu-info {
		display: flex;
		flex-direction: column;
		gap: 2px;
		min-width: 0;
	}

	.menu-email {
		font-size: 12px;
		font-weight: 500;
		color: var(--text-primary);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.menu-role {
		font-size: 10px;
		font-weight: 600;
		color: var(--accent);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.menu-divider {
		height: 1px;
		background: var(--border);
	}

	.menu-item {
		display: flex;
		align-items: center;
		gap: 8px;
		width: 100%;
		padding: 9px 14px;
		background: none;
		border: none;
		cursor: pointer;
		font-size: 13px;
		color: var(--text-secondary);
		text-align: left;
		transition: background var(--transition-fast);
	}

	.menu-item:hover {
		background: var(--bg-hover);
		color: var(--text-primary);
	}

	.menu-item--danger { color: var(--accent-red); }
	.menu-item--danger:hover { background: var(--accent-red-muted); color: var(--accent-red); }

	.menu-item--superadmin { color: var(--accent-yellow); text-decoration: none; }
	.menu-item--superadmin:hover { background: var(--accent-yellow-muted); color: var(--accent-yellow); }

	.menu-item--update {
		color: var(--accent-green);
		width: 100%;
		padding: 6px 14px;
		margin-top: 4px;
	}
	.menu-item--update:hover { background: var(--accent-green-muted); color: var(--accent-green); }
	.menu-item--update:disabled { opacity: 0.55; cursor: not-allowed; }

	/* ── Update section ── */
	.update-badge {
		position: absolute;
		top: 1px;
		right: 1px;
		width: 8px;
		height: 8px;
		border-radius: 50%;
		background: var(--accent-green);
		border: 1.5px solid var(--bg-surface);
		pointer-events: none;
	}

	.avatar-btn { position: relative; }

	.menu-update {
		padding: 10px 14px 2px;
		display: flex;
		flex-direction: column;
		gap: 4px;
	}

	.update-info {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.update-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--accent-green);
		text-transform: uppercase;
		letter-spacing: 0.06em;
	}

	.update-version {
		font-size: 11px;
		color: var(--text-dim);
		font-variant-numeric: tabular-nums;
	}

	.update-link {
		display: inline-flex;
		align-items: center;
		color: var(--text-dim);
		transition: color var(--transition-fast);
		align-self: flex-start;
	}
	.update-link:hover { color: var(--text-primary); }

	:global(.spin) {
		animation: spin 1s linear infinite;
	}

	@keyframes spin {
		from { transform: rotate(0deg); }
		to   { transform: rotate(360deg); }
	}

	@media (max-width: 639px) {
		.icon-sidebar {
			width: 100%;
			height: 56px;
			top: auto;
			bottom: 0;
			left: 0;
			flex-direction: row;
			border-right: none;
			border-top: 1px solid var(--border);
			z-index: 60;
		}

		.logo-slot { display: none; }

		.bottom-slot {
			display: flex;
			flex-direction: row;
			align-items: center;
			justify-content: center;
			border-top: none;
			padding: 0;
			width: auto;
			gap: 0;
		}

		.toggle-btn { display: none; }
		.palette-btn { display: none; }

		.avatar-slot {
			width: 60px;
			height: 56px;
			display: flex;
			align-items: center;
			justify-content: center;
		}

		.nav-icons {
			flex-direction: row;
			justify-content: space-around;
			align-items: center;
			flex: 1;
			padding: 0;
			gap: 0;
		}

		.nav-icon-btn {
			flex-direction: column;
			width: auto;
			height: 100%;
			padding: 6px 20px;
			border-radius: 0;
			gap: 3px;
		}

		.tooltip {
			position: static;
			transform: none;
			opacity: 1;
			font-size: 10px;
			font-weight: 500;
			background: none;
			border: none;
			box-shadow: none;
			padding: 0;
			color: inherit;
			pointer-events: none;
		}

		.nav-icon-btn.active {
			box-shadow: none;
			border-top: 2px solid var(--accent);
		}

		.avatar-menu {
			left: auto;
			right: 10px;
			bottom: 60px;
		}
	}
</style>
