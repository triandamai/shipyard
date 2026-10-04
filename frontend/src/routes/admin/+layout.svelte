<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/stores';
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import {
		Home, Building2, Users, ShieldCheck, FolderKanban, Rocket, Server,
		Cpu, Container, Waypoints, Package, HardDrive, Radio, Globe,
		Mail, Database, ClipboardList, CreditCard, Wallet, RefreshCw, SlidersHorizontal,
		Sun, Moon, LogOut
	} from '@lucide/svelte';
	import { NavRail, NavDrawer } from '$lib/components/ui';
	import PanelContainer from '$lib/components/PanelContainer.svelte';

	let { children } = $props();
	let checking      = $state(true);
	let theme         = $state<'light' | 'dark'>('light');
	let mobileOpen    = $state(false);

	onMount(async () => {
		// Same theme storage key and mechanism as IconSidebar.svelte — one
		// shared theme state for the whole app, not a separate admin one.
		const saved = localStorage.getItem('shipyard_theme');
		theme = saved === 'dark' ? 'dark' : 'light';
		document.documentElement.setAttribute('data-theme', theme === 'dark' ? 'dark' : '');

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
		localStorage.setItem('shipyard_theme', theme);
		document.documentElement.setAttribute('data-theme', theme === 'dark' ? 'dark' : '');
	}
	function closeMobile() { mobileOpen = false; }

	type NavItem  = { href: string; label: string; icon: typeof Home };
	type NavGroup = { key: string; label: string; icon: typeof Home; items: NavItem[] };

	const navGroups: NavGroup[] = [
		{
			key: 'platform', label: 'Platform', icon: Home,
			items: [
				{ href: '/admin',              label: 'Overview',      icon: Home },
				{ href: '/admin/orgs',         label: 'Organizations', icon: Building2 },
				{ href: '/admin/users',        label: 'Users',         icon: Users },
				{ href: '/admin/staff',        label: 'Staff',         icon: ShieldCheck },
				{ href: '/admin/projects',     label: 'Projects',      icon: FolderKanban },
				{ href: '/admin/deployments',  label: 'Deployments',   icon: Rocket },
				{ href: '/admin/deployments/provisioning', label: 'Provisioning', icon: Waypoints },
				{ href: '/admin/nodes',        label: 'Compute',       icon: Server },
			]
		},
		{
			key: 'infra', label: 'Infrastructure', icon: Cpu,
			items: [
				{ href: '/admin/infra',    label: 'System',       icon: Cpu },
				{ href: '/admin/docker',   label: 'Docker',       icon: Container },
				{ href: '/admin/traefik',  label: 'Traefik',      icon: Waypoints },
				{ href: '/admin/registry', label: 'Registry',     icon: Package },
				{ href: '/admin/storage',  label: 'Storage (S3)', icon: HardDrive },
				{ href: '/admin/mqtt',     label: 'MQTT',         icon: Radio },
				{ href: '/admin/static',   label: 'Static Sites', icon: Globe },
			]
		},
		{
			key: 'services', label: 'Services', icon: Mail,
			items: [
				{ href: '/admin/smtp',     label: 'SMTP',       icon: Mail },
				{ href: '/admin/database', label: 'Database',   icon: Database },
				{ href: '/admin/audit',    label: 'Audit Log',  icon: ClipboardList },
				{ href: '/admin/plan',     label: 'Plans',      icon: CreditCard },
				{ href: '/admin/payments', label: 'Payments',   icon: Wallet },
				{ href: '/admin/updates',  label: 'Updates',    icon: RefreshCw },
				{ href: '/admin/config',   label: 'Config',     icon: SlidersHorizontal },
			]
		},
	];

	let activeGroup = $state<string | null>(null);

	function isActive(href: string): boolean {
		if (href === '/admin') return $page.url.pathname === '/admin';
		return $page.url.pathname.startsWith(href + '/') || $page.url.pathname === href;
	}

	// Keep activeGroup in sync with the current route. Re-resolves whenever
	// $page.url.pathname changes (read indirectly via isActive below), NOT
	// whenever activeGroup changes — so it fires on first load AND after
	// every client-side navigation (fixing a bug where a drawer-item click
	// would momentarily see the OLD pathname and lock onto the group being
	// left), while a manual rail click that only opens/closes a drawer
	// (no navigation, no pathname change) never re-triggers this effect and
	// so never fights the user's manual toggle.
	$effect(() => {
		const current = navGroups.find((g) => g.items.some((i) => isActive(i.href)));
		if (current) activeGroup = current.key;
	});
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
		<NavRail
			groups={navGroups.map((g) => ({ key: g.key, label: g.label, icon: g.icon }))}
			bind:activeGroup
			onSelectGroup={(key) => (activeGroup = activeGroup === key ? null : key)}
		/>
		<div class="rail-footer">
			<button class="rail-footer-btn" onclick={toggleTheme} aria-label={theme === 'dark' ? 'Light mode' : 'Dark mode'} title={theme === 'dark' ? 'Light mode' : 'Dark mode'}>
				{#if theme === 'dark'}
					<Sun size={14} />
				{:else}
					<Moon size={14} />
				{/if}
			</button>
			<a href="/orgs" class="rail-footer-btn" aria-label="Back to dashboard" title="Exit admin" onclick={closeMobile}>
				<LogOut size={14} />
			</a>
		</div>
		{#each navGroups as group (group.key)}
			<NavDrawer
				open={activeGroup === group.key}
				title={group.label}
				items={group.items.map((i) => ({ href: i.href, label: i.label, icon: i.icon, active: isActive(i.href) }))}
				onNavigate={() => (activeGroup = null)}
			/>
		{/each}

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

		<PanelContainer />
	</div>
{/if}

<style>
	/* ── Gate ─────────────────────────────── */
	.gate { display:flex; align-items:center; justify-content:center; height:100vh; background:#0d0d0d; }
	.gate-ring { width:24px; height:24px; border:2px solid rgba(255,255,255,0.1); border-top-color:rgba(59,130,246,0.8); border-radius:50%; animation:spin 0.75s linear infinite; }
	@keyframes spin { to { transform:rotate(360deg); } }

	/* ── Design tokens: alias admin's old var names onto the shared design
	   system's real tokens (defined in frontend/src/routes/layout.css, which
	   the root layout already imports globally). This keeps every
	   not-yet-migrated admin page's existing `var(--bg)` etc. references
	   working during the page-by-page migration (Part B of the plan) — once
	   a page migrates, it stops using these names entirely and this alias
	   layer becomes dead weight to delete in a final cleanup task. */
	.shell {
		--bg:            var(--bg-base);
		--surface:       var(--bg-surface);
		--surface-2:     var(--bg-elevated);
		/* --border and --accent are intentionally NOT redeclared here: their
		   new-token name is identical to the old admin name, and CSS treats
		   `--border: var(--border);` on the same rule as a self-reference
		   cycle, which resolves to an invalid (empty) value rather than the
		   inherited one — verified live in-browser, where it silently broke
		   every var(--border)/var(--accent) consumer (transparent borders,
		   invisible accent-colored buttons). Omitting the declaration lets
		   the identically-named token inherit straight from :root instead,
		   which is what we actually want. */
		--border-2:      var(--border-hover);
		--text:          var(--text-primary);
		--text-2:        var(--text-secondary);
		--text-3:        var(--text-muted);
		--text-4:        var(--text-dim);
		--accent-soft:   var(--accent-muted);
		--accent-ring:   color-mix(in srgb, var(--accent) 25%, transparent);
		--ok:            var(--accent-green);
		--ok-soft:       var(--accent-green-muted);
		--warn:          var(--accent-yellow);
		--warn-soft:     var(--accent-yellow-muted);
		--danger:        var(--accent-red);
		--danger-soft:   var(--accent-red-muted);
		--row-hover:     var(--bg-hover);
		/* --shadow-sm: same self-reference issue as --border/--accent above —
		   omitted so it inherits :root's --shadow-sm directly. */
		--shadow:        var(--shadow-md);
		--shadow-md:     var(--shadow-lg);
		--radius:        var(--radius-lg);
		--radius-sm:     var(--radius-md);
		--font:          var(--font-sans);
		--mono:          var(--font-mono);
		display:flex;
		height:100vh;
		overflow:hidden;
		font-family:var(--font);
		font-size:13px;
		-webkit-font-smoothing:antialiased;
	}

	/* ── Rail footer ──────────────────────── */
	/* Small fixed overlay pinned to the bottom of NavRail's 60px column, kept
	   local to this layout rather than folded into NavRail's own API: these
	   two actions (theme toggle, exit admin) are admin-specific concerns, and
	   NavRail/NavDrawer are shared design-system components other areas of
	   the app may reuse — baking an admin footer into their prop surface
	   would couple a shared component to one consumer for the sake of two
	   buttons. */
	.rail-footer {
		position: fixed;
		left: 0;
		bottom: 0;
		width: 60px;
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 4px;
		padding: 10px 0;
		z-index: 6;
	}
	.rail-footer-btn {
		display:flex; align-items:center; justify-content:center;
		width:36px; height:36px; border-radius:var(--radius-md); border:none;
		background:none; color:var(--text-muted);
		cursor:pointer; text-decoration:none;
		transition:background var(--transition-fast), color var(--transition-fast);
	}
	.rail-footer-btn:hover { background:var(--bg-hover); color:var(--accent); }

	/* ── Main ─────────────────────────────── */
	.main { flex:1; overflow-y:auto; background:var(--bg); transition:background 0.18s; display:flex; flex-direction:column; min-width:0; padding: 24px 28px; }

	/* ── Mobile topbar ────────────────────── */
	/* NOTE: mob-menu-btn still toggles `mobileOpen`, which was wired to the
	   old 220px sliding `.sidebar` panel. That panel no longer exists (it's
	   NavRail/NavDrawer now), so this button is currently a no-op on mobile.
	   This plan's mockups/prototype were desktop-only — mobile nav-rail
	   behavior hasn't been designed yet. Needs a follow-up task once that
	   design exists. */
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
		.mob-topbar { display:flex; }
	}
</style>
