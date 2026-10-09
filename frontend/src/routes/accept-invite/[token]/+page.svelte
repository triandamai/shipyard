<script lang="ts">
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { setAuthCookies } from '$lib/auth/cookies';
	import type { PublicInvite } from '$lib/api/types';
	import { CheckCircle, AlertCircle, Eye, EyeOff, Lock, Shield, UserCheck, XCircle } from '@lucide/svelte';
	import { Button, Card, FormField, TextField, InlineAlert, Spinner, Avatar } from '$lib/components/ui';

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
	let declined = $state(false);

	// Decline / reject invitation
	let declining  = $state(false);
	let declineError = $state('');

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

	// ── Decline invitation ────────────────────────────────────────────────────────
	async function handleDecline() {
		if (declining) return;
		declining = true;
		declineError = '';
		const res = await api.rejectInvitation(token);
		if (res.error) {
			declineError = res.error.message;
			declining = false;
			return;
		}
		declined = true;
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
	<div class="accept-wrap">
		<Card padding="0">
			<!-- Header -->
			<div class="card-header">
				<div class="logo-mark"><Shield size={20} /></div>
				<h1 class="card-title">Shipyard</h1>
			</div>

			{#if loading}
				<div class="state-block">
					<Spinner size={22} />
					<span class="state-text">Loading invitation…</span>
				</div>

			{:else if loadError}
				<div class="state-block">
					<span class="state-icon error-icon"><AlertCircle size={32} /></span>
					<p class="state-title">Invitation not found</p>
					<p class="state-sub">{loadError}</p>
					<Button href="/login">Go to login</Button>
				</div>

			{:else if invite?.is_expired}
				<div class="state-block">
					<span class="state-icon error-icon"><AlertCircle size={32} /></span>
					<p class="state-title">This invitation has expired</p>
					<p class="state-sub">Ask an admin to send a new invite to <strong>{invite.email}</strong>.</p>
					<Button href="/login">Go to login</Button>
				</div>

			{:else if invite?.is_accepted}
				<div class="state-block">
					<span class="state-icon success-icon"><CheckCircle size={32} /></span>
					<p class="state-title">Invitation already accepted</p>
					<p class="state-sub">This link has already been used. Log in to access <strong>{invite.org_name}</strong>.</p>
					<Button href="/login">Log in</Button>
				</div>

			{:else if declined}
				<div class="state-block">
					<span class="state-icon decline-icon"><XCircle size={32} /></span>
					<p class="state-title">Invitation declined</p>
					<p class="state-sub">You have declined the invitation to <strong>{invite?.org_name}</strong>. The admin can send a new invite if needed.</p>
					<Button href="/login">Go to login</Button>
				</div>

			{:else if done}
				<div class="state-block" role="status">
					<span class="state-icon success-icon"><CheckCircle size={32} /></span>
					<p class="state-title">Welcome to {invite?.org_name}!</p>
					<p class="state-sub">{isLoggedIn ? 'You now have access.' : 'Your account is ready.'} Redirecting you to the dashboard…</p>
				</div>

			{:else if invite}
				<!-- Invitation details (shared) -->
				<div class="invite-details">
					<div class="invite-avatar"><Avatar initials={invite.org_name} size={44} /></div>
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
							<div role="alert"><InlineAlert tone="error">{acceptError}</InlineAlert></div>
						{/if}

						<div class="full">
							<Button onclick={handleAccept} disabled={accepting || declining}>
								{#if accepting}
									<Spinner size={14} tone="current" />Joining…
								{:else}
									<UserCheck size={14} />Accept &amp; join {invite.org_name}
								{/if}
							</Button>
						</div>

						{#if declineError}
							<div role="alert"><InlineAlert tone="error">{declineError}</InlineAlert></div>
						{/if}

						<div class="full">
							<Button variant="ghost" onclick={handleDecline} disabled={accepting || declining}>
								{#if declining}
									<Spinner size={13} tone="current" />Declining…
								{:else}
									<XCircle size={13} />Decline invitation
								{/if}
							</Button>
						</div>
					</div>

				{:else}
					<!-- ── New user — create password ─────────────────────────────────────── -->
					<form class="password-form" onsubmit={(e) => { e.preventDefault(); handleSubmit(); }}>
						<p class="form-label-top">Create a password for your new account</p>

						<FormField label="Password" for="pw">
							<div class="pw-wrap">
								<TextField
									id="pw"
									type={showPw ? 'text' : 'password'}
									placeholder="At least 8 characters"
									bind:value={password}
									autocomplete="new-password"
								>
									{#snippet icon()}<Lock size={13} />{/snippet}
								</TextField>
								<button
									type="button"
									class="pw-toggle"
									onclick={() => showPw = !showPw}
									tabindex="-1"
									aria-label={showPw ? 'Hide password' : 'Show password'}
								>
									{#if showPw}<EyeOff size={13} />{:else}<Eye size={13} />{/if}
								</button>
							</div>
						</FormField>

						<FormField label="Confirm password" for="pw2">
							<TextField
								id="pw2"
								type={showPw ? 'text' : 'password'}
								placeholder="Repeat password"
								bind:value={password2}
								autocomplete="new-password"
							>
								{#snippet icon()}<Lock size={13} />{/snippet}
							</TextField>
						</FormField>

						{#if submitError}
							<div role="alert"><InlineAlert tone="error">{submitError}</InlineAlert></div>
						{/if}

						<div class="full">
							<Button type="submit" disabled={!password || !password2 || submitting || declining}>
								{#if submitting}
									<Spinner size={14} tone="current" />Creating account…
								{:else}
									Create account &amp; join {invite.org_name}
								{/if}
							</Button>
						</div>

						{#if declineError}
							<div role="alert"><InlineAlert tone="error">{declineError}</InlineAlert></div>
						{/if}

						<div class="full">
							<Button variant="ghost" onclick={handleDecline} disabled={submitting || declining}>
								{#if declining}
									<Spinner size={13} tone="current" />Declining…
								{:else}
									<XCircle size={13} />Decline invitation
								{/if}
							</Button>
						</div>

						<p class="login-hint">
							Already have an account? <a href="/login">Log in</a> — then revisit this link to accept.
						</p>
					</form>
				{/if}
			{/if}
		</Card>
	</div>
</div>

<style>
	.accept-page {
		min-height: 100vh;
		display: flex;
		align-items: center;
		justify-content: center;
		background: var(--bg-base);
		padding: 20px;
	}

	.accept-wrap {
		width: 100%;
		max-width: 440px;
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
		color: var(--accent-fg);
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

	.state-text { font-size: 13px; color: var(--text-muted); }
	.state-title { font-size: 15px; font-weight: 600; color: var(--text-primary); margin: 0; }
	.state-sub { font-size: 13px; color: var(--text-muted); margin: 0; line-height: 1.5; }
	.state-sub strong { color: var(--text-primary); }

	.state-icon { display: flex; opacity: 0.85; }
	.error-icon { color: var(--accent-red); }
	.success-icon { color: var(--accent-green); }
	.decline-icon { color: var(--text-muted); }

	/* ── Invite details ── */
	.invite-details {
		padding: 20px 24px 0;
		text-align: center;
	}

	.invite-avatar { display: flex; justify-content: center; margin-bottom: 10px; }
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
		font-weight: 600; color: var(--text-dim); font-size: 10px;
	}
	.meta-value { color: var(--text-primary); min-width: 0; overflow-wrap: anywhere; }
	.meta-value.mono { font-family: var(--font-mono); font-size: 12px; }
	.role-chip {
		display: inline-block;
		font-size: 11px; font-weight: 600;
		padding: 2px 8px; border-radius: 999px;
		background: var(--accent-muted); color: var(--accent);
		border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
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

	.full :global(.ui-btn) { width: 100%; }

	/* ── Password form ── */
	.password-form {
		display: flex; flex-direction: column; gap: 14px;
		padding: 20px 24px 24px;
	}

	.form-label-top {
		font-size: 13px; color: var(--text-muted);
		margin: 0; text-align: center;
	}

	.pw-wrap { position: relative; }
	.pw-wrap :global(.ui-textfield) { padding-right: 36px; }
	.pw-toggle {
		position: absolute; right: 8px; top: 50%; transform: translateY(-50%);
		background: none; border: none; cursor: pointer;
		color: var(--text-dim); padding: 4px;
		display: flex; align-items: center;
		transition: color var(--transition-fast);
	}
	.pw-toggle:hover { color: var(--text-primary); }

	.login-hint {
		font-size: 12px; color: var(--text-dim);
		text-align: center; margin: 0; line-height: 1.5;
	}
	.login-hint a { color: var(--accent); text-decoration: none; }
	.login-hint a:hover { text-decoration: underline; }

	@media (max-width: 480px) {
		.accept-wrap { border-radius: 0; }
		.accept-wrap :global(.ui-card) { border-radius: 0; border-left: none; border-right: none; }
		.accept-page { padding: 0; align-items: flex-start; }
	}
</style>
