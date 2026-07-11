<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { setAuthCookies } from '$lib/auth/cookies';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	type Step = 'welcome' | 'account' | 'done';

	let step = $state<Step>('welcome');

	// Step 1 — Docker check
	let dockerStatus = $state<'idle' | 'checking' | 'ok' | 'error'>('idle');
	let dockerMessage = $state('');

	// Step 2 — Admin account
	let adminEmail = $state('');
	let adminPassword = $state('');
	let orgName = $state('');
	let orgSlug = $state('');
	let submitError = $state('');
	let submitting = $state(false);

	// Auto-derive slug from org name
	$effect(() => {
		orgSlug = orgName
			.toLowerCase()
			.replace(/\s+/g, '-')
			.replace(/[^a-z0-9-]/g, '')
			.replace(/-+/g, '-')
			.replace(/^-|-$/g, '');
	});

	async function checkDocker() {
		dockerStatus = 'checking';
		const res = await api.checkDocker();
		if (res.data?.success) {
			dockerStatus = 'ok';
			dockerMessage = res.data.message;
		} else {
			dockerStatus = 'error';
			dockerMessage = res.error?.message ?? res.data?.message ?? 'Docker is not available.';
		}
	}

	async function handleSetupSubmit(e: SubmitEvent) {
		e.preventDefault();
		submitError = '';
		submitting = true;

		try {
			const res = await api.setupInit({
				admin_email: adminEmail,
				admin_password: adminPassword,
				org_name: orgName,
				org_slug: orgSlug || undefined
			});

			if (res.error) {
				submitError = res.error.message;
				return;
			}

			// Auto sign-in: store tokens and redirect to admin panel.
			if (res.data?.access_token) {
				setAuthCookies(res.data.access_token);
				// Fetch the full user object to populate the auth store.
				const meRes = await api.getMe();
				if (meRes.data) {
					authStore.setUser(meRes.data as any, { access_token: res.data.access_token });
				}
				// Redirect to admin panel — superadmin lands there first.
				goto('/admin');
				return;
			}

			step = 'done';
		} finally {
			submitting = false;
		}
	}

	const stepIndex = $derived(step === 'welcome' ? 0 : step === 'account' ? 1 : 2);
	const steps = ['Welcome', 'Create Admin', 'Done'];
</script>

