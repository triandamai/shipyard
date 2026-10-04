<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { setAuthCookies } from '$lib/auth/cookies';
	import { Anchor } from '@lucide/svelte';
	import { Button, FormField, TextField, InlineAlert, Spinner } from '$lib/components/ui';

	let email = $state('');
	let password = $state('');
	let error = $state('');
	let loading = $state(false);

	let sessionNotice = $derived(
		page.url.searchParams.get('reason') === 'expired'
			? 'Your session has expired. Please sign in again.'
			: ''
	);

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		error = '';
		loading = true;

		try {
			const res = await api.login(email, password);

			if (res.error || !res.data) {
				error = res.error?.message ?? 'Login failed. Please try again.';
				return;
			}

			const { access_token, user } = res.data;

			authStore.setUser(user, { access_token });
			setAuthCookies(access_token);

			goto('/orgs');
		} finally {
			loading = false;
		}
	}
</script>

<svelte:window oncontextmenu={(e) => e.preventDefault()} />

<div class="login-root">
	<!-- Left brand panel (dark navy) -->
	<div class="brand-panel">
		<div class="brand-content">
			<div class="brand-logo">
				<Anchor size={28} />
			</div>
			<h1 class="brand-name">Shipyard</h1>
			<p class="brand-tagline">
				Deploy and manage containerised services with confidence.
			</p>

			<ul class="brand-features">
				<li>Orchestrate Docker Swarm workloads</li>
				<li>Git-triggered deployments</li>
				<li>Real-time monitoring &amp; logs</li>
				<li>Role-based access control</li>
			</ul>
		</div>

		<p class="brand-footer">
			&copy; {new Date().getFullYear()} Shipyard
		</p>
	</div>

	<!-- Right form panel (light) -->
	<div class="form-panel">
		<div class="form-inner">
			<div class="form-header">
				<h2 class="form-title">Sign in</h2>
				<p class="form-subtitle">Enter your credentials to continue</p>
			</div>

			{#if sessionNotice}
				<div role="status">
					<InlineAlert tone="warning">{sessionNotice}</InlineAlert>
				</div>
			{/if}

			{#if error}
				<div role="alert">
					<InlineAlert tone="error">{error}</InlineAlert>
				</div>
			{/if}

			<form onsubmit={handleSubmit} class="form-body">
				<FormField label="Email address" for="email">
					<TextField
						id="email"
						type="email"
						placeholder="you@company.com"
						bind:value={email}
						required
						autocomplete="email"
					/>
				</FormField>

				<FormField label="Password" for="password">
					<TextField
						id="password"
						type="password"
						placeholder="••••••••"
						bind:value={password}
						required
						autocomplete="current-password"
					/>
				</FormField>

				<div class="submit-wrap">
					<Button type="submit" disabled={loading}>
						{#if loading}
							<Spinner size={14} tone="current" />
							Signing in…
						{:else}
							Sign in
						{/if}
					</Button>
				</div>
			</form>

			<p class="register-link">
				Don't have an account?&nbsp;
				<a href="/register">Create one</a>
			</p>
			<p class="admin-link">
				<a href="/admin/login">Admin login</a>
			</p>
		</div>
	</div>
</div>

<style>
	.login-root {
		display: flex;
		min-height: 100vh;
		overflow: hidden;
	}

	/* ── Left dark panel ─────────────────────────────────── */
	.brand-panel {
		display: none;
		flex-direction: column;
		justify-content: space-between;
		width: 420px;
		flex-shrink: 0;
		/* Brand panel is always dark (white title), independent of theme */
		--brand-text: #5E7A96;
		--brand-text-strong: #9DB8D0;
		background: #0F1827;
		padding: 48px 48px 36px;
		border-right: 1px solid rgba(255, 255, 255, 0.07);
	}

	@media (min-width: 900px) {
		.brand-panel {
			display: flex;
		}
	}

	.brand-content {
		display: flex;
		flex-direction: column;
		gap: 24px;
	}

	.brand-logo {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 48px;
		height: 48px;
		border-radius: 12px;
		background: rgba(59, 130, 246, 0.16);
		color: #60A5FA;
	}

	.brand-name {
		font-size: 28px;
		font-weight: 700;
		color: #FFFFFF;
		letter-spacing: -0.03em;
		margin: 0;
	}

	.brand-tagline {
		font-size: 15px;
		line-height: 1.6;
		color: var(--brand-text-strong);
		margin: 0;
		max-width: 300px;
	}

	.brand-features {
		list-style: none;
		margin: 16px 0 0;
		padding: 0;
		display: flex;
		flex-direction: column;
		gap: 12px;
	}

	.brand-features li {
		font-size: 13px;
		color: var(--brand-text);
		padding-left: 18px;
		position: relative;
	}

	.brand-features li::before {
		content: '';
		position: absolute;
		left: 0;
		top: 6px;
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: #3B82F6;
		opacity: 0.7;
	}

	.brand-footer {
		font-size: 12px;
		color: var(--brand-text);
		margin: 0;
		opacity: 0.5;
	}

	/* ── Right form panel ────────────────────────────────── */
	.form-panel {
		flex: 1;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-base);
		padding: 32px 24px;
	}

	.form-inner {
		width: 100%;
		max-width: 380px;
		display: flex;
		flex-direction: column;
		gap: 28px;
	}

	.form-header {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.form-title {
		font-size: 24px;
		font-weight: 700;
		color: var(--text-primary);
		letter-spacing: -0.02em;
	}

	.form-subtitle {
		font-size: 14px;
		color: var(--text-muted);
	}

	.form-body {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.submit-wrap {
		margin-top: 4px;
	}

	.submit-wrap :global(.ui-btn) {
		width: 100%;
	}

	.register-link {
		text-align: center;
		font-size: 13px;
		color: var(--text-muted);
	}

	.register-link a {
		color: var(--accent);
		font-weight: 500;
	}

	.register-link a:hover {
		color: var(--accent-hover);
	}

	.admin-link {
		text-align: center;
		font-size: 11.5px;
		margin-top: 4px;
	}

	.admin-link a {
		color: var(--text-dim, #9ca3af);
		text-decoration: none;
	}

	.admin-link a:hover {
		color: var(--text-muted, #6b7280);
		text-decoration: underline;
	}
</style>
