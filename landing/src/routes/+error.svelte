<script lang="ts">
	import { page } from '$app/state';
	import '$lib/styles/tokens.css';

	let status = $derived(page.status ?? 404);
	let title = $derived(status === 404 ? 'Page not found' : 'Something went wrong');
	let body = $derived(
		status === 404
			? "This address doesn't lead anywhere. The page may have moved, or the link is out of date."
			: 'The page failed to load. Try again in a moment.'
	);
</script>

<svelte:head>
	<title>{status} {title} · Shipyard</title>
	<meta name="robots" content="noindex" />
</svelte:head>

<main class="err">
	<div class="body">
		<p class="code" aria-hidden="true">{status}</p>
		<hr class="rule" />
		<h1>{title}</h1>
		<p class="desc">{body}</p>
		<div class="actions">
			<a class="btn primary" href="/">Go to home page</a>
			<a class="btn" href="/docs">Read the docs</a>
		</div>
	</div>
</main>

<style>
	:global(body) {
		margin: 0;
		background: var(--bg-base);
		color: var(--text-primary);
		font-family: var(--font-sans);
	}
	.err {
		min-height: 100vh;
		display: flex;
		align-items: center;
		padding: 48px 24px;
		box-sizing: border-box;
	}
	.body {
		width: 100%;
		max-width: 560px;
		margin: 0 auto;
	}
	.code {
		margin: 0 0 8px;
		font-family: var(--font-mono);
		font-weight: 500;
		font-size: clamp(72px, 16vw, 128px);
		line-height: 0.9;
		letter-spacing: -0.04em;
	}
	.rule {
		width: 48px;
		height: 3px;
		margin: 20px 0;
		border: 0;
		background: var(--accent);
	}
	h1 {
		margin: 0 0 12px;
		font-size: 24px;
		line-height: 1.25;
		font-weight: 600;
	}
	.desc {
		margin: 0;
		max-width: 60ch;
		font-size: 15px;
		line-height: 1.6;
		color: var(--text-secondary);
	}
	.actions {
		margin-top: 24px;
		display: flex;
		flex-wrap: wrap;
		gap: 8px;
	}
	.btn {
		padding: 8px 16px;
		border-radius: 6px;
		border: 1px solid var(--border);
		font-size: 14px;
		font-weight: 500;
		color: var(--text-primary);
		text-decoration: none;
	}
	.btn.primary {
		background: var(--accent);
		border-color: var(--accent);
		color: var(--accent-fg);
	}
	.btn:focus-visible {
		outline: 2px solid var(--accent);
		outline-offset: 2px;
	}
</style>
