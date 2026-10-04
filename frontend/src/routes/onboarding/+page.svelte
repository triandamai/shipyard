<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { get } from 'svelte/store';
	import { Check, ArrowRight } from '@lucide/svelte';
	import { Card, Button, FormField, TextField, Checkbox, InlineAlert, Spinner } from '$lib/components/ui';

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
							<Check size={12} strokeWidth={3} />
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
			<Card padding="clamp(18px, 5vw, 32px)">
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
										<span class="check-icon"><Check size={12} strokeWidth={3} /></span>
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
					<Button onclick={() => step = 2}>
						Continue with {PLANS.find(p => p.id === selectedPlan)?.name}
						<ArrowRight size={14} />
					</Button>
				</div>
			</Card>
		</div>
	{/if}

	<!-- Step 2: Create org -->
	{#if step === 2}
		<div class="panel">
			<Card padding="clamp(18px, 5vw, 32px)">
				<h2 class="panel-title">Name your organization</h2>
				<p class="panel-sub">This is your workspace where you'll manage projects and deployments.</p>

				{#if createErr}
					<div role="alert" class="err-wrap">
						<InlineAlert tone="error">{createErr}</InlineAlert>
					</div>
				{/if}

				<form onsubmit={submitOrg} class="form">
					<FormField label="Organization name" for="org-name">
						<TextField
							id="org-name"
							type="text"
							placeholder="Acme Inc"
							bind:value={orgName}
							required
							maxlength={80}
						/>
					</FormField>

					<FormField label="URL slug" for="org-slug" hint="Lowercase letters, numbers, and hyphens only.">
						<div class="slug-wrap">
							<span class="slug-prefix">shipyard.app/orgs/</span>
							<TextField
								id="org-slug"
								type="text"
								placeholder="acme-inc"
								bind:value={orgSlug}
								required
								pattern="[a-z0-9-]+"
								maxlength={40}
							/>
						</div>
					</FormField>

					<div class="actions">
						<Button variant="secondary" onclick={() => step = 1}>Back</Button>
						<Button type="submit" disabled={creating || !orgName.trim() || !orgSlug.trim()}>
							{#if creating}
								<Spinner size={14} tone="current" /> Creating…
							{:else}
								Create organization
							{/if}
						</Button>
					</div>
				</form>
			</Card>
		</div>
	{/if}

	<!-- Step 3: Region / VM -->
	{#if step === 3}
		<div class="panel">
			<Card padding="clamp(18px, 5vw, 32px)">
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
										<span class="region-check"><Check size={12} strokeWidth={3} /></span>
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
					<Checkbox bind:checked={skipVM} label="Skip for now — I'll set up compute nodes later" />
				</div>

				<div class="actions">
					<Button variant="secondary" onclick={() => step = 2}>Back</Button>
					<Button onclick={finish}>
						{skipVM ? 'Go to dashboard' : 'Finish setup'}
						<ArrowRight size={14} />
					</Button>
				</div>
			</Card>
		</div>
	{/if}
</div>

<style>
	:global(body) { margin: 0; }

	.root {
		min-height: 100vh;
		background: var(--bg-base);
		display: flex;
		flex-direction: column;
		align-items: center;
		padding: 48px 20px 80px;
		font-family: var(--font-sans);
		-webkit-font-smoothing: antialiased;
	}

	/* ── Stepper ── */
	.stepper { width: 100%; max-width: 480px; margin-bottom: 36px; }
	.stepper-inner { display: flex; align-items: center; gap: 0; }
	.step { display: flex; flex-direction: column; align-items: center; gap: 6px; }
	.step-dot {
		width: 28px; height: 28px; border-radius: 50%;
		display: flex; align-items: center; justify-content: center;
		font-size: 12px; font-weight: 700;
		background: var(--bg-hover); color: var(--text-muted);
		transition: background var(--transition-normal), color var(--transition-normal);
		flex-shrink: 0;
	}
	.step.active .step-dot { background: var(--accent); color: #fff; }
	.step.done .step-dot { background: var(--accent-green); color: #fff; }
	.step-label { font-size: 11px; font-weight: 600; color: var(--text-dim); white-space: nowrap; }
	.step.active .step-label { color: var(--accent); }
	.step.done .step-label { color: var(--accent-green); }
	.step-line {
		flex: 1; height: 2px; background: var(--border);
		margin: 0 6px 18px;
		transition: background var(--transition-normal);
	}
	.step-line.done { background: var(--accent-green); }

	/* ── Panel ── */
	.panel { width: 100%; max-width: 680px; }
	.panel-title { font-size: 20px; font-weight: 700; color: var(--text-primary); margin: 0 0 6px; letter-spacing: -0.02em; }
	.panel-sub { font-size: 13.5px; color: var(--text-muted); margin: 0 0 28px; }

	/* ── Plans ── */
	.plans { display: grid; grid-template-columns: repeat(3, 1fr); gap: 12px; margin-bottom: 28px; }
	.plan-card {
		position: relative;
		display: flex; flex-direction: column;
		background: var(--bg-elevated); border: 2px solid var(--border);
		border-radius: var(--radius-lg); padding: 18px 16px;
		cursor: pointer; text-align: left;
		transition: border-color var(--transition-fast), background var(--transition-fast);
		font-family: inherit;
	}
	.plan-card:hover { border-color: var(--border-hover); background: var(--bg-hover); }
	.plan-card.selected { border-color: var(--accent); background: var(--accent-muted); }
	.plan-card.highlight:not(.selected) { border-color: color-mix(in srgb, var(--accent) 50%, var(--border)); }
	.plan-badge {
		position: absolute; top: -1px; left: 50%; transform: translateX(-50%);
		font-size: 10px; font-weight: 700; color: #fff;
		background: var(--accent); border-radius: 0 0 6px 6px;
		padding: 2px 10px; white-space: nowrap;
	}
	.plan-name { font-size: 15px; font-weight: 700; color: var(--text-primary); margin-bottom: 2px; }
	.plan-price { font-size: 18px; font-weight: 700; color: var(--accent); margin-bottom: 6px; }
	.plan-desc { font-size: 12px; color: var(--text-muted); margin-bottom: 12px; line-height: 1.4; }
	.plan-features { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 5px; flex: 1; }
	.plan-features li { display: flex; align-items: flex-start; gap: 6px; font-size: 12px; color: var(--text-secondary); }
	.check-icon { display: flex; flex-shrink: 0; margin-top: 1px; color: var(--accent-green); }
	.plan-selected-indicator {
		margin-top: 12px; padding: 5px 0;
		border-top: 1px solid var(--border);
		font-size: 11px; font-weight: 700; color: var(--accent);
		text-transform: uppercase; letter-spacing: .05em; text-align: center;
	}

	/* ── Form ── */
	.form { display: flex; flex-direction: column; gap: 18px; }
	.err-wrap { margin-bottom: 14px; }

	.slug-wrap {
		display: flex; align-items: center;
		border: 1px solid var(--border); border-radius: var(--radius-sm);
		background: var(--bg-elevated);
		overflow: hidden;
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.slug-prefix {
		padding: 0 10px 0 12px; background: var(--bg-hover); color: var(--text-dim);
		font-size: 12px; white-space: nowrap; line-height: 34px;
		border-right: 1px solid var(--border);
	}
	.slug-wrap :global(.ui-textfield) { border: none; border-radius: 0; box-shadow: none; flex: 1; min-width: 0; }
	.slug-wrap:focus-within { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }

	/* ── Setup grid ── */
	.setup-grid { display: flex; flex-direction: column; gap: 20px; margin-bottom: 16px; }
	.setup-section { display: flex; flex-direction: column; gap: 8px; }
	.setup-label { font-size: 11.5px; font-weight: 700; color: var(--text-muted); text-transform: uppercase; letter-spacing: .07em; }

	.provider-row { display: flex; gap: 8px; flex-wrap: wrap; }
	.pv-btn {
		padding: 6px 14px; border-radius: var(--radius-md);
		border: 1.5px solid var(--border); background: var(--bg-elevated);
		font-size: 12.5px; font-weight: 600; cursor: pointer; font-family: inherit;
		color: var(--text-muted); transition: all var(--transition-fast);
	}
	.pv-btn:hover { border-color: var(--pv-color, var(--accent)); color: var(--pv-color, var(--accent)); background: var(--bg-hover); }
	.pv-btn.active {
		border-color: var(--pv-color, var(--accent)); color: var(--pv-color, var(--accent));
		background: color-mix(in srgb, var(--pv-color, var(--accent)) 12%, transparent);
	}

	.region-list { display: flex; flex-direction: column; gap: 4px; }
	.region-item {
		display: flex; align-items: center; gap: 9px;
		padding: 9px 12px; border-radius: var(--radius-md);
		border: 1.5px solid var(--border); background: var(--bg-elevated);
		font-size: 13px; color: var(--text-secondary); cursor: pointer; font-family: inherit;
		text-align: left; transition: all var(--transition-fast);
	}
	.region-item:hover { border-color: var(--border-hover); background: var(--bg-hover); }
	.region-item.active { border-color: var(--accent); background: var(--accent-muted); color: var(--text-primary); }
	.flag { font-size: 16px; line-height: 1; }
	.region-check { display: flex; margin-left: auto; color: var(--accent); }

	.vm-list { display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }
	.vm-item {
		display: flex; align-items: center; justify-content: space-between;
		padding: 10px 12px; border-radius: var(--radius-md);
		border: 1.5px solid var(--border); background: var(--bg-elevated);
		cursor: pointer; font-family: inherit; text-align: left;
		transition: all var(--transition-fast);
	}
	.vm-item:hover { border-color: var(--border-hover); background: var(--bg-hover); }
	.vm-item.active { border-color: var(--accent); background: var(--accent-muted); }
	.vm-info { display: flex; flex-direction: column; gap: 2px; }
	.vm-name { font-size: 13px; font-weight: 600; color: var(--text-primary); }
	.vm-spec { font-size: 11px; color: var(--text-dim); }
	.vm-price { font-size: 12.5px; font-weight: 700; color: var(--accent); }

	.skip-row { margin-bottom: 24px; }

	/* ── Actions ── */
	.actions { display: flex; align-items: center; justify-content: flex-end; gap: 10px; margin-top: 4px; }

	/* ── Responsive ── */
	@media (max-width: 600px) {
		.plans { grid-template-columns: 1fr; }
		.vm-list { grid-template-columns: 1fr; }
		.panel-title { font-size: 18px; }
	}
</style>
