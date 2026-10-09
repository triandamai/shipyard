<script lang="ts">
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { setAuthCookies } from '$lib/auth/cookies';
	import { Anchor } from '@lucide/svelte';

	let email    = $state('');
	let password = $state('');
	let error    = $state('');
	let loading  = $state(false);

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		error   = '';
		loading = true;
		try {
			const res = await api.login(email, password);
			if (res.error || !res.data) {
				error = res.error?.message ?? 'Login failed.';
				return;
			}
			const { access_token } = res.data;
			// Set token so getMe can authenticate
			api.setToken(access_token);
			// Fetch full user profile — login response only returns id + email
			const meRes = await api.getMe();
			if (!meRes.data) {
				error = 'Failed to verify account.';
				return;
			}
			const user = meRes.data;
			if (!user.is_superadmin && !((user.staff_permissions?.length ?? 0) > 0)) {
				error = 'Access denied. Admin credentials required.';
				return;
			}
			authStore.setUser(user, { access_token });
			setAuthCookies(access_token);
			window.location.href = '/admin';
		} finally {
			loading = false;
		}
	}
</script>

<svelte:window oncontextmenu={(e) => e.preventDefault()} />

<div class="root">
	<div class="login-card">
		<div class="logo">
			<Anchor size={22} strokeWidth={2.5} />
		</div>
		<h1 class="title">Admin Login</h1>
		<p class="sub">Shipyard administration panel</p>

		{#if error}
			<div class="err">{error}</div>
		{/if}

		<form onsubmit={handleSubmit} class="form">
			<div class="field">
				<label for="email" class="label">Email</label>
				<input
					id="email"
					type="email"
					class="inp"
					placeholder="admin@example.com"
					bind:value={email}
					required
					autocomplete="email"
				/>
			</div>

			<div class="field">
				<label for="password" class="label">Password</label>
				<input
					id="password"
					type="password"
					class="inp"
					placeholder="••••••••"
					bind:value={password}
					required
					autocomplete="current-password"
				/>
			</div>

			<button type="submit" class="login-btn" disabled={loading || !email || !password}>
				{#if loading}
					<span class="spin"></span>
					Signing in…
				{:else}
					Sign in to Admin
				{/if}
			</button>
		</form>

		<a href="/login" class="back">Back to main login</a>
	</div>
</div>

<style>
	/* The staff sign-in is graphite regardless of theme. */
	:global(body) { margin:0; font-family:var(--font-sans); }

	.root {
		display:flex; align-items:center; justify-content:center;
		min-height:100vh;
		background:var(--rail-bg);
		padding:16px;
	}

	.login-card {
		width:100%; max-width:360px;
		background:#1B1D21;
		border:1px solid #2A2D32;
		border-radius:var(--radius-lg);
		padding:32px 28px;
	}

	.logo {
		width:44px; height:44px; border-radius:var(--radius-md);
		background:#262A30;
		color:var(--accent);
		display:flex; align-items:center; justify-content:center;
		margin:0 0 20px;
	}

	.title {
		font-size:20px; font-weight:600; color:#ECEDEE;
		margin:0 0 4px;
		letter-spacing:-0.01em;
	}

	.sub {
		font-size:13px; color:#9097A0;
		margin:0 0 24px;
	}

	.err {
		padding:10px 12px;
		background:rgba(248,113,113,0.12);
		border:1px solid rgba(248,113,113,0.3);
		border-radius:var(--radius-md);
		color:#FCA5A5;
		font-size:13px;
		margin-bottom:16px;
	}

	.form { display:flex; flex-direction:column; gap:14px; }

	.field { display:flex; flex-direction:column; gap:6px; }

	.label { font-size:13px; font-weight:500; color:#C3C6CA; }

	.inp {
		height:40px; padding:0 12px;
		background:#141619;
		border:1px solid #3A3E44;
		border-radius:var(--radius-md);
		font-size:14px; color:#ECEDEE;
		font-family:inherit;
		transition:border-color .15s;
	}
	.inp::placeholder { color:#767C83; }
	.inp:focus { border-color:#F26B1D; outline:none; }
	.inp:focus-visible { outline:2px solid #F26B1D; outline-offset:2px; }

	.login-btn {
		height:40px; padding:0 16px; margin-top:4px;
		background:#F26B1D; color:#141619;
		border:none; border-radius:var(--radius-md);
		font-size:14px; font-weight:600;
		cursor:pointer; font-family:inherit;
		display:flex; align-items:center; justify-content:center; gap:8px;
		transition:background .15s, opacity .15s;
	}
	.login-btn:hover:not(:disabled) { background:#FB8B47; }
	.login-btn:disabled { opacity:.5; cursor:not-allowed; }

	.spin {
		width:14px; height:14px;
		border:2px solid rgba(20,22,25,0.25);
		border-top-color:#141619;
		border-radius:50%;
		animation:spin .7s linear infinite;
	}
	@keyframes spin { to { transform:rotate(360deg); } }

	.back {
		display:block; margin-top:18px;
		font-size:13px; color:#9097A0;
		text-decoration:none;
		transition:color .15s;
	}
	.back:hover { color:#ECEDEE; }
</style>
