<script lang="ts">
	import { goto } from '$app/navigation';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { setAuthCookies } from '$lib/auth/cookies';
	import { Anchor } from '@lucide/svelte';

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
				<div class="notice-banner" role="status">
					{sessionNotice}
				</div>
			{/if}

			{#if error}
				<div class="error-banner" role="alert">
					{error}
				</div>
			{/if}

			<form onsubmit={handleSubmit} class="form-body">
				<div class="field">
					<label class="field-label" for="email">Email address</label>
					<input
						id="email"
						type="email"
						class="input"
						placeholder="you@company.com"
						bind:value={email}
						required
						autocomplete="email"
					/>
				</div>

				<div class="field">
					<label class="field-label" for="password">Password</label>
					<input
						id="password"
						type="password"
						class="input"
						placeholder="••••••••"
						bind:value={password}
						required
						autocomplete="current-password"
					/>
				</div>

				<button
					type="submit"
					class="btn btn-primary submit-btn"
					disabled={loading}
				>
					{#if loading}
						<span class="spinner"></span>
						Signing in…
					{:else}
						Sign in
					{/if}
				</button>
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
		background: var(--sidebar-bg);
		padding: 48px 48px 36px;
		border-right: 1px solid var(--sidebar-border);
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
		color: var(--sidebar-text-hover);
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
		color: var(--sidebar-text);
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
		color: var(--sidebar-text);
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

	.notice-banner {
		padding: 10px 14px;
		background: color-mix(in srgb, var(--accent-yellow, #f59e0b) 12%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent-yellow, #f59e0b) 35%, transparent);
		border-radius: var(--radius-md);
		color: var(--accent-yellow, #92400e);
		font-size: 13px;
	}

	.error-banner {
		padding: 10px 14px;
		background: var(--accent-red-muted);
		border: 1px solid rgba(220, 38, 38, 0.25);
		border-radius: var(--radius-md);
		color: var(--accent-red);
		font-size: 13px;
	}

	.form-body {
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.field {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.field-label {
		font-size: 13px;
		font-weight: 500;
		color: var(--text-secondary);
	}

	.submit-btn {
		width: 100%;
		justify-content: center;
		padding: 10px 16px;
		font-size: 14px;
		margin-top: 4px;
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

	/* Spinner */
	.spinner {
		width: 14px;
		height: 14px;
		border: 2px solid rgba(255, 255, 255, 0.30);
		border-top-color: white;
		border-radius: 50%;
		display: inline-block;
		animation: spin 0.65s linear infinite;
	}

	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
