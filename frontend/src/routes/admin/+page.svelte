<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AdminStats } from '$lib/api/types';

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

<div class="p">
	<!-- Page header -->
	<header class="hdr">
		<div>
			<h1 class="ttl">Platform Overview</h1>
			<p class="sub">{today}</p>
		</div>
	</header>

	{#if loading}
		<div class="grid4">
			{#each [0,1,2,3] as _}
				<div class="sk-card">
					<div class="sk sk-lg"></div>
					<div class="sk sk-sm"></div>
					<div class="sk sk-xs"></div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if stats}
		<!-- Stat cards — value + label only, no color gimmicks -->
		<div class="grid4">
			{#each cards as c}
				<div class="card">
					<div class="card-icon">
						{#if c.icon === 'org'}
							<svg viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M4 4a2 2 0 012-2h8a2 2 0 012 2v12a1 1 0 01-1 1H5a1 1 0 01-1-1V4zm3 1h2v2H7V5zm2 4H7v2h2V9zm2-4h2v2h-2V5zm2 4h-2v2h2V9z" clip-rule="evenodd"/></svg>
						{:else if c.icon === 'user'}
							<svg viewBox="0 0 20 20" fill="currentColor"><path d="M9 6a3 3 0 11-6 0 3 3 0 016 0zM17 6a3 3 0 11-6 0 3 3 0 016 0zM12.93 17c.046-.327.07-.66.07-1a6.97 6.97 0 00-1.5-4.33A5 5 0 0119 16v1h-6.07zM6 11a5 5 0 015 5v1H1v-1a5 5 0 015-5z"/></svg>
						{:else if c.icon === 'paid'}
							<svg viewBox="0 0 20 20" fill="currentColor"><path d="M4 4a2 2 0 00-2 2v1h16V6a2 2 0 00-2-2H4zm14 5H2v5a2 2 0 002 2h12a2 2 0 002-2V9zM4 13h2a1 1 0 010 2H4a1 1 0 010-2zm4 0h2a1 1 0 010 2H8a1 1 0 010-2z"/></svg>
						{:else}
							<svg viewBox="0 0 20 20" fill="currentColor"><path fill-rule="evenodd" d="M2 5a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2V5zm14 1a1 1 0 11-2 0 1 1 0 012 0zM2 13a2 2 0 012-2h12a2 2 0 012 2v2a2 2 0 01-2 2H4a2 2 0 01-2-2v-2zm14 1a1 1 0 11-2 0 1 1 0 012 0z" clip-rule="evenodd"/></svg>
						{/if}
					</div>
					<span class="card-val">{c.value}</span>
					<span class="card-lbl">{c.label}</span>
					<span class="card-sub">{c.sub}</span>
				</div>
			{/each}
		</div>

		<!-- Divider -->
		<div class="divider"></div>

		<!-- Quick navigation -->
		<section>
			<h2 class="sec-lbl">Jump to</h2>
			<div class="grid4">
				{#each links as lk}
					<a href={lk.href} class="nav-card">
						<span class="nc-lbl">{lk.label}</span>
						<span class="nc-desc">{lk.desc}</span>
						<svg viewBox="0 0 20 20" fill="currentColor" class="nc-arrow" width="12" height="12">
							<path fill-rule="evenodd" d="M7.293 14.707a1 1 0 010-1.414L10.586 10 7.293 6.707a1 1 0 011.414-1.414l4 4a1 1 0 010 1.414l-4 4a1 1 0 01-1.414 0z" clip-rule="evenodd"/>
						</svg>
					</a>
				{/each}
			</div>
		</section>
	{/if}
</div>

<style>
	.p {
		padding: 40px 36px;
	}

	/* Header */
	.hdr { margin-bottom: 32px; }
	.ttl {
		font-size: 20px;
		font-weight: 700;
		color: var(--text);
		margin: 0 0 4px;
		letter-spacing: -0.025em;
		line-height: 1.2;
	}
	.sub {
		font-size: 12.5px;
		color: var(--text-3);
		margin: 0;
	}

	/* Grids */
	.grid4 {
		display: grid;
		grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
		gap: 12px;
	}
	@media (max-width: 1024px) {
		.p { padding: 28px 20px; }
	}
	@media (max-width: 768px) {
		.p { padding: 20px 14px; }
	}
	@media (max-width: 640px) {
		.p { padding: 16px 12px; }
		.grid4 { grid-template-columns: 1fr 1fr; }
		.hdr { margin-bottom: 16px; }
	}
	@media (max-width: 420px) {
		.grid4 { grid-template-columns: 1fr; }
	}

	/* Skeleton */
	.sk-card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 10px;
	}
	.sk {
		background: var(--border);
		border-radius: 4px;
		animation: sk 1.3s ease-in-out infinite;
	}
	.sk-lg { width: 28px; height: 28px; border-radius: 7px; }
	.sk-sm { width: 48px; height: 24px; }
	.sk-xs { width: 90px; height: 11px; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	/* Error */
	.err {
		padding: 11px 14px;
		background: var(--danger-soft);
		border: 1px solid rgba(220,38,38,0.2);
		border-radius: var(--radius);
		font-size: 13px;
		color: var(--danger);
	}

	/* Stat card */
	.card {
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 2px;
		box-shadow: var(--shadow-sm);
		transition: border-color 0.15s, box-shadow 0.15s;
	}
	.card:hover {
		border-color: var(--border-2);
		box-shadow: var(--shadow);
	}
	.card-icon {
		width: 28px;
		height: 28px;
		border-radius: 7px;
		background: var(--surface-2);
		border: 1px solid var(--border);
		display: flex;
		align-items: center;
		justify-content: center;
		color: var(--text-3);
		margin-bottom: 10px;
	}
	.card-icon svg { width: 14px; height: 14px; }
	.card-val {
		font-size: 28px;
		font-weight: 800;
		color: var(--text);
		letter-spacing: -0.04em;
		line-height: 1;
		font-variant-numeric: tabular-nums;
	}
	.card-lbl {
		font-size: 12.5px;
		font-weight: 600;
		color: var(--text-2);
		margin-top: 5px;
	}
	.card-sub {
		font-size: 11.5px;
		color: var(--text-3);
	}

	/* Divider */
	.divider {
		height: 1px;
		background: var(--border);
		margin: 32px 0 28px;
	}

	/* Section label */
	.sec-lbl {
		font-size: 11px;
		font-weight: 700;
		color: var(--text-3);
		text-transform: uppercase;
		letter-spacing: 0.07em;
		margin: 0 0 12px;
	}

	/* Nav card */
	.nav-card {
		position: relative;
		display: flex;
		flex-direction: column;
		gap: 3px;
		padding: 15px 16px;
		background: var(--surface);
		border: 1px solid var(--border);
		border-radius: var(--radius);
		text-decoration: none;
		cursor: pointer;
		transition: border-color 0.15s, background 0.15s;
	}
	.nav-card:hover {
		border-color: var(--accent);
		background: var(--accent-soft);
	}
	.nc-lbl {
		font-size: 13px;
		font-weight: 600;
		color: var(--text);
		padding-right: 18px;
	}
	.nc-desc {
		font-size: 11.5px;
		color: var(--text-3);
	}
	.nc-arrow {
		position: absolute;
		top: 50%;
		right: 14px;
		transform: translateY(-50%);
		color: var(--text-4);
		transition: color 0.15s, right 0.15s;
	}
	.nav-card:hover .nc-arrow {
		color: var(--accent);
		right: 12px;
	}
</style>
