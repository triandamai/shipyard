<script lang="ts">
	import { page } from '$app/state';
	import { goto, invalidateAll } from '$app/navigation';
	import { Home, ArrowLeft, RefreshCw, LogIn } from '@lucide/svelte';
	import { Button } from '$lib/components/ui';

	let status = $derived(page.status ?? 404);
	let message = $derived(page.error?.message ?? '');

	interface Copy {
		title: string;
		desc: string;
		retry?: boolean;
		signIn?: boolean;
	}

	// One entry per status the dashboard can realistically surface. Anything
	// else falls back on the 4xx/5xx defaults below.
	const COPY: Record<number, Copy> = {
		400: { title: 'That request was not valid', desc: 'Something in the address or the data sent was malformed. Go back and try again.' },
		401: { title: 'Please sign in', desc: 'Your session has ended or you are not signed in yet.', signIn: true },
		403: { title: "You don't have access to this page", desc: 'Ask an owner of this organization to give you access, or sign in with a different account.' },
		404: { title: "This page doesn't exist", desc: 'Check the address, or go back to your projects. Links to deleted services and projects end up here.' },
		408: { title: 'The request timed out', desc: 'The connection was idle for too long. Try again.', retry: true },
		409: { title: 'That conflicts with the current state', desc: 'Something changed while you were working. Reload to see the latest, then try again.', retry: true },
		429: { title: 'Too many requests', desc: 'You are going faster than the server allows. Wait a few seconds, then try again.', retry: true },
		500: { title: 'Something went wrong on our side', desc: 'The server hit an error handling this request. The message below helps your administrator find the cause.', retry: true },
		502: { title: "Can't reach the Shipyard backend", desc: 'The API is not answering. It may be restarting after an update; this usually clears within a minute.', retry: true },
		503: { title: 'Shipyard is temporarily unavailable', desc: 'The server is starting up or under maintenance. Try again shortly.', retry: true },
		504: { title: 'The server took too long to respond', desc: 'The request did not finish in time. Try again; if it keeps happening, check the backend logs.', retry: true }
	};

	let copy = $derived<Copy>(
		COPY[status] ??
			(status >= 500
				? { title: 'Something went wrong on our side', desc: `The request failed with status ${status}. Try again.`, retry: true }
				: { title: 'That did not work', desc: `The request failed with status ${status}.` })
	);

	// Reloading re-runs the load that failed; invalidating alone keeps the error page.
	function retry() {
		location.reload();
	}

	function goBack() {
		if (typeof history !== 'undefined' && history.length > 1) {
			history.back();
		} else {
			goto('/orgs');
		}
	}
</script>

<svelte:head>
	<title>{status} {copy.title} — Shipyard</title>
</svelte:head>

<main class="err-page">
	<div class="err-body">
		<p class="code" aria-hidden="true">{status}</p>
		<hr class="rule" />
		<h1 class="title">{copy.title}</h1>
		<p class="desc">{copy.desc}</p>
		{#if message && message !== copy.title}
			<code class="detail">{message}</code>
		{/if}
		<div class="actions">
			{#if copy.signIn}
				<Button href="/login"><LogIn size={14} /> Sign in</Button>
			{:else if copy.retry}
				<Button onclick={retry}><RefreshCw size={14} /> Try again</Button>
			{:else}
				<Button href="/orgs"><Home size={14} /> Go to projects</Button>
			{/if}
			{#if copy.retry}
				<Button variant="secondary" href="/orgs"><Home size={14} /> Go to projects</Button>
			{/if}
			<Button variant="secondary" onclick={goBack}><ArrowLeft size={14} /> Go back</Button>
		</div>
	</div>
</main>

<style>
	.err-page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		padding: 48px 24px;
		background: var(--bg-base);
	}
	.err-body {
		width: 100%;
		max-width: 560px;
		margin: 0 auto;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}
	/* The status code is the page's one loud element. */
	.code {
		margin: 0 0 8px;
		font-family: var(--font-mono);
		font-size: clamp(72px, 16vw, 128px);
		font-weight: 500;
		line-height: 0.9;
		letter-spacing: -0.04em;
		color: var(--text-primary);
	}
	.rule {
		width: 48px;
		height: 3px;
		margin: 4px 0;
		border: 0;
		background: var(--accent);
	}
	.title {
		font-size: 24px;
		line-height: 1.25;
	}
	.desc {
		font-size: 15px;
		line-height: 1.6;
		max-width: 60ch;
	}
	.detail {
		align-self: flex-start;
		max-width: 100%;
		padding: 8px 12px;
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		background: var(--bg-surface);
		font-size: 12.5px;
		color: var(--text-secondary);
		overflow-wrap: anywhere;
	}
	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
		margin-top: 8px;
	}
</style>
