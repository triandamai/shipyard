<script lang="ts">
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { setAuthCookies } from '$lib/auth/cookies';
	import type { Plan } from '$lib/api/types';
	import type { PageData } from './$types';

	let { data }: { data: PageData } = $props();

	type Step = 'welcome' | 'account' | 'plan' | 'done';

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

	// Step 3 — Plan selection
	let plans = $state<Plan[]>([]);
	let plansLoading = $state(false);
	let plansError = $state<string | null>(null);
	let selectedPlanId = $state<string | null>(null);

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

	async function goToPlan() {
		plansLoading = true;
		plansError = null;
		const res = await api.getPlans();
		if (res.data) {
			plans = res.data;
			// Pre-select free plan
			const free = plans.find((p) => p.price_monthly === 0);
			if (free) selectedPlanId = free.id;
		} else {
			plansError = res.error?.message ?? 'Failed to load plans';
		}
		plansLoading = false;
		step = 'plan';
	}

	async function handleSetupSubmit() {
		submitError = '';
		submitting = true;

		try {
			const res = await api.setupInit({
				admin_email: adminEmail,
				admin_password: adminPassword,
				org_name: orgName,
				org_slug: orgSlug || undefined,
				plan_id: selectedPlanId || undefined,
			});

			if (res.error) {
				submitError = res.error.message;
				step = 'account';
				return;
			}

			if ((res.data as any)?.access_token) {
				setAuthCookies((res.data as any).access_token);
				const meRes = await api.getMe();
				if (meRes.data) {
					authStore.setUser(meRes.data as any, { access_token: (res.data as any).access_token });
				}
				goto('/admin');
				return;
			}

			step = 'done';
		} finally {
			submitting = false;
		}
	}

	function formatLimit(v: number) {
		return v === -1 ? '∞' : String(v);
	}

	const stepLabels = ['Welcome', 'Create Admin', 'Select Plan', 'Done'];
	const stepIndex = $derived(
		step === 'welcome' ? 0 : step === 'account' ? 1 : step === 'plan' ? 2 : 3
	);
</script>

