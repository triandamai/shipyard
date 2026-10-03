<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import type { Plan } from '$lib/api/types';
	import {
		PageHeader,
		Card,
		Toggle,
		Button,
		Modal,
		FormField,
		TextField,
		InlineAlert,
		EmptyState,
		Skeleton
	} from '$lib/components/ui';

	function fmtMem(mb: number): string {
		if (mb < 1024) return `${mb} MB`;
		const gb = mb / 1024;
		return Number.isInteger(gb) ? `${gb} GB` : `${gb.toFixed(1)} GB`;
	}

	// -1 is the "unlimited" sentinel on these quota-style fields (same
	// convention as orgs' quota modal) — render it as ∞ rather than the
	// literal number. Matches this page's pre-migration formatting exactly.
	function fmtLimit(n: number): string {
		return n === -1 ? '∞' : String(n);
	}

	let plans = $state<Plan[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let saving = $state<string | null>(null);

	// ── Shared 9-field numeric quota grid ───────────────────────────────────
	// Same fields/shape as orgs' quota modal (Task 41): cpu_cores, memory_mb,
	// max_replicas, node_count, max_members, max_projects, max_orgs,
	// max_parallel_deployments, max_git_providers. Reused as-is by both the
	// Create and Edit modals below.
	//
	// NOTE: a type="number" TextField numerically coerces its bound value on
	// every edit (Svelte checks the live DOM input's own `type` at runtime,
	// regardless of how dynamic the `type` prop looked at the call site) — so
	// these form fields are strings only until the user touches one, at which
	// point that field becomes a real JS number. planNumber() below normalizes
	// either shape before building the save payload for the backend's
	// strict numeric-typed plan endpoint. A previous task (orgs' quota-edit
	// modal, which this modal explicitly reuses the pattern from) originally
	// wrote this helper as `(raw: string) => raw.trim()...` and crashed with
	// "raw.trim is not a function" the instant a numeric field was edited,
	// leaving Save stuck on "Saving…" forever with no visible error.
	type QuotaKey =
		| 'cpu_cores'
		| 'memory_mb'
		| 'max_replicas'
		| 'node_count'
		| 'max_members'
		| 'max_projects'
		| 'max_orgs'
		| 'max_parallel_deployments'
		| 'max_git_providers';

	type QuotaFormFields = Record<QuotaKey, string>;

	const quotaFieldDefs: { key: QuotaKey; label: string; hint?: string }[] = [
		{ key: 'cpu_cores', label: 'CPU Cores' },
		{ key: 'memory_mb', label: 'Memory (MB)' },
		{ key: 'max_replicas', label: 'Max Replicas', hint: '-1 = unlimited' },
		{ key: 'node_count', label: 'Node Count' },
		{ key: 'max_members', label: 'Max Members', hint: '-1 = unlimited' },
		{ key: 'max_projects', label: 'Max Projects', hint: '-1 = unlimited' },
		{ key: 'max_orgs', label: 'Max Orgs', hint: '-1 = unlimited' },
		{ key: 'max_parallel_deployments', label: 'Parallel Deployments', hint: '-1 = unlimited' },
		{ key: 'max_git_providers', label: 'Max Git Providers', hint: '-1 = unlimited' }
	];

	function emptyQuotaForm(): QuotaFormFields {
		return {
			cpu_cores: '1',
			memory_mb: '1024',
			max_replicas: '2',
			node_count: '1',
			max_members: '5',
			max_projects: '5',
			max_orgs: '1',
			max_parallel_deployments: '2',
			max_git_providers: '1'
		};
	}

	// `raw` is typed `string | number` defensively: a type="number" TextField
	// bound via a computed member expression (`form[field.key]`) inside an
	// {#each} loop can end up holding a real JS number at runtime the instant
	// the user edits that field, not just a numeric string. -1 is a valid
	// sentinel ("unlimited"); blank or non-numeric input falls back to 0
	// rather than being sent to the backend as a string.
	function planNumber(raw: string | number): number {
		const trimmed = String(raw).trim();
		if (trimmed === '') return 0;
		const n = Number(trimmed);
		return Number.isFinite(n) ? n : 0;
	}

	onMount(() => load());

	async function load() {
		loading = true;
		const res = await api.get<Plan[]>('/admin/plans');
		if (res.data) plans = res.data;
		else error = res.error?.message ?? 'Failed to load';
		loading = false;
	}

	async function toggleEnabled(plan: Plan) {
		saving = plan.id;
		try {
			await api.patch(`/admin/plans/${plan.id}`, { enabled: !plan.enabled });
			await load();
		} finally {
			saving = null;
		}
	}

	// ── Create modal ─────────────────────────────────────────────────────────
	// 11 fields total: name, price/mo, + the 9 shared quota fields above —
	// plus an Enabled toggle. The Edit modal (below) exposes the same
	// price_monthly field too — the pre-migration page let admins edit price
	// after creation, so this migration preserves that.
	let showCreate = $state(false);
	let creating = $state(false);
	let createError = $state<string | null>(null);

	type CreateForm = QuotaFormFields & { name: string; enabled: boolean; price_monthly: string };

	function defaultCreateForm(): CreateForm {
		return { ...emptyQuotaForm(), name: '', enabled: true, price_monthly: '0' };
	}

	let createForm = $state<CreateForm>(defaultCreateForm());

	function openCreate() {
		createForm = defaultCreateForm();
		createError = null;
		showCreate = true;
	}

	function closeCreate() {
		showCreate = false;
		createError = null;
	}

	async function createPlan() {
		creating = true;
		createError = null;
		try {
			const payload = {
				name: createForm.name,
				enabled: createForm.enabled,
				price_monthly: planNumber(createForm.price_monthly),
				cpu_cores: planNumber(createForm.cpu_cores),
				memory_mb: planNumber(createForm.memory_mb),
				max_replicas: planNumber(createForm.max_replicas),
				node_count: planNumber(createForm.node_count),
				max_members: planNumber(createForm.max_members),
				max_projects: planNumber(createForm.max_projects),
				max_orgs: planNumber(createForm.max_orgs),
				max_parallel_deployments: planNumber(createForm.max_parallel_deployments),
				max_git_providers: planNumber(createForm.max_git_providers)
			};
			const res = await api.post('/admin/plans', payload);
			if (res.error) {
				createError = res.error.message;
			} else {
				closeCreate();
				await load();
			}
		} catch (e) {
			// Belt-and-suspenders: an unexpected error here must never leave the
			// modal permanently stuck mid-save with no feedback.
			createError = e instanceof Error ? e.message : 'Failed to create plan.';
		} finally {
			creating = false;
		}
	}

	// ── Edit modal ───────────────────────────────────────────────────────────
	// The same 9 shared quota fields as orgs' quota modal, plus name, enabled,
	// and price_monthly (11 fields total). The pre-migration page's planFields
	// array had price_monthly as its first entry and was shared verbatim by
	// both the Create and Edit modal templates — i.e. price was always
	// editable after creation too. (Confirmed against pre-migration source;
	// an earlier draft of this migration read too much into the task brief's
	// paraphrased "9 numeric fields... plus name+enabled" description for
	// Edit and dropped price here, which was a behavior regression — fixed.)
	let editModalOpen = $state(false);
	let editPlan = $state<Plan | null>(null);
	let editSaving = $state(false);
	let editError = $state<string | null>(null);

	type EditForm = QuotaFormFields & { name: string; enabled: boolean; price_monthly: string };

	let editForm = $state<EditForm>({ ...emptyQuotaForm(), name: '', enabled: true, price_monthly: '0' });

	function openEdit(plan: Plan) {
		editPlan = plan;
		editForm = {
			name: plan.name,
			enabled: plan.enabled,
			price_monthly: String(plan.price_monthly),
			cpu_cores: String(plan.cpu_cores),
			memory_mb: String(plan.memory_mb),
			max_replicas: String(plan.max_replicas),
			node_count: String(plan.node_count),
			max_members: String(plan.max_members),
			max_projects: String(plan.max_projects),
			max_orgs: String(plan.max_orgs),
			max_parallel_deployments: String(plan.max_parallel_deployments),
			max_git_providers: String(plan.max_git_providers)
		};
		editError = null;
		editModalOpen = true;
	}

	function closeEdit() {
		editModalOpen = false;
		editPlan = null;
		editError = null;
	}

	// Modal can also close itself (Escape key / backdrop click) without going
	// through closeEdit() — keep the rest of the edit state in sync either way.
	$effect(() => {
		if (!editModalOpen && editPlan) {
			editPlan = null;
			editError = null;
		}
	});

	async function saveEdit() {
		if (!editPlan) return;
		editSaving = true;
		editError = null;
		try {
			const payload = {
				name: editForm.name,
				enabled: editForm.enabled,
				price_monthly: planNumber(editForm.price_monthly),
				cpu_cores: planNumber(editForm.cpu_cores),
				memory_mb: planNumber(editForm.memory_mb),
				max_replicas: planNumber(editForm.max_replicas),
				node_count: planNumber(editForm.node_count),
				max_members: planNumber(editForm.max_members),
				max_projects: planNumber(editForm.max_projects),
				max_orgs: planNumber(editForm.max_orgs),
				max_parallel_deployments: planNumber(editForm.max_parallel_deployments),
				max_git_providers: planNumber(editForm.max_git_providers)
			};
			const res = await api.patch<unknown>(`/admin/plans/${editPlan.id}`, payload);
			if (res.error) {
				editError = res.error.message;
			} else {
				closeEdit();
				await load();
			}
		} catch (e) {
			editError = e instanceof Error ? e.message : 'Failed to save plan.';
		} finally {
			editSaving = false;
		}
	}
</script>

<PageHeader title="Subscription Plans" subtitle="Manage available plans for organizations.">
	{#snippet actions()}
		<Button size="sm" onclick={openCreate}>+ New Plan</Button>
	{/snippet}
</PageHeader>

{#if loading}
	<div class="pl-grid">
		{#each [0, 1, 2] as _}
			<Card padding="18px">
				<Skeleton variant="text" width="90px" />
				<div class="pl-sk-gap"></div>
				<Skeleton variant="row" height="100px" />
			</Card>
		{/each}
	</div>
{:else if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else if plans.length === 0}
	<EmptyState message="No plans yet." sub="Create one to get started." />
{:else}
	<div class="pl-grid">
		{#each plans as plan (plan.id)}
			<Card padding="18px">
				<div class="pl-card" class:pl-card--disabled={!plan.enabled}>
					<div class="pl-hdr">
						<div>
							<span class="pl-name">{plan.name}</span>
							<span class="pl-price">{plan.price_monthly > 0 ? `$${plan.price_monthly}/mo` : 'Free'}</span>
						</div>
						<div class="pl-hdr-r">
							<div
								class="pl-toggle-wrap"
								role="button"
								tabindex="0"
								aria-label="Toggle plan enabled"
								onclick={() => toggleEnabled(plan)}
								onkeydown={(e) => {
									if (e.key === 'Enter' || e.key === ' ') {
										e.preventDefault();
										toggleEnabled(plan);
									}
								}}
							>
								<Toggle checked={plan.enabled} disabled={saving === plan.id} label="Enabled" />
							</div>
							<Button variant="secondary" size="sm" onclick={() => openEdit(plan)}>Edit</Button>
						</div>
					</div>
					{#if !plan.enabled}
						<span class="pl-disabled-badge">Disabled</span>
					{/if}
					<div class="pl-stat-grid">
						<div class="pl-stat"><span class="pl-stat-l">CPU Cores</span><span class="pl-stat-v">{fmtLimit(plan.cpu_cores)}</span></div>
						<div class="pl-stat"><span class="pl-stat-l">Memory</span><span class="pl-stat-v">{fmtMem(plan.memory_mb)}</span></div>
						<div class="pl-stat"><span class="pl-stat-l">Max Replicas</span><span class="pl-stat-v">{fmtLimit(plan.max_replicas)}</span></div>
						<div class="pl-stat"><span class="pl-stat-l">Nodes</span><span class="pl-stat-v">{fmtLimit(plan.node_count)}</span></div>
						<div class="pl-stat"><span class="pl-stat-l">Members</span><span class="pl-stat-v">{fmtLimit(plan.max_members)}</span></div>
						<div class="pl-stat"><span class="pl-stat-l">Projects</span><span class="pl-stat-v">{fmtLimit(plan.max_projects)}</span></div>
						<div class="pl-stat"><span class="pl-stat-l">Orgs</span><span class="pl-stat-v">{fmtLimit(plan.max_orgs)}</span></div>
						<div class="pl-stat"><span class="pl-stat-l">Parallel Deploys</span><span class="pl-stat-v">{fmtLimit(plan.max_parallel_deployments)}</span></div>
						<div class="pl-stat"><span class="pl-stat-l">Git Providers</span><span class="pl-stat-v">{fmtLimit(plan.max_git_providers)}</span></div>
					</div>
				</div>
			</Card>
		{/each}
	</div>
{/if}

<!-- ── Create Plan Modal ───────────────────────────────────────────────────── -->
<Modal bind:open={showCreate} title="Create Plan">
	{#snippet children()}
		{#if createError}
			<InlineAlert tone="error">{createError}</InlineAlert>
		{/if}
		<div class="pl-form-grid">
			<div class="pl-field-wide">
				<FormField label="Plan Name" for="cf-name">
					<TextField id="cf-name" placeholder="e.g. Pro" bind:value={createForm.name} />
				</FormField>
			</div>
			<FormField label="Price / Month ($)" hint="0 = free" for="cf-price">
				<TextField id="cf-price" type="number" bind:value={createForm.price_monthly} />
			</FormField>
			{#each quotaFieldDefs as field}
				<FormField label={field.label} hint={field.hint} for="cf-{field.key}">
					<TextField id="cf-{field.key}" type="number" bind:value={createForm[field.key]} />
				</FormField>
			{/each}
			<div class="pl-toggle-field">
				<Toggle bind:checked={createForm.enabled} label="Enabled" />
				<span class="pl-toggle-field-lbl">Enabled</span>
			</div>
		</div>
	{/snippet}
	{#snippet footer()}
		<Button variant="secondary" size="sm" onclick={closeCreate} disabled={creating}>Cancel</Button>
		<Button size="sm" onclick={createPlan} disabled={creating || !createForm.name}>
			{creating ? 'Creating…' : 'Create Plan'}
		</Button>
	{/snippet}
</Modal>

<!-- ── Edit Plan Modal ─────────────────────────────────────────────────────── -->
<Modal bind:open={editModalOpen} title={editPlan ? `Edit Plan — ${editPlan.name}` : 'Edit Plan'}>
	{#snippet children()}
		{#if editError}
			<InlineAlert tone="error">{editError}</InlineAlert>
		{/if}
		<div class="pl-form-grid">
			<div class="pl-field-wide">
				<FormField label="Plan Name" for="ef-name">
					<TextField id="ef-name" placeholder="e.g. Pro" bind:value={editForm.name} />
				</FormField>
			</div>
			<FormField label="Price / Month ($)" hint="0 = free" for="ef-price">
				<TextField id="ef-price" type="number" bind:value={editForm.price_monthly} />
			</FormField>
			{#each quotaFieldDefs as field}
				<FormField label={field.label} hint={field.hint} for="ef-{field.key}">
					<TextField id="ef-{field.key}" type="number" bind:value={editForm[field.key]} />
				</FormField>
			{/each}
			<div class="pl-toggle-field">
				<Toggle bind:checked={editForm.enabled} label="Enabled" />
				<span class="pl-toggle-field-lbl">Enabled</span>
			</div>
		</div>
	{/snippet}
	{#snippet footer()}
		<Button variant="secondary" size="sm" onclick={closeEdit} disabled={editSaving}>Cancel</Button>
		<Button size="sm" onclick={saveEdit} disabled={editSaving || !editForm.name}>
			{editSaving ? 'Saving…' : 'Save Changes'}
		</Button>
	{/snippet}
</Modal>

<style>
	.pl-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 14px; }
	.pl-sk-gap { height: 10px; }

	.pl-card.pl-card--disabled { opacity: 0.65; }
	.pl-hdr { display: flex; justify-content: space-between; align-items: flex-start; margin-bottom: 12px; gap: 8px; }
	.pl-hdr-r { display: flex; align-items: center; gap: 10px; flex-shrink: 0; }
	.pl-name { font-size: 15px; font-weight: 700; color: var(--text-primary); display: block; letter-spacing: -0.01em; }
	.pl-price { font-size: 12px; font-weight: 600; color: var(--text-muted); display: block; margin-top: 2px; }
	.pl-toggle-wrap { display: inline-flex; cursor: pointer; }

	.pl-disabled-badge {
		display: inline-flex;
		padding: 2px 9px;
		border-radius: 999px;
		font-size: 10.5px;
		font-weight: 700;
		background: var(--accent-red-muted);
		color: var(--accent-red);
		margin-bottom: 10px;
	}

	.pl-stat-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 6px; }
	.pl-stat { display: flex; justify-content: space-between; padding: 5px 8px; background: var(--bg-hover); border-radius: var(--radius-sm); font-size: 11.5px; }
	.pl-stat-l { color: var(--text-muted); }
	.pl-stat-v { font-weight: 700; color: var(--text-primary); font-variant-numeric: tabular-nums; }

	/* 2-column grid, same pattern as orgs' `.qd-grid` (Task 41) / smtp's
	   `.smtp-row2` (Task 32), sized for this modal's numeric fields. */
	.pl-form-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }
	.pl-field-wide { grid-column: 1 / -1; }
	.pl-toggle-field { grid-column: 1 / -1; display: flex; align-items: center; gap: 10px; }
	.pl-toggle-field-lbl { font-size: 11.5px; font-weight: 600; color: var(--text-secondary); }

	@media (max-width: 420px) {
		.pl-form-grid { grid-template-columns: 1fr; }
	}
</style>
