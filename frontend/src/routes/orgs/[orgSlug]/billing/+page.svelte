<script lang="ts">
	import { page } from '$app/stores';
	import { orgStore } from '$lib/stores/org.store';
	import { billingStore } from '$lib/stores/billing.store';
	import type { SubscriptionTier } from '$lib/api/types';
	import { CreditCard, Server } from '@lucide/svelte';
	import {
		PageHeader, Card, Badge, StatusDot, Button, InlineAlert, Spinner,
		KeyValueList, SectionLabel, DataTable
	} from '$lib/components/ui';
	import type { DotStatus } from '$lib/utils/status';
	import { api } from '$lib/api/client';
	import { toastStore } from '$lib/stores/toast.store';

	let orgId = $derived($orgStore.activeOrg?.id ?? '');

	let billing = $derived($billingStore.billing);
	let nodes   = $derived($billingStore.nodes);
	let loading = $derived($billingStore.loading);
	let error   = $derived($billingStore.error);

	let upgradingTo = $state<'pro' | 'max' | null>(null);
	let upgradeError = $state<string | null>(null);
	let upgradeSuccess = $derived($page.url.searchParams.get('upgraded') === '1');

	$effect(() => {
		if (orgId) {
			billingStore.loadBilling(orgId);
			billingStore.loadNodes(orgId);
		}
	});

	async function upgradeTier(tier: 'pro' | 'max') {
		if (upgradingTo) return;
		upgradingTo = tier;
		upgradeError = null;

		const currentUrl = window.location.href;
		const successUrl = `${window.location.origin}/orgs/${$page.params.orgSlug}/billing?upgraded=1`;
		const cancelUrl = currentUrl;

		const res = await api.createCheckoutSession(orgId, tier, successUrl, cancelUrl);

		if (res.error || !res.data) {
			upgradeError = res.error?.message ?? 'Failed to start checkout. Please try again.';
			upgradingTo = null;
			return;
		}

		window.location.href = res.data.url;
	}

	// ─── Tier helpers ────────────────────────────────────────────────
	function tierLabel(tier: SubscriptionTier): string {
		switch (tier) {
			case 'free': return 'Free';
			case 'pro':  return 'Pro';
			case 'max':  return 'Max';
		}
	}

	type Tone = 'green' | 'red' | 'yellow' | 'blue' | 'neutral';

	function tierTone(tier: SubscriptionTier): Tone {
		switch (tier) {
			case 'free': return 'green';
			case 'pro':  return 'blue';
			case 'max':  return 'yellow';
		}
	}

	function subStatusTone(status: string): Tone {
		switch (status) {
			case 'active':   return 'green';
			case 'past_due': return 'yellow';
			case 'canceled': return 'red';
			default:         return 'neutral';
		}
	}

	function subStatusLabel(status: string): string {
		switch (status) {
			case 'active':   return 'Active';
			case 'past_due': return 'Past Due';
			case 'canceled': return 'Canceled';
			default:         return status;
		}
	}

	function formatDate(iso: string | null): string {
		if (!iso) return 'N/A';
		try {
			return new Date(iso).toLocaleDateString(undefined, {
				year: 'numeric',
				month: 'long',
				day: 'numeric',
			});
		} catch {
			return iso;
		}
	}

	// ─── Node status helpers ─────────────────────────────────────────
	function nodeDot(status: string): DotStatus {
		switch (status) {
			case 'active':                             return 'running';
			case 'provisioning':
			case 'cloud_init_running':
			case 'wireguard_joined':                   return 'deploying';
			case 'failed':                             return 'failed';
			case 'degraded':                           return 'warning';
			case 'stopped':                            return 'stopped';
			default:                                   return 'deploying';
		}
	}

	function nodeStatusLabel(status: string): string {
		switch (status) {
			case 'provisioning':      return 'Provisioning...';
			case 'cloud_init_running': return 'Running cloud-init...';
			case 'wireguard_joined':  return 'Joining network...';
			case 'active':            return 'Active';
			case 'degraded':          return 'Degraded';
			case 'failed':            return 'Failed';
			case 'stopped':           return 'Stopped';
			default:                  return status;
		}
	}

	function isNodeTransient(status: string): boolean {
		return status === 'provisioning' || status === 'cloud_init_running' || status === 'wireguard_joined';
	}

	function ramLabel(mb: number): string {
		if (mb >= 1024) return `${(mb / 1024).toFixed(0)} GB`;
		return `${mb} MB`;
	}

	// ─── Billing history ─────────────────────────────────────────────
	import type { PaymentRecord } from '$lib/api/types';
	let history = $state<PaymentRecord[]>([]);
	let historyLoading = $state(false);

	$effect(() => {
		if (orgId) {
			historyLoading = true;
			api.getBillingHistory(orgId).then(res => {
				if (res.data) history = res.data;
				historyLoading = false;
			});
		}
	});

	function fmtAmount(amount: number, currency: string): string {
		return new Intl.NumberFormat('en-US', { style: 'currency', currency: currency.toUpperCase() }).format(amount / 100);
	}

	function paymentTone(status: string): Tone {
		if (status === 'succeeded') return 'green';
		if (status === 'failed') return 'red';
		return 'neutral';
	}

	// ─── Active-transition notification ─────────────────────────────
	let prevNodeStatuses = $state(new Map<string, string>());

	$effect(() => {
		const current = billingStore.nodes;
		const transitionalStates = ['provisioning', 'cloud_init_running', 'wireguard_joined'];

		for (const node of current) {
			const prev = prevNodeStatuses.get(node.id);
			if (prev && transitionalStates.includes(prev) && node.status === 'active') {
				toastStore.add({
					type: 'success',
					title: 'Server ready',
					message: `${node.name} is now active. New deployments will route to your dedicated server.`,
				});
			}
		}

		const next = new Map<string, string>();
		for (const node of current) {
			next.set(node.id, node.status);
		}
		prevNodeStatuses = next;
	});

	// ─── Provider label helper ───────────────────────────────────────
	function providerLabel(provider: string): string {
		switch (provider.toLowerCase()) {
			case 'hetzner':      return 'Hetzner';
			case 'digitalocean': return 'DigitalOcean';
			default:             return provider;
		}
	}

	// ─── Node migration ──────────────────────────────────────────────
	let migratingNode = $state<string | null>(null);
	let migrateResult = $state<{ nodeId: string; message: string } | null>(null);
	let migrateError  = $state<string | null>(null);

	async function migrateNodeServices(nodeId: string) {
		if (migratingNode) return;
		migratingNode = nodeId;
		migrateResult = null;
		migrateError = null;

		const res = await api.migrateNode(orgId, nodeId);

		if (res.error || !res.data) {
			migrateError = res.error?.message ?? 'Migration failed.';
		} else {
			migrateResult = { nodeId, message: res.data.message };
			toastStore.add({ type: 'success', title: 'Migration started', message: res.data.message });
		}
		migratingNode = null;
	}

	// ─── Provisioning polling ────────────────────────────────────────
	let hasProvisioningNodes = $derived(
		$billingStore.nodes.some(n =>
			n.status === 'provisioning' ||
			n.status === 'cloud_init_running' ||
			n.status === 'wireguard_joined'
		)
	);

	$effect(() => {
		if (!hasProvisioningNodes || !orgId) return;

		const interval = setInterval(() => {
			billingStore.refreshNodes(orgId);
		}, 10_000);

		return () => clearInterval(interval);
	});
