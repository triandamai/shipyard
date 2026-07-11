<script lang="ts">
	import { page } from '$app/stores';
	import { orgStore } from '$lib/stores/org.store';
	import { billingStore } from '$lib/stores/billing.store';
	import type { SubscriptionTier } from '$lib/api/types';
	import { CreditCard, Server, AlertCircle } from '@lucide/svelte';
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

	function tierClass(tier: SubscriptionTier): string {
		switch (tier) {
			case 'free': return 'tier-free';
			case 'pro':  return 'tier-pro';
			case 'max':  return 'tier-max';
		}
	}

	function subStatusClass(status: string): string {
		switch (status) {
			case 'active':   return 'status-active';
			case 'past_due': return 'status-past-due';
			case 'canceled': return 'status-canceled';
			default:         return 'status-unknown';
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
	type NodeStatusClass = 'node-active' | 'node-provisioning' | 'node-failed' | 'node-degraded' | 'node-stopped';

	function nodeStatusClass(status: string): NodeStatusClass {
		switch (status) {
			case 'active':                             return 'node-active';
			case 'provisioning':
			case 'cloud_init_running':
			case 'wireguard_joined':                   return 'node-provisioning';
			case 'failed':                             return 'node-failed';
			case 'degraded':                           return 'node-degraded';
			case 'stopped':                            return 'node-stopped';
			default:                                   return 'node-provisioning';
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

	function paymentStatusClass(status: string): string {
		if (status === 'succeeded') return 'pay-success';
		if (status === 'failed') return 'pay-failed';
		return 'pay-pending';
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
	<div class="page-header">
		<div class="page-header-text">
			<h2 class="page-title">Billing &amp; Plan</h2>
			<p class="page-desc">Manage your subscription and compute resources.</p>
		</div>
		{#if billing}
			<span class="tier-badge {tierClass(billing.tier)}">{tierLabel(billing.tier)}</span>
		{/if}
	</div>

	{#if upgradeSuccess}
		<div class="success-banner">
			Your plan upgrade is being processed. Your server will be ready shortly.
		</div>
	{/if}

	{#if loading}
		<div class="load-row">
			<div class="spinner"></div>
			<span>Loading billing info&hellip;</span>
		</div>
	{:else if error}
		<div class="error-banner">
			<AlertCircle size={14} />{error}
		</div>
	{:else if billing}

		<!-- ── Current plan card ─────────────────────────────────────── -->
		<section class="settings-section">
			<div class="section-header">
				<div class="section-icon"><CreditCard size={16} /></div>
				<div>
					<h2 class="section-title">Current Plan</h2>
					<p class="section-desc">Your active subscription details</p>
				</div>
			</div>
			<div class="plan-detail-body">
				<div class="plan-detail-row">
					<span class="plan-detail-label">Plan</span>
					<span class="tier-badge {tierClass(billing.tier)}">{tierLabel(billing.tier)}</span>
				</div>
				<div class="plan-detail-row">
					<span class="plan-detail-label">Status</span>
					<span class="status-badge {subStatusClass(billing.sub_status)}">{subStatusLabel(billing.sub_status)}</span>
				</div>
				<div class="plan-detail-row">
					<span class="plan-detail-label">Renewal date</span>
					<span class="plan-detail-value">{formatDate(billing.current_period_end)}</span>
				</div>
			</div>
		</section>

		<!-- ── Plan comparison ──────────────────────────────────────── -->
		<section class="plan-grid">

			<!-- Free -->
			<div class="plan-card" class:plan-card-current={billing.tier === 'free'}>
				{#if billing.tier === 'free'}
					<span class="plan-current-label">Current plan</span>
				{/if}
				<div class="plan-name tier-free-text">Free</div>
				<div class="plan-price">$0 <span class="plan-price-period">/ month</span></div>
				<ul class="plan-features">
					<li>Shared sandbox environment</li>
					<li>512 MB RAM</li>
					<li>1 replica per service</li>
					<li>Community support</li>
				</ul>
				{#if billing.tier !== 'free'}
					<button class="plan-btn plan-btn-ghost" disabled>Downgrade</button>
				{:else}
					<button class="plan-btn plan-btn-current" disabled>Current plan</button>
				{/if}
			</div>

			<!-- Pro -->
			<div class="plan-card" class:plan-card-current={billing.tier === 'pro'}>
				{#if billing.tier === 'pro'}
					<span class="plan-current-label">Current plan</span>
				{/if}
				<div class="plan-name tier-pro-text">Pro</div>
				<div class="plan-price">$29 <span class="plan-price-period">/ month</span></div>
				<ul class="plan-features">
					<li>1 dedicated VM</li>
					<li>4 GB RAM</li>
					<li>Up to 5 replicas</li>
					<li>Priority support</li>
				</ul>
				{#if billing.tier === 'pro'}
					<button class="plan-btn plan-btn-current" disabled>Current plan</button>
				{:else if billing.tier === 'max'}
					<button class="plan-btn plan-btn-ghost" disabled>Downgrade</button>
				{:else}
					<button
						class="plan-btn plan-btn-upgrade tier-pro-btn"
						disabled={upgradingTo !== null}
						onclick={() => upgradeTier('pro')}
					>
						{upgradingTo === 'pro' ? 'Redirecting...' : 'Upgrade to Pro'}
					</button>
				{/if}
			</div>

			<!-- Max -->
			<div class="plan-card" class:plan-card-current={billing.tier === 'max'}>
				{#if billing.tier === 'max'}
					<span class="plan-current-label">Current plan</span>
				{/if}
				<div class="plan-name tier-max-text">Max</div>
				<div class="plan-price">$99 <span class="plan-price-period">/ month</span></div>
				<ul class="plan-features">
					<li>Up to 5 dedicated VMs</li>
					<li>16 GB RAM</li>
					<li>20 replicas per service</li>
					<li>Auto-scaling</li>
					<li>Priority support</li>
				</ul>
				{#if billing.tier === 'max'}
					<button class="plan-btn plan-btn-current" disabled>Current plan</button>
				{:else}
					<button
						class="plan-btn plan-btn-upgrade tier-max-btn"
						disabled={upgradingTo !== null}
						onclick={() => upgradeTier('max')}
					>
						{upgradingTo === 'max' ? 'Redirecting...' : 'Upgrade to Max'}
					</button>
				{/if}
			</div>

		</section>

		{#if upgradeError}
			<p class="upgrade-error">{upgradeError}</p>
		{/if}

		<!-- ── Provisioning progress ────────────────────────────────── -->
		{#if hasProvisioningNodes}
		<div class="provisioning-progress">
			<div class="provision-header">
				<div class="provision-spinner"></div>
				<span>Setting up your dedicated server...</span>
			</div>
			<div class="provision-steps">
				{#each $billingStore.nodes.filter(n => ['provisioning','cloud_init_running','wireguard_joined'].includes(n.status)) as node}
				<div class="provision-node">
					<span class="provision-node-name">{node.name}</span>
					<div class="provision-step-list">
						<div class="provision-step" class:done={node.status !== 'provisioning'} class:active={node.status === 'provisioning'}>
							<span class="step-dot"></span> Requesting VM from {node.provider}
						</div>
						<div class="provision-step" class:done={node.status === 'wireguard_joined' || node.status === 'active'} class:active={node.status === 'cloud_init_running'}>
							<span class="step-dot"></span> Installing Docker &amp; WireGuard
						</div>
						<div class="provision-step" class:active={node.status === 'wireguard_joined'}>
							<span class="step-dot"></span> Joining secure network
						</div>
					</div>
				</div>
				{/each}
			</div>
			<p class="provision-note">This takes 2–3 minutes. This page updates automatically.</p>
		</div>
		{/if}

		<!-- ── Compute nodes (pro / max only) ───────────────────────── -->
		{#if billing.tier !== 'free'}
			<section class="settings-section">
				<div class="section-header">
					<div class="section-icon"><Server size={16} /></div>
					<div>
						<h2 class="section-title">Compute Nodes</h2>
						<p class="section-desc">
							{nodes.length} node{nodes.length === 1 ? '' : 's'} assigned to this organization
						</p>
					</div>
				</div>

				{#if nodes.length === 0}
					<div class="list-empty">
						Your dedicated server will appear here once provisioning begins.
					</div>
				{:else}
					<div class="nodes-list">
						{#each nodes as node (node.id)}
							{@const statusClass = nodeStatusClass(node.status)}
							{@const transient = isNodeTransient(node.status)}
							<div class="node-card">
								<div class="node-card-top">
									<div class="node-name">{node.name}</div>
									<div class="node-badges">
										<span class="provider-badge">{providerLabel(node.provider)}</span>
										<span class="region-badge">{node.region}</span>
									</div>
								</div>

								<div class="node-card-meta">
									<span class="node-meta-item">{node.cpu_cores} vCPU</span>
									<span class="node-meta-sep">·</span>
									<span class="node-meta-item">{ramLabel(node.ram_mb)}</span>
									{#if node.ip_address}
										<span class="node-meta-sep">·</span>
										<span class="node-meta-item node-ip">{node.ip_address}</span>
									{/if}
								</div>

								<div class="node-status-row">
									<span class="node-status-dot {statusClass}" class:node-dot-pulse={transient}></span>
									<span class="node-status-label {statusClass}-text">{nodeStatusLabel(node.status)}</span>
									{#if transient}
										<div class="node-progress">
											<div class="node-progress-bar"></div>
										</div>
									{/if}
								</div>

								{#if node.provision_error}
									<div class="node-error">{node.provision_error}</div>
								{/if}

								{#if node.status === 'active'}
									<button
										class="migrate-btn"
										onclick={() => migrateNodeServices(node.id)}
										disabled={migratingNode === node.id}
									>
										{migratingNode === node.id ? 'Starting...' : 'Migrate services'}
									</button>
								{/if}
							</div>
						{/each}
					</div>
				{/if}
			</section>
		{/if}

	{/if}

	<!-- Billing History -->
	<section class="section">
		<h2 class="section-title">Billing History</h2>
		{#if historyLoading}
			<div class="hist-empty">Loading…</div>
		{:else if history.length === 0}
			<div class="hist-empty">No payments recorded yet.</div>
		{:else}
			<div class="hist-table">
				<div class="hist-head">
					<span style="flex:2">Date</span>
					<span style="flex:2">Plan</span>
					<span style="flex:2">Description</span>
					<span style="flex:1">Amount</span>
					<span style="flex:1">Status</span>
				</div>
				{#each history as row (row.id)}
					<div class="hist-row">
						<span style="flex:2" class="hist-date">{formatDate(row.created_at)}</span>
						<span style="flex:2">{row.plan_name ?? '—'}</span>
						<span style="flex:2" class="hist-desc">{row.description ?? '—'}</span>
						<span style="flex:1" class="hist-amount">{fmtAmount(row.amount, row.currency)}</span>
						<span style="flex:1">
							<span class="pay-badge {paymentStatusClass(row.status)}">{row.status}</span>
						</span>
					</div>
				{/each}
			</div>
		{/if}
	</section>

</div>

<style>
	@keyframes spin   { to { transform: rotate(360deg); } }
	@keyframes pulse  { 0%, 100% { opacity: 1; } 50% { opacity: 0.35; } }
	@keyframes slide  { 0% { transform: translateX(-100%); } 100% { transform: translateX(400%); } }

	/* ── Page shell ── */
	.billing-page { display: flex; flex-direction: column; gap: 20px; }

	.page-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 12px;
	}
	.page-title { font-size: 16px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.page-desc  { font-size: 13px; color: var(--text-muted); margin: 0; }

	/* ── Shared section chrome ── */
	.settings-section {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		overflow: hidden;
	}

	.section-header {
		display: flex;
		gap: 14px;
		padding: 18px 20px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}

	.section-icon {
		width: 32px; height: 32px; border-radius: var(--radius-md);
		background: rgba(37,99,235,0.1); color: var(--accent);
		display: flex; align-items: center; justify-content: center;
		flex-shrink: 0; margin-top: 1px;
	}

	.section-title { font-size: 14px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.section-desc  { font-size: 12px; color: var(--text-muted); margin: 0; line-height: 1.5; }

	/* ── Loading / error ── */
	.load-row {
		display: flex; align-items: center; gap: 10px;
		padding: 24px 0; color: var(--text-muted); font-size: 13px;
	}
	.spinner {
		width: 16px; height: 16px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
		flex-shrink: 0;
	}
	.error-banner {
		display: flex; align-items: center; gap: 8px;
		padding: 12px 16px;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.25);
		border-radius: var(--radius-md);
		color: #EF4444; font-size: 13px;
	}

	.success-banner {
		padding: 12px 16px;
		background: rgba(16,185,129,0.08);
		border: 1px solid rgba(16,185,129,0.25);
		border-radius: var(--radius-md);
		color: #10B981; font-size: 13px;
	}

	.upgrade-error {
		margin: 0;
		padding: 10px 14px;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.25);
		border-radius: var(--radius-md);
		color: #EF4444; font-size: 13px;
	}

	.list-empty {
		padding: 24px 20px;
		color: var(--text-dim);
		font-size: 13px;
		font-style: italic;
	}

	/* ── Tier badges ── */
	.tier-badge {
		display: inline-flex; align-items: center;
		font-size: 11px; font-weight: 700; padding: 3px 10px;
		border-radius: 999px; letter-spacing: 0.03em; text-transform: uppercase;
		flex-shrink: 0;
	}
	.tier-free { background: rgba(16,185,129,0.12); color: #10B981; border: 1px solid rgba(16,185,129,0.3); }
	.tier-pro  { background: rgba(37,99,235,0.12);  color: #2563EB; border: 1px solid rgba(37,99,235,0.3); }
	.tier-max  { background: rgba(139,92,246,0.12); color: #7C3AED; border: 1px solid rgba(139,92,246,0.3); }

	/* ── Sub-status badges ── */
	.status-badge {
		display: inline-flex; align-items: center;
		font-size: 11px; font-weight: 600; padding: 3px 9px;
		border-radius: 999px;
	}
	.status-active   { background: rgba(16,185,129,0.12); color: #10B981; border: 1px solid rgba(16,185,129,0.3); }
	.status-past-due { background: rgba(245,158,11,0.12); color: #D97706; border: 1px solid rgba(245,158,11,0.3); }
	.status-canceled { background: rgba(239,68,68,0.12);  color: #EF4444; border: 1px solid rgba(239,68,68,0.3); }
	.status-unknown  { background: var(--bg-elevated); color: var(--text-muted); border: 1px solid var(--border); }

	/* ── Current plan detail card body ── */
	.plan-detail-body { padding: 4px 0; }
	.plan-detail-row {
		display: flex; align-items: center; justify-content: space-between;
		padding: 12px 20px; border-bottom: 1px solid var(--border);
	}
	.plan-detail-row:last-child { border-bottom: none; }
	.plan-detail-label { font-size: 13px; color: var(--text-muted); }
	.plan-detail-value { font-size: 13px; color: var(--text-primary); font-weight: 500; }

	/* ── Plan comparison grid ── */
	.plan-grid {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: 16px;
	}

	.plan-card {
		position: relative;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 14px;
		transition: border-color 0.15s;
	}
	.plan-card-current {
		border-color: var(--accent);
		box-shadow: 0 0 0 1px var(--accent) inset;
	}

	.plan-current-label {
		position: absolute;
		top: -1px; right: 16px;
		font-size: 10px; font-weight: 700; text-transform: uppercase; letter-spacing: 0.05em;
		padding: 3px 9px;
		background: var(--accent);
		color: #fff;
		border-radius: 0 0 var(--radius-sm) var(--radius-sm);
	}

	.plan-name { font-size: 18px; font-weight: 700; }
	.tier-free-text { color: #10B981; }
	.tier-pro-text  { color: #2563EB; }
	.tier-max-text  { color: #7C3AED; }

	.plan-price {
		font-size: 24px; font-weight: 800; color: var(--text-primary);
	}
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
		background: var(--border);
	}

	/* Plan CTA buttons */
	.plan-btn {
		width: 100%; padding: 9px 16px;
		border-radius: var(--radius-md);
		font-size: 13px; font-weight: 600;
		cursor: pointer; border: none;
		transition: opacity 0.15s, background 0.15s;
	}
	.plan-btn:disabled { cursor: default; }

	.plan-btn-current {
		background: var(--bg-elevated);
		color: var(--text-dim);
		border: 1px solid var(--border);
	}
	.plan-btn-ghost {
		background: transparent;
		color: var(--text-muted);
		border: 1px solid var(--border);
	}
	.plan-btn-upgrade { color: #fff; }
	.plan-btn-upgrade:hover { opacity: 0.88; }

	.tier-pro-btn { background: #2563EB; }
	.tier-max-btn { background: #7C3AED; }

	/* ── Nodes list ── */
	.nodes-list {
		display: flex;
		flex-direction: column;
	}

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
	}

	.node-badges { display: flex; gap: 6px; }

	.provider-badge {
		display: inline-block;
		font-size: 11px;
		font-weight: 600;
		padding: 2px 8px;
		border-radius: 4px;
		background: rgba(59, 130, 246, 0.1);
		color: #3b82f6;
		border: 1px solid rgba(59, 130, 246, 0.2);
	}

	.region-badge {
		font-size: 11px; font-weight: 500;
		padding: 2px 8px; border-radius: 999px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		color: var(--text-muted);
	}

	.migrate-btn {
		margin-top: 10px;
		padding: 6px 14px;
		border-radius: var(--radius-md, 6px);
		border: 1px solid rgba(16, 185, 129, 0.3);
		background: rgba(16, 185, 129, 0.08);
		color: #059669;
		font-size: 12px;
		font-weight: 600;
		cursor: pointer;
		transition: all 0.15s;
	}
	.migrate-btn:hover:not(:disabled) {
		background: rgba(16, 185, 129, 0.15);
		border-color: rgba(16, 185, 129, 0.5);
	}
	.migrate-btn:disabled {
		opacity: 0.55;
		cursor: not-allowed;
	}

	.node-card-meta {
		display: flex; align-items: center; gap: 6px;
		font-size: 12px; color: var(--text-muted);
	}
	.node-meta-sep { color: var(--border); }
	.node-ip { font-family: var(--font-mono); }

	/* ── Node status ── */
	.node-status-row {
		display: flex; align-items: center; gap: 8px;
	}

	.node-status-dot {
		width: 8px; height: 8px;
		border-radius: 50%;
		flex-shrink: 0;
	}
	.node-active     { background: #10B981; }
	.node-provisioning { background: #2563EB; }
	.node-failed     { background: #EF4444; }
	.node-degraded   { background: #D97706; }
	.node-stopped    { background: var(--text-dim); }

	.node-dot-pulse { animation: pulse 1.4s ease-in-out infinite; }

	.node-status-label { font-size: 12px; font-weight: 500; }
	.node-active-text      { color: #10B981; }
	.node-provisioning-text { color: #2563EB; }
	.node-failed-text      { color: #EF4444; }
	.node-degraded-text    { color: #D97706; }
	.node-stopped-text     { color: var(--text-dim); }

	/* Progress bar for transient states */
	.node-progress {
		flex: 1;
		height: 3px;
		background: var(--border);
		border-radius: 999px;
		overflow: hidden;
		max-width: 120px;
	}
	.node-progress-bar {
		height: 100%;
		width: 40%;
		background: #2563EB;
		border-radius: 999px;
		animation: slide 1.6s ease-in-out infinite;
	}

	.node-error {
		font-size: 12px;
		color: #EF4444;
		background: rgba(239,68,68,0.08);
		border: 1px solid rgba(239,68,68,0.2);
		border-radius: var(--radius-sm);
		padding: 6px 10px;
	}

	/* ── Provisioning progress ── */
	.provisioning-progress {
		background: rgba(59, 130, 246, 0.06);
		border: 1px solid rgba(59, 130, 246, 0.2);
		border-radius: var(--radius-lg);
		padding: 20px 24px;
		margin-bottom: 4px;
	}

	.provision-header {
		display: flex;
		align-items: center;
		gap: 10px;
		font-weight: 600;
		color: var(--text-primary);
		margin-bottom: 16px;
	}

	.provision-spinner {
		width: 16px;
		height: 16px;
		border: 2px solid rgba(59, 130, 246, 0.2);
		border-top-color: #3b82f6;
		border-radius: 50%;
		animation: spin 1s linear infinite;
		flex-shrink: 0;
	}

	.provision-node {
		margin-bottom: 12px;
	}

	.provision-node-name {
		font-size: 12px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-muted);
		margin-bottom: 8px;
		display: block;
	}

	.provision-step-list {
		display: flex;
		flex-direction: column;
		gap: 6px;
	}

	.provision-step {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: var(--text-dim);
	}

	.provision-step.done {
		color: #16a34a;
	}

	.provision-step.active {
		color: #3b82f6;
		font-weight: 500;
	}

	.step-dot {
		width: 6px;
		height: 6px;
		border-radius: 50%;
		background: currentColor;
		flex-shrink: 0;
	}

	.provision-note {
		font-size: 12px;
		color: var(--text-muted);
		margin-top: 12px;
		margin-bottom: 0;
	}

	/* ── Responsive ── */
	@media (max-width: 860px) {
		.plan-grid { grid-template-columns: 1fr; }
	}
	@media (max-width: 639px) {
		.billing-page { gap: 16px; }
		.section-header { padding: 14px 16px; }
		.node-card { padding: 14px 16px; }
		.plan-detail-row { padding: 10px 16px; }
	}

	/* ── Billing history ── */
	.hist-empty { padding: 24px 20px; color: var(--text-muted); font-size: 13px; text-align: center; }
	.hist-table { border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden; }
	.hist-head { display: flex; gap: 8px; padding: 9px 16px; background: var(--bg-elevated); border-bottom: 1px solid var(--border); font-size: 10.5px; font-weight: 700; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.06em; }
	.hist-row { display: flex; gap: 8px; padding: 11px 16px; border-bottom: 1px solid var(--border); font-size: 12.5px; align-items: center; }
	.hist-row:last-child { border-bottom: none; }
	.hist-date { color: var(--text-secondary); font-size: 12px; }
	.hist-desc { color: var(--text-muted); font-size: 12px; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
	.hist-amount { font-weight: 600; color: var(--text-primary); }
	.pay-badge { display: inline-flex; padding: 2px 8px; border-radius: 999px; font-size: 11px; font-weight: 600; }
	.pay-success { background: var(--accent-green-muted, rgba(34,197,94,.12)); color: var(--accent-green, #22c55e); border: 1px solid rgba(34,197,94,.2); }
	.pay-failed  { background: var(--accent-red-muted); color: var(--accent-red); border: 1px solid rgba(220,38,38,.2); }
	.pay-pending { background: var(--bg-elevated); color: var(--text-muted); border: 1px solid var(--border); }
</style>
