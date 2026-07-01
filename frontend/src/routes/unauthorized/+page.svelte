<script lang="ts">
	import { goto } from '$app/navigation';
	import { ShieldOff, ArrowLeft, Home } from '@lucide/svelte';
	import { page } from '$app/state';

	function goBack() {
		history.length > 1 ? history.back() : goto('/');
	}
</script>

<svelte:head>
	<title>Unauthorized — Shipyard</title>
</svelte:head>

<div class="page">
	<div class="card">
		<div class="icon-wrap">
			<ShieldOff size={32} />
		</div>

		<h1 class="title">Access Denied</h1>
		<p class="desc">
			You don't have permission to perform this action. Contact your organization owner or admin
			to request access.
		</p>

		<div class="actions">
			<button class="btn btn-secondary" onclick={goBack}>
				<ArrowLeft size={15} />
				Go back
			</button>
			<a class="btn btn-primary" href="/">
				<Home size={15} />
				Home
			</a>
		</div>
	</div>
</div>

<style>
	.page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-base);
		padding: 24px;
	}

	.card {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
		max-width: 400px;
		width: 100%;
		text-align: center;
		padding: 40px 32px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
	}

	.icon-wrap {
		width: 64px;
		height: 64px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		background: color-mix(in srgb, var(--accent-red, #ef4444) 10%, transparent);
		color: var(--accent-red, #ef4444);
		border: 1px solid color-mix(in srgb, var(--accent-red, #ef4444) 25%, transparent);
	}

	.title {
		font-size: 22px;
		font-weight: 700;
		color: var(--text-primary);
		margin: 0;
	}

	.desc {
		font-size: 14px;
		color: var(--text-muted);
		line-height: 1.6;
		margin: 0;
	}

	.actions {
		display: flex;
		gap: 10px;
		margin-top: 8px;
	}

	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 9px 18px;
		border-radius: var(--radius-sm);
		font-size: 14px;
		font-weight: 600;
		cursor: pointer;
		text-decoration: none;
		border: none;
		transition: all var(--transition-fast);
	}

	.btn-secondary {
		background: var(--bg-surface);
		color: var(--text-secondary);
		border: 1px solid var(--border);
	}

	.btn-secondary:hover {
		background: var(--bg-elevated);
		color: var(--text-primary);
	}

	.btn-primary {
		background: var(--accent);
		color: white;
	}

	.btn-primary:hover {
		opacity: 0.88;
	}
</style>
