<script lang="ts">
	import { User, Lock, CheckCircle, AlertCircle, LogOut } from '@lucide/svelte';
	import { formatDistanceToNow } from 'date-fns';
	import { authStore } from '$lib/stores/auth.store';
	import { api } from '$lib/api/client';
	import { clearAuthCookies } from '$lib/auth/cookies';
	import {
		PageHeader, Card, FormField, TextField, Button, InlineAlert, Avatar,
		SectionLabel, KeyValueList, Spinner
	} from '$lib/components/ui';

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

	let accountItems = $derived([
		{ key: 'Email', value: email },
		{ key: 'Account ID', value: user?.id ?? '—', mono: true },
		{ key: 'Joined', value: memberSince }
	]);

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
	<PageHeader title="Profile" subtitle="Your account information and security settings" />

<div class="profile-page">

	<!-- ── Profile card ─────────────────────────────────────────────────── -->
	<section>
		<SectionLabel><span class="sl"><User size={12} />Account</span></SectionLabel>
		<Card>
			<div class="card-body">
				<div class="avatar-row">
					<Avatar {initials} size={52} />
					<div class="avatar-info">
						<span class="avatar-email">{email}</span>
						<span class="avatar-since">Member {memberSince}</span>
					</div>
				</div>
				<KeyValueList items={accountItems} />
			</div>
		</Card>
	</section>

	<!-- ── Change password ──────────────────────────────────────────────── -->
	<section>
		<SectionLabel><span class="sl"><Lock size={12} />Change Password</span></SectionLabel>
		<Card>
			<form class="card-body" onsubmit={handleChangePassword}>
				<div class="form-fields">
					<FormField label="Current password" for="current-password">
						<TextField
							id="current-password"
							type="password"
							placeholder="••••••••"
							bind:value={currentPassword}
							autocomplete="current-password"
							required
						/>
					</FormField>
					<FormField
						label="New password"
						for="new-password"
						error={newPassword.length > 0 && newPassword.length < 8 ? 'Must be at least 8 characters' : undefined}
					>
						<TextField
							id="new-password"
							type="password"
							placeholder="Min. 8 characters"
							bind:value={newPassword}
							autocomplete="new-password"
							required
							minlength={8}
						/>
					</FormField>
					<FormField
						label="Confirm new password"
						for="confirm-password"
						error={passwordMismatch ? 'Passwords do not match' : undefined}
					>
						<TextField
							id="confirm-password"
							type="password"
							placeholder="••••••••"
							bind:value={confirmPassword}
							autocomplete="new-password"
							required
						/>
					</FormField>
				</div>

				{#if saveError}
					<div role="alert"><InlineAlert tone="error">{saveError}</InlineAlert></div>
				{/if}
				{#if saveSuccess}
					<div role="status"><InlineAlert tone="success">Password updated successfully.</InlineAlert></div>
				{/if}

				<div class="form-footer">
					<Button type="submit" disabled={!canSave}>
						{#if saving}
							<Spinner size={12} tone="current" />Saving…
						{:else}
							<Lock size={13} />Update Password
						{/if}
					</Button>
				</div>
			</form>
		</Card>
	</section>

	<!-- ── Danger zone ──────────────────────────────────────────────────── -->
	<section>
		<SectionLabel><span class="sl"><LogOut size={12} />Session</span></SectionLabel>
		<Card tone="danger">
			<div class="danger-row">
				<div>
					<p class="danger-title">Sign out of Shipyard</p>
					<p class="danger-desc">You will need to sign in again to access the dashboard.</p>
				</div>
				<Button variant="danger-outline" onclick={logout}>
					<LogOut size={13} />Sign out
				</Button>
			</div>
		</Card>
	</section>

</div>
</div>

<style>
	.profile-root {
		padding: 28px 32px 40px;
		height: 100%;
		overflow-y: auto;
		box-sizing: border-box;
	}

	@media (max-width: 639px) {
		.profile-root { padding: 16px 16px 72px; }
	}

	.profile-page {
		max-width: 560px;
		display: flex;
		flex-direction: column;
		gap: 20px;
	}

	.sl { display: inline-flex; align-items: center; gap: 6px; }

	.card-body {
		display: flex;
		flex-direction: column;
		gap: 14px;
	}

	.avatar-row {
		display: flex;
		align-items: center;
		gap: 14px;
	}
	.avatar-info { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
	.avatar-email { font-size: 14px; font-weight: 600; color: var(--text-primary); word-break: break-all; }
	.avatar-since { font-size: 12px; color: var(--text-muted); }

	.form-fields { display: flex; flex-direction: column; gap: 12px; }
	.form-footer { display: flex; justify-content: flex-end; }

	.danger-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 16px;
	}
	.danger-title { font-size: 13px; font-weight: 500; color: var(--text-primary); margin: 0 0 2px; }
	.danger-desc { font-size: 12px; color: var(--text-muted); margin: 0; }
</style>