{#if data.alreadySetup}
<div style="min-height: 100vh; background: var(--bg-base); display: flex; align-items: center; justify-content: center; padding: 24px;">
	<div style="width: 100%; max-width: 480px; text-align: center; display: flex; flex-direction: column; align-items: center; gap: 24px;">
		<div style="
			width: 72px; height: 72px; border-radius: 50%;
			background: rgba(59,130,246,0.1); border: 1px solid rgba(59,130,246,0.25);
			display: flex; align-items: center; justify-content: center; font-size: 32px;
		">⚓</div>
		<div>
			<h1 style="font-size: 24px; font-weight: 700; margin: 0 0 10px; color: var(--text-primary);">
				Shipyard is already set up
			</h1>
			<p style="font-size: 14px; line-height: 1.7; color: var(--text-muted); margin: 0;">
				This instance has already been initialized. Setup can only be performed once.<br />
				If you need to make changes, sign in as an owner and use the Settings page.
			</p>
		</div>
		<button class="btn btn-primary" onclick={() => goto('/login')}>
			Go to Login
		</button>
	</div>
</div>
{:else}
<div
	style="
		min-height: 100vh;
		background: var(--bg-base);
		display: flex;
		align-items: center;
		justify-content: center;
		padding: 24px;
	"
>
	<div style="width: 100%; max-width: 560px; display: flex; flex-direction: column; gap: 32px;">

		<!-- Header -->
		<div style="text-align: center;">
			<h1 style="font-size: 28px; font-weight: 700; margin-bottom: 8px;">Shipyard Setup</h1>
			<p style="color: var(--text-muted); font-size: 14px;">
				Configure your Shipyard instance
			</p>
		</div>

		<!-- Step indicators -->
		<div style="display: flex; align-items: center; gap: 0;">
			{#each steps as label, i}
				<div style="display: flex; align-items: center; flex: 1;">
					<div style="display: flex; flex-direction: column; align-items: center; gap: 6px; flex: 1;">
						<div
							style="
								width: 32px;
								height: 32px;
								border-radius: 50%;
								display: flex;
								align-items: center;
								justify-content: center;
								font-size: 13px;
								font-weight: 600;
								transition: all 0.2s;
								background: {i < stepIndex
									? 'var(--accent-green)'
									: i === stepIndex
									? 'var(--accent)'
									: 'var(--bg-elevated)'};
								color: {i <= stepIndex ? 'white' : 'var(--text-muted)'};
								border: 1px solid {i < stepIndex
									? 'var(--accent-green)'
									: i === stepIndex
									? 'var(--accent)'
									: 'var(--border)'};
							"
						>
							{#if i < stepIndex}
								✓
							{:else}
								{i + 1}
							{/if}
						</div>
						<span
							style="
								font-size: 11px;
								font-weight: 500;
								color: {i === stepIndex ? 'var(--text-primary)' : 'var(--text-muted)'};
							"
						>
							{label}
						</span>
					</div>

					{#if i < steps.length - 1}
						<div
							style="
								height: 1px;
								flex: 1;
								margin-bottom: 20px;
								background: {i < stepIndex ? 'var(--accent-green)' : 'var(--border)'};
								transition: background 0.3s;
							"
						></div>
					{/if}
				</div>
			{/each}
		</div>

		<!-- Step content card -->
		<div
			style="
				background: var(--bg-surface);
				border: 1px solid var(--border);
				border-radius: var(--radius-lg);
				padding: 32px;
			"
		>

			<!-- Step 1: Welcome / Docker check -->
			{#if step === 'welcome'}
				<div style="display: flex; flex-direction: column; gap: 24px;">
					<div>
						<h2 style="font-size: 18px; margin-bottom: 8px;">Welcome to Shipyard</h2>
						<p style="color: var(--text-muted); font-size: 14px; line-height: 1.6;">
							Before getting started, let's verify that your environment is ready.
							Click the button below to check Docker availability.
						</p>
					</div>

					<!-- Docker status -->
					<div
						style="
							display: flex;
							align-items: center;
							gap: 12px;
							padding: 14px 16px;
							border-radius: var(--radius-md);
							background: var(--bg-elevated);
							border: 1px solid var(--border);
						"
					>
						<div
							style="
								width: 36px;
								height: 36px;
								border-radius: var(--radius-sm);
								display: flex;
								align-items: center;
								justify-content: center;
								font-size: 18px;
								background: {dockerStatus === 'ok'
									? 'var(--accent-green-muted)'
									: dockerStatus === 'error'
									? 'var(--accent-red-muted)'
									: 'var(--bg-hover)'};
							"
						>
							{#if dockerStatus === 'checking'}
								<span
									style="
										width: 16px;
										height: 16px;
										border: 2px solid var(--border);
										border-top-color: var(--accent);
										border-radius: 50%;
										display: inline-block;
										animation: spin 0.7s linear infinite;
									"
								></span>
							{:else if dockerStatus === 'ok'}
								✅
							{:else if dockerStatus === 'error'}
								❌
							{:else}
								🐳
							{/if}
						</div>
						<div style="flex: 1; min-width: 0;">
							<div style="font-size: 13px; font-weight: 500; color: var(--text-primary);">Docker</div>
							<div style="font-size: 12px; color: var(--text-muted); margin-top: 2px;">
								{#if dockerStatus === 'idle'}
									Not checked yet
								{:else if dockerStatus === 'checking'}
									Checking…
								{:else if dockerStatus === 'ok'}
									{dockerMessage || 'Docker is available'}
								{:else}
									{dockerMessage || 'Docker not available'}
								{/if}
							</div>
						</div>
						<button
							class="btn btn-secondary btn-sm"
							onclick={checkDocker}
							disabled={dockerStatus === 'checking'}
						>
							{dockerStatus === 'idle' ? 'Check' : 'Retry'}
						</button>
					</div>

					<div style="display: flex; justify-content: flex-end;">
						<button
							class="btn btn-primary"
							disabled={dockerStatus !== 'ok'}
							onclick={() => (step = 'account')}
						>
							Continue
						</button>
					</div>
				</div>

			<!-- Step 2: Create admin account -->
			{:else if step === 'account'}
				<div style="display: flex; flex-direction: column; gap: 24px;">
					<div>
						<h2 style="font-size: 18px; margin-bottom: 8px;">Create Admin Account</h2>
						<p style="color: var(--text-muted); font-size: 14px; line-height: 1.6;">
							Set up your admin user and the first organization.
						</p>
					</div>

					{#if submitError}
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
							{submitError}
						</div>
					{/if}

					<form
						onsubmit={handleSetupSubmit}
						style="display: flex; flex-direction: column; gap: 16px;"
					>
						<div style="display: flex; flex-direction: column; gap: 6px;">
							<label
								for="admin-email"
								style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
							>
								Admin Email
							</label>
							<input
								id="admin-email"
								type="email"
								class="input"
								placeholder="admin@example.com"
								bind:value={adminEmail}
								required
								autocomplete="email"
							/>
						</div>

						<div style="display: flex; flex-direction: column; gap: 6px;">
							<label
								for="admin-password"
								style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
							>
								Admin Password
							</label>
							<input
								id="admin-password"
								type="password"
								class="input"
								placeholder="••••••••"
								bind:value={adminPassword}
								required
								autocomplete="new-password"
							/>
						</div>

						<div style="display: flex; flex-direction: column; gap: 6px;">
							<label
								for="org-name"
								style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
							>
								Organization Name
							</label>
							<input
								id="org-name"
								type="text"
								class="input"
								placeholder="My Organization"
								bind:value={orgName}
								required
							/>
						</div>

						<div style="display: flex; flex-direction: column; gap: 6px;">
							<label
								for="org-slug"
								style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
							>
								Organization Slug
							</label>
							<input
								id="org-slug"
								type="text"
								class="input"
								placeholder="my-organization"
								bind:value={orgSlug}
								required
								pattern="[a-z0-9][a-z0-9-]*"
							/>
							<span style="font-size: 11px; color: var(--text-dim);">
								Used in URLs — lowercase letters, numbers, and hyphens only
							</span>
						</div>

						<div
							style="display: flex; justify-content: space-between; align-items: center; margin-top: 4px;"
						>
							<button
								type="button"
								class="btn btn-ghost"
								onclick={() => (step = 'welcome')}
								disabled={submitting}
							>
								Back
							</button>
							<button
								type="submit"
								class="btn btn-primary"
								disabled={submitting}
							>
								{#if submitting}
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
									Setting up…
								{:else}
									Initialize Shipyard
								{/if}
							</button>
						</div>
					</form>
				</div>

			<!-- Step 3: Done -->
			{:else}
				<div
					style="
						display: flex;
						flex-direction: column;
						align-items: center;
						gap: 20px;
						text-align: center;
						padding: 16px 0;
					"
				>
					<div
						style="
							width: 64px;
							height: 64px;
							border-radius: 50%;
							background: var(--accent-green-muted);
							border: 1px solid var(--accent-green);
							display: flex;
							align-items: center;
							justify-content: center;
							font-size: 28px;
						"
					>
						✓
					</div>
					<div>
						<h2 style="font-size: 20px; margin-bottom: 8px; color: var(--accent-green);">
							Shipyard is ready!
						</h2>
						<p style="color: var(--text-muted); font-size: 14px; line-height: 1.6;">
							Your super admin account and organization have been created.
						</p>
					</div>
					<button
						class="btn btn-primary"
						style="margin-top: 8px;"
						onclick={() => goto('/admin')}
					>
						Go to Admin Panel
					</button>
				</div>
			{/if}

		</div>
	</div>
</div>
{/if}

<style>
	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
