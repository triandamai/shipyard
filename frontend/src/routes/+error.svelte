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

<div class="err-page">
	<div class="glow-sphere s1"></div>
	<div class="glow-sphere s2"></div>

	<div class="err-card">
		<div class="code-badge">{status}</div>
		
		<h1 class="title">
			{#if status === 404}
				Lost in Space
			{:else if status === 403}
				Restricted Area
			{:else}
				Systems Failure
			{/if}
		</h1>
		
		<p class="desc">
			{#if status === 404}
				The page you are looking for doesn't exist, has been moved, or is temporarily unavailable.
			{:else if status === 403}
				You do not have permission to access this resource. Please verify your authentication or contact your administrator.
			{:else}
				An unexpected internal server error occurred. Our engineers have been alerted and are investigating.
			{/if}
		</p>

		<div class="err-detail">
			<span class="detail-label">Error Details:</span>
			<code class="detail-msg">{message}</code>
		</div>

		<div class="actions">
			<Button href="/orgs"><Home size={14} /> Return Home</Button>
			<Button variant="secondary" onclick={goBack}><ArrowLeft size={14} /> Go Back</Button>
		</div>
	</div>
</div>

<style>
	.err-page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: #0b0f19;
		position: relative;
		overflow: hidden;
		font-family: system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Oxygen, Ubuntu, Cantarell, 'Open Sans', 'Helvetica Neue', sans-serif;
		padding: 24px;
		box-sizing: border-box;
	}

	/* Glow Spheres for rich aesthetic background */
	.glow-sphere {
		position: absolute;
		width: 400px;
		height: 400px;
		border-radius: 50%;
		filter: blur(140px);
		opacity: 0.15;
		pointer-events: none;
	}
	.s1 {
		background: #6366f1; /* Indigo */
		top: -100px;
		left: -100px;
	}
	.s2 {
		background: #f43f5e; /* Rose */
		bottom: -100px;
		right: -100px;
	}

	/* Glassmorphism Card styling */
	.err-card {
		position: relative;
		z-index: 10;
		width: 100%;
		max-width: 460px;
		background: rgba(17, 25, 40, 0.65);
		backdrop-filter: blur(12px);
		-webkit-backdrop-filter: blur(12px);
		border: 1px solid rgba(255, 255, 255, 0.08);
		border-radius: 16px;
		padding: 40px;
		box-sizing: border-box;
		box-shadow: 0 20px 40px rgba(0, 0, 0, 0.3);
		text-align: center;
	}

	/* Code Badge styles */
	.code-badge {
		display: inline-block;
		font-size: 64px;
		font-weight: 900;
		background: linear-gradient(135deg, #a5b4fc, #fda4af); /* Indigo / Rose Soft */
		-webkit-background-clip: text;
		-webkit-text-fill-color: transparent;
		line-height: 1;
		margin-bottom: 20px;
		letter-spacing: -0.04em;
		animation: pulse 4s ease-in-out infinite;
	}

	@keyframes pulse {
		0%, 100% { transform: scale(1); }
		50% { transform: scale(1.03); }
	}

	/* Titles & Text */
	.title {
		font-size: 24px;
		font-weight: 700;
		color: #ffffff;
		margin: 0 0 12px;
		letter-spacing: -0.02em;
	}
	.desc {
		font-size: 14.5px;
		color: #9cbdca;
		line-height: 1.6;
		margin: 0 0 24px;
	}

	/* Details area */
	.err-detail {
		background: rgba(0, 0, 0, 0.25);
		border: 1px solid rgba(255, 255, 255, 0.05);
		border-radius: 8px;
		padding: 12px 16px;
		margin-bottom: 28px;
		display: flex;
		flex-direction: column;
		align-items: flex-start;
		gap: 4px;
		text-align: left;
	}
	.detail-label {
		font-size: 11px;
		font-weight: 600;
		color: #64748b;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.detail-msg {
		font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, "Liberation Mono", "Courier New", monospace;
		font-size: 12.5px;
		color: #fda4af; /* Rose Soft */
		word-break: break-all;
	}

	/* Actions */
	.actions {
		display: flex;
		gap: 12px;
		justify-content: center;
	}

	@media (max-width: 480px) {
		.err-card {
			padding: 30px 20px;
		}
		.actions {
			flex-direction: column;
			width: 100%;
		}
		.actions > :global(.ui-btn) {
			width: 100%;
		}
	}
</style>
