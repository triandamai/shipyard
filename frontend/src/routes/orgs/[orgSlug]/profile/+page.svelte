<script lang="ts">
	import { User, Lock, CheckCircle, AlertCircle, LogOut } from '@lucide/svelte';
	import { formatDistanceToNow } from 'date-fns';
	import { authStore } from '$lib/stores/auth.store';
	import { api } from '$lib/api/client';
	import { clearAuthCookies } from '$lib/auth/cookies';

	// ── User info ─────────────────────────────────────────────────────────────
	let user      = $derived($authStore.user);
	let email     = $derived(user?.email ?? '');
	let initials  = $derived(email ? email.slice(0, 2).toUpperCase() : 'U');
	let memberSince = $derived(
		user?.created_at
			? formatDistanceToNow(new Date(user.created_at), { addSuffix: true })
			: '—'
	);

	// ── Password change ────────────────────────────────────────────────────────
	let currentPassword = $state('');
	let newPassword     = $state('');
	let confirmPassword = $state('');
	let saving          = $state(false);
	let saveError       = $state('');
	let saveSuccess     = $state(false);

	let passwordMismatch = $derived(confirmPassword.length > 0 && newPassword !== confirmPassword);
	let canSave = $derived(
		currentPassword.length > 0 &&
		newPassword.length >= 8 &&
		newPassword === confirmPassword &&
		!saving
	);

	async function handleChangePassword(e: SubmitEvent) {
		e.preventDefault();
		if (!canSave) return;
		saving = true;
		saveError = '';
		saveSuccess = false;

		const res = await api.changePassword(currentPassword, newPassword);
		if (res.error) {
			saveError = res.error.message;
		} else {
			saveSuccess = true;
			currentPassword = '';
			newPassword = '';
			confirmPassword = '';
			setTimeout(() => { saveSuccess = false; }, 3500);
		}
		saving = false;
	}

	// ── Sign out ───────────────────────────────────────────────────────────────
	async function logout() {
		await api.logout();
		clearAuthCookies();
		authStore.logout();
		api.setToken(null);
		window.location.href = '/login';
	}
</script>

<div class="profile-root">
	<div class="profile-header">
		<h1 class="profile-title">Profile</h1>
		<p class="profile-subtitle">Your account information and security settings</p>
	</div>

