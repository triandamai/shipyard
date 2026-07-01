<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { get } from 'svelte/store';

	onMount(async () => {
		const res = await api.getSetupStatus();

		if (res.error || !res.data) {
			goto('/login');
			return;
		}

		if (!res.data.initialized) {
			goto('/setup');
			return;
		}

		const auth = get(authStore);
		if (!auth.token) {
			goto('/login');
			return;
		}

		goto('/orgs');
	});
</script>

<div
	style="
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100vh;
		background: var(--bg-base);
	"
>
	<div style="display: flex; flex-direction: column; align-items: center; gap: 12px;">
		<div
			style="
				width: 32px;
				height: 32px;
				border: 2px solid var(--border);
				border-top-color: var(--accent);
				border-radius: 50%;
				animation: spin 0.7s linear infinite;
			"
		></div>
		<span style="color: var(--text-muted); font-size: 13px;">Loading…</span>
	</div>
</div>

<style>
	@keyframes spin {
		to {
			transform: rotate(360deg);
		}
	}
</style>
