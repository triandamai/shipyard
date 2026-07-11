<script lang="ts">
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { setAuthCookies } from '$lib/auth/cookies';

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
	<div class="card">
		<div class="logo">
			<svg viewBox="0 0 24 24" fill="none" stroke="white" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" width="22" height="22">
				<circle cx="12" cy="5" r="3"/><line x1="12" y1="22" x2="12" y2="8"/><path d="M5 12H2a10 10 0 0 0 20 0h-3"/>
			</svg>
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

			<button type="submit" class="btn" disabled={loading || !email || !password}>
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
	:global(body) { margin:0; font-family:system-ui,-apple-system,'Segoe UI',sans-serif; }

	.root {
		display:flex; align-items:center; justify-content:center;
		min-height:100vh;
		background:#0d0d0d;
	}

	.card {
		width:100%; max-width:360px;
		background:#1a1a1a;
		border:1px solid rgba(255,255,255,0.08);
		border-radius:14px;
		padding:36px 32px;
		box-shadow:0 20px 60px rgba(0,0,0,0.5);
	}

	.logo {
		width:48px; height:48px; border-radius:12px;
		background:#2563eb;
		display:flex; align-items:center; justify-content:center;
		margin:0 auto 20px;
	}

	.title {
		font-size:20px; font-weight:700; color:#fff;
		text-align:center; margin:0 0 4px;
		letter-spacing:-0.02em;
	}

	.sub {
		font-size:12.5px; color:rgba(255,255,255,0.32);
		text-align:center; margin:0 0 24px;
	}

	.err {
		padding:10px 13px;
		background:rgba(239,68,68,0.12);
		border:1px solid rgba(239,68,68,0.3);
		border-radius:8px;
		color:#f87171;
		font-size:13px;
		margin-bottom:16px;
	}

	.form { display:flex; flex-direction:column; gap:14px; }

	.field { display:flex; flex-direction:column; gap:5px; }

	.label { font-size:12px; font-weight:600; color:rgba(255,255,255,0.45); }

	.inp {
		height:38px; padding:0 12px;
		background:rgba(255,255,255,0.05);
		border:1px solid rgba(255,255,255,0.1);
		border-radius:8px;
		font-size:13.5px; color:#fff;
		font-family:inherit;
		outline:none;
		transition:border-color .15s, box-shadow .15s;
	}
	.inp::placeholder { color:rgba(255,255,255,0.2); }
	.inp:focus {
		border-color:rgba(37,99,235,0.7);
		box-shadow:0 0 0 3px rgba(37,99,235,0.2);
	}

	.btn {
		height:40px; padding:0 16px; margin-top:4px;
		background:#2563eb; color:#fff;
		border:none; border-radius:8px;
		font-size:13.5px; font-weight:600;
		cursor:pointer; font-family:inherit;
		display:flex; align-items:center; justify-content:center; gap:8px;
		transition:background .15s, opacity .15s;
	}
	.btn:hover:not(:disabled) { background:#1d4ed8; }
	.btn:disabled { opacity:.5; cursor:not-allowed; }

	.spin {
		width:14px; height:14px;
		border:2px solid rgba(255,255,255,0.25);
		border-top-color:#fff;
		border-radius:50%;
		animation:spin .7s linear infinite;
	}
	@keyframes spin { to { transform:rotate(360deg); } }

	.back {
		display:block; text-align:center; margin-top:18px;
		font-size:12px; color:rgba(255,255,255,0.28);
		text-decoration:none;
		transition:color .15s;
	}
	.back:hover { color:rgba(255,255,255,0.6); }
</style>
