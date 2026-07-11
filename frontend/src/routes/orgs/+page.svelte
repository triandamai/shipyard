<script lang="ts">
	import { onMount } from 'svelte';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { authStore } from '$lib/stores/auth.store';
	import { orgStore } from '$lib/stores/org.store';
	import { get } from 'svelte/store';
	import type { Organization } from '$lib/api/types';

	let orgs = $state<Organization[]>([]);
	let loading = $state(true);
	let fetchError = $state('');

	// New org modal
	let showModal = $state(false);
	let newOrgName = $state('');
	let newOrgSlug = $state('');
	let creating = $state(false);
	let createError = $state('');

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

	async function handleCreateOrg(e: SubmitEvent) {
		e.preventDefault();
		createError = '';
		creating = true;

		try {
			const res = await api.createOrg(newOrgName, newOrgSlug);

			if (res.error || !res.data) {
				createError = res.error?.message ?? 'Failed to create organization.';
				return;
			}

			orgs = [...orgs, res.data];
			orgStore.setOrganizations(orgs);
			closeModal();
		} finally {
			creating = false;
		}
	}

	function openModal() {
		newOrgName = '';
		newOrgSlug = '';
		createError = '';
		showModal = true;
	}

	function closeModal() {
		showModal = false;
		newOrgName = '';
		newOrgSlug = '';
		createError = '';
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
	<div
		style="
			position: fixed;
			inset: 0;
			background: rgba(0,0,0,0.6);
			z-index: 50;
			display: flex;
			align-items: center;
			justify-content: center;
			padding: 24px;
		"
		onclick={(e) => { if (e.target === e.currentTarget) closeModal(); }}
		onkeydown={(e) => { if (e.key === 'Escape') closeModal(); }}
		role="dialog"
		aria-modal="true"
		aria-label="Create organization"
		tabindex="-1"
	>
		<div
			style="
				background: var(--bg-elevated);
				border: 1px solid var(--border);
				border-radius: var(--radius-xl);
				padding: 28px;
				width: 100%;
				max-width: 420px;
				box-shadow: var(--shadow-lg);
			"
		>
			<h2 style="font-size: 16px; font-weight: 600; margin-bottom: 20px;">
				New Organization
			</h2>

			{#if createError}
				<div
					style="
						padding: 10px 14px;
						background: var(--accent-red-muted);
						border: 1px solid var(--accent-red);
						border-radius: var(--radius-md);
						color: var(--accent-red);
						font-size: 13px;
						margin-bottom: 16px;
					"
				>
					{createError}
				</div>
			{/if}

			<form
				onsubmit={handleCreateOrg}
				style="display: flex; flex-direction: column; gap: 16px;"
			>
				<div style="display: flex; flex-direction: column; gap: 6px;">
					<label
						for="org-name"
						style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
					>
						Name
					</label>
					<input
						id="org-name"
						type="text"
						class="input"
						placeholder="My Organization"
						bind:value={newOrgName}
						required
					/>
				</div>

				<div style="display: flex; flex-direction: column; gap: 6px;">
					<label
						for="org-slug"
						style="font-size: 13px; font-weight: 500; color: var(--text-secondary);"
					>
						Slug
					</label>
					<input
						id="org-slug"
						type="text"
						class="input"
						placeholder="my-organization"
						bind:value={newOrgSlug}
						required
						pattern="[a-z0-9-]+"
					/>
					<span style="font-size: 11px; color: var(--text-dim);">
						Lowercase letters, numbers, and hyphens only
					</span>
				</div>

				<div
					style="
						display: flex;
						justify-content: flex-end;
						gap: 10px;
						margin-top: 4px;
					"
				>
					<button
						type="button"
						class="btn btn-secondary"
						onclick={closeModal}
						disabled={creating}
					>
						Cancel
					</button>
					<button
						type="submit"
						class="btn btn-primary"
						disabled={creating || !newOrgName.trim()}
					>
						{#if creating}
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
							Creating…
						{:else}
							Create
						{/if}
					</button>
				</div>
			</form>
		</div>
	</div>
{/if}

<style>
	@keyframes spin {
		to { transform: rotate(360deg); }
	}
</style>
