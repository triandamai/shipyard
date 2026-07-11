<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { get } from 'svelte/store';
	import type { Organization, Plan } from '$lib/api/types';

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

	function goToOrg(org: Organization) {
		orgStore.setActiveOrg(org);
		goto(`/orgs/${org.slug}/projects`);
	}
</script>

<div
	style="
		min-height: 100vh;
		background: var(--bg-base);
		padding: 40px 32px;
	"
>
	<div style="max-width: 960px; margin: 0 auto;">

		<!-- Header -->
		<div
			style="
				display: flex;
				align-items: center;
				justify-content: space-between;
				margin-bottom: 32px;
			"
		>
			<div>
				<h1 style="font-size: 22px; font-weight: 700; margin-bottom: 4px;">
					Your Organizations
				</h1>
				<p style="color: var(--text-muted); font-size: 14px;">
					Select an organization to view its projects
				</p>
			</div>
			<button class="btn btn-primary" onclick={openModal}>
				+ New
			</button>
		</div>

		<!-- Loading -->
		{#if loading}
			<div
				style="
					display: flex;
					align-items: center;
					justify-content: center;
					padding: 80px 0;
					gap: 12px;
					color: var(--text-muted);
					font-size: 14px;
				"
			>
				<span
					style="
						width: 20px;
						height: 20px;
						border: 2px solid var(--border);
						border-top-color: var(--accent);
						border-radius: 50%;
						display: inline-block;
						animation: spin 0.7s linear infinite;
					"
				></span>
				Loading organizations…
			</div>

		<!-- Error -->
		{:else if fetchError}
			<div
				style="
					padding: 16px 20px;
					background: var(--accent-red-muted);
					border: 1px solid var(--accent-red);
					border-radius: var(--radius-md);
					color: var(--accent-red);
					font-size: 13px;
					display: flex;
					align-items: center;
					justify-content: space-between;
				"
			>
				<span>{fetchError}</span>
				<button class="btn btn-ghost btn-sm" onclick={loadOrgs}>Retry</button>
			</div>

		<!-- Empty state -->
		{:else if orgs.length === 0}
			<div
				style="
					display: flex;
					flex-direction: column;
					align-items: center;
					justify-content: center;
					padding: 80px 0;
					gap: 16px;
					text-align: center;
				"
			>
				<div
					style="
						width: 56px;
						height: 56px;
						border-radius: var(--radius-lg);
						background: var(--bg-elevated);
						border: 1px solid var(--border);
						display: flex;
						align-items: center;
						justify-content: center;
						font-size: 24px;
					"
				>
					🏢
				</div>
				<div>
					<p style="color: var(--text-primary); font-weight: 500; margin-bottom: 4px;">
						No organizations yet
					</p>
					<p style="color: var(--text-muted); font-size: 13px;">
						Create your first organization to get started
					</p>
				</div>
				<button class="btn btn-primary" onclick={openModal}>
					+ New Organization
				</button>
			</div>

		<!-- Org grid -->
		{:else}
			<div
				style="
					display: grid;
					grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
					gap: 16px;
				"
			>
				{#each orgs as org (org.id)}
					<button
						class="card card-interactive"
						style="
							text-align: left;
							background: var(--bg-surface);
							border: 1px solid var(--border);
							border-radius: var(--radius-lg);
							padding: 20px;
							cursor: pointer;
							display: flex;
							flex-direction: column;
							gap: 10px;
							transition: all var(--transition-fast);
						"
						onclick={() => goToOrg(org)}
					>
						<div
							style="
								width: 40px;
								height: 40px;
								border-radius: var(--radius-md);
								background: var(--accent-muted);
								border: 1px solid rgba(124, 106, 247, 0.3);
								display: flex;
								align-items: center;
								justify-content: center;
								font-size: 18px;
								font-weight: 700;
								color: var(--accent);
							"
						>
							{org.name[0]?.toUpperCase() ?? '?'}
						</div>
						<div>
							<div
								style="
									font-size: 14px;
									font-weight: 600;
									color: var(--text-primary);
									margin-bottom: 2px;
								"
							>
								{org.name}
							</div>
							<div
								style="
									font-size: 12px;
									color: var(--text-muted);
									font-family: var(--font-mono);
								"
							>
								{org.slug}
							</div>
						</div>
					</button>
				{/each}
			</div>
		{/if}

	</div>
</div>

<!-- New Org Modal -->
{#if showModal}
	<!-- Backdrop -->
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<div
		class="modal-backdrop"
		onclick={(e) => { if (e.target === e.currentTarget) closeModal(); }}
		onkeydown={(e) => { if (e.key === 'Escape') closeModal(); }}
		role="dialog"
		aria-modal="true"
		aria-label="Create organization"
		tabindex="-1"
	>
		<div class="modal" style="max-width: {modalStep === 2 ? '640px' : '420px'}">
			<!-- Header -->
			<div class="modal-header">
				<div>
					<h2 class="modal-title">New Organization</h2>
					<div class="step-indicator">
						<span class="step" class:active={modalStep === 1} class:done={modalStep > 1}>1 Details</span>
						<span class="step-sep">→</span>
						<span class="step" class:active={modalStep === 2}>2 Plan</span>
					</div>
				</div>
				<!-- svelte-ignore a11y_consider_explicit_label -->
				<button class="modal-close" onclick={closeModal}>✕</button>
			</div>

			{#if createError}
				<div class="err-banner">{createError}</div>
			{/if}

			<!-- Step 1: Name + Slug -->
			{#if modalStep === 1}
				<div class="modal-body">
					<div class="field">
						<label class="lbl" for="org-name">Name</label>
						<input id="org-name" type="text" class="inp" placeholder="My Organization" bind:value={newOrgName} />
					</div>
					<div class="field">
						<label class="lbl" for="org-slug">Slug</label>
						<input id="org-slug" type="text" class="inp" placeholder="my-organization" bind:value={newOrgSlug} pattern="[a-z0-9-]+" />
						<span class="hint">Lowercase letters, numbers, and hyphens only</span>
					</div>
				</div>
				<div class="modal-foot">
					<button class="btn-cancel" onclick={closeModal}>Cancel</button>
					<button class="btn-confirm" onclick={goToStep2} disabled={!newOrgName.trim() || !newOrgSlug.trim()}>
						Next →
					</button>
				</div>

			<!-- Step 2: Plan selection -->
			{:else}
				<div class="modal-body">
					{#if plansLoading}
						<div class="plans-loading">Loading plans…</div>
					{:else}
						<div class="plans-grid">
							{#each plans as plan (plan.id)}
								<!-- svelte-ignore a11y_click_events_have_key_events -->
								<!-- svelte-ignore a11y_no_static_element_interactions -->
								<div
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
										<li>{plan.cpu_cores} CPU · {plan.memory_gb} GB RAM</li>
										<li>{formatLimit(plan.max_members)} members</li>
										<li>{formatLimit(plan.max_projects)} projects</li>
										<li>{formatLimit(plan.max_replicas)} replicas</li>
										<li>{formatLimit(plan.max_git_providers)} git providers</li>
									</ul>
									{#if selectedPlanId === plan.id}
										<div class="plan-check">✓</div>
									{/if}
								</div>
							{/each}
						</div>
						{#if selectedPlanId && plans.find(p => p.id === selectedPlanId && p.price_monthly > 0)}
							<p class="paid-note">You'll be redirected to checkout after the organization is created.</p>
						{/if}
					{/if}
				</div>
				<div class="modal-foot">
					<button class="btn-cancel" onclick={() => (modalStep = 1)}>← Back</button>
					<button class="btn-confirm" onclick={handleCreateOrg} disabled={creating || !selectedPlanId}>
						{#if creating}Creating…{:else}Create Organization{/if}
					</button>
				</div>
			{/if}
		</div>
	</div>
{/if}

<style>
	@keyframes spin { to { transform: rotate(360deg); } }

	.modal-backdrop { position:fixed; inset:0; background:rgba(0,0,0,0.6); z-index:50; display:flex; align-items:center; justify-content:center; padding:24px; }
	.modal { background:var(--bg-elevated); border:1px solid var(--border); border-radius:var(--radius-xl); width:100%; box-shadow:var(--shadow-lg); display:flex; flex-direction:column; }
	.modal-header { display:flex; align-items:flex-start; justify-content:space-between; padding:22px 24px 0; }
	.modal-title { font-size:16px; font-weight:600; margin-bottom:6px; }
	.modal-close { background:none; border:none; color:var(--text-muted); font-size:14px; cursor:pointer; padding:4px; border-radius:4px; line-height:1; }
	.modal-close:hover { color:var(--text-primary); background:var(--bg-surface); }
	.modal-body { padding:20px 24px; display:flex; flex-direction:column; gap:16px; }
	.modal-foot { display:flex; justify-content:flex-end; gap:10px; padding:16px 24px; border-top:1px solid var(--border); }

	.step-indicator { display:flex; align-items:center; gap:8px; font-size:11.5px; }
	.step { color:var(--text-muted); font-weight:500; }
	.step.active { color:var(--accent); font-weight:700; }
	.step.done { color:var(--text-secondary); }
	.step-sep { color:var(--text-dim); font-size:10px; }

	.field { display:flex; flex-direction:column; gap:5px; }
	.lbl { font-size:12px; font-weight:600; color:var(--text-secondary); }
	.inp { height:34px; padding:0 10px; background:var(--bg-surface); border:1px solid var(--border); border-radius:var(--radius-md); font-size:13px; color:var(--text-primary); outline:none; width:100%; box-sizing:border-box; }
	.inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-muted); }
	.hint { font-size:11px; color:var(--text-dim); }

	.err-banner { margin:12px 24px 0; padding:10px 14px; background:var(--accent-red-muted); border:1px solid var(--accent-red); border-radius:var(--radius-md); color:var(--accent-red); font-size:13px; }

	.plans-loading { text-align:center; padding:32px; color:var(--text-muted); font-size:13px; }
	.plans-grid { display:grid; grid-template-columns:repeat(auto-fill, minmax(160px, 1fr)); gap:12px; }
	.plan-card { position:relative; padding:16px; border:2px solid var(--border); border-radius:var(--radius-lg); cursor:pointer; transition:border-color .15s, background .15s; background:var(--bg-surface); }
	.plan-card:hover { border-color:var(--accent); }
	.plan-card.selected { border-color:var(--accent); background:var(--accent-muted); }
	.plan-top { display:flex; flex-direction:column; gap:4px; margin-bottom:12px; }
	.plan-name { font-size:13px; font-weight:700; color:var(--text-primary); text-transform:capitalize; }
	.plan-price { font-size:18px; font-weight:700; color:var(--accent); }
	.plan-period { font-size:11px; font-weight:400; color:var(--text-muted); }
	.plan-features { list-style:none; padding:0; margin:0; display:flex; flex-direction:column; gap:4px; }
	.plan-features li { font-size:11.5px; color:var(--text-secondary); }
	.plan-features li::before { content:'· '; color:var(--text-dim); }
	.plan-check { position:absolute; top:10px; right:10px; width:18px; height:18px; border-radius:50%; background:var(--accent); color:#000; font-size:10px; font-weight:800; display:flex; align-items:center; justify-content:center; }
	.paid-note { font-size:12px; color:var(--text-muted); background:var(--bg-surface); border:1px solid var(--border); border-radius:var(--radius-md); padding:10px 12px; margin-top:4px; }

	.btn-cancel { padding:6px 14px; border-radius:var(--radius-md); font-size:13px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--bg-surface); color:var(--text-secondary); }
	.btn-cancel:hover { background:var(--bg-elevated); }
	.btn-confirm { padding:6px 16px; border-radius:var(--radius-md); font-size:13px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; }
	.btn-confirm:hover:not(:disabled) { opacity:.88; }
	.btn-confirm:disabled { opacity:.45; cursor:not-allowed; }
</style>
