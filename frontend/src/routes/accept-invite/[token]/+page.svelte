<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { setAuthCookies } from '$lib/auth/cookies';
	import type { PublicInvite } from '$lib/api/types';
	import { CheckCircle, AlertCircle, Loader2, Eye, EyeOff, Lock, Shield, UserCheck } from '@lucide/svelte';

	let token = $derived($page.params.token ?? '');

	let invite     = $state<PublicInvite | null>(null);
	let loading    = $state(true);
	let loadError  = $state('');

	// Authenticated accept
	let accepting  = $state(false);
	let acceptError = $state('');

	// New-user form
	let password   = $state('');
	let password2  = $state('');
	let showPw     = $state(false);
	let submitting = $state(false);
	let submitError = $state('');

	let done = $state(false);

	const ROLE_LABELS: Record<string, string> = {
		owner: 'Owner', admin: 'Admin', member: 'Member', viewer: 'Viewer',
	};

	let isLoggedIn = $derived(!!$authStore.user);

	onMount(async () => {
		const res = await api.getInvitation(token);
		if (res.data) {
			invite = res.data;
		} else {
			loadError = res.error?.message ?? 'Invitation not found.';
		}
		loading = false;
	});

	// ── Authenticated accept (existing user) ─────────────────────────────────────
	async function handleAccept() {
		if (!invite || accepting) return;
		accepting = true;
		acceptError = '';

		const res = await api.acceptInvitation(invite.org_id, token);

		if (res.error) {
			acceptError = res.error.message;
			accepting = false;
			return;
		}

		done = true;
		setTimeout(() => goto('/'), 1500);
	}

	// ── New-user complete (unauthenticated) ──────────────────────────────────────
	async function handleSubmit() {
		if (!password || !password2 || submitting) return;
		if (password !== password2) { submitError = 'Passwords do not match.'; return; }
		if (password.length < 8)   { submitError = 'Password must be at least 8 characters.'; return; }

		submitting = true;
		submitError = '';

		const res = await api.completeInvitation(token, password);

		if (res.error) {
			submitError = res.error.message;
			submitting = false;
			return;
		}

		if (res.data) {
			setAuthCookies(res.data.access_token);
			api.setToken(res.data.access_token);
			authStore.restoreToken(res.data.access_token);
			const me = await api.getMe();
			if (me.data) authStore.setUser(me.data, { access_token: res.data.access_token });
		}

		done = true;
		setTimeout(() => goto('/'), 2000);
	}
</script>

<svelte:head><title>Accept Invitation — Shipyard</title></svelte:head>