</script>

<div class="billing-page">

	<!-- ── Page header ──────────────────────────────────────────────── -->
	<PageHeader title="Billing &amp; Plan" subtitle="Manage your subscription and compute resources.">
		{#snippet actions()}
			{#if billing}
				<Badge tone={tierTone(billing.tier)}>{tierLabel(billing.tier)}</Badge>
			{/if}
		{/snippet}
	</PageHeader>

	{#if upgradeSuccess}
		<div role="status">
			<InlineAlert tone="success">
				Your plan upgrade is being processed. Your server will be ready shortly.
			</InlineAlert>
		</div>
	{/if}

	{#if loading}
		<div class="load-row">
			<Spinner size={16} />
			<span>Loading billing info&hellip;</span>
		</div>
	{:else if error}
		<div role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{:else if billing}

		<!-- ── Current plan card ─────────────────────────────────────── -->
		<section>
			<SectionLabel><span class="sl"><CreditCard size={12} />Current Plan</span></SectionLabel>
			<Card>
				<KeyValueList
					keyWidth="140px"
					items={[
						{ key: 'Plan' },
						{ key: 'Status' },
						{ key: 'Renewal date', value: formatDate(billing.current_period_end) }
					]}
				>
					{#snippet value(item)}
						{#if item.key === 'Plan'}
							<Badge tone={tierTone(billing.tier)}>{tierLabel(billing.tier)}</Badge>
						{:else if item.key === 'Status'}
							<Badge tone={subStatusTone(billing.sub_status)}>{subStatusLabel(billing.sub_status)}</Badge>
						{:else}
							{item.value}
						{/if}
					{/snippet}
				</KeyValueList>
			</Card>
		</section>

		<!-- ── Plan comparison ──────────────────────────────────────── -->
		<section class="plan-grid">

			<!-- Free -->
			<div class="plan" class:plan-current={billing.tier === 'free'}>
				<Card padding="20px">
					<div class="plan-body">
						<div class="plan-top">
							<span class="plan-name">Free</span>
							{#if billing.tier === 'free'}<Badge tone="blue">Current plan</Badge>{/if}
						</div>
						<div class="plan-price">$0 <span class="plan-price-period">/ month</span></div>
						<ul class="plan-features">
							<li>Shared sandbox environment</li>
							<li>512 MB RAM</li>
							<li>1 replica per service</li>
							<li>Community support</li>
						</ul>
						<div class="plan-cta">
							{#if billing.tier !== 'free'}
								<Button variant="ghost" disabled>Downgrade</Button>
							{:else}
								<Button variant="secondary" disabled>Current plan</Button>
							{/if}
						</div>
					</div>
				</Card>
			</div>

			<!-- Pro -->
			<div class="plan" class:plan-current={billing.tier === 'pro'}>
				<Card padding="20px">
					<div class="plan-body">
						<div class="plan-top">
							<span class="plan-name">Pro</span>
							{#if billing.tier === 'pro'}<Badge tone="blue">Current plan</Badge>{/if}
						</div>
						<div class="plan-price">$29 <span class="plan-price-period">/ month</span></div>
						<ul class="plan-features">
							<li>1 dedicated VM</li>
							<li>4 GB RAM</li>
							<li>Up to 5 replicas</li>
							<li>Priority support</li>
						</ul>
						<div class="plan-cta">
							{#if billing.tier === 'pro'}
								<Button variant="secondary" disabled>Current plan</Button>
							{:else if billing.tier === 'max'}
								<Button variant="ghost" disabled>Downgrade</Button>
							{:else}
								<Button disabled={upgradingTo !== null} onclick={() => upgradeTier('pro')}>
									{upgradingTo === 'pro' ? 'Redirecting...' : 'Upgrade to Pro'}
								</Button>
							{/if}
						</div>
					</div>
				</Card>
			</div>

			<!-- Max -->
			<div class="plan" class:plan-current={billing.tier === 'max'}>
				<Card padding="20px">
					<div class="plan-body">
						<div class="plan-top">
							<span class="plan-name">Max</span>
							{#if billing.tier === 'max'}<Badge tone="blue">Current plan</Badge>{/if}
						</div>
						<div class="plan-price">$99 <span class="plan-price-period">/ month</span></div>
						<ul class="plan-features">
							<li>Up to 5 dedicated VMs</li>
							<li>16 GB RAM</li>
							<li>20 replicas per service</li>
							<li>Auto-scaling</li>
							<li>Priority support</li>
						</ul>
						<div class="plan-cta">
							{#if billing.tier === 'max'}
								<Button variant="secondary" disabled>Current plan</Button>
							{:else}
								<Button disabled={upgradingTo !== null} onclick={() => upgradeTier('max')}>
									{upgradingTo === 'max' ? 'Redirecting...' : 'Upgrade to Max'}
								</Button>
							{/if}
						</div>
					</div>
				</Card>
			</div>

		</section>

		{#if upgradeError}
			<div role="alert"><InlineAlert tone="error">{upgradeError}</InlineAlert></div>
		{/if}

		<!-- ── Provisioning progress ────────────────────────────────── -->
		{#if hasProvisioningNodes}
		<Card>
			<div class="provision-header">
				<Spinner size={16} />
				<span>Setting up your dedicated server...</span>
			</div>
			<div class="provision-steps">
				{#each $billingStore.nodes.filter(n => ['provisioning','cloud_init_running','wireguard_joined'].includes(n.status)) as node}
				<div class="provision-node">
					<span class="provision-node-name">{node.name}</span>
					<div class="provision-step-list">
						<div class="provision-step" class:done={node.status !== 'provisioning'} class:active={node.status === 'provisioning'}>
							<StatusDot status={node.status !== 'provisioning' ? 'running' : 'deploying'} /> Requesting VM from {node.provider}
						</div>
						<div class="provision-step" class:done={node.status === 'wireguard_joined' || node.status === 'active'} class:active={node.status === 'cloud_init_running'}>
							<StatusDot status={node.status === 'wireguard_joined' || node.status === 'active' ? 'running' : node.status === 'cloud_init_running' ? 'deploying' : 'stopped'} /> Installing Docker &amp; WireGuard
						</div>
						<div class="provision-step" class:active={node.status === 'wireguard_joined'}>
							<StatusDot status={node.status === 'wireguard_joined' ? 'deploying' : 'stopped'} /> Joining secure network
						</div>
					</div>
				</div>
				{/each}
			</div>
			<p class="provision-note">This takes 2–3 minutes. This page updates automatically.</p>
		</Card>
		{/if}

		<!-- ── Compute nodes (pro / max only) ───────────────────────── -->
		{#if billing.tier !== 'free'}
			<section>
				<SectionLabel>
					<span class="sl"><Server size={12} />Compute Nodes · {nodes.length} node{nodes.length === 1 ? '' : 's'} assigned to this organization</span>
				</SectionLabel>

				{#if nodes.length === 0}
					<Card>
						<p class="list-empty">Your dedicated server will appear here once provisioning begins.</p>
					</Card>
				{:else}
					<Card padding="0">
						{#each nodes as node (node.id)}
							{@const transient = isNodeTransient(node.status)}
							<div class="node-card">
								<div class="node-card-top">
									<div class="node-name">{node.name}</div>
									<div class="node-badges">
										<Badge tone="blue">{providerLabel(node.provider)}</Badge>
										<Badge tone="neutral">{node.region}</Badge>
									</div>
								</div>

								<div class="node-card-meta">
									<span>{node.cpu_cores} vCPU</span>
									<span class="node-meta-sep">·</span>
									<span>{ramLabel(node.ram_mb)}</span>
									{#if node.ip_address}
										<span class="node-meta-sep">·</span>
										<span class="node-ip">{node.ip_address}</span>
									{/if}
								</div>

								<div class="node-status-row">
									<StatusDot status={nodeDot(node.status)} />
									<span class="node-status-label">{nodeStatusLabel(node.status)}</span>
									{#if transient}<Spinner size={12} />{/if}
								</div>

								{#if node.provision_error}
									<div><InlineAlert tone="error">{node.provision_error}</InlineAlert></div>
								{/if}

								{#if node.status === 'active'}
									<div>
										<Button
											variant="secondary"
											size="sm"
											onclick={() => migrateNodeServices(node.id)}
											disabled={migratingNode === node.id}
										>
											{migratingNode === node.id ? 'Starting...' : 'Migrate services'}
										</Button>
									</div>
								{/if}
							</div>
						{/each}
					</Card>
				{/if}
			</section>
		{/if}

	{/if}

	<!-- Billing History -->
	<section>
		<SectionLabel>Billing History</SectionLabel>
		{#if historyLoading}
			<Card><p class="list-empty">Loading…</p></Card>
		{:else}
			<DataTable
				items={history}
				rowKey={(row: PaymentRecord) => row.id}
				searchable={false}
				columns={[
					{ key: 'date', label: 'Date' },
					{ key: 'plan', label: 'Plan' },
					{ key: 'description', label: 'Description' },
					{ key: 'amount', label: 'Amount' },
					{ key: 'status', label: 'Status' }
				]}
				emptyMessage="No payments recorded yet."
			>
				{#snippet row(r: PaymentRecord)}
					<tr>
						<td class="hist-date">{formatDate(r.created_at)}</td>
						<td>{r.plan_name ?? '—'}</td>
						<td class="hist-desc">{r.description ?? '—'}</td>
						<td class="hist-amount">{fmtAmount(r.amount, r.currency)}</td>
						<td><Badge tone={paymentTone(r.status)}>{r.status}</Badge></td>
					</tr>
				{/snippet}
			</DataTable>
		{/if}
	</section>

</div>

<style>
	.billing-page {
		display: flex;
		flex-direction: column;
		gap: 20px;
		padding: 28px 32px 40px;
		height: 100%;
		overflow-y: auto;
		box-sizing: border-box;
	}
	.billing-page > :global(.ui-page-header) { margin-bottom: 4px; }

	.sl { display: inline-flex; align-items: center; gap: 6px; }
	.load-row {
		display: flex; align-items: center; gap: 10px;
		padding: 24px 0; color: var(--text-muted); font-size: 13px;
	}
	.list-empty {
		margin: 0;
		color: var(--text-dim);
		font-size: 13px;
		text-align: center;
	}

	/* ── Plan comparison grid ── */
	.plan-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 16px;
	}
	.plan-current :global(.ui-card) {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
	}
	.plan-body { display: flex; flex-direction: column; gap: 14px; height: 100%; }
	.plan { display: flex; flex-direction: column; }
	.plan > :global(.ui-card) { flex: 1; }
	.plan-top { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
	.plan-name { font-size: 18px; font-weight: 700; color: var(--text-primary); }
	.plan-price { font-size: 24px; font-weight: 800; color: var(--text-primary); }
	.plan-price-period { font-size: 13px; font-weight: 400; color: var(--text-muted); }
	.plan-features {
		list-style: none; margin: 0; padding: 0;
		display: flex; flex-direction: column; gap: 7px;
		flex: 1;
	}
	.plan-features li {
		font-size: 13px; color: var(--text-muted);
		padding-left: 16px;
		position: relative;
	}
	.plan-features li::before {
		content: '';
		position: absolute; left: 0; top: 6px;
		width: 6px; height: 6px;
		border-radius: 50%;
		background: var(--border-hover);
	}
	.plan-cta :global(.ui-btn) { width: 100%; }

	/* ── Nodes ── */
	.node-card {
		padding: 16px 20px;
		border-bottom: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		gap: 8px;
	}
	.node-card:last-child { border-bottom: none; }
	.node-card-top {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.node-name {
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
		font-family: var(--font-mono);
		min-width: 0;
		overflow-wrap: anywhere;
	}
	.node-badges { display: flex; gap: 6px; flex-shrink: 0; }
	.node-card-meta {
		display: flex; align-items: center; gap: 6px; flex-wrap: wrap;
		font-size: 12px; color: var(--text-muted);
	}
	.node-meta-sep { color: var(--border-hover); }
	.node-ip { font-family: var(--font-mono); }
	.node-status-row { display: flex; align-items: center; gap: 8px; }
	.node-status-label { font-size: 12px; font-weight: 500; color: var(--text-secondary); }

	/* ── Provisioning progress ── */
	.provision-header {
		display: flex;
		align-items: center;
		gap: 10px;
		font-weight: 600;
		color: var(--text-primary);
		margin-bottom: 16px;
	}
	.provision-node { margin-bottom: 12px; }
	.provision-node-name {
		font-size: 12px;
		font-weight: 600;
		color: var(--text-muted);
		margin-bottom: 8px;
		display: block;
	}
	.provision-step-list { display: flex; flex-direction: column; gap: 6px; }
	.provision-step {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: var(--text-dim);
	}
	.provision-step.done { color: var(--accent-green); }
	.provision-step.active { color: var(--accent); font-weight: 500; }
	.provision-note {
		font-size: 12px;
		color: var(--text-muted);
		margin-top: 12px;
		margin-bottom: 0;
	}

	/* ── Billing history ── */
	.hist-date { color: var(--text-secondary); font-size: 12px; }
	.hist-desc { color: var(--text-muted); font-size: 12px; }
	.hist-amount { font-weight: 600; color: var(--text-primary); }

	@media (max-width: 860px) {
		.plan-grid { grid-template-columns: 1fr; }
	}
	@media (max-width: 639px) {
		.billing-page { gap: 16px; padding: 16px 16px 72px; }
		.node-card { padding: 14px 16px; }
	}
</style>
