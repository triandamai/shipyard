<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { AdminOrg, OrgQuota } from '$lib/api/types';
	import {
		DataTable,
		Avatar,
		Badge,
		StatusDot,
		Button,
		Modal,
		FormField,
		TextField,
		PageHeader,
		InlineAlert
	} from '$lib/components/ui';

	let orgs = $state<AdminOrg[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let patchingId = $state<string | null>(null);

	// ── Quota modal ───────────────────────────────────────────────────────────
	// NOTE: a type="number" TextField numerically coerces its bound value on
	// every edit (Svelte's bind:value checks the live DOM element's own `type`
	// at runtime, regardless of how dynamic the `type` prop looked at the call
	// site) — so quotaForm's fields are strings only until the user touches one,
	// at which point that field becomes a real JS number. quotaNumber() below
	// normalizes either shape before building the save payload for the
	// backend's strict numeric-typed quota endpoint.
	let quotaModalOpen = $state(false);
	let quotaOrg = $state<AdminOrg | null>(null);
	let quotaLoading = $state(false);
	let quotaSaving = $state(false);
	let quotaError = $state<string | null>(null);

	type QuotaForm = {
		max_projects: string;
		max_members: string;
		max_replicas: string;
		max_parallel_deployments: string;
		max_git_providers: string;
		max_orgs: string;
		node_count: string;
		cpu_cores: string;
		memory_mb: string;
	};

	let quotaForm = $state<QuotaForm>({
		max_projects: '3',
		max_members: '5',
		max_replicas: '1',
		max_parallel_deployments: '1',
		max_git_providers: '1',
		max_orgs: '1',
		node_count: '1',
		cpu_cores: '1',
		memory_mb: '2048',
	});

	onMount(() => load());

	async function load() {
		loading = true;
		const res = await api.getAdminOrgs();
		if (res.data) orgs = res.data;
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	}

	async function toggleSuspend(org: AdminOrg) {
		patchingId = org.id;
		const next = org.sub_status === 'suspended' ? 'active' : 'suspended';
		await api.patchAdminOrg(org.id, { sub_status: next });
		await load();
		patchingId = null;
	}

	function openOrg(org: AdminOrg) {
		window.open(`/orgs/${org.slug}`, '_blank', 'noopener');
	}

	async function openQuota(org: AdminOrg) {
		quotaOrg = org;
		quotaModalOpen = true;
		quotaLoading = true;
		quotaError = null;
		const res = await api.getAdminOrgQuota(org.id);
		if (res.data) {
			const q = res.data as OrgQuota;
			quotaForm = {
				max_projects: String(q.max_projects),
				max_members: String(q.max_members),
				max_replicas: String(q.max_replicas),
				max_parallel_deployments: String(q.max_parallel_deployments),
				max_git_providers: String(q.max_git_providers),
				max_orgs: String(q.max_orgs),
				node_count: String(q.node_count),
				cpu_cores: String(q.cpu_cores),
				memory_mb: String(q.memory_mb),
			};
		} else {
			quotaError = res.error?.message ?? 'Failed to load quota';
		}
		quotaLoading = false;
	}

	function closeQuota() {
		quotaModalOpen = false;
		quotaOrg = null;
		quotaError = null;
	}

	// Modal can also close itself (Escape key / backdrop click) without going
	// through closeQuota() — keep the rest of the quota state in sync either way.
	$effect(() => {
		if (!quotaModalOpen && quotaOrg) {
			quotaOrg = null;
			quotaError = null;
		}
	});

	// -1 is a valid sentinel ("unlimited"); blank or non-numeric input falls
	// back to 0 rather than being sent to the backend as a string.
	//
	// `raw` is typed `string` (QuotaForm's declared shape, matching TextField's
	// `value: string` contract) but accepts `string | number` defensively: a
	// type="number" TextField bound via a computed member expression
	// (`quotaForm[field.key]`) inside an {#each} loop can end up holding a real
	// JS number at runtime the instant the user edits it, not just a numeric
	// string — confirmed live (quotaForm.max_projects became the number 6, not
	// "6", immediately after editing that one field). Calling `.trim()` on an
	// actual number throws, which previously crashed saveQuota() silently and
	// left the Save button stuck on "Saving…" forever with no error shown.
	function quotaNumber(raw: string | number): number {
		const trimmed = String(raw).trim();
		if (trimmed === '') return 0;
		const n = Number(trimmed);
		return Number.isFinite(n) ? n : 0;
	}

	async function saveQuota() {
		if (!quotaOrg) return;
		quotaSaving = true;
		quotaError = null;
		try {
			const payload = {
				max_projects: quotaNumber(quotaForm.max_projects),
				max_members: quotaNumber(quotaForm.max_members),
				max_replicas: quotaNumber(quotaForm.max_replicas),
				max_parallel_deployments: quotaNumber(quotaForm.max_parallel_deployments),
				max_git_providers: quotaNumber(quotaForm.max_git_providers),
				max_orgs: quotaNumber(quotaForm.max_orgs),
				node_count: quotaNumber(quotaForm.node_count),
				cpu_cores: quotaNumber(quotaForm.cpu_cores),
				memory_mb: quotaNumber(quotaForm.memory_mb),
			};
			const res = await api.putAdminOrgQuota(quotaOrg.id, payload);
			if (res.error) {
				quotaError = res.error.message;
			} else {
				closeQuota();
			}
		} catch (e) {
			// Belt-and-suspenders: an unexpected error here must never leave the
			// modal permanently stuck mid-save with no feedback.
			quotaError = e instanceof Error ? e.message : 'Failed to save quota.';
		} finally {
			quotaSaving = false;
		}
	}

	function tierTone(t: string | null): 'green' | 'blue' | 'yellow' | 'neutral' {
		if (!t || t === 'free') return 'neutral';
		if (t === 'pro') return 'blue';
		return 'yellow';
	}
	function tierLabel(t: string | null): string {
		if (!t || t === 'free') return 'Free';
		if (t === 'pro') return 'Pro';
		return 'Max';
	}

	function statusDotStatus(s: string | null): 'running' | 'pending' | 'failed' | 'stopped' {
		if (s === 'active') return 'running';
		if (s === 'suspended') return 'failed';
		if (s === 'past_due') return 'pending';
		return 'stopped';
	}
	function statusLabel(s: string | null): string {
		if (s === 'active') return 'Active';
		if (s === 'suspended') return 'Suspended';
		if (s === 'past_due') return 'Past due';
		return s ?? 'Free';
	}

	// Same deterministic index pattern as the Users page (Task 40): email/name
	// charCodeAt(0) % length picks one of Avatar's fixed `tone` values.
	const avaTones: Array<'blue' | 'green' | 'red' | 'yellow' | 'purple'> = ['blue', 'purple', 'green', 'yellow', 'red'];
	function avaTone(name: string): 'blue' | 'green' | 'red' | 'yellow' | 'purple' {
		return avaTones[name.charCodeAt(0) % avaTones.length];
	}

	const quotaFields: { key: keyof QuotaForm; label: string; hint: string }[] = [
		{ key: 'max_projects',             label: 'Max Projects',             hint: '-1 = unlimited' },
		{ key: 'max_members',              label: 'Max Members',              hint: '-1 = unlimited' },
		{ key: 'max_replicas',             label: 'Max Replicas / Service',   hint: '-1 = unlimited' },
		{ key: 'max_parallel_deployments', label: 'Max Parallel Deployments', hint: '-1 = unlimited' },
		{ key: 'max_git_providers',        label: 'Max Git Providers',        hint: '-1 = unlimited' },
		{ key: 'max_orgs',                 label: 'Max Orgs',                 hint: '-1 = unlimited' },
		{ key: 'node_count',               label: 'Node Count',               hint: 'compute nodes allowed' },
		{ key: 'cpu_cores',                label: 'CPU Cores',                hint: 'total cores' },
		{ key: 'memory_mb',                label: 'Memory (MB)',              hint: 'total RAM in MB, e.g. 2048 = 2 GB' },
	];
</script>

<PageHeader title="Organizations">
	{#snippet actions()}
		<Badge tone="neutral">{orgs.length} total</Badge>
	{/snippet}
</PageHeader>

{#if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else}
	<DataTable
		items={orgs}
		rowKey={(o) => o.id}
		searchFields={['name', 'slug']}
		columns={[
			{ key: 'org', label: 'Organization', width: '26%' },
			{ key: 'tier', label: 'Tier', width: '10%' },
			{ key: 'status', label: 'Status', width: '14%' },
			{ key: 'members', label: 'Members', width: '9%' },
			{ key: 'nodes', label: 'Nodes', width: '9%' },
			{ key: 'created', label: 'Created', width: '12%' },
			{ key: 'actions', label: 'Actions', width: '20%' }
		]}
		emptyMessage="No organizations yet."
	>
		{#snippet row(org)}
			<tr>
				<td>
					<div class="og-org-cell">
						<Avatar initials={org.name[0]} tone={avaTone(org.name)} size={32} />
						<div class="og-org-info">
							<span class="og-name">{org.name}</span>
							<span class="og-slug">{org.slug}</span>
						</div>
					</div>
				</td>
				<td><Badge tone={tierTone(org.tier)}>{tierLabel(org.tier)}</Badge></td>
				<td>
					<span class="og-status">
						<StatusDot status={statusDotStatus(org.sub_status)} />
						{statusLabel(org.sub_status)}
					</span>
				</td>
				<td class="og-num">{org.member_count}</td>
				<td class="og-num">{org.node_count}</td>
				<td class="og-date">{new Date(org.created_at).toLocaleDateString()}</td>
				<td>
					<div class="og-actions">
						<Button variant="secondary" size="sm" onclick={() => openOrg(org)}>Open</Button>
						<Button variant="secondary" size="sm" onclick={() => openQuota(org)}>Quota</Button>
						<Button
							variant={org.sub_status === 'suspended' ? 'secondary' : 'danger-outline'}
							size="sm"
							disabled={patchingId === org.id}
							onclick={() => toggleSuspend(org)}
						>
							{patchingId === org.id ? '…' : org.sub_status === 'suspended' ? 'Restore' : 'Suspend'}
						</Button>
					</div>
				</td>
			</tr>
		{/snippet}
	</DataTable>
{/if}

<!-- ── Quota Modal ─────────────────────────────────────────────────────────── -->
<Modal
	bind:open={quotaModalOpen}
	title={quotaOrg ? `Edit Quota — ${quotaOrg.name} (${quotaOrg.slug})` : 'Edit Quota'}
>
	{#snippet children()}
		{#if quotaLoading}
			<div class="qd-loading">Loading quota…</div>
		{:else}
			{#if quotaError}
				<InlineAlert tone="error">{quotaError}</InlineAlert>
			{/if}
			<p class="qd-hint">Set -1 for unlimited. Changes take effect immediately for all new actions.</p>
			<div class="qd-grid">
				{#each quotaFields as field}
					<FormField label={field.label} hint={field.hint} for="qf-{field.key}">
						<TextField id="qf-{field.key}" type="number" bind:value={quotaForm[field.key]} />
					</FormField>
				{/each}
			</div>
		{/if}
	{/snippet}
	{#snippet footer()}
		<Button variant="secondary" size="sm" onclick={closeQuota} disabled={quotaSaving}>Cancel</Button>
		<Button size="sm" onclick={saveQuota} disabled={quotaSaving || quotaLoading}>
			{quotaSaving ? 'Saving…' : 'Save Quota'}
		</Button>
	{/snippet}
</Modal>

<style>
	.og-org-cell { display: flex; align-items: center; gap: 9px; min-width: 0; }
	.og-org-info { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
	.og-name { font-size: 12.5px; font-weight: 600; color: var(--text-primary); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
	.og-slug { font-size: 10.5px; color: var(--text-dim); font-family: var(--font-mono); }
	.og-num { font-variant-numeric: tabular-nums; color: var(--text-secondary); }
	.og-date { font-size: 11.5px; color: var(--text-dim); white-space: nowrap; }
	.og-status { display: inline-flex; align-items: center; gap: 6px; font-size: 12px; font-weight: 500; }
	.og-actions { display: flex; justify-content: flex-end; gap: 6px; }

	/* ── Quota modal body ───────────────────────────────────────────────── */
	.qd-hint { font-size: 11.5px; color: var(--text-dim); margin: 0 0 2px; }
	.qd-loading { font-size: 13px; color: var(--text-dim); padding: 20px 0; text-align: center; }

	/* 2-column grid, same pattern as the SMTP page's `.smtp-row2` (Task 32),
	   sized for this modal's 9 numeric fields. */
	.qd-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }

	@media (max-width: 420px) {
		.qd-grid { grid-template-columns: 1fr; }
	}
</style>
