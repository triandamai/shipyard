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
		Sun, Moon, LogOut, Menu, Anchor
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

	// false: the drawer overlays the page and collapses on outside click,
	// Escape, or item click. true: it stays docked beside the content.
	const navPersistent = false;

	let activeGroup = $state<string | null>(null);

	function isActive(href: string): boolean {
		if (href === '/admin') return $page.url.pathname === '/admin';
		return $page.url.pathname.startsWith(href + '/') || $page.url.pathname === href;
	}

	let currentGroup = $derived(navGroups.find((g) => g.items.some((i) => isActive(i.href)))?.key ?? null);

	// Persistent mode only: follow the route so the docked drawer shows the
	// current section. Depends on currentGroup (the route), not activeGroup,
	// so a manual rail toggle is never overridden.
	$effect(() => {
		if (navPersistent && currentGroup) activeGroup = currentGroup;
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
			{currentGroup}
			labelOverflow="ellipsis"
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
				persistent={navPersistent}
				onNavigate={() => { if (!navPersistent) activeGroup = null; }}
				onClose={() => (activeGroup = null)}
			/>
		{/each}

		<main class="main">
			<!-- Mobile topbar -->
			<div class="mob-topbar">
				<button class="mob-menu-btn" onclick={() => (mobileOpen = !mobileOpen)} aria-label="Toggle menu">
					<Menu size={18} />
				</button>
				<div class="mob-brand">
					<div class="mob-brand-icon">
						<Anchor size={14} color="white" strokeWidth={2.5} />
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

	.shell {
		display:flex;
		height:100vh;
		overflow:hidden;
		font-family:var(--font-sans);
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
	.main { flex:1; overflow-y:auto; background:var(--bg-base); transition:background 0.18s; display:flex; flex-direction:column; min-width:0; padding: 24px 28px; }
	/* Page blocks must keep their natural height so .main scrolls. Without
	   this, an overflow:hidden child (e.g. DataTable) gets a min-height of 0
	   and shrinks to the viewport, clipping its own rows. */
	/* width:100% — pages center a max-width wrapper with margin:0 auto, and
	   auto margins in a column flex container disable stretching, so the
	   wrapper would otherwise shrink to its content and change width as
	   content changes (empty → loaded, detail panel opening, row expanding). */
	.main > :global(*) { flex-shrink: 0; width: 100%; }

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
