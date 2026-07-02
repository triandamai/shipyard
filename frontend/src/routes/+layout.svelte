<script lang="ts">
	import './layout.css';
	import favicon from '$lib/assets/favicon.svg';
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { initMqtt } from '$lib/mqtt/client';
	import { initDeploymentHandler } from '$lib/mqtt/handlers/deployment.handler';
	import { initToastHandler } from '$lib/mqtt/handlers/toast.handler';
	import Toast from '$lib/components/Toast.svelte';
	import { authStore } from '$lib/stores/auth.store';
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
</script>

<svelte:head><link rel="icon" href={favicon} /></svelte:head>

{@render children()}
<Toast />
