<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { Card, Button, FormField, TextField, InlineAlert, Spinner } from '$lib/components/ui';

	let email = $state('');
	let password = $state('');
	let confirmPassword = $state('');
	let error = $state('');
	let loading = $state(false);

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		error = '';

		if (password !== confirmPassword) {
			error = 'Passwords do not match.';
			return;
		}

		loading = true;

		try {
			const res = await api.register(email, password);

			if (res.error || !res.data) {
				error = res.error?.message ?? 'Registration failed. Please try again.';
				return;
			}

			// Registration successful — redirect to login
			goto('/login');
		} finally {
			loading = false;
		}
	}
</script>

<div class="reg-root">
	<div class="reg-inner">
		<!-- Logo / Title -->
		<div class="reg-header">
			<h1 class="reg-title">Shipyard</h1>
			<p class="reg-subtitle">Create a new account</p>
		</div>

		<Card padding="28px">
			<div class="reg-card-body">
				{#if error}
					<InlineAlert tone="error">{error}</InlineAlert>
				{/if}

				<form onsubmit={handleSubmit} class="reg-form">
					<FormField label="Email" for="email">
						<TextField
							id="email"
							type="email"
							placeholder="you@example.com"
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
							autocomplete="new-password"
						/>
					</FormField>

					<FormField label="Confirm Password" for="confirm-password">
						<TextField
							id="confirm-password"
							type="password"
							placeholder="••••••••"
							bind:value={confirmPassword}
							required
							autocomplete="new-password"
						/>
					</FormField>

					<div class="reg-submit">
						<Button type="submit" disabled={loading}>
							{#if loading}
								<Spinner size={14} />
								Creating account…
							{:else}
								Register
							{/if}
						</Button>
					</div>
				</form>
			</div>
		</Card>

		<!-- Login link -->
		<p class="reg-login-link">
			Already have an account?
			<a href="/login">Login</a>
		</p>
	</div>
</div>

<style>
	.reg-root {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-base);
		padding: 24px;
	}
	.reg-inner {
		width: 100%;
		max-width: 400px;
		display: flex;
		flex-direction: column;
		gap: 32px;
	}
	.reg-header { text-align: center; }
	.reg-title {
		font-size: 28px;
		font-weight: 700;
		color: var(--text-primary);
		margin-bottom: 8px;
	}
	.reg-subtitle { color: var(--text-muted); font-size: 14px; }
	.reg-card-body { display: flex; flex-direction: column; gap: 20px; }
	.reg-form { display: flex; flex-direction: column; gap: 16px; }
	.reg-submit { margin-top: 4px; }
	.reg-submit :global(.ui-btn) { width: 100%; }
	.reg-login-link { text-align: center; font-size: 13px; color: var(--text-muted); }
	.reg-login-link a { color: var(--accent); }
</style>
