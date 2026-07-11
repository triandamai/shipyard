<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';

	let { children } = $props();
	let checking      = $state(true);
	let theme         = $state<'light' | 'dark'>('light');
	let collapsed     = $state(false);
	let mobileOpen    = $state(false);

	onMount(async () => {
		const savedTheme = localStorage.getItem('shipyard-admin-theme');
		if (savedTheme === 'dark' || savedTheme === 'light') theme = savedTheme;
		else if (window.matchMedia('(prefers-color-scheme: dark)').matches) theme = 'dark';

		const savedCollapsed = localStorage.getItem('shipyard-admin-sidebar');
		if (savedCollapsed === '1') collapsed = true;

		let storeVal: typeof $authStore;
		const unsub = authStore.subscribe((v) => (storeVal = v));
		unsub();
		const cached = storeVal!;
		if (cached.isAuthenticated && (cached.user?.is_superadmin || (cached.user?.staff_permissions?.length ?? 0) > 0)) {
			checking = false;
			return;
		}
		const res = await api.getMe();
		const u = res.data;
		if (!u?.is_superadmin && !((u?.staff_permissions?.length ?? 0) > 0)) { goto('/orgs'); return; }
		checking = false;
	});

	function toggleTheme() {
		theme = theme === 'light' ? 'dark' : 'light';
		localStorage.setItem('shipyard-admin-theme', theme);
	}
	function toggleSidebar() {
		collapsed = !collapsed;
		localStorage.setItem('shipyard-admin-sidebar', collapsed ? '1' : '0');
	}
	function closeMobile() { mobileOpen = false; }

	type NavItem  = { href: string; label: string; d: string };
	type NavGroup = { label: string; items: NavItem[] };

	const navGroups: NavGroup[] = [
		{
			label: 'Platform',
			items: [
				{ href: '/admin',              label: 'Overview',      d: 'M10.707 2.293a1 1 0 00-1.414 0l-7 7a1 1 0 001.414 1.414L4 10.414V17a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 011-1h2a1 1 0 011 1v2a1 1 0 001 1h2a1 1 0 001-1v-6.586l.293.293a1 1 0 001.414-1.414l-7-7z' },
				{ href: '/admin/orgs',         label: 'Organizations', d: 'M4 4a2 2 0 012-2h8a2 2 0 012 2v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4zm3 1h2v2H7V5zm2 4H7v2h2V9zm2-4h2v2h-2V5zm2 4h-2v2h2V9z' },
				{ href: '/admin/users',        label: 'Users',         d: 'M9 6a3 3 0 11-6 0 3 3 0 016 0zM17 6a3 3 0 11-6 0 3 3 0 016 0zM12.93 17c.046-.327.07-.66.07-1a6.97 6.97 0 00-1.5-4.33A5 5 0 0119 16v1h-6.07zM6 11a5 5 0 015 5v1H1v-1a5 5 0 015-5z' },
				{ href: '/admin/staff',        label: 'Staff',         d: 'M13 6a3 3 0 11-6 0 3 3 0 016 0zM18 8a2 2 0 11-4 0 2 2 0 014 0zM14 15a4 4 0 00-8 0v3h8v-3zM6 8a2 2 0 11-4 0 2 2 0 014 0zM16 18v-3a5.972 5.972 0 00-.75-2.906A3.005 3.005 0 0119 15v3h-3zM4.75 12.094A5.973 5.973 0 004 15v3H1v-3a3 3 0 013.75-2.906z' },
				{ href: '/admin/projects',     label: 'Projects',      d: 'M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z' },
				{ href: '/admin/deployments',  label: 'Deployments',   d: 'M11.3 1.046A1 1 0 0112 2v5h4a1 1 0 01.82 1.573l-7 10A1 1 0 018 18v-5H4a1 1 0 01-.82-1.573l7-10a1 1 0 011.12-.38z' },
				{ href: '/admin/deployments/provisioning', label: 'Provisioning', d: 'M5 3a2 2 0 00-2 2v2a2 2 0 002 2h2a2 2 0 002-2V5a2 2 0 00-2-2H5zM5 11a2 2 0 00-2 2v2a2 2 0 002 2h2a2 2 0 002-2v-2a2 2 0 00-2-2H5zM11 5a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V5zM11 13a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2v-2z' },
				{ href: '/admin/nodes',        label: 'Compute',       d: 'M2 5a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2V5zm14 1a1 1 0 11-2 0 1 1 0 012 0zM2 13a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2v-2zm14 1a1 1 0 11-2 0 1 1 0 012 0z' },
			]
		},
		{
			label: 'Infrastructure',
			items: [
				{ href: '/admin/infra',   label: 'System',       d: 'M2 9a2 2 0 012-2h.93a2 2 0 001.664-.89l.812-1.22A2 2 0 019.07 4h1.86a2 2 0 011.664.89l.812 1.22A2 2 0 0015.07 7H16a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V9zm6 2a2 2 0 100 4 2 2 0 000-4zm0-1a3 3 0 110 6 3 3 0 010-6z' },
				{ href: '/admin/docker',  label: 'Docker',       d: 'M2 5a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2V5zm14 1a1 1 0 11-2 0 1 1 0 012 0zM2 13a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2v-2zm14 1a1 1 0 11-2 0 1 1 0 012 0z' },
				{ href: '/admin/traefik', label: 'Traefik',      d: 'M13 7H7v6h6V7z M2 3a1 1 0 011-1h14a1 1 0 011 1v14a1 1 0 01-1 1H3a1 1 0 01-1-1V3z' },
				{ href: '/admin/mqtt',    label: 'MQTT',         d: 'M5 12a1 1 0 01-1-1V7a1 1 0 012 0v4a1 1 0 01-1 1zm5 4a1 1 0 01-1-1V3a1 1 0 012 0v12a1 1 0 01-1 1zm5-8a1 1 0 01-1-1V7a1 1 0 012 0v0a1 1 0 01-1 1z' },
				{ href: '/admin/static',  label: 'Static Sites', d: 'M2 6a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2V6zM4 12h12v2a2 2 0 01-2 2H6a2 2 0 01-2-2v-2zm8-2a1 1 0 000 2h2a1 1 0 000-2h-2z' },
			]
		},
		{
			label: 'Services',
			items: [
				{ href: '/admin/smtp',     label: 'SMTP',       d: 'M2.003 5.884L10 9.882l7.997-3.998A2 2 0 0016 4H4a2 2 0 00-1.997 1.884zM18 8.118l-8 4-8-4V14a2 2 0 002 2h12a2 2 0 002-2V8.118z' },
				{ href: '/admin/database', label: 'Database',   d: 'M3 12v3c0 1.657 3.134 3 7 3s7-1.343 7-3v-3c0 1.657-3.134 3-7 3s-7-1.343-7-3zm0-5v3c0 1.657 3.134 3 7 3s7-1.343 7-3V7c0 1.657-3.134 3-7 3S3 8.657 3 7zm7-5C6.134 2 3 3.343 3 5s3.134 3 7 3 7-1.343 7-3-3.134-3-7-3z' },
				{ href: '/admin/audit',    label: 'Audit Log',  d: 'M9 2a1 1 0 000 2h2a1 1 0 100-2H9z M4 5a2 2 0 012-2 3 3 0 003 3h2a3 3 0 003-3 2 2 0 012 2v11a2 2 0 01-2 2H6a2 2 0 01-2-2V5zm3 4a1 1 0 000 2h.01a1 1 0 100-2H7zm3 0a1 1 0 000 2h3a1 1 0 100-2h-3zm-3 4a1 1 0 100 2h.01a1 1 0 100-2H7zm3 0a1 1 0 100 2h3a1 1 0 100-2h-3z' },
				{ href: '/admin/plan',     label: 'Plans',      d: 'M5 3a2 2 0 00-2 2v2a2 2 0 002 2h2a2 2 0 002-2V5a2 2 0 00-2-2H5zM5 11a2 2 0 00-2 2v2a2 2 0 002 2h2a2 2 0 002-2v-2a2 2 0 00-2-2H5zM11 5a2 2 0 012-2h2a2 2 0 012 2v2a2 2 0 01-2 2h-2a2 2 0 01-2-2V5zM14 11a1 1 0 011 1v1h1a1 1 0 110 2h-1v1a1 1 0 11-2 0v-1h-1a1 1 0 110-2h1v-1a1 1 0 011-1z' },
				{ href: '/admin/payments', label: 'Payments',   d: 'M4 4a2 2 0 00-2 2v1h16V6a2 2 0 00-2-2H4zM2 9h16v5a2 2 0 01-2 2H4a2 2 0 01-2-2V9zm4 3a1 1 0 011-1h1a1 1 0 110 2H7a1 1 0 01-1-1zm5 0a1 1 0 011-1h2a1 1 0 110 2h-2a1 1 0 01-1-1z' },
				{ href: '/admin/updates',  label: 'Updates',    d: 'M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z' },
				{ href: '/admin/config',   label: 'Config',     d: 'M11.49 3.17c-.38-1.56-2.6-1.56-2.98 0a1.532 1.532 0 01-2.286.948c-1.372-.836-2.942.734-2.106 2.106.54.886.061 2.042-.947 2.287-1.561.379-1.561 2.6 0 2.978a1.532 1.532 0 01.947 2.287c-.836 1.372.734 2.942 2.106 2.106a1.532 1.532 0 012.287.947c.379 1.561 2.6 1.561 2.978 0a1.533 1.533 0 012.287-.947c1.372.836 2.942-.734 2.106-2.106a1.533 1.533 0 01.947-2.287c1.561-.379 1.561-2.6 0-2.978a1.532 1.532 0 01-.947-2.287c.836-1.372-.734-2.942-2.106-2.106a1.532 1.532 0 01-2.287-.947zM10 13a3 3 0 100-6 3 3 0 000 6z' },
			]
		},
	];

	function isActive(href: string): boolean {
		if (href === '/admin') return $page.url.pathname === '/admin';
		return $page.url.pathname.startsWith(href + '/') || $page.url.pathname === href;
	}

	let userEmail   = $derived($authStore.user?.email ?? '');
	let userInitial = $derived(userEmail ? userEmail[0].toUpperCase() : 'A');
