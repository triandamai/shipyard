<script lang="ts">
	import { goto } from '$app/navigation';
	import { ShieldOff, ArrowLeft, Home } from '@lucide/svelte';
	import { Card, Button } from '$lib/components/ui';
	
	function goBack() {
		history.length > 1 ? history.back() : goto('/');
	}
</script>

<svelte:head>
	<title>Unauthorized — Shipyard</title>
</svelte:head>

<div class="page">
	<div class="card-wrap">
		<Card padding="40px 32px">
			<div class="content">
				<div class="icon-wrap">
					<ShieldOff size={32} />
				</div>

				<h1 class="title">Access Denied</h1>
				<p class="desc">
					You don't have permission to perform this action. Contact your organization owner or admin
					to request access.
				</p>

				<div class="actions">
					<Button variant="secondary" onclick={goBack}>
						<ArrowLeft size={15} />
						Go back
					</Button>
					<Button href="/">
						<Home size={15} />
						Home
					</Button>
				</div>
			</div>
		</Card>
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

	.card-wrap {
		max-width: 400px;
		width: 100%;
	}

	.content {
		display: flex;
		flex-direction: column;
		align-items: center;
		gap: 16px;
		text-align: center;
	}

	.icon-wrap {
		width: 64px;
		height: 64px;
		border-radius: 50%;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--accent-red-muted);
		color: var(--accent-red);
		border: 1px solid color-mix(in srgb, var(--accent-red) 25%, transparent);
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
		flex-wrap: wrap;
		justify-content: center;
		gap: 10px;
		margin-top: 8px;
	}
</style>
