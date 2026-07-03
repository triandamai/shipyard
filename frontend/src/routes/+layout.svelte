<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { initMqtt } from '$lib/mqtt/client';
	import { initDeploymentHandler } from '$lib/mqtt/handlers/deployment.handler';
	import { initToastHandler } from '$lib/mqtt/handlers/toast.handler';
	import Toast from '$lib/components/Toast.svelte';
	import CommandPalette from '$lib/components/CommandPalette.svelte';
	import { authStore } from '$lib/stores/auth.store';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { getAuthToken, clearAuthCookies } from '$lib/auth/cookies';

	let { children } = $props();

	const token = getAuthToken();

	if (token) {
		authStore.restoreToken(token);
		api.setToken(token);
	} else {
		authStore.setLoading(false);
	}

	onMount(() => {
		// Apply saved theme before first paint so login/setup pages also respect it
		try {
			const saved = localStorage.getItem('shipyard_theme');
			if (saved === 'dark') document.documentElement.setAttribute('data-theme', 'dark');
		} catch { /* localStorage unavailable */ }

		if (token) {
			api.getMe().then((res) => {
				if (res.data) {
					authStore.setUser(res.data, { access_token: token });
				} else {
					clearAuthCookies();
					authStore.logout();
					api.setToken(null);
				}
			});
		}

		initMqtt();
		initDeploymentHandler();
		initToastHandler();
	});

	$effect(() => {
		const unsub = authStore.subscribe((s) => api.setToken(s.token));
		return unsub;
	});

	// Redirect to login whenever the refresh token is also rejected.
	$effect(() => {
		if ($authStore.sessionExpired) {
			clearAuthCookies();
			authStore.logout();
			api.setToken(null);
			goto('/login?reason=expired');
		}
	});

	// Redirect to /unauthorized whenever the API returns 403.
	$effect(() => {
		if ($authStore.forbidden) {
			authStore.clearForbidden();
			goto('/unauthorized');
		}
	});

	function handleContextMenu(e: MouseEvent) {
		// Allow right-click on input/textarea/select so users can still paste.
		const tag = (e.target as HTMLElement)?.tagName;
		if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return;
		e.preventDefault();
		uiStore.openCommandPalette();
	}

	function handleGlobalKeydown(e: KeyboardEvent) {
		// Cmd+K / Ctrl+K opens the palette.
		if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
			e.preventDefault();
			uiStore.toggleCommandPalette();
		}
	}
</script>

<svelte:document oncontextmenu={handleContextMenu} onkeydown={handleGlobalKeydown} />

<svelte:head>
	<link rel="icon" href={favicon} />

	<!-- Primary -->
	<title>Shipyard</title>
	<meta name="description" content="Manage Docker services, deployments, and infrastructure with Shipyard — the self-hosted container orchestration platform." />

	<!-- Open Graph -->
	<meta property="og:type"        content="website" />
	<meta property="og:url"         content={page.url.origin} />
	<meta property="og:site_name"   content="Shipyard" />
	<meta property="og:title"       content="Shipyard" />
	<meta property="og:description" content="Manage Docker services, deployments, and infrastructure with Shipyard — the self-hosted container orchestration platform." />
	<meta property="og:image"       content="{page.url.origin}/og-image.svg" />
	<meta property="og:image:type"  content="image/svg+xml" />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:alt"   content="Shipyard — self-hosted container orchestration platform" />
	<meta property="og:locale"      content="en_US" />

	<!-- Twitter / X Card -->
	<meta name="twitter:card"        content="summary_large_image" />
	<meta name="twitter:title"       content="Shipyard" />
	<meta name="twitter:description" content="Manage Docker services, deployments, and infrastructure with Shipyard — the self-hosted container orchestration platform." />
	<meta name="twitter:image"       content="{page.url.origin}/og-image.svg" />
	<meta name="twitter:image:alt"   content="Shipyard — self-hosted container orchestration platform" />

	<!-- Crawlers & theme -->
	<meta name="robots"      content="noindex, nofollow" />
	<meta name="theme-color" content="#0F1827" />
</svelte:head>

{@render children()}
<Toast />
<CommandPalette open={$uiStore.commandPaletteOpen} onClose={() => uiStore.closeCommandPalette()} />
