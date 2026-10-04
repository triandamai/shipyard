<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { get } from 'svelte/store';
	import type { Organization, Plan } from '$lib/api/types';
	import { Plus, Folder, Building2, Check, ArrowRight, ArrowLeft } from '@lucide/svelte';
	import { PageHeader, Card, Avatar, Button, Modal, FormField, TextField, InlineAlert, EmptyState, Spinner } from '$lib/components/ui';

	let orgs = $state<Organization[]>([]);
	let loading = $state(true);
	let fetchError = $state('');

	// New org modal
	let showModal = $state(false);
	let modalStep = $state<1 | 2>(1);
	let newOrgName = $state('');
	let newOrgSlug = $state('');
	let creating = $state(false);
	let createError = $state('');

	// Plan selection
	let plans = $state<Plan[]>([]);
	let plansLoading = $state(false);
	let selectedPlanId = $state<string | null>(null);

	// Auto-generate slug from name
	$effect(() => {
		newOrgSlug = newOrgName
			.toLowerCase()
			.replace(/\s+/g, '-')
			.replace(/[^a-z0-9-]/g, '');
	});

	onMount(async () => {
		const auth = get(authStore);
		if (!auth.token) {
			goto('/login');
			return;
		}

		await loadOrgs();
	});

	async function loadOrgs() {
		loading = true;
		fetchError = '';

		const res = await api.getOrgs();
		loading = false;

		if (res.error || !res.data) {
			if (res.error?.code === 'UNAUTHORIZED' || res.error?.code === 'AUTH_REQUIRED') {
				goto('/login');
				return;
			}
			fetchError = res.error?.message ?? 'Failed to load organizations.';
			return;
		}

		orgs = res.data;
		orgStore.setOrganizations(res.data);

		if (orgs.length === 0) {
			goto('/onboarding');
			return;
		}
	}

	async function goToStep2() {
		if (!newOrgName.trim() || !newOrgSlug.trim()) return;
		plansLoading = true;
		modalStep = 2;
		const res = await api.getPlans();
		if (res.data) {
			plans = res.data;
			// Default to free plan
			const free = plans.find(p => p.price_monthly === 0);
			selectedPlanId = free?.id ?? plans[0]?.id ?? null;
		}
		plansLoading = false;
	}

	async function handleCreateOrg() {
		createError = '';
		creating = true;

		try {
			const res = await api.createOrg(newOrgName, newOrgSlug, selectedPlanId ?? undefined);

			if (res.error || !res.data) {
				createError = res.error?.message ?? 'Failed to create organization.';
				modalStep = 1;
				return;
			}

			const newOrg = res.data;
			orgs = [...orgs, newOrg];
			orgStore.setOrganizations(orgs);
			closeModal();

			// If a paid plan was selected, go straight to checkout.
			const selectedPlan = plans.find(p => p.id === selectedPlanId);
			if (selectedPlan && selectedPlan.price_monthly > 0) {
				goto(`/orgs/${newOrg.slug}/billing`);
			} else {
				goto(`/orgs/${newOrg.slug}/projects`);
			}
		} finally {
			creating = false;
		}
	}

	function openModal() {
		newOrgName = '';
		newOrgSlug = '';
		createError = '';
		modalStep = 1;
		selectedPlanId = null;
		plans = [];
		showModal = true;
	}

	function closeModal() {
		showModal = false;
		newOrgName = '';
		newOrgSlug = '';
		createError = '';
		modalStep = 1;
		selectedPlanId = null;
	}

	function formatLimit(val: number): string {
		return val === -1 ? 'Unlimited' : String(val);
	}

	function fmtMem(mb: number): string {
		if (mb < 1024) return `${mb} MB`;
		const gb = mb / 1024;
		return Number.isInteger(gb) ? `${gb} GB` : `${gb.toFixed(1)} GB`;
	}

	function goToOrg(org: Organization) {
		orgStore.setActiveOrg(org);
		goto(`/orgs/${org.slug}/projects`);
	}
	const AVATAR_TONES = ['blue', 'green', 'red', 'yellow', 'purple'] as const;
	function orgAvatarTone(name: string): (typeof AVATAR_TONES)[number] {
		const hash = name.split('').reduce((acc, char) => acc + char.charCodeAt(0), 0);
		return AVATAR_TONES[hash % AVATAR_TONES.length];
	}

	function formatDate(dateStr?: string): string {
		if (!dateStr) return 'Recent';
		try {
			return new Date(dateStr).toLocaleDateString(undefined, { year: 'numeric', month: 'short', day: 'numeric' });
		} catch {
			return 'Recent';
		}
	}
