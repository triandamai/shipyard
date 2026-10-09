<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { Home, ArrowLeft } from '@lucide/svelte';
	import { Button } from '$lib/components/ui';

	let status = $derived(page.status ?? 404);
	let message = $derived(page.error?.message ?? 'Page not found');

	function goBack() {
		if (typeof history !== 'undefined') {
			history.back();
		} else {
			goto('/orgs');
		}
	}
</script>

<svelte:head>
	<title>{status} Error — Shipyard</title>
</svelte:head>

<main class="err-page">
	<div class="err-body">
		<p class="code" aria-hidden="true">{status}</p>
		<h1 class="title">
			{#if status === 404}
				This page doesn't exist
			{:else if status === 403}
				You don't have access to this page
			{:else}
				Something went wrong on our side
			{/if}
		</h1>
		<p class="desc">
			{#if status === 404}
				Check the address, or go back to your projects. Links to deleted services and projects end up here.
			{:else if status === 403}
				Ask an owner of this organization to give you access, or sign in with a different account.
			{:else}
				The request failed with status {status}. Try again; if it keeps failing, the message below helps your administrator find the cause.
			{/if}
		</p>
		<code class="detail">{message}</code>
		<div class="actions">
			<Button href="/orgs"><Home size={14} /> Go to projects</Button>
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
