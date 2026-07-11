<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { get } from 'svelte/store';

	// ── Steps ──────────────────────────────────────────────────────────────────
	let step = $state<1 | 2 | 3>(1);

	// ── Plan selection (step 1) ────────────────────────────────────────────────
	interface Plan {
		id: 'free' | 'pro' | 'max';
		name: string;
		price: string;
		desc: string;
		features: string[];
		highlight?: boolean;
	}

	const PLANS: Plan[] = [
		{
			id: 'free',
			name: 'Free',
			price: '$0 / mo',
			desc: 'Perfect for side projects and experiments.',
			features: ['1 organization', '3 projects', 'Community support'],
		},
		{
			id: 'pro',
			name: 'Pro',
			price: '$29 / mo',
			desc: 'For teams shipping production workloads.',
			features: ['Unlimited projects', 'Custom domains', 'Priority support', 'Advanced monitoring'],
			highlight: true,
		},
		{
			id: 'max',
			name: 'Max',
			price: '$99 / mo',
			desc: 'Full power for high-traffic applications.',
			features: ['Everything in Pro', 'SLA guarantee', 'Dedicated support', 'White-glove onboarding'],
		},
	];

	let selectedPlan = $state<'free' | 'pro' | 'max'>('free');

	// ── Org creation (step 2) ─────────────────────────────────────────────────
	let orgName    = $state('');
	let orgSlug    = $state('');
	let creating   = $state(false);
	let createErr  = $state('');
	let createdOrg = $state<{ id: string; slug: string; name: string } | null>(null);

	$effect(() => {
		orgSlug = orgName
			.toLowerCase()
			.replace(/\s+/g, '-')
			.replace(/[^a-z0-9-]/g, '');
	});

	// ── Region / VM selection (step 3) ────────────────────────────────────────
	interface Provider { id: string; name: string; color: string; }
	interface Region   { id: string; label: string; provider: string; flag: string; }
	interface VMSize   { id: string; label: string; vcpu: number; ram: number; price: string; }

	const PROVIDERS: Provider[] = [
		{ id: 'hetzner',      name: 'Hetzner',         color: '#e53e3e' },
		{ id: 'digitalocean', name: 'DigitalOcean',    color: '#1a81c2' },
		{ id: 'aws',          name: 'AWS EC2',          color: '#f59e0b' },
		{ id: 'vultr',        name: 'Vultr',            color: '#007bfc' },
	];

	const REGIONS: Region[] = [
		{ id: 'eu-central',   label: 'EU Central (Frankfurt)',    provider: 'hetzner',      flag: '🇩🇪' },
		{ id: 'eu-west',      label: 'EU West (Helsinki)',        provider: 'hetzner',      flag: '🇫🇮' },
		{ id: 'us-east-1',    label: 'US East (New York)',        provider: 'digitalocean', flag: '🇺🇸' },
		{ id: 'us-west-1',    label: 'US West (San Francisco)',   provider: 'digitalocean', flag: '🇺🇸' },
		{ id: 'ap-sgp',       label: 'Asia (Singapore)',          provider: 'vultr',        flag: '🇸🇬' },
		{ id: 'ap-syd',       label: 'Asia Pacific (Sydney)',     provider: 'aws',          flag: '🇦🇺' },
	];

	const VM_SIZES: VMSize[] = [
		{ id: 'cx11',   label: 'Starter',    vcpu: 1, ram: 2,   price: '$4 / mo' },
		{ id: 'cx21',   label: 'Basic',      vcpu: 2, ram: 4,   price: '$8 / mo' },
		{ id: 'cx31',   label: 'Standard',   vcpu: 2, ram: 8,   price: '$16 / mo' },
		{ id: 'cx41',   label: 'Performance', vcpu: 4, ram: 16, price: '$32 / mo' },
	];

	let selectedProvider = $state('hetzner');
	let selectedRegion   = $state('eu-central');
	let selectedVM       = $state('cx21');
	let skipVM           = $state(false);

	let filteredRegions = $derived(REGIONS.filter(r => r.provider === selectedProvider));
	$effect(() => {
		const first = filteredRegions[0];
		if (first) selectedRegion = first.id;
	});

	// ── Navigation ────────────────────────────────────────────────────────────
	onMount(async () => {
		const auth = get(authStore);
		if (!auth.token) { goto('/login'); return; }
		const res = await api.getOrgs();
		if (res.data && res.data.length > 0) {
			const first = res.data[0];
			orgStore.setOrganizations(res.data);
			orgStore.setActiveOrg(first);
			goto(`/orgs/${first.slug}/projects`);
		}
	});

	async function submitOrg(e: SubmitEvent) {
		e.preventDefault();
		createErr = '';
		creating  = true;
		try {
			const res = await api.createOrg(orgName, orgSlug);
			if (res.error || !res.data) { createErr = res.error?.message ?? 'Failed to create organization.'; return; }
			createdOrg = { id: res.data.id, slug: res.data.slug, name: res.data.name };
			orgStore.setOrganizations([res.data]);
			orgStore.setActiveOrg(res.data);
			step = 3;
		} finally {
			creating = false;
		}
	}

	function finish() {
		if (!createdOrg) { goto('/orgs'); return; }
		goto(`/orgs/${createdOrg.slug}/projects`);
	}

	let providerColor = $derived(PROVIDERS.find(p => p.id === selectedProvider)?.color ?? '#6b7280');
