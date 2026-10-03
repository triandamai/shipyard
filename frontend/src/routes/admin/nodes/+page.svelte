<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AdminNode } from '$lib/api/types';
	import { DataTable, StatusDot, PageHeader, Badge, InlineAlert } from '$lib/components/ui';

	let nodes = $state<AdminNode[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);

	onMount(async () => {
		const res = await api.getAdminNodes();
		if (res.data) nodes = res.data;
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	});

	// ── Status labels + StatusDot mapping ──────────────────────────────────────
	// StatusDot only has 5 statuses (running/pending/deploying/failed/stopped),
	// so the 7 node states collapse onto those. Pulsing is reserved for the
	// genuinely transitional bring-up states — exactly the 3 states the old
	// page's own `pulse:true` flagged *other than* `active` (provisioning,
	// cloud_init_running, wireguard_joined). `active` pulsed in the old
	// hand-rolled dot too, but there is no pulsing-green StatusDot variant, and
	// "active" is a settled/healthy state rather than a transitional one, so it
	// maps to the static `running` dot.
	//
	// `provisioning` (queued/not started yet) -> `pending` (yellow, pulsing).
	// `cloud_init_running` / `wireguard_joined` (actively configuring) ->
	// `deploying` (blue, pulsing) — a step further along than merely queued.
	//
	// `degraded` has no dedicated static-amber token (StatusDot's only yellow
	// is `pending`, which always pulses, and pulsing is reserved above for
	// transitional states). Among the remaining static options, `failed` (red)
	// is the closest match: degraded is a non-transitional "something's wrong"
	// state, same category as failed, just less severe — the text label
	// ("Degraded" vs "Failed") still keeps the two distinguishable.
	type StatusKey =
		| 'active'
		| 'degraded'
		| 'failed'
		| 'provisioning'
		| 'cloud_init_running'
		| 'wireguard_joined'
		| 'stopped';

	const STATUS_LABEL: Record<StatusKey, string> = {
		active: 'Active',
		degraded: 'Degraded',
		failed: 'Failed',
		provisioning: 'Provisioning',
		cloud_init_running: 'Init',
		wireguard_joined: 'Joining',
		stopped: 'Stopped'
	};

	const STATUS_DOT: Record<StatusKey, 'running' | 'pending' | 'deploying' | 'failed' | 'stopped'> = {
		active: 'running',
		degraded: 'failed',
		failed: 'failed',
		provisioning: 'pending',
		cloud_init_running: 'deploying',
		wireguard_joined: 'deploying',
		stopped: 'stopped'
	};

	function statusLabel(s: string): string {
		return STATUS_LABEL[s as StatusKey] ?? s;
	}
	function statusDot(s: string): 'running' | 'pending' | 'deploying' | 'failed' | 'stopped' {
		return STATUS_DOT[s as StatusKey] ?? 'stopped';
	}

	// ── Provider color mapping ──────────────────────────────────────────────────
	// The old page hardcoded a 5th hex per provider (hetzner #e53e3e red,
	// digitalocean #1a81c2 blue, aws #f59e0b amber, gcp #34a853 green, vultr
	// #007bfc blue). Only 3 tokens are allowed now (--accent blue #2563EB,
	// --accent-green #059669, --accent-yellow #D97706), so each provider maps to
	// whichever token is closest by hue:
	//  - gcp's green -> --accent-green (direct match).
	//  - digitalocean and vultr were already near-identical blues in the old
	//    palette -> both -> --accent (preserves that existing grouping).
	//  - aws's amber -> --accent-yellow (direct match).
	//  - hetzner's red has no token in the allowed set; of the 3, amber/yellow
	//    is the closest hue to red (far closer than green or blue), so it maps
	//    to --accent-yellow alongside aws.
	const PROVIDER_COLOR: Record<string, string> = {
		hetzner: 'var(--accent-yellow)',
		aws: 'var(--accent-yellow)',
		digitalocean: 'var(--accent)',
		vultr: 'var(--accent)',
		gcp: 'var(--accent-green)'
	};
	function providerColor(p: string): string {
		return PROVIDER_COLOR[p.toLowerCase()] ?? 'var(--text-dim)';
	}

	// ── Compact status-summary bar ──────────────────────────────────────────────
	// No shared component covers this one-off strip, so it stays page-local, but
	// each count's hand-rolled pulsing dot is replaced with StatusDot. The 4
	// summary buckets map onto StatusDot statuses the same way the per-row
	// mapping above does: active -> running, provisioning (+ its two sub-states)
	// -> pending, degraded -> failed, stopped (+ failed) -> stopped... except the
	// "stopped" bucket already absorbs real `failed` nodes (as the old page did),
	// so to keep that bucket visually distinct from the "degraded" bucket (which
	// now also reads as `failed`-colored) it uses the static `stopped` dot.
	let summary = $derived({
		active: nodes.filter((n) => n.status === 'active').length,
		provisioning: nodes.filter((n) =>
			['provisioning', 'cloud_init_running', 'wireguard_joined'].includes(n.status)
		).length,
		degraded: nodes.filter((n) => n.status === 'degraded').length,
		stopped: nodes.filter((n) => ['stopped', 'failed'].includes(n.status)).length
	});
