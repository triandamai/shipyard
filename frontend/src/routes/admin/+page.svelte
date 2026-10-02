<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AdminStats } from '$lib/api/types';
	import { Building2, Users, CreditCard, Server, ArrowRight } from '@lucide/svelte';
	import { PageHeader, StatCard, SectionLabel, Skeleton, InlineAlert } from '$lib/components/ui';

	let stats = $state<AdminStats | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		const res = await api.getAdminStats();
		if (res.data) stats = res.data;
		else error = res.error?.message ?? 'Failed to load stats';
		loading = false;
	});

	type StatCard = {
		label: string;
		value: number;
		sub: string;
		icon: string;
	};

	let cards = $derived<StatCard[]>(
		stats
			? [
					{ label: 'Organizations', value: stats.total_orgs,   sub: `${stats.paid_orgs} on paid plan`,    icon: 'org' },
					{ label: 'Total Users',   value: stats.total_users,  sub: 'across all orgs',                   icon: 'user' },
					{ label: 'Paid Orgs',     value: stats.paid_orgs,    sub: 'active subscriptions',              icon: 'paid' },
					{ label: 'Active Nodes',  value: stats.active_nodes, sub: 'compute running now',               icon: 'node' }
				]
			: []
	);

	const links = [
		{ href: '/admin/orgs',   label: 'Organizations', desc: 'Manage tiers & suspension' },
		{ href: '/admin/users',  label: 'Users',         desc: 'Grant or revoke admin roles' },
		{ href: '/admin/nodes',  label: 'Compute',       desc: 'Monitor node infrastructure' },
		{ href: '/admin/config', label: 'Config',        desc: 'Edit platform-wide settings' }
	];

	let today = new Intl.DateTimeFormat('en-US', { weekday: 'long', month: 'long', day: 'numeric' }).format(new Date());
</script>

<PageHeader title="Platform Overview" subtitle={today} />

{#if loading}
	<div class="dash-grid4">
		{#each Array(4) as _}
			<Skeleton variant="card" height="96px" />
		{/each}
	</div>
{:else if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else if stats}
	<div class="dash-grid4">
		{#each cards as c}
			<StatCard
				tone="blue"
				value={c.value}
				label={c.label}
			>
				{#snippet icon()}
					{#if c.icon === 'org'}<Building2 size={16} />
					{:else if c.icon === 'user'}<Users size={16} />
					{:else if c.icon === 'paid'}<CreditCard size={16} />
					{:else}<Server size={16} />{/if}
				{/snippet}
			</StatCard>
		{/each}
	</div>

	<div class="dash-divider"></div>

	<section>
		<SectionLabel>Jump to</SectionLabel>
		<div class="dash-grid4">
			{#each links as lk}
				<a href={lk.href} class="dash-nav-card">
					<span class="dash-nav-card-lbl">{lk.label}</span>
					<span class="dash-nav-card-desc">{lk.desc}</span>
					<ArrowRight size={12} class="dash-nav-card-arrow" />
				</a>
			{/each}
		</div>
	</section>
{/if}

<style>
	.dash-grid4 {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 12px;
	}
	@media (max-width: 640px) {
		.dash-grid4 { grid-template-columns: 1fr 1fr; }
	}
	@media (max-width: 420px) {
		.dash-grid4 { grid-template-columns: 1fr; }
	}

	.dash-divider {
		height: 1px;
		background: var(--border);
		margin: 32px 0 28px;
	}

	.dash-nav-card {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 15px 16px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		text-decoration: none;
		cursor: pointer;
		transition: border-color var(--transition-fast), background var(--transition-fast);
	}
	.dash-nav-card:hover { border-color: var(--accent); background: var(--accent-muted); }
	.dash-nav-card-lbl { font-size: 13px; font-weight: 600; color: var(--text-primary); padding-right: 18px; }
	.dash-nav-card-desc { font-size: 11.5px; color: var(--text-muted); }
	:global(.dash-nav-card-arrow) {
		position: absolute;
		top: 50%;
		right: 14px;
		transform: translateY(-50%);
		color: var(--text-dim);
		transition: color var(--transition-fast), right var(--transition-fast);
	}
	.dash-nav-card:hover :global(.dash-nav-card-arrow) { color: var(--accent); right: 12px; }
</style>
