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
	import { NavRail, NavDrawer, Button } from '$lib/components/ui';
	import PanelContainer from '$lib/components/PanelContainer.svelte';

	let { children } = $props();
	let checking      = $state(true);
	let theme         = $state<'light' | 'dark'>('light');

	onMount(async () => {
		// Same theme storage key and mechanism as AppNav.svelte — one
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
	<div class="shell" data-theme={theme}>
		<NavRail
			groups={navGroups.map((g) => ({ key: g.key, label: g.label, icon: g.icon }))}
			bind:activeGroup
			{currentGroup}
			labelOverflow="ellipsis"
			phoneBar
			onSelectGroup={(key) => (activeGroup = activeGroup === key ? null : key)}
		>
			{#snippet footer()}
				<Button variant="ghost" size="icon" aria-label={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'} onclick={toggleTheme}>
					{#if theme === 'dark'}<Sun size={16} />{:else}<Moon size={16} />{/if}
				</Button>
				<Button variant="ghost" size="icon" href="/orgs" aria-label="Exit admin">
					<LogOut size={16} />
				</Button>
			{/snippet}
		</NavRail>
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
			{@render children()}
		</main>

		<PanelContainer />
	</div>
{/if}

<style>
	/* ── Gate ─────────────────────────────── */
	.gate { display:flex; align-items:center; justify-content:center; height:100vh; background:var(--rail-bg); }
	.gate-ring { width:24px; height:24px; border:2px solid var(--rail-active-bg); border-top-color:var(--accent); border-radius:50%; animation:spin 0.75s linear infinite; }
	@keyframes spin { to { transform:rotate(360deg); } }

	.shell {
		display:flex;
		height:100vh;
		height:100dvh;
		overflow:hidden;
		font-family:var(--font-sans);
		font-size:13px;
		-webkit-font-smoothing:antialiased;
	}

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

	/* ── Phone ────────────────────────────── */
	/* NavRail turns into a fixed 56px bottom bar at ≤639px, so the page needs
	   room beneath it and tighter side padding. */
	@media (max-width: 639px) {
		.main { padding: 16px 14px calc(56px + 20px); }
	}
</style>
