<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';

	let status = $derived(page.status ?? 404);
	let message = $derived(page.error?.message ?? 'Page not found');

	function goHome() {
		goto('/orgs');
	}

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

	<div class="card">
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
			<button class="btn-primary" onclick={goHome}>
				<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14" style="margin-right: 6px;"><path d="M10.707 2.293a1 1 0 00-1.414 0l-7 7a1 1 0 001.414 1.414L4 10.414V17a1 1 0 001 1h2a1 1 0 001-1v-2a1 1 0 01-1-1h2a1 1 0 011 1v2a1 1 0 001 1h2a1 1 0 001-1v-6.586l.293.293a1 1 0 001.414-1.414l-7-7z"/></svg>
				Return Home
			</button>
			<button class="btn-secondary" onclick={goBack}>
				<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14" style="margin-right: 6px;"><path fill-rule="evenodd" d="M9.707 16.707a1 1 0 01-1.414 0l-6-6a1 1 0 010-1.414l6-6a1 1 0 011.414 1.414L5.414 9H17a1 1 0 110 2H5.414l4.293 4.293a1 1 0 010 1.414z" clip-rule="evenodd"/></svg>
				Go Back
			</button>
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
	.card {
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
	.btn-primary {
		display: inline-flex;
		align-items: center;
		padding: 9px 18px;
		height: 38px;
		background: #6366f1;
		color: #ffffff;
		border: none;
		border-radius: 8px;
		font-size: 13.5px;
		font-weight: 600;
		cursor: pointer;
		transition: background 0.15s, transform 0.1s;
	}
	.btn-primary:hover {
		background: #4f46e5;
	}
	.btn-primary:active {
		transform: scale(0.98);
	}

	.btn-secondary {
		display: inline-flex;
		align-items: center;
		padding: 9px 18px;
		height: 38px;
		background: transparent;
		color: #e2e8f0;
		border: 1px solid rgba(255, 255, 255, 0.15);
		border-radius: 8px;
		font-size: 13.5px;
		font-weight: 600;
		cursor: pointer;
		transition: background 0.15s, border-color 0.15s, transform 0.1s;
	}
	.btn-secondary:hover {
		background: rgba(255, 255, 255, 0.05);
		border-color: rgba(255, 255, 255, 0.25);
	}
	.btn-secondary:active {
		transform: scale(0.98);
	}

	@media (max-width: 480px) {
		.card {
			padding: 30px 20px;
		}
		.actions {
			flex-direction: column;
			width: 100%;
		}
		.btn-primary, .btn-secondary {
			width: 100%;
			justify-content: center;
		}
	}
</style>