</script>

{#if checking}
	<div class="gate">
		<div class="gate-ring"></div>
	</div>
{:else}
	{#if mobileOpen}
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="mob-backdrop" onclick={closeMobile}></div>
	{/if}

	<div class="shell" data-theme={theme}>
		<aside class="sidebar" class:collapsed class:mob-open={mobileOpen}>
			<!-- Brand -->
			<div class="brand">
				<div class="brand-icon">
					<svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" width="16" height="16">
						<circle cx="12" cy="5" r="3"/><line x1="12" y1="22" x2="12" y2="8"/><path d="M5 12H2a10 10 0 0 0 20 0h-3"/>
					</svg>
				</div>
				<div class="brand-text">
					<span class="brand-name">Shipyard</span>
					<span class="brand-role">Admin</span>
				</div>
			</div>

			<!-- Nav groups -->
			<div class="nav-scroll">
				{#each navGroups as group}
					<div class="nav-section-label">{group.label}</div>
					<nav class="nav">
						{#each group.items as item}
							<a
								href={item.href}
								class="nav-item"
								class:active={isActive(item.href)}
								title={collapsed ? item.label : undefined}
								onclick={closeMobile}
							>
								<svg viewBox="0 0 20 20" fill="currentColor" class="nav-icon" aria-hidden="true">
									<path fill-rule="evenodd" clip-rule="evenodd" d={item.d} />
								</svg>
								<span class="nav-label">{item.label}</span>
								{#if isActive(item.href)}<span class="pip"></span>{/if}
							</a>
						{/each}
					</nav>
				{/each}
			</div>

			<!-- Footer -->
			<div class="sidebar-footer">
				<div class="user">
					<div class="user-dot" title={userEmail}>{userInitial}</div>
					<span class="user-label">{userEmail}</span>
				</div>
				<div class="footer-btns">
					<button class="ftr-btn" onclick={toggleTheme} aria-label={theme === 'dark' ? 'Light mode' : 'Dark mode'} title={theme === 'dark' ? 'Light mode' : 'Dark mode'}>
						{#if theme === 'dark'}
							<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
								<path fill-rule="evenodd" d="M10 2a1 1 0 011 1v1a1 1 0 11-2 0V3a1 1 0 011-1zm4 8a4 4 0 11-8 0 4 4 0 018 0zm-.464 4.95l.707.707a1 1 0 01-1.414 1.414l-.707-.707a1 1 0 011.414-1.414zm2.12-10.607a1 1 0 010 1.414l-.706.707a1 1 0 11-1.414-1.414l.707-.707a1 1 0 011.414 0zM17 11a1 1 0 100-2h-1a1 1 0 100 2h1zm-7 4a1 1 0 011 1v1a1 1 0 11-2 0v-1a1 1 0 011-1zM5.05 6.464A1 1 0 106.465 5.05l-.708-.707a1 1 0 00-1.414 1.414l.707.707zm1.414 8.486l-.707.707a1 1 0 01-1.414-1.414l.707-.707a1 1 0 011.414 1.414zM4 11a1 1 0 100-2H3a1 1 0 000 2h1z" clip-rule="evenodd"/>
							</svg>
						{:else}
							<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
								<path d="M17.293 13.293A8 8 0 016.707 2.707a8.001 8.001 0 1010.586 10.586z"/>
							</svg>
						{/if}
					</button>
					<a href="/orgs" class="ftr-btn" aria-label="Back to dashboard" title="Exit admin" onclick={closeMobile}>
						<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
							<path fill-rule="evenodd" d="M9.707 16.707a1 1 0 01-1.414 0l-6-6a1 1 0 010-1.414l6-6a1 1 0 011.414 1.414L5.414 9H17a1 1 0 110 2H5.414l4.293 4.293a1 1 0 010 1.414z" clip-rule="evenodd"/>
						</svg>
					</a>
					<button class="ftr-btn ftr-collapse" onclick={toggleSidebar} aria-label={collapsed ? 'Expand sidebar' : 'Collapse sidebar'} title={collapsed ? 'Expand' : 'Collapse'}>
						<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14">
							{#if collapsed}
								<path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
							{:else}
								<path fill-rule="evenodd" d="M12.707 5.293a1 1 0 010 1.414L9.414 10l3.293 3.293a1 1 0 01-1.414 1.414l-4-4a1 1 0 010-1.414l4-4a1 1 0 011.414 0z" clip-rule="evenodd"/>
							{/if}
						</svg>
					</button>
				</div>
			</div>
		</aside>

		<main class="main">
			<!-- Mobile topbar -->
			<div class="mob-topbar">
				<button class="mob-menu-btn" onclick={() => (mobileOpen = !mobileOpen)} aria-label="Toggle menu">
					<svg viewBox="0 0 20 20" fill="currentColor" width="18" height="18">
						<path fill-rule="evenodd" d="M3 5a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM3 10a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1zM3 15a1 1 0 011-1h12a1 1 0 110 2H4a1 1 0 01-1-1z" clip-rule="evenodd"/>
					</svg>
				</button>
				<div class="mob-brand">
					<div class="mob-brand-icon">
						<svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" width="14" height="14">
							<circle cx="12" cy="5" r="3"/><line x1="12" y1="22" x2="12" y2="8"/><path d="M5 12H2a10 10 0 0 0 20 0h-3"/>
						</svg>
					</div>
					<span class="mob-brand-name">Admin</span>
				</div>
			</div>
			{@render children()}
		</main>
	</div>
{/if}

<style>
	/* ── Gate ─────────────────────────────── */
	.gate { display:flex; align-items:center; justify-content:center; height:100vh; background:#0d0d0d; }
	.gate-ring { width:24px; height:24px; border:2px solid rgba(255,255,255,0.1); border-top-color:rgba(59,130,246,0.8); border-radius:50%; animation:spin 0.75s linear infinite; }
	@keyframes spin { to { transform:rotate(360deg); } }

	/* ── Design tokens ────────────────────── */
	.shell {
		--bg:            #f5f5f5;
		--surface:       #ffffff;
		--surface-2:     #f0f0f0;
		--border:        #e3e3e3;
		--border-2:      #cecece;
		--text:          #111111;
		--text-2:        #555555;
		--text-3:        #9a9a9a;
		--text-4:        #c4c4c4;
		--accent:        #1d4ed8;
		--accent-soft:   rgba(29,78,216,0.09);
		--accent-ring:   rgba(29,78,216,0.25);
		--ok:            #16a34a;
		--ok-soft:       rgba(22,163,74,0.08);
		--warn:          #b45309;
		--warn-soft:     rgba(180,83,9,0.08);
		--danger:        #dc2626;
		--danger-soft:   rgba(220,38,38,0.08);
		--row-hover:     rgba(0,0,0,0.022);
		--shadow-sm:     0 1px 2px rgba(0,0,0,0.07);
		--shadow:        0 1px 3px rgba(0,0,0,0.09), 0 1px 2px rgba(0,0,0,0.05);
		--shadow-md:     0 4px 8px rgba(0,0,0,0.08), 0 2px 4px rgba(0,0,0,0.04);
		--radius:        9px;
		--radius-sm:     6px;
		--font:          system-ui, -apple-system, 'Segoe UI', sans-serif;
		--mono:          ui-monospace, 'JetBrains Mono', 'Fira Code', monospace;
		display:flex;
		height:100vh;
		overflow:hidden;
		font-family:var(--font);
		font-size:13px;
		-webkit-font-smoothing:antialiased;
	}
	.shell[data-theme="dark"] {
		--bg:            #111111;
		--surface:       #1a1a1a;
		--surface-2:     #212121;
		--border:        #2d2d2d;
		--border-2:      #3d3d3d;
		--text:          #efefef;
		--text-2:        #999999;
		--text-3:        #585858;
		--text-4:        #3d3d3d;
		--accent:        #3b82f6;
		--accent-soft:   rgba(59,130,246,0.1);
		--accent-ring:   rgba(59,130,246,0.22);
		--ok:            #22c55e;
		--ok-soft:       rgba(34,197,94,0.1);
		--warn:          #f59e0b;
		--warn-soft:     rgba(245,158,11,0.1);
		--danger:        #ef4444;
		--danger-soft:   rgba(239,68,68,0.1);
		--row-hover:     rgba(255,255,255,0.025);
		--shadow-sm:     0 1px 2px rgba(0,0,0,0.35);
		--shadow:        0 1px 3px rgba(0,0,0,0.45), 0 1px 2px rgba(0,0,0,0.3);
		--shadow-md:     0 4px 8px rgba(0,0,0,0.5), 0 2px 4px rgba(0,0,0,0.3);
	}

	/* ── Sidebar ──────────────────────────── */
	.sidebar {
		width: 220px;
		flex-shrink: 0;
		background: #0d0d0d;
		border-right: 1px solid rgba(255,255,255,0.055);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		transition: width 0.22s cubic-bezier(0.4, 0, 0.2, 1);
		z-index: 40;
	}
	.sidebar.collapsed { width: 52px; }

	/* Brand */
	.brand {
		display: flex; align-items: center; gap: 9px;
		padding: 16px 12px;
		border-bottom: 1px solid rgba(255,255,255,0.06);
		min-height: 56px; flex-shrink: 0; overflow: hidden;
	}
	.brand-icon { width:28px; height:28px; border-radius:7px; background:#2563eb; display:flex; align-items:center; justify-content:center; flex-shrink:0; }
	.brand-text { display:flex; flex-direction:column; line-height:1; overflow:hidden; min-width:0; }
	.brand-name { font-size:13px; font-weight:700; color:#fff; letter-spacing:-0.01em; white-space:nowrap; }
	.brand-role { font-size:10px; font-weight:600; color:#60a5fa; letter-spacing:0.04em; text-transform:uppercase; margin-top:2px; }

	/* When collapsed, hide text-only elements via opacity + max-width trick on the sidebar */
	.sidebar.collapsed .brand-text { display:none; }
	.sidebar.collapsed .nav-label { display:none; }
	.sidebar.collapsed .nav-section-label { display:none; }
	.sidebar.collapsed .user-label { display:none; }

	/* Navigation */
	.nav-scroll { flex:1; overflow-y:auto; overflow-x:hidden; scrollbar-width:none; padding-bottom:4px; }
	.nav-scroll::-webkit-scrollbar { display:none; }

	.nav-section-label {
		padding: 14px 16px 5px;
		font-size: 9.5px; font-weight: 700;
		color: rgba(255,255,255,0.22);
		letter-spacing: 0.1em; text-transform: uppercase;
		white-space: nowrap;
	}
	.nav-section-label:first-child { padding-top: 10px; }

	.nav { padding:0 6px 4px; display:flex; flex-direction:column; gap:1px; }
	.nav-item {
		position: relative; display: flex; align-items: center; gap: 8px;
		padding: 7px 8px; border-radius: 7px;
		color: rgba(255,255,255,0.42); font-size: 13px; font-weight: 500;
		text-decoration: none; cursor: pointer;
		transition: color 0.15s, background 0.15s;
		line-height: 1; white-space: nowrap; overflow: hidden;
	}
	.nav-item:hover { color:rgba(255,255,255,0.82); background:rgba(255,255,255,0.05); }
	.nav-item.active { color:#fff; background:rgba(255,255,255,0.07); }
	.nav-icon { width:15px; height:15px; flex-shrink:0; opacity:0.7; }
	.nav-item.active .nav-icon { opacity:1; color:#60a5fa; }
	.pip { position:absolute; right:0; top:50%; transform:translateY(-50%); width:2.5px; height:14px; border-radius:2px 0 0 2px; background:#2563eb; }

	/* Footer */
	.sidebar-footer {
		padding: 10px 8px;
		border-top: 1px solid rgba(255,255,255,0.06);
		display: flex; flex-direction: column; gap: 8px;
		flex-shrink: 0;
	}
	.user { display:flex; align-items:center; gap:8px; padding:2px 4px; overflow:hidden; }
	.user-dot { width:24px; height:24px; border-radius:6px; background:rgba(37,99,235,0.2); border:1px solid rgba(37,99,235,0.35); color:#60a5fa; font-size:10px; font-weight:800; display:flex; align-items:center; justify-content:center; flex-shrink:0; letter-spacing:0.02em; cursor:default; }
	.user-label { font-size:11.5px; color:rgba(255,255,255,0.38); white-space:nowrap; overflow:hidden; text-overflow:ellipsis; min-width:0; }

	.footer-btns { display:flex; gap:4px; }
	.ftr-btn {
		display:flex; align-items:center; justify-content:center;
		width:30px; height:30px; border-radius:7px; border:none;
		background:rgba(255,255,255,0.05); color:rgba(255,255,255,0.38);
		cursor:pointer; text-decoration:none;
		transition:background 0.15s, color 0.15s; flex-shrink:0;
	}
	.ftr-btn:hover { background:rgba(255,255,255,0.1); color:rgba(255,255,255,0.8); }
	.ftr-collapse { margin-left:auto; }
	.sidebar.collapsed .ftr-collapse { margin-left:0; }
	.sidebar.collapsed .footer-btns { justify-content:center; }
	.sidebar.collapsed .footer-btns .ftr-btn:not(.ftr-collapse) { display:none; }

	/* ── Main ─────────────────────────────── */
	.main { flex:1; overflow-y:auto; background:var(--bg); transition:background 0.18s; display:flex; flex-direction:column; min-width:0; }

	/* ── Mobile topbar ────────────────────── */
	.mob-topbar {
		display: none;
		align-items: center; gap: 10px;
		padding: 12px 16px;
		background: #0d0d0d;
		border-bottom: 1px solid rgba(255,255,255,0.07);
		flex-shrink: 0;
	}
	.mob-menu-btn { display:flex; align-items:center; justify-content:center; width:36px; height:36px; border-radius:8px; border:none; background:rgba(255,255,255,0.07); color:rgba(255,255,255,0.7); cursor:pointer; }
	.mob-menu-btn:hover { background:rgba(255,255,255,0.12); }
	.mob-brand { display:flex; align-items:center; gap:8px; }
	.mob-brand-icon { width:24px; height:24px; border-radius:6px; background:#2563eb; display:flex; align-items:center; justify-content:center; }
	.mob-brand-name { font-size:13px; font-weight:700; color:#fff; }

	/* ── Mobile backdrop ──────────────────── */
	.mob-backdrop { position:fixed; inset:0; z-index:39; background:rgba(0,0,0,0.55); }

	/* ── Responsive ───────────────────────── */
	@media (max-width: 768px) {
		.shell { flex-direction: column; }
		.sidebar {
			position: fixed; top:0; left:0; bottom:0;
			width: 220px !important;
			transform: translateX(-100%);
			transition: transform 0.25s cubic-bezier(0.4, 0, 0.2, 1), width 0s;
		}
		.sidebar.mob-open { transform: translateX(0); }
		/* Show all labels on mobile even if desktop-collapsed */
		.sidebar .brand-text { display:flex !important; }
		.sidebar .nav-label { display:inline !important; }
		.sidebar .nav-section-label { display:block !important; }
		.sidebar .user-label { display:block !important; }
		.mob-topbar { display:flex; }
	}
</style>