</script>

<div class="orgs-container">
	<div class="orgs-inner">

		<PageHeader
			title="Your Organizations"
			subtitle="Select an organization to manage your services and infrastructure."
		>
			{#snippet actions()}
				<Button onclick={openModal}>
					<Plus size={16} />
					New Organization
				</Button>
			{/snippet}
		</PageHeader>

		<!-- Loading -->
		{#if loading}
			<div class="orgs-loading-wrap">
				<Spinner size={20} />
				Loading organizations…
			</div>

		<!-- Error -->
		{:else if fetchError}
			<div role="alert">
				<InlineAlert tone="error">
					<div class="orgs-error-row">
						<span>{fetchError}</span>
						<Button variant="ghost" size="sm" onclick={loadOrgs}>Retry</Button>
					</div>
				</InlineAlert>
			</div>

		<!-- Empty state -->
		{:else if orgs.length === 0}
			<div class="orgs-empty">
				<EmptyState message="No organizations yet" sub="Create your first organization to get started">
					{#snippet icon()}<Building2 size={28} />{/snippet}
				</EmptyState>
				<Button onclick={openModal}>Create Organization</Button>
			</div>

		<!-- Org grid -->
		{:else}
			<div class="orgs-grid">
				{#each orgs as org (org.id)}
					<button class="org-card" onclick={() => goToOrg(org)}>
						<Card padding="20px">
							<div class="org-card-body">
								<Avatar initials={org.name.slice(0, 1) || '?'} tone={orgAvatarTone(org.name)} size={40} />
								<div class="org-details">
									<div class="org-name">{org.name}</div>
									<div class="org-slug">{org.slug}</div>
								</div>
								<div class="org-footer">
									<span class="org-meta-item">
										<Folder size={12} />
										Projects
									</span>
									<span class="org-date">Created {formatDate(org.created_at)}</span>
								</div>
							</div>
						</Card>
					</button>
				{/each}
			</div>
		{/if}

	</div>
</div>

<!-- New Org Modal -->
<Modal bind:open={showModal} title="New Organization">
	<div class="step-indicator">
		<span class="step" class:active={modalStep === 1} class:done={modalStep > 1}>1 Details</span>
		<ArrowRight size={10} class="step-sep" />
		<span class="step" class:active={modalStep === 2}>2 Plan</span>
	</div>

	{#if createError}
		<div role="alert"><InlineAlert tone="error">{createError}</InlineAlert></div>
	{/if}

	{#if modalStep === 1}
		<FormField label="Name" for="org-name">
			<TextField id="org-name" type="text" placeholder="My Organization" bind:value={newOrgName} />
		</FormField>
		<FormField label="Slug" for="org-slug" hint="Lowercase letters, numbers, and hyphens only">
			<TextField id="org-slug" type="text" placeholder="my-organization" bind:value={newOrgSlug} pattern="[a-z0-9-]+" />
		</FormField>
	{:else}
		{#if plansLoading}
			<div class="plans-loading"><Spinner size={16} /> Loading plans…</div>
		{:else}
			<div class="plans-grid">
				{#each plans as plan (plan.id)}
					<button
						type="button"
						class="plan-card"
						class:selected={selectedPlanId === plan.id}
						onclick={() => (selectedPlanId = plan.id)}
					>
						<div class="plan-top">
							<span class="plan-name">{plan.name.charAt(0).toUpperCase() + plan.name.slice(1)}</span>
							<span class="plan-price">
								{#if plan.price_monthly === 0}
									Free
								{:else}
									${plan.price_monthly}<span class="plan-period">/mo</span>
								{/if}
							</span>
						</div>
						<ul class="plan-features">
							<li>{plan.cpu_cores} CPU · {fmtMem(plan.memory_mb)} RAM</li>
							<li>{formatLimit(plan.max_members)} members</li>
							<li>{formatLimit(plan.max_projects)} projects</li>
							<li>{formatLimit(plan.max_replicas)} replicas</li>
							<li>{formatLimit(plan.max_git_providers)} git providers</li>
						</ul>
						{#if selectedPlanId === plan.id}
							<div class="plan-check"><Check size={11} strokeWidth={3} /></div>
						{/if}
					</button>
				{/each}
			</div>
			{#if selectedPlanId && plans.find(p => p.id === selectedPlanId && p.price_monthly > 0)}
				<p class="paid-note">You'll be redirected to checkout after the organization is created.</p>
			{/if}
		{/if}
	{/if}

	{#snippet footer()}
		{#if modalStep === 1}
			<Button variant="secondary" onclick={closeModal}>Cancel</Button>
			<Button onclick={goToStep2} disabled={!newOrgName.trim() || !newOrgSlug.trim()}>
				Next <ArrowRight size={14} />
			</Button>
		{:else}
			<Button variant="secondary" onclick={() => (modalStep = 1)}><ArrowLeft size={14} /> Back</Button>
			<Button onclick={handleCreateOrg} disabled={creating || !selectedPlanId}>
				{#if creating}
					<Spinner size={14} tone="current" /> Creating…
				{:else}
					Create Organization
				{/if}
			</Button>
		{/if}
	{/snippet}
</Modal>

<style>
	/* ── Modal content ── */
	.step-indicator { display: flex; align-items: center; gap: 8px; font-size: 11.5px; }
	.step { color: var(--text-muted); font-weight: 500; }
	.step.active { color: var(--accent); font-weight: 700; }
	.step.done { color: var(--text-secondary); }
	.step-indicator :global(.step-sep) { color: var(--text-dim); }

	.plans-loading { display: flex; align-items: center; justify-content: center; gap: 10px; padding: 32px; color: var(--text-muted); font-size: 13px; }
	.plans-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(150px, 1fr)); gap: 12px; }
	.plan-card {
		position: relative; text-align: left; font-family: inherit;
		padding: 16px; border: 2px solid var(--border); border-radius: var(--radius-lg);
		cursor: pointer; transition: border-color var(--transition-fast), background var(--transition-fast);
		background: var(--bg-elevated);
	}
	.plan-card:hover { border-color: var(--accent); }
	.plan-card.selected { border-color: var(--accent); background: var(--accent-muted); }
	.plan-top { display: flex; flex-direction: column; gap: 4px; margin-bottom: 12px; }
	.plan-name { font-size: 13px; font-weight: 700; color: var(--text-primary); text-transform: capitalize; }
	.plan-price { font-size: 18px; font-weight: 700; color: var(--accent); }
	.plan-period { font-size: 11px; font-weight: 400; color: var(--text-muted); }
	.plan-features { list-style: none; padding: 0; margin: 0; display: flex; flex-direction: column; gap: 4px; }
	.plan-features li { font-size: 11.5px; color: var(--text-secondary); }
	.plan-features li::before { content: '· '; color: var(--text-dim); }
	.plan-check {
		position: absolute; top: 10px; right: 10px; width: 18px; height: 18px; border-radius: 50%;
		background: var(--accent); color: #fff;
		display: flex; align-items: center; justify-content: center;
	}
	.paid-note { font-size: 12px; color: var(--text-muted); background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-md); padding: 10px 12px; margin: 4px 0 0; }

	/* ── Orgs list ── */
	.orgs-container { min-height: 100vh; background: var(--bg-base); padding: 40px 32px; }
	.orgs-inner { max-width: 960px; margin: 0 auto; }

	.orgs-loading-wrap { display: flex; align-items: center; justify-content: center; padding: 80px 0; gap: 12px; color: var(--text-muted); font-size: 14px; }
	.orgs-error-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; width: 100%; }
	.orgs-empty { display: flex; flex-direction: column; align-items: center; gap: 16px; padding: 40px 0; }

	.orgs-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(240px, 1fr)); gap: 18px; }
	.org-card {
		display: block; width: 100%; padding: 0; border: none; background: none;
		text-align: left; font-family: inherit; cursor: pointer;
		border-radius: var(--radius-lg);
		transition: transform var(--transition-normal), box-shadow var(--transition-normal);
	}
	.org-card :global(.ui-card) { transition: border-color var(--transition-normal); box-shadow: var(--shadow-sm); }
	.org-card:hover { transform: translateY(-2px); }
	.org-card:hover :global(.ui-card) { border-color: var(--accent); }
	.org-card-body { display: flex; flex-direction: column; gap: 14px; }
	.org-details { display: flex; flex-direction: column; gap: 2px; }
	.org-name { font-size: 14px; font-weight: 600; color: var(--text-primary); }
	.org-slug { font-size: 12px; color: var(--text-muted); font-family: var(--font-mono); }
	.org-footer { display: flex; align-items: center; justify-content: space-between; padding-top: 10px; border-top: 1px solid var(--border); font-size: 11px; color: var(--text-muted); }
	.org-meta-item { display: inline-flex; align-items: center; gap: 4px; font-weight: 500; }
	.org-date { font-size: 11px; }

	@media (max-width: 640px) {
		.orgs-container { padding: 24px 16px; }
		.orgs-grid { grid-template-columns: 1fr; }
	}
</style>