{#if data.alreadySetup}
<div style="min-height:100vh;background:var(--bg-base);display:flex;align-items:center;justify-content:center;padding:24px;">
	<div style="width:100%;max-width:480px;text-align:center;display:flex;flex-direction:column;align-items:center;gap:24px;">
		<div style="width:72px;height:72px;border-radius:50%;background:rgba(59,130,246,0.1);border:1px solid rgba(59,130,246,0.25);display:flex;align-items:center;justify-content:center;font-size:32px;">⚓</div>
		<div>
			<h1 style="font-size:24px;font-weight:700;margin:0 0 10px;color:var(--text-primary);">Shipyard is already set up</h1>
			<p style="font-size:14px;line-height:1.7;color:var(--text-muted);margin:0;">This instance has already been initialized. Setup can only be performed once.</p>
		</div>
		<button class="btn btn-primary" onclick={() => goto('/login')}>Go to Login</button>
	</div>
</div>
{:else}
<div class="setup-wrap">
	<div class="setup-inner" class:setup-wide={step === 'plan'}>

		<!-- Header -->
		<div style="text-align:center;">
			<h1 style="font-size:28px;font-weight:700;margin-bottom:8px;">Shipyard Setup</h1>
			<p style="color:var(--text-muted);font-size:14px;">Configure your Shipyard instance</p>
		</div>

		<!-- Step indicators -->
		<div style="display:flex;align-items:center;gap:0;">
			{#each stepLabels as label, i}
				<div style="display:flex;align-items:center;flex:1;">
					<div style="display:flex;flex-direction:column;align-items:center;gap:6px;flex:1;">
						<div class="step-circle" class:step-done={i < stepIndex} class:step-active={i === stepIndex}>
							{#if i < stepIndex}✓{:else}{i + 1}{/if}
						</div>
						<span class="step-label" class:step-label-active={i === stepIndex}>{label}</span>
					</div>
					{#if i < stepLabels.length - 1}
						<div class="step-line" class:step-line-done={i < stepIndex}></div>
					{/if}
				</div>
			{/each}
		</div>

		<!-- Step content card -->
		<div class="step-card">

			<!-- Step 1: Welcome / Docker check -->
			{#if step === 'welcome'}
				<div class="step-body">
					<div>
						<h2 class="step-title">Welcome to Shipyard</h2>
						<p class="step-desc">Before getting started, let's verify that your environment is ready.</p>
					</div>

					<div class="docker-row">
						<div class="docker-icon"
							style="background:{dockerStatus === 'ok' ? 'var(--accent-green-muted)' : dockerStatus === 'error' ? 'var(--accent-red-muted)' : 'var(--bg-hover)'}">
							{#if dockerStatus === 'checking'}
								<span class="spinner"></span>
							{:else if dockerStatus === 'ok'}✅
							{:else if dockerStatus === 'error'}❌
							{:else}🐳{/if}
						</div>
						<div style="flex:1;min-width:0;">
							<div style="font-size:13px;font-weight:500;color:var(--text-primary);">Docker</div>
							<div style="font-size:12px;color:var(--text-muted);margin-top:2px;">
								{#if dockerStatus === 'idle'}Not checked yet
								{:else if dockerStatus === 'checking'}Checking…
								{:else if dockerStatus === 'ok'}{dockerMessage || 'Docker is available'}
								{:else}{dockerMessage || 'Docker not available'}{/if}
							</div>
						</div>
						<button class="btn btn-secondary btn-sm" onclick={checkDocker} disabled={dockerStatus === 'checking'}>
							{dockerStatus === 'idle' ? 'Check' : 'Retry'}
						</button>
					</div>

					<div style="display:flex;justify-content:flex-end;">
						<button class="btn btn-primary" disabled={dockerStatus !== 'ok'} onclick={() => (step = 'account')}>
							Continue
						</button>
					</div>
				</div>

			<!-- Step 2: Create admin account -->
			{:else if step === 'account'}
				<div class="step-body">
					<div>
						<h2 class="step-title">Create Admin Account</h2>
						<p class="step-desc">Set up your admin user and the first organization.</p>
					</div>

					{#if submitError}
						<div class="form-error">{submitError}</div>
					{/if}

					<div style="display:flex;flex-direction:column;gap:16px;">
						<div class="field">
							<label for="admin-email" class="flbl">Admin Email</label>
							<input id="admin-email" type="email" class="input" placeholder="admin@example.com" bind:value={adminEmail} autocomplete="email" />
						</div>
						<div class="field">
							<label for="admin-password" class="flbl">Admin Password</label>
							<input id="admin-password" type="password" class="input" placeholder="••••••••" bind:value={adminPassword} autocomplete="new-password" />
						</div>
						<div class="field">
							<label for="org-name" class="flbl">Organization Name</label>
							<input id="org-name" type="text" class="input" placeholder="My Organization" bind:value={orgName} />
						</div>
						<div class="field">
							<label for="org-slug" class="flbl">Organization Slug</label>
							<input id="org-slug" type="text" class="input" placeholder="my-organization" bind:value={orgSlug} pattern="[a-z0-9][a-z0-9-]*" />
							<span style="font-size:11px;color:var(--text-dim);">Lowercase letters, numbers, and hyphens only</span>
						</div>
						<div style="display:flex;justify-content:space-between;align-items:center;margin-top:4px;">
							<button type="button" class="btn btn-ghost" onclick={() => (step = 'welcome')}>Back</button>
							<button
								class="btn btn-primary"
								disabled={!adminEmail || !adminPassword || !orgName || !orgSlug || plansLoading}
								onclick={goToPlan}
							>
								{plansLoading ? 'Loading plans…' : 'Continue'}
							</button>
						</div>
					</div>
				</div>

			<!-- Step 3: Select Plan -->
			{:else if step === 'plan'}
				<div class="step-body">
					<div>
						<h2 class="step-title">Select a Plan</h2>
						<p class="step-desc">Choose the plan for your organization. You can upgrade later.</p>
					</div>

					{#if plansError}
						<div class="form-error">{plansError}</div>
					{/if}

					{#if plans.length === 0 && !plansError}
						<div class="plans-empty">No plans available — the free plan will be applied.</div>
					{:else}
						<div class="plans-grid">
							{#each plans as plan}
								<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
								<div
									class="plan-card"
									class:plan-selected={selectedPlanId === plan.id}
									onclick={() => (selectedPlanId = plan.id)}
								>
									<div class="plan-card-hdr">
										<div>
											<div class="plan-name">{plan.name}</div>
											<div class="plan-price">
												{#if plan.price_monthly === 0}Free{:else}${plan.price_monthly}/mo{/if}
											</div>
										</div>
										<div class="plan-radio" class:plan-radio-on={selectedPlanId === plan.id}></div>
									</div>
									<div class="plan-stats">
										<div class="ps"><span class="ps-l">Projects</span><span class="ps-v">{formatLimit(plan.max_projects)}</span></div>
										<div class="ps"><span class="ps-l">Members</span><span class="ps-v">{formatLimit(plan.max_members)}</span></div>
										<div class="ps"><span class="ps-l">Replicas</span><span class="ps-v">{formatLimit(plan.max_replicas)}</span></div>
										<div class="ps"><span class="ps-l">CPU Cores</span><span class="ps-v">{plan.cpu_cores}</span></div>
										<div class="ps"><span class="ps-l">Memory</span><span class="ps-v">{plan.memory_gb} GB</span></div>
										<div class="ps"><span class="ps-l">Git Providers</span><span class="ps-v">{formatLimit(plan.max_git_providers)}</span></div>
									</div>
								</div>
							{/each}
						</div>
					{/if}

					{#if submitError}
						<div class="form-error">{submitError}</div>
					{/if}

					<div style="display:flex;justify-content:space-between;align-items:center;margin-top:4px;">
						<button class="btn btn-ghost" onclick={() => (step = 'account')} disabled={submitting}>Back</button>
						<button class="btn btn-primary" onclick={handleSetupSubmit} disabled={submitting}>
							{#if submitting}
								<span class="spinner" style="border-color:rgba(255,255,255,0.3);border-top-color:white;"></span>
								Setting up…
							{:else}
								Initialize Shipyard
							{/if}
						</button>
					</div>
				</div>

			<!-- Step 4: Done -->
			{:else}
				<div class="done-body">
					<div class="done-icon">✓</div>
					<div>
						<h2 class="done-title">Shipyard is ready!</h2>
						<p class="step-desc">Your super admin account and organization have been created.</p>
					</div>
					<button class="btn btn-primary" style="margin-top:8px;" onclick={() => goto('/admin')}>
						Go to Admin Panel
					</button>
				</div>
			{/if}

		</div>
	</div>
</div>
{/if}

<style>
	@keyframes spin { to { transform: rotate(360deg); } }

	.setup-wrap {
		min-height:100vh; background:var(--bg-base);
		display:flex; align-items:center; justify-content:center; padding:24px;
	}
	.setup-inner {
		width:100%; max-width:560px;
		display:flex; flex-direction:column; gap:32px;
	}
	.setup-wide { max-width:860px; }

	/* Step indicator */
	.step-circle {
		width:32px; height:32px; border-radius:50%;
		display:flex; align-items:center; justify-content:center;
		font-size:13px; font-weight:600; transition:all .2s;
		background:var(--bg-elevated); color:var(--text-muted);
		border:1px solid var(--border);
	}
	.step-done { background:var(--accent-green) !important; color:white !important; border-color:var(--accent-green) !important; }
	.step-active { background:var(--accent) !important; color:white !important; border-color:var(--accent) !important; }
	.step-label { font-size:11px; font-weight:500; color:var(--text-muted); }
	.step-label-active { color:var(--text-primary); }
	.step-line { height:1px; flex:1; margin-bottom:20px; background:var(--border); transition:background .3s; }
	.step-line-done { background:var(--accent-green); }

	/* Card */
	.step-card {
		background:var(--bg-surface); border:1px solid var(--border);
		border-radius:var(--radius-lg); padding:32px;
	}
	.step-body { display:flex; flex-direction:column; gap:24px; }
	.step-title { font-size:18px; margin-bottom:8px; }
	.step-desc { color:var(--text-muted); font-size:14px; line-height:1.6; margin:0; }

	/* Docker row */
	.docker-row {
		display:flex; align-items:center; gap:12px; padding:14px 16px;
		border-radius:var(--radius-md); background:var(--bg-elevated); border:1px solid var(--border);
	}
	.docker-icon {
		width:36px; height:36px; border-radius:var(--radius-sm);
		display:flex; align-items:center; justify-content:center; font-size:18px; flex-shrink:0;
	}

	/* Form */
	.field { display:flex; flex-direction:column; gap:6px; }
	.flbl { font-size:13px; font-weight:500; color:var(--text-secondary); }
	.form-error {
		padding:10px 14px; background:var(--accent-red-muted);
		border:1px solid var(--accent-red); border-radius:var(--radius-md);
		color:var(--accent-red); font-size:13px;
	}

	/* Plan grid */
	.plans-grid {
		display:grid; grid-template-columns:repeat(auto-fill, minmax(230px, 1fr)); gap:12px;
	}
	.plans-empty {
		padding:24px; text-align:center; font-size:13px; color:var(--text-muted);
		background:var(--bg-elevated); border:1px solid var(--border); border-radius:var(--radius-md);
	}
	.plan-card {
		background:var(--bg-elevated); border:2px solid var(--border);
		border-radius:var(--radius-md); padding:16px; cursor:pointer;
		transition:border-color .15s, box-shadow .15s;
	}
	.plan-card:hover { border-color:var(--accent); }
	.plan-selected { border-color:var(--accent) !important; box-shadow:0 0 0 3px var(--accent-ring, rgba(99,102,241,0.15)); }
	.plan-card-hdr { display:flex; justify-content:space-between; align-items:flex-start; margin-bottom:12px; }
	.plan-name { font-size:15px; font-weight:800; color:var(--text-primary); letter-spacing:-0.01em; }
	.plan-price { font-size:12px; color:var(--text-muted); margin-top:2px; font-weight:600; }
	.plan-radio {
		width:16px; height:16px; border-radius:50%; border:2px solid var(--border);
		flex-shrink:0; margin-top:2px; transition:border-color .15s, background .15s;
	}
	.plan-radio-on { border-color:var(--accent); background:var(--accent); }
	.plan-stats { display:grid; grid-template-columns:1fr 1fr; gap:5px; }
	.ps { display:flex; justify-content:space-between; padding:4px 7px; background:var(--bg-hover); border-radius:4px; font-size:11.5px; }
	.ps-l { color:var(--text-muted); }
	.ps-v { font-weight:700; color:var(--text-primary); }

	/* Done */
	.done-body { display:flex; flex-direction:column; align-items:center; gap:20px; text-align:center; padding:16px 0; }
	.done-icon {
		width:64px; height:64px; border-radius:50%;
		background:var(--accent-green-muted); border:1px solid var(--accent-green);
		display:flex; align-items:center; justify-content:center; font-size:28px;
	}
	.done-title { font-size:20px; margin-bottom:8px; color:var(--accent-green); }

	/* Spinner */
	.spinner {
		width:14px; height:14px; border:2px solid var(--border);
		border-top-color:var(--accent); border-radius:50%;
		display:inline-block; animation:spin .7s linear infinite;
	}

	@media (max-width: 640px) {
		.step-card { padding:20px; }
		.plans-grid { grid-template-columns:1fr; }
	}
</style>
