<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';

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

<div
	style="
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-base);
		padding: 24px;
	"
>
	<div
		style="
			width: 100%;
			max-width: 400px;
			display: flex;
			flex-direction: column;
			gap: 32px;
		"
	>
		<!-- Logo / Title -->
		<div style="text-align: center;">
			<h1 style="font-size: 28px; font-weight: 700; color: var(--text-primary); margin-bottom: 8px;">
				Shipyard
			</h1>
			<p style="color: var(--text-muted); font-size: 14px;">Create a new account</p>
		</div>

		<!-- Card -->
		<div
			style="
				background: var(--bg-surface);
				border: 1px solid var(--border);
				border-radius: var(--radius-lg);
				padding: 28px;
				display: flex;
				flex-direction: column;
				gap: 20px;
			"
		>
			{#if error}
				<div
					style="
						padding: 10px 14px;
						background: var(--accent-red-muted);
						border: 1px solid var(--accent-red);
						border-radius: var(--radius-md);
						color: var(--accent-red);
						font-size: 13px;
					"
				>
					{error}
				</div>
			{/if}

			<form
				onsubmit={handleSubmit}
				style="display: flex; flex-direction: column; gap: 16px;"
			>
				<div style="display: flex; flex-direction: column; gap: 6px;">
					<label
						for="email"
						style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
					>
						Email
					</label>
					<input
						id="email"
						type="email"
						class="input"
						placeholder="you@example.com"
						bind:value={email}
						required
						autocomplete="email"
					/>
				</div>

				<div style="display: flex; flex-direction: column; gap: 6px;">
					<label
						for="password"
						style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
					>
						Password
					</label>
					<input
						id="password"
						type="password"
						class="input"
						placeholder="••••••••"
						bind:value={password}
						required
						autocomplete="new-password"
					/>
				</div>

				<div style="display: flex; flex-direction: column; gap: 6px;">
					<label
						for="confirm-password"
						style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
					>
						Confirm Password
					</label>
					<input
						id="confirm-password"
						type="password"
						class="input"
						placeholder="••••••••"
						bind:value={confirmPassword}
						required
						autocomplete="new-password"
					/>
				</div>

				<button
					type="submit"
					class="btn btn-primary"
					disabled={loading}
					style="width: 100%; justify-content: center; margin-top: 4px;"
				>
					{#if loading}
						<span
							style="
								width: 14px;
								height: 14px;
								border: 2px solid rgba(255,255,255,0.3);
								border-top-color: white;
								border-radius: 50%;
								display: inline-block;
								animation: spin 0.7s linear infinite;
							"
						></span>
						Creating account…
					{:else}
						Register
					{/if}
				</button>
			</form>
		</div>

		<!-- Login link -->
		<p style="text-align: center; font-size: 13px; color: var(--text-muted);">
			Already have an account?
			<a href="/login" style="color: var(--accent);">Login</a>
		</p>
	</div>
</div>

<style>
	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