</script>

<div class="root">
	<!-- Stepper -->
	<div class="stepper">
		<div class="stepper-inner">
			{#each [{ n:1, label:'Plan' }, { n:2, label:'Organization' }, { n:3, label:'Setup' }] as s}
				<div class="step" class:done={step > s.n} class:active={step === s.n}>
					<div class="step-dot">
						{#if step > s.n}
							<svg viewBox="0 0 12 12" fill="none" width="12" height="12">
								<path d="M2 6l3 3 5-5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
							</svg>
						{:else}
							{s.n}
						{/if}
					</div>
					<span class="step-label">{s.label}</span>
				</div>
				{#if s.n < 3}
					<div class="step-line" class:done={step > s.n}></div>
				{/if}
			{/each}
		</div>
	</div>

	<!-- Step 1: Plan -->
	{#if step === 1}
		<div class="panel">
			<h2 class="panel-title">Choose your plan</h2>
			<p class="panel-sub">You can upgrade or downgrade at any time.</p>

			<div class="plans">
				{#each PLANS as plan}
					<button
						class="plan-card"
						class:selected={selectedPlan === plan.id}
						class:highlight={plan.highlight}
						onclick={() => selectedPlan = plan.id}
					>
						{#if plan.highlight}
							<span class="plan-badge">Most popular</span>
						{/if}
						<div class="plan-name">{plan.name}</div>
						<div class="plan-price">{plan.price}</div>
						<div class="plan-desc">{plan.desc}</div>
						<ul class="plan-features">
							{#each plan.features as f}
								<li>
									<svg viewBox="0 0 12 12" fill="none" width="11" height="11" class="check-icon">
										<path d="M2 6l3 3 5-5" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>
									</svg>
									{f}
								</li>
							{/each}
						</ul>
						{#if selectedPlan === plan.id}
							<div class="plan-selected-indicator">Selected</div>
						{/if}
					</button>
				{/each}
			</div>

			<div class="actions">
				<button class="btn-primary" onclick={() => step = 2}>
					Continue with {PLANS.find(p => p.id === selectedPlan)?.name}
					<svg viewBox="0 0 16 16" fill="currentColor" width="14" height="14"><path fill-rule="evenodd" d="M1 8a.5.5 0 01.5-.5h11.793L11.146 5.354a.5.5 0 11.708-.708l3 3a.5.5 0 010 .708l-3 3a.5.5 0 01-.708-.708L13.293 8.5H1.5A.5.5 0 011 8z"/></svg>
				</button>
			</div>
		</div>
	{/if}

	<!-- Step 2: Create org -->
	{#if step === 2}
		<div class="panel">
			<h2 class="panel-title">Name your organization</h2>
			<p class="panel-sub">This is your workspace where you'll manage projects and deployments.</p>

			{#if createErr}
				<div class="form-err">{createErr}</div>
			{/if}

			<form onsubmit={submitOrg} class="form">
				<div class="field">
					<label for="org-name" class="field-label">Organization name</label>
					<input
						id="org-name"
						type="text"
						class="inp"
						placeholder="Acme Inc"
						bind:value={orgName}
						required
						maxlength={80}
					/>
				</div>

				<div class="field">
					<label for="org-slug" class="field-label">URL slug</label>
					<div class="slug-wrap">
						<span class="slug-prefix">shipyard.app/orgs/</span>
						<input
							id="org-slug"
							type="text"
							class="inp slug-inp"
							placeholder="acme-inc"
							bind:value={orgSlug}
							required
							pattern="[a-z0-9-]+"
							maxlength={40}
						/>
					</div>
					<span class="field-hint">Lowercase letters, numbers, and hyphens only.</span>
				</div>

				<div class="actions">
					<button type="button" class="btn-ghost" onclick={() => step = 1}>Back</button>
					<button type="submit" class="btn-primary" disabled={creating || !orgName.trim() || !orgSlug.trim()}>
						{#if creating}
							<span class="spin"></span> Creating…
						{:else}
							Create organization
						{/if}
					</button>
				</div>
			</form>
		</div>
	{/if}

	<!-- Step 3: Region / VM -->
	{#if step === 3}
		<div class="panel">
			<h2 class="panel-title">Select your region</h2>
			<p class="panel-sub">Choose where you want to run your workloads. You can add more regions later.</p>

			<div class="setup-grid">
				<!-- Provider -->
				<div class="setup-section">
					<div class="setup-label">Cloud provider</div>
					<div class="provider-row">
						{#each PROVIDERS as pv}
							<button
								class="pv-btn"
								class:active={selectedProvider === pv.id}
								onclick={() => selectedProvider = pv.id}
								style:--pv-color={pv.color}
							>
								{pv.name}
							</button>
						{/each}
					</div>
				</div>

				<!-- Region -->
				<div class="setup-section">
					<div class="setup-label">Region</div>
					<div class="region-list">
						{#each filteredRegions as r}
							<button
								class="region-item"
								class:active={selectedRegion === r.id}
								onclick={() => selectedRegion = r.id}
							>
								<span class="flag">{r.flag}</span>
								{r.label}
								{#if selectedRegion === r.id}
									<span class="region-check">
										<svg viewBox="0 0 12 12" fill="none" width="10" height="10">
											<path d="M2 6l3 3 5-5" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" stroke-linejoin="round"/>
										</svg>
									</span>
								{/if}
							</button>
						{/each}
					</div>
				</div>

				<!-- VM size -->
				<div class="setup-section">
					<div class="setup-label">VM size</div>
					<div class="vm-list">
						{#each VM_SIZES as vm}
							<button
								class="vm-item"
								class:active={selectedVM === vm.id}
								onclick={() => selectedVM = vm.id}
							>
								<div class="vm-info">
									<span class="vm-name">{vm.label}</span>
									<span class="vm-spec">{vm.vcpu} vCPU · {vm.ram} GB RAM</span>
								</div>
								<span class="vm-price">{vm.price}</span>
							</button>
						{/each}
					</div>
				</div>
			</div>

			<div class="skip-row">
				<label class="skip-label">
					<input type="checkbox" bind:checked={skipVM} />
					Skip for now — I'll set up compute nodes later
				</label>
			</div>

			<div class="actions">
				<button class="btn-ghost" onclick={() => step = 2}>Back</button>
				<button class="btn-primary" onclick={finish}>
					{skipVM ? 'Go to dashboard' : 'Finish setup'}
					<svg viewBox="0 0 16 16" fill="currentColor" width="14" height="14"><path fill-rule="evenodd" d="M1 8a.5.5 0 01.5-.5h11.793L11.146 5.354a.5.5 0 11.708-.708l3 3a.5.5 0 010 .708l-3 3a.5.5 0 01-.708-.708L13.293 8.5H1.5A.5.5 0 011 8z"/></svg>
				</button>
			</div>
		</div>
	{/if}
</div>

<style>
	:global(body) { margin:0; }

	.root {
		min-height:100vh;
		background:var(--bg-base, #f6f6f8);
		display:flex;
		flex-direction:column;
		align-items:center;
		padding:48px 20px 80px;
		font-family:system-ui,-apple-system,'Segoe UI',sans-serif;
		-webkit-font-smoothing:antialiased;
	}

	/* ── Stepper ── */
	.stepper {
		width:100%; max-width:480px;
		margin-bottom:36px;
	}
	.stepper-inner {
		display:flex; align-items:center; gap:0;
	}
	.step {
		display:flex; flex-direction:column; align-items:center; gap:6px;
	}
	.step-dot {
		width:28px; height:28px; border-radius:50%;
		display:flex; align-items:center; justify-content:center;
		font-size:12px; font-weight:700;
		background:#e5e7eb; color:#6b7280;
		transition:background .2s, color .2s;
		flex-shrink:0;
	}
	.step.active .step-dot { background:#2563eb; color:#fff; }
	.step.done .step-dot   { background:#16a34a; color:#fff; }
	.step-label { font-size:11px; font-weight:600; color:#9ca3af; white-space:nowrap; }
	.step.active .step-label { color:#2563eb; }
	.step.done .step-label   { color:#16a34a; }
	.step-line { flex:1; height:2px; background:#e5e7eb; margin:0 6px; margin-bottom:18px; transition:background .2s; }
	.step-line.done { background:#16a34a; }

	/* ── Panel ── */
	.panel {
		width:100%; max-width:680px;
		background:#fff;
		border:1px solid #e5e7eb;
		border-radius:14px;
		padding:36px 32px;
		box-shadow:0 2px 8px rgba(0,0,0,0.05);
	}
	.panel-title { font-size:20px; font-weight:700; color:#111; margin:0 0 6px; letter-spacing:-0.02em; }
	.panel-sub   { font-size:13.5px; color:#6b7280; margin:0 0 28px; }

	/* ── Plans ── */
	.plans {
		display:grid; grid-template-columns:repeat(3, 1fr);
		gap:12px; margin-bottom:28px;
	}
	.plan-card {
		position:relative;
		display:flex; flex-direction:column;
		background:#f9fafb; border:2px solid #e5e7eb;
		border-radius:10px; padding:18px 16px;
		cursor:pointer; text-align:left;
		transition:border-color .15s, box-shadow .15s, background .15s;
		font-family:inherit;
	}
	.plan-card:hover { border-color:#93c5fd; background:#fafbff; }
	.plan-card.selected { border-color:#2563eb; background:#eff6ff; }
	.plan-card.highlight { border-color:#3b82f6; }
	.plan-badge {
		position:absolute; top:-1px; left:50%; transform:translateX(-50%);
		font-size:10px; font-weight:700; color:#fff;
		background:#2563eb; border-radius:0 0 6px 6px;
		padding:2px 10px; white-space:nowrap;
	}
	.plan-name  { font-size:15px; font-weight:700; color:#111; margin-bottom:2px; }
	.plan-price { font-size:18px; font-weight:700; color:#2563eb; margin-bottom:6px; }
	.plan-desc  { font-size:12px; color:#6b7280; margin-bottom:12px; line-height:1.4; }
	.plan-features { list-style:none; margin:0; padding:0; display:flex; flex-direction:column; gap:5px; flex:1; }
	.plan-features li { display:flex; align-items:flex-start; gap:6px; font-size:12px; color:#374151; }
	.check-icon { flex-shrink:0; margin-top:1px; color:#16a34a; }
	.plan-selected-indicator {
		margin-top:12px; padding:5px 0;
		border-top:1px solid rgba(37,99,235,0.15);
		font-size:11px; font-weight:700; color:#2563eb;
		text-transform:uppercase; letter-spacing:.05em; text-align:center;
	}

	/* ── Form ── */
	.form { display:flex; flex-direction:column; gap:18px; }
	.field { display:flex; flex-direction:column; gap:5px; }
	.field-label { font-size:12.5px; font-weight:600; color:#374151; }
	.field-hint  { font-size:11.5px; color:#9ca3af; }
	.inp {
		height:38px; padding:0 12px;
		background:#fff; border:1px solid #d1d5db;
		border-radius:8px; font-size:13.5px; color:#111;
		font-family:inherit; outline:none;
		transition:border-color .15s, box-shadow .15s;
	}
	.inp::placeholder { color:#9ca3af; }
	.inp:focus { border-color:#2563eb; box-shadow:0 0 0 3px rgba(37,99,235,0.12); }

	.slug-wrap { display:flex; align-items:center; border:1px solid #d1d5db; border-radius:8px; overflow:hidden; }
	.slug-prefix { padding:0 10px 0 12px; background:#f3f4f6; color:#9ca3af; font-size:12px; white-space:nowrap; line-height:38px; border-right:1px solid #d1d5db; }
	.slug-inp { border:none; flex:1; border-radius:0; box-shadow:none; }
	.slug-wrap:focus-within { outline:none; border-color:#2563eb; box-shadow:0 0 0 3px rgba(37,99,235,0.12); }

	.form-err {
		padding:10px 13px; background:#fef2f2;
		border:1px solid #fecaca; border-radius:8px;
		color:#dc2626; font-size:13px; margin-bottom:4px;
	}

	/* ── Setup grid ── */
	.setup-grid { display:flex; flex-direction:column; gap:20px; margin-bottom:16px; }
	.setup-section { display:flex; flex-direction:column; gap:8px; }
	.setup-label { font-size:11.5px; font-weight:700; color:#6b7280; text-transform:uppercase; letter-spacing:.07em; }

	.provider-row { display:flex; gap:8px; flex-wrap:wrap; }
	.pv-btn {
		padding:6px 14px; border-radius:7px;
		border:1.5px solid #e5e7eb; background:#f9fafb;
		font-size:12.5px; font-weight:600; cursor:pointer; font-family:inherit;
		color:#6b7280; transition:all .15s;
	}
	.pv-btn:hover { border-color:var(--pv-color,#2563eb); color:var(--pv-color,#2563eb); background:#fafbff; }
	.pv-btn.active { border-color:var(--pv-color,#2563eb); color:var(--pv-color,#2563eb); background:#eff6ff; }

	.region-list { display:flex; flex-direction:column; gap:4px; }
	.region-item {
		display:flex; align-items:center; gap:9px;
		padding:9px 12px; border-radius:8px;
		border:1.5px solid #e5e7eb; background:#f9fafb;
		font-size:13px; color:#374151; cursor:pointer; font-family:inherit;
		text-align:left; transition:all .15s;
	}
	.region-item:hover { border-color:#93c5fd; background:#fafbff; }
	.region-item.active { border-color:#2563eb; background:#eff6ff; color:#1d4ed8; }
	.flag { font-size:16px; line-height:1; }
	.region-check { margin-left:auto; color:#2563eb; }

	.vm-list { display:grid; grid-template-columns:repeat(2, 1fr); gap:8px; }
	.vm-item {
		display:flex; align-items:center; justify-content:space-between;
		padding:10px 12px; border-radius:8px;
		border:1.5px solid #e5e7eb; background:#f9fafb;
		cursor:pointer; font-family:inherit; text-align:left;
		transition:all .15s;
	}
	.vm-item:hover { border-color:#93c5fd; background:#fafbff; }
	.vm-item.active { border-color:#2563eb; background:#eff6ff; }
	.vm-info { display:flex; flex-direction:column; gap:2px; }
	.vm-name { font-size:13px; font-weight:600; color:#111; }
	.vm-spec { font-size:11px; color:#9ca3af; }
	.vm-price { font-size:12.5px; font-weight:700; color:#2563eb; }

	.skip-row { margin-bottom:24px; }
	.skip-label { display:flex; align-items:center; gap:8px; font-size:13px; color:#6b7280; cursor:pointer; }
	.skip-label input { accent-color:#2563eb; }

	/* ── Actions ── */
	.actions { display:flex; align-items:center; justify-content:flex-end; gap:10px; margin-top:4px; }
	.btn-primary {
		display:inline-flex; align-items:center; gap:7px;
		height:40px; padding:0 20px;
		background:#2563eb; color:#fff; border:none;
		border-radius:9px; font-size:13.5px; font-weight:600;
		cursor:pointer; font-family:inherit;
		transition:background .15s;
	}
	.btn-primary:hover:not(:disabled) { background:#1d4ed8; }
	.btn-primary:disabled { opacity:.5; cursor:not-allowed; }

	.btn-ghost {
		display:inline-flex; align-items:center;
		height:40px; padding:0 16px;
		background:transparent; color:#6b7280;
		border:1.5px solid #e5e7eb; border-radius:9px;
		font-size:13.5px; font-weight:500; cursor:pointer; font-family:inherit;
		transition:all .15s;
	}
	.btn-ghost:hover { border-color:#d1d5db; color:#374151; }

	.spin {
		width:13px; height:13px;
		border:2px solid rgba(255,255,255,0.3);
		border-top-color:#fff;
		border-radius:50%;
		animation:spin .7s linear infinite;
	}
	@keyframes spin { to { transform:rotate(360deg); } }

	/* ── Responsive ── */
	@media (max-width: 600px) {
		.panel { padding:24px 18px; }
		.plans { grid-template-columns:1fr; }
		.vm-list { grid-template-columns:1fr; }
		.panel-title { font-size:18px; }
	}
</style>