</script>

<PageHeader title="Compute">
	{#snippet actions()}
		<Badge tone="neutral">{nodes.length} total</Badge>
	{/snippet}
</PageHeader>

{#if !loading && nodes.length > 0}
	<div class="nd-summary">
		<div class="nd-sum-item">
			<StatusDot status="running" />
			<span>{summary.active} active</span>
		</div>
		{#if summary.provisioning > 0}
			<div class="nd-sum-item">
				<StatusDot status="pending" />
				<span>{summary.provisioning} provisioning</span>
			</div>
		{/if}
		{#if summary.degraded > 0}
			<div class="nd-sum-item">
				<StatusDot status="failed" />
				<span class="nd-warn">{summary.degraded} degraded</span>
			</div>
		{/if}
		{#if summary.stopped > 0}
			<div class="nd-sum-item nd-muted">
				<StatusDot status="stopped" />
				<span>{summary.stopped} stopped</span>
			</div>
		{/if}
	</div>
{/if}

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={nodes}
		rowKey={(n) => n.id}
		searchFields={['name', 'org_name', 'provider']}
		columns={[
			{ key: 'name', label: 'Node', width: '20%' },
			{ key: 'org', label: 'Organization', width: '16%' },
			{ key: 'provider', label: 'Provider', width: '11%' },
			{ key: 'region', label: 'Region', width: '11%' },
			{ key: 'status', label: 'Status', width: '14%' },
			{ key: 'ip', label: 'Public IP', width: '14%' },
			{ key: 'created', label: 'Created', width: '14%' }
		]}
		emptyMessage="No compute nodes yet."
	>
		{#snippet row(node)}
			<tr>
				<td class="nd-mono nd-trunc">{node.name}</td>
				<td class="nd-trunc">{node.org_name}</td>
				<td><span style="color:{providerColor(node.provider)}; font-weight:700">{node.provider}</span></td>
				<td class="nd-dim">{node.region}</td>
				<td>
					<span class="nd-status">
						<StatusDot status={statusDot(node.status)} />
						{statusLabel(node.status)}
					</span>
				</td>
				<td class="nd-mono nd-dim">{node.public_ip ?? '—'}</td>
				<td class="nd-dim">{new Date(node.created_at).toLocaleDateString()}</td>
			</tr>
		{/snippet}
	</DataTable>
{/if}

<style>
	/* Compact status-summary bar — a one-off strip with no shared-component
	   equivalent (see Task 44 brief); only its dots come from StatusDot. */
	.nd-summary {
		display: flex;
		align-items: center;
		gap: 20px;
		flex-wrap: wrap;
		padding: 9px 14px;
		margin-bottom: 14px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		box-shadow: var(--shadow-sm);
	}
	.nd-sum-item { display: flex; align-items: center; gap: 6px; font-size: 12px; color: var(--text-secondary); }
	.nd-sum-item.nd-muted { color: var(--text-dim); }
	/* Matches the StatusDot `failed` dot used for the degraded bucket (see the
	   statusDot()/STATUS_DOT comment above) so the dot and label read as one
	   consistent color rather than a red dot next to amber text. */
	.nd-warn { color: var(--accent-red); }

	.nd-status { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 500; }
	.nd-mono { font-family: var(--font-mono); }
	.nd-dim { color: var(--text-dim); font-size: 11.5px; white-space: nowrap; }
	.nd-trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; max-width: 0; }
</style>