<div class="accept-page">
	<div class="accept-card">
		<!-- Header -->
		<div class="card-header">
			<div class="logo-mark"><Shield size={20} /></div>
			<h1 class="card-title">Shipyard</h1>
		</div>

		{#if loading}
			<div class="state-block">
				<div class="spinner"></div>
				<span class="state-text">Loading invitation…</span>
			</div>

		{:else if loadError}
			<div class="state-block error-block">
				<AlertCircle size={32} class="state-icon error-icon" />
				<p class="state-title">Invitation not found</p>
				<p class="state-sub">{loadError}</p>
				<a class="btn btn-primary" href="/login">Go to login</a>
			</div>

		{:else if invite?.is_expired}
			<div class="state-block error-block">
				<AlertCircle size={32} class="state-icon error-icon" />
				<p class="state-title">This invitation has expired</p>
				<p class="state-sub">Ask an admin to send a new invite to <strong>{invite.email}</strong>.</p>
				<a class="btn btn-primary" href="/login">Go to login</a>
			</div>

		{:else if invite?.is_accepted}
			<div class="state-block success-block">
				<CheckCircle size={32} class="state-icon success-icon" />
				<p class="state-title">Invitation already accepted</p>
				<p class="state-sub">This link has already been used. Log in to access <strong>{invite.org_name}</strong>.</p>
				<a class="btn btn-primary" href="/login">Log in</a>
			</div>

		{:else if done}
			<div class="state-block success-block">
				<CheckCircle size={32} class="state-icon success-icon" />
				<p class="state-title">Welcome to {invite?.org_name}!</p>
				<p class="state-sub">{isLoggedIn ? 'You now have access.' : 'Your account is ready.'} Redirecting you to the dashboard…</p>
			</div>

		{:else if invite}
			<!-- Invitation details (shared) -->
			<div class="invite-details">
				<p class="invite-greeting">You've been invited to join</p>
				<h2 class="invite-org">{invite.org_name}</h2>
				<div class="invite-meta">
					<span class="meta-item">
						<span class="meta-label">Email</span>
						<span class="meta-value mono">{invite.email}</span>
					</span>
					<span class="meta-item">
						<span class="meta-label">Role</span>
						<span class="meta-value role-chip">{ROLE_LABELS[invite.role] ?? invite.role}</span>
					</span>
					{#if invite.permissions.length > 0}
						<span class="meta-item">
							<span class="meta-label">Permissions</span>
							<span class="meta-value">{invite.permissions.length} granted</span>
						</span>
					{/if}
					{#if Array.isArray(invite.project_assignments) && invite.project_assignments.length > 0}
						<span class="meta-item">
							<span class="meta-label">Projects</span>
							<span class="meta-value">{invite.project_assignments.length} assigned</span>
						</span>
					{/if}
				</div>
			</div>

			{#if isLoggedIn}
				<!-- ── Already logged in — just accept ───────────────────────────────── -->
				<div class="accept-section">
					<div class="logged-in-note">
						<UserCheck size={14} />
						Logged in as <strong>{$authStore.user?.email}</strong>
					</div>

					{#if acceptError}
						<div class="error-banner"><AlertCircle size={13} />{acceptError}</div>
					{/if}

					<button
						class="btn btn-primary accept-btn"
						onclick={handleAccept}
						disabled={accepting}
					>
						{#if accepting}
							<Loader2 size={14} class="spin" />Joining…
						{:else}
							<UserCheck size={14} />Accept &amp; join {invite.org_name}
						{/if}
					</button>
				</div>

			{:else}
				<!-- ── New user — create password ─────────────────────────────────────── -->
				<form class="password-form" onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
					<p class="form-label-top">Create a password for your new account</p>

					<div class="field-wrap">
						<label class="field-label" for="pw">Password</label>
						<div class="pw-wrap">
							<Lock size={13} class="field-icon" />
							<input
								id="pw"
								class="field-input"
								type={showPw ? 'text' : 'password'}
								placeholder="At least 8 characters"
								bind:value={password}
								autocomplete="new-password"
							/>
							<button type="button" class="pw-toggle" onclick={() => showPw = !showPw} tabindex="-1">
								{#if showPw}<EyeOff size={13} />{:else}<Eye size={13} />{/if}
							</button>
						</div>
					</div>

					<div class="field-wrap">
						<label class="field-label" for="pw2">Confirm password</label>
						<div class="pw-wrap">
							<Lock size={13} class="field-icon" />
							<input
								id="pw2"
								class="field-input"
								type={showPw ? 'text' : 'password'}
								placeholder="Repeat password"
								bind:value={password2}
								autocomplete="new-password"
							/>
						</div>
					</div>

					{#if submitError}
						<div class="error-banner"><AlertCircle size={13} />{submitError}</div>
					{/if}

					<button
						type="submit"
						class="btn btn-primary submit-btn"
						disabled={!password || !password2 || submitting}
					>
						{#if submitting}
							<Loader2 size={14} class="spin" />Creating account…
						{:else}
							Create account &amp; join {invite.org_name}
						{/if}
					</button>

					<p class="login-hint">
						Already have an account? <a href="/login">Log in</a> — then revisit this link to accept.
					</p>
				</form>
			{/if}
		{/if}
	</div>
</div>

<style>
	@keyframes spin { to { transform: rotate(360deg); } }
	:global(.spin) { animation: spin 0.8s linear infinite; }

	.accept-page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-base);
		padding: 20px;
	}

	.accept-card {
		width: 100%;
		max-width: 440px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	/* ── Header ── */
	.card-header {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 20px 24px 18px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.logo-mark {
		width: 34px; height: 34px;
		background: var(--accent);
		border-radius: var(--radius-md);
		display: flex; align-items: center; justify-content: center;
		color: white;
	}

	.card-title {
		font-size: 16px;
		font-weight: 700;
		color: var(--text-primary);
		margin: 0;
		letter-spacing: -0.02em;
	}

	/* ── State blocks ── */
	.state-block {
		display: flex;
		flex-direction: column;
		align-items: center;
		text-align: center;
		padding: 40px 24px;
		gap: 12px;
	}

	.spinner {
		width: 22px; height: 22px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	.state-text { font-size: 13px; color: var(--text-muted); }
	.state-title { font-size: 15px; font-weight: 600; color: var(--text-primary); margin: 0; }
	.state-sub { font-size: 13px; color: var(--text-muted); margin: 0; line-height: 1.5; }
	.state-sub strong { color: var(--text-primary); }

	:global(.state-icon) { opacity: 0.85; }
	:global(.error-icon)   { color: #EF4444; }
	:global(.success-icon) { color: #10B981; }

	/* ── Invite details ── */
	.invite-details {
		padding: 20px 24px 0;
		text-align: center;
	}

	.invite-greeting { font-size: 13px; color: var(--text-muted); margin: 0 0 4px; }
	.invite-org {
		font-size: 20px; font-weight: 700; color: var(--text-primary);
		margin: 0 0 16px; letter-spacing: -0.02em;
	}

	.invite-meta {
		display: flex;
		flex-direction: column;
		gap: 6px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		padding: 12px 14px;
		text-align: left;
	}

	.meta-item { display: flex; align-items: center; gap: 8px; font-size: 12px; }
	.meta-label {
		width: 80px; flex-shrink: 0;
		font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; font-size: 10px; letter-spacing: 0.06em;
	}
	.meta-value { color: var(--text-primary); }
	.meta-value.mono { font-family: var(--font-mono); font-size: 12px; }
	.role-chip {
		display: inline-block;
		font-size: 11px; font-weight: 600;
		padding: 2px 8px; border-radius: 999px;
		background: rgba(37,99,235,0.1); color: var(--accent);
		border: 1px solid rgba(37,99,235,0.2);
	}

	/* ── Authenticated accept section ── */
	.accept-section {
		display: flex; flex-direction: column; gap: 14px;
		padding: 20px 24px 24px;
	}

	.logged-in-note {
		display: flex; align-items: center; gap: 6px;
		font-size: 12px; color: var(--text-muted);
		padding: 8px 12px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
	}
	.logged-in-note strong { color: var(--text-primary); }

	.accept-btn {
		display: flex; align-items: center; justify-content: center; gap: 8px;
		padding: 10px 16px; font-size: 14px; font-weight: 600;
	}

	/* ── Password form ── */
	.password-form {
		display: flex; flex-direction: column; gap: 14px;
		padding: 20px 24px 24px;
	}

	.form-label-top {
		font-size: 13px; color: var(--text-muted);
		margin: 0; text-align: center;
	}

	.field-wrap { display: flex; flex-direction: column; gap: 5px; }
	.field-label { font-size: 11px; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.05em; }

	.pw-wrap { position: relative; display: flex; align-items: center; }
	:global(.field-icon) { position: absolute; left: 10px; color: var(--text-dim); pointer-events: none; }
	.field-input {
		width: 100%; padding: 9px 36px; box-sizing: border-box;
		background: var(--bg-base); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary);
		font-size: 13px; font-family: var(--font-sans); outline: none;
		transition: border-color var(--transition-fast);
	}
	.field-input:focus { border-color: var(--accent); }

	.pw-toggle {
		position: absolute; right: 8px;
		background: none; border: none; cursor: pointer;
		color: var(--text-dim); padding: 4px;
		display: flex; align-items: center;
		transition: color var(--transition-fast);
	}
	.pw-toggle:hover { color: var(--text-primary); }

	.error-banner {
		display: flex; align-items: center; gap: 7px;
		padding: 9px 12px; font-size: 12px;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.2);
		border-radius: var(--radius-sm);
		color: #EF4444;
	}

	.submit-btn {
		display: flex; align-items: center; justify-content: center; gap: 8px;
		padding: 10px 16px; font-size: 14px; font-weight: 600;
		margin-top: 2px;
	}

	.login-hint {
		font-size: 12px; color: var(--text-dim);
		text-align: center; margin: 0; line-height: 1.5;
	}
	.login-hint a { color: var(--accent); text-decoration: none; }
	.login-hint a:hover { text-decoration: underline; }

	/* ── btn ── */
	.btn {
		display: inline-flex; align-items: center; justify-content: center;
		padding: 8px 16px; border-radius: var(--radius-sm);
		font-size: 13px; font-weight: 500; cursor: pointer;
		border: none; text-decoration: none; transition: opacity var(--transition-fast);
	}
	.btn:disabled { opacity: 0.55; cursor: not-allowed; }
	.btn-primary { background: var(--accent); color: white; }
	.btn-primary:hover:not(:disabled) { opacity: 0.88; }

	@media (max-width: 480px) {
		.accept-card { border-radius: 0; border-left: none; border-right: none; }
		.accept-page { padding: 0; align-items: flex-start; }
	}
</style>