<div class="profile-page">

	<!-- ── Profile card ─────────────────────────────────────────────────── -->
	<section class="card">
		<div class="card-header">
			<User size={14} />
			<span>Account</span>
		</div>
		<div class="card-body">
			<div class="avatar-row">
				<div class="avatar">{initials}</div>
				<div class="avatar-info">
					<span class="avatar-email">{email}</span>
					<span class="avatar-since">Member {memberSince}</span>
				</div>
			</div>
			<div class="fields">
				<div class="field">
					<span class="field-label">Email</span>
					<span class="field-value">{email}</span>
				</div>
				<div class="field">
					<span class="field-label">Account ID</span>
					<span class="field-value mono">{user?.id ?? '—'}</span>
				</div>
				<div class="field">
					<span class="field-label">Joined</span>
					<span class="field-value">{memberSince}</span>
				</div>
			</div>
		</div>
	</section>

	<!-- ── Change password ──────────────────────────────────────────────── -->
	<section class="card">
		<div class="card-header">
			<Lock size={14} />
			<span>Change Password</span>
		</div>
		<form class="card-body" onsubmit={handleChangePassword}>
			<div class="form-fields">
				<div class="form-field">
					<label class="form-label" for="current-password">Current password</label>
					<input
						id="current-password"
						class="form-input"
						type="password"
						placeholder="••••••••"
						bind:value={currentPassword}
						autocomplete="current-password"
						required
					/>
				</div>
				<div class="form-field">
					<label class="form-label" for="new-password">New password</label>
					<input
						id="new-password"
						class="form-input"
						type="password"
						placeholder="Min. 8 characters"
						bind:value={newPassword}
						autocomplete="new-password"
						required
						minlength="8"
					/>
					{#if newPassword.length > 0 && newPassword.length < 8}
						<span class="field-hint error">Must be at least 8 characters</span>
					{/if}
				</div>
				<div class="form-field">
					<label class="form-label" for="confirm-password">Confirm new password</label>
					<input
						id="confirm-password"
						class="form-input"
						class:input-error={passwordMismatch}
						type="password"
						placeholder="••••••••"
						bind:value={confirmPassword}
						autocomplete="new-password"
						required
					/>
					{#if passwordMismatch}
						<span class="field-hint error">Passwords do not match</span>
					{/if}
				</div>
			</div>

			{#if saveError}
				<div class="alert alert-error">
					<AlertCircle size={13} />{saveError}
				</div>
			{/if}
			{#if saveSuccess}
				<div class="alert alert-success">
					<CheckCircle size={13} />Password updated successfully.
				</div>
			{/if}

			<div class="form-footer">
				<button class="btn btn-primary" type="submit" disabled={!canSave}>
					{#if saving}
						<span class="btn-spinner"></span>Saving…
					{:else}
						<Lock size={13} />Update Password
					{/if}
				</button>
			</div>
		</form>
	</section>

	<!-- ── Danger zone ──────────────────────────────────────────────────── -->
	<section class="card card-danger">
		<div class="card-header danger">
			<LogOut size={14} />
			<span>Session</span>
		</div>
		<div class="card-body">
			<div class="danger-row">
				<div>
					<p class="danger-title">Sign out of Shipyard</p>
					<p class="danger-desc">You will need to sign in again to access the dashboard.</p>
				</div>
				<button class="btn btn-danger-outline" onclick={logout}>
					<LogOut size={13} />Sign out
				</button>
			</div>
		</div>
	</section>

</div>
</div>

<style>
	.profile-root {
		padding: 28px 32px 40px;
		display: flex;
		flex-direction: column;
		gap: 24px;
		height: 100%;
		overflow-y: auto;
		box-sizing: border-box;
	}

	@media (max-width: 639px) {
		.profile-root { padding: 16px 16px 72px; }
	}
	.profile-header { display: flex; flex-direction: column; gap: 3px; }
	.profile-title { font-size: 22px; font-weight: 700; color: var(--text-primary); letter-spacing: -0.02em; margin: 0; }
	.profile-subtitle { font-size: 13px; color: var(--text-muted); margin: 0; }

	.profile-page {
		max-width: 560px;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	@media (max-width: 639px) {
		.profile-root { padding: 16px 16px 32px; }
	}

	/* ── Card ── */
	.card {
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
		background: var(--bg-surface);
	}
	.card-danger { border-color: rgba(239, 68, 68, 0.25); }

	.card-header {
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 11px 16px;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.07em;
	}
	.card-header.danger { color: #EF4444; background: rgba(239, 68, 68, 0.04); border-bottom-color: rgba(239, 68, 68, 0.2); }

	.card-body {
		padding: 16px;
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	/* ── Avatar row ── */
	.avatar-row {
		display: flex;
		align-items: center;
		gap: 14px;
	}
	.avatar {
		width: 52px;
		height: 52px;
		border-radius: 50%;
		background: rgba(59, 130, 246, 0.15);
		border: 2px solid rgba(59, 130, 246, 0.3);
		display: flex;
		align-items: center;
		justify-content: center;
		font-size: 17px;
		font-weight: 700;
		color: #60A5FA;
		flex-shrink: 0;
		letter-spacing: 0.02em;
	}
	.avatar-info { display: flex; flex-direction: column; gap: 3px; }
	.avatar-email { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.avatar-since { font-size: 12px; color: var(--text-muted); }

	/* ── Fields ── */
	.fields {
		display: flex;
		flex-direction: column;
		gap: 0;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
	}
	.field {
		display: flex;
		align-items: baseline;
		gap: 12px;
		padding: 9px 12px;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
	}
	.field:last-child { border-bottom: none; }
	.field-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		min-width: 90px;
		flex-shrink: 0;
	}
	.field-value { color: var(--text-primary); word-break: break-all; }
	.field-value.mono { font-family: var(--font-mono); font-size: 12px; color: var(--text-secondary); }

	/* ── Form ── */
	.form-fields { display: flex; flex-direction: column; gap: 12px; }
	.form-field { display: flex; flex-direction: column; gap: 5px; }
	.form-label {
		font-size: 12px;
		font-weight: 500;
		color: var(--text-secondary);
	}
	.form-input {
		height: 36px;
		padding: 0 10px;
		font-size: 13px;
		font-family: var(--font-sans);
		color: var(--text-primary);
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		outline: none;
		transition: border-color var(--transition-fast);
		width: 100%;
		box-sizing: border-box;
	}
	.form-input:focus { border-color: var(--accent); }
	.form-input.input-error { border-color: #EF4444; }
	.field-hint { font-size: 11px; }
	.field-hint.error { color: #EF4444; }

	.alert {
		display: flex;
		align-items: center;
		gap: 7px;
		padding: 9px 12px;
		border-radius: var(--radius-sm);
		font-size: 13px;
	}
	.alert-error { background: rgba(239,68,68,0.08); border: 1px solid rgba(239,68,68,0.2); color: #EF4444; }
	.alert-success { background: rgba(16,185,129,0.08); border: 1px solid rgba(16,185,129,0.2); color: #10B981; }

	.form-footer { display: flex; justify-content: flex-end; }

	/* ── Danger row ── */
	.danger-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.danger-title { font-size: 13px; font-weight: 500; color: var(--text-primary); margin: 0 0 2px; }
	.danger-desc { font-size: 12px; color: var(--text-muted); margin: 0; }

	/* ── Buttons ── */
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		font-size: 13px;
		font-weight: 500;
		font-family: var(--font-sans);
		border-radius: var(--radius-sm);
		border: 1px solid transparent;
		cursor: pointer;
		transition: all var(--transition-fast);
		white-space: nowrap;
	}
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-primary {
		background: var(--accent);
		color: white;
		border-color: var(--accent);
	}
	.btn-primary:hover:not(:disabled) { background: var(--accent-hover); }
	.btn-danger-outline {
		background: transparent;
		color: #EF4444;
		border-color: rgba(239, 68, 68, 0.4);
	}
	.btn-danger-outline:hover { background: rgba(239, 68, 68, 0.08); border-color: #EF4444; }

	.btn-spinner {
		width: 12px; height: 12px;
		border: 2px solid rgba(255,255,255,0.35);
		border-top-color: white;
		border-radius: 50%;
		animation: spin 0.65s linear infinite;
	}
	@keyframes spin { to { transform: rotate(360deg); } }
</style>
