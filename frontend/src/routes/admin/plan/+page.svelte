<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface Plan {
		id: string;
		name: string;
		enabled: boolean;
		cpu_cores: number;
		memory_gb: number;
		max_replicas: number;
		node_count: number;
		max_members: number;
		max_projects: number;
		max_orgs: number;
		max_parallel_deployments: number;
		max_git_providers: number;
		price_monthly: number;
	}

	let plans = $state<Plan[]>([]);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let saving = $state<string | null>(null);
	let showCreate = $state(false);
	let creating = $state(false);

	// ── Edit modal ────────────────────────────────────────────────────────────
	let editPlan = $state<Plan | null>(null);
	let editForm = $state<Omit<Plan, 'id'> | null>(null);
	let editSaving = $state(false);
	let editError = $state<string | null>(null);

	function openEdit(plan: Plan) {
		editPlan = plan;
		editForm = { ...plan };
		editError = null;
	}

	function closeEdit() {
		editPlan = null;
		editForm = null;
		editError = null;
	}

	async function saveEdit() {
		if (!editPlan || !editForm) return;
		editSaving = true;
		editError = null;
		const res = await api.patch<unknown>(`/admin/plans/${editPlan.id}`, editForm);
		if (res.error) {
			editError = res.error.message;
		} else {
			closeEdit();
			await load();
		}
		editSaving = false;
	}

	const defaultForm = (): Partial<Plan> => ({
		name: '',
		enabled: true,
		cpu_cores: 1,
		memory_gb: 1,
		max_replicas: 2,
		node_count: 1,
		max_members: 5,
		max_projects: 5,
		max_orgs: 1,
		max_parallel_deployments: 2,
		max_git_providers: 1,
		price_monthly: 0,
	});

	let form = $state<Partial<Plan>>(defaultForm());

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
		await api.patch(`/admin/plans/${plan.id}`, { enabled: !plan.enabled });
		await load();
		saving = null;
	}

	async function createPlan() {
		creating = true;
		const res = await api.post('/admin/plans', form);
		if (!res.error) {
			showCreate = false;
			form = defaultForm();
			await load();
		}
		creating = false;
	}

	type FieldDef = { key: keyof Omit<Plan, 'id' | 'name' | 'enabled'>; label: string; hint?: string };
	const planFields: FieldDef[] = [
		{ key: 'price_monthly',            label: 'Price / Month ($)',        hint: '0 = free' },
		{ key: 'cpu_cores',                label: 'CPU Cores' },
		{ key: 'memory_gb',                label: 'Memory (GB)' },
		{ key: 'max_replicas',             label: 'Max Replicas',             hint: '-1 = unlimited' },
		{ key: 'node_count',               label: 'Node Count' },
		{ key: 'max_members',              label: 'Max Members',              hint: '-1 = unlimited' },
		{ key: 'max_projects',             label: 'Max Projects',             hint: '-1 = unlimited' },
		{ key: 'max_orgs',                 label: 'Max Orgs',                 hint: '-1 = unlimited' },
		{ key: 'max_parallel_deployments', label: 'Parallel Deployments',     hint: '-1 = unlimited' },
		{ key: 'max_git_providers',        label: 'Max Git Providers',        hint: '-1 = unlimited' },
	];
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Subscription Plans</h1>
			<p class="sub">Manage available plans for organizations.</p>
		</div>
		<button class="add-btn" onclick={() => (showCreate = true)}>+ New Plan</button>
	</header>

	{#if loading}
		<div class="cards-grid">
			{#each [0,1,2] as _}
				<div class="plan-card sk-card">
					<div class="sk" style="width:80px;height:16px;margin-bottom:10px"></div>
					<div class="sk" style="width:100%;height:12px;margin-bottom:6px"></div>
					<div class="sk" style="width:60%;height:12px"></div>
				</div>
			{/each}
		</div>
	{:else if error}
		<div class="err">{error}</div>
	{:else if plans.length === 0}
		<div class="empty">No plans yet. Create one above.</div>
	{:else}
		<div class="cards-grid">
			{#each plans as plan}
				<div class="plan-card" class:plan-disabled={!plan.enabled}>
					<div class="plan-hdr">
						<div>
							<span class="plan-name">{plan.name}</span>
							{#if plan.price_monthly > 0}
								<span class="plan-price">${plan.price_monthly}/mo</span>
							{:else}
								<span class="plan-price">Free</span>
							{/if}
						</div>
						<div class="plan-hdr-r">
							<label class="toggle">
								<input type="checkbox" checked={plan.enabled} onchange={() => toggleEnabled(plan)} disabled={saving === plan.id} />
								<span class="toggle-track"></span>
							</label>
							<button class="edit-btn" onclick={() => openEdit(plan)} title="Edit plan">
								<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13">
									<path d="M13.586 3.586a2 2 0 112.828 2.828l-.793.793-2.828-2.828.793-.793zM11.379 5.793L3 14.172V17h2.828l8.38-8.379-2.83-2.828z"/>
								</svg>
								Edit
							</button>
						</div>
					</div>
					{#if !plan.enabled}
						<div class="disabled-badge">Disabled</div>
					{/if}
					<div class="plan-grid">
						<div class="plan-stat"><span class="stat-l">CPU Cores</span><span class="stat-v">{plan.cpu_cores}</span></div>
						<div class="plan-stat"><span class="stat-l">Memory</span><span class="stat-v">{plan.memory_gb} GB</span></div>
						<div class="plan-stat"><span class="stat-l">Max Replicas</span><span class="stat-v">{plan.max_replicas === -1 ? '∞' : plan.max_replicas}</span></div>
						<div class="plan-stat"><span class="stat-l">Nodes</span><span class="stat-v">{plan.node_count}</span></div>
						<div class="plan-stat"><span class="stat-l">Members</span><span class="stat-v">{plan.max_members === -1 ? '∞' : plan.max_members}</span></div>
						<div class="plan-stat"><span class="stat-l">Projects</span><span class="stat-v">{plan.max_projects === -1 ? '∞' : plan.max_projects}</span></div>
						<div class="plan-stat"><span class="stat-l">Orgs</span><span class="stat-v">{plan.max_orgs === -1 ? '∞' : plan.max_orgs}</span></div>
						<div class="plan-stat"><span class="stat-l">Parallel Deploys</span><span class="stat-v">{plan.max_parallel_deployments === -1 ? '∞' : plan.max_parallel_deployments}</span></div>
						<div class="plan-stat"><span class="stat-l">Git Providers</span><span class="stat-v">{plan.max_git_providers === -1 ? '∞' : plan.max_git_providers}</span></div>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>

<!-- ── Edit Plan Modal ─────────────────────────────────────────────────────── -->
{#if editPlan && editForm}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div class="modal-bg" onclick={(e) => { if (e.target === e.currentTarget) closeEdit(); }}>
		<div class="modal">
			<div class="modal-hdr">
				<div>
					<h2 class="modal-title">Edit Plan</h2>
					<span class="modal-sub">{editPlan.name}</span>
				</div>
				<button class="modal-close" onclick={closeEdit}>
					<svg viewBox="0 0 20 20" fill="currentColor" width="15" height="15">
						<path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"/>
					</svg>
				</button>
			</div>
			<div class="modal-body">
				{#if editError}
					<div class="edit-err">{editError}</div>
				{/if}
				<div class="form-grid">
					<div class="field" style="grid-column:1/-1">
						<label class="lbl">Plan Name</label>
						<input class="inp" placeholder="e.g. Pro" bind:value={editForm.name} />
					</div>
					{#each planFields as field}
						<div class="field">
							<label class="lbl" for="ef-{field.key}">
								{field.label}
								{#if field.hint}<span class="field-hint">{field.hint}</span>{/if}
							</label>
							<input
								id="ef-{field.key}"
								class="inp"
								type="number"
								min="-1"
								bind:value={editForm[field.key]}
							/>
						</div>
					{/each}
					<div class="field toggle-field">
						<label class="toggle">
							<input type="checkbox" bind:checked={editForm.enabled} />
							<span class="toggle-track"></span>
						</label>
						<span class="lbl" style="margin:0">Enabled</span>
					</div>
				</div>
			</div>
			<div class="modal-foot">
				<button class="btn-cancel" onclick={closeEdit} disabled={editSaving}>Cancel</button>
				<button class="btn-confirm" onclick={saveEdit} disabled={editSaving || !editForm.name}>
					{editSaving ? 'Saving…' : 'Save Changes'}
				</button>
			</div>
		</div>
	</div>
{/if}

<!-- ── Create Plan Modal ───────────────────────────────────────────────────── -->
{#if showCreate}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-bg" onclick={() => (showCreate = false)}>
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="modal" onclick={(e) => e.stopPropagation()}>
			<div class="modal-hdr">
				<h2 class="modal-title">Create Plan</h2>
				<button class="modal-close" onclick={() => (showCreate = false)}>✕</button>
			</div>
			<div class="modal-body">
				<div class="form-grid">
					<div class="field">
						<label class="lbl">Name</label>
						<input class="inp" placeholder="e.g. Pro" bind:value={form.name} />
					</div>
					<div class="field">
						<label class="lbl">Price/mo ($)</label>
						<input class="inp" type="number" min="0" bind:value={form.price_monthly} />
					</div>
					<div class="field">
						<label class="lbl">CPU Cores</label>
						<input class="inp" type="number" min="1" bind:value={form.cpu_cores} />
					</div>
					<div class="field">
						<label class="lbl">Memory (GB)</label>
						<input class="inp" type="number" min="1" bind:value={form.memory_gb} />
					</div>
					<div class="field">
						<label class="lbl">Max Replicas</label>
						<input class="inp" type="number" min="1" bind:value={form.max_replicas} />
					</div>
					<div class="field">
						<label class="lbl">Node Count</label>
						<input class="inp" type="number" min="1" bind:value={form.node_count} />
					</div>
					<div class="field">
						<label class="lbl">Max Members</label>
						<input class="inp" type="number" min="1" bind:value={form.max_members} />
					</div>
					<div class="field">
						<label class="lbl">Max Projects</label>
						<input class="inp" type="number" min="1" bind:value={form.max_projects} />
					</div>
					<div class="field">
						<label class="lbl">Max Orgs</label>
						<input class="inp" type="number" min="1" bind:value={form.max_orgs} />
					</div>
					<div class="field">
						<label class="lbl">Parallel Deploys (-1=fixed 1)</label>
						<input class="inp" type="number" min="-1" bind:value={form.max_parallel_deployments} />
					</div>
					<div class="field">
						<label class="lbl">Git Providers</label>
						<input class="inp" type="number" min="1" bind:value={form.max_git_providers} />
					</div>
					<div class="field toggle-field">
						<label class="toggle">
							<input type="checkbox" bind:checked={form.enabled} />
							<span class="toggle-track"></span>
						</label>
						<span class="lbl" style="margin:0">Enabled</span>
					</div>
				</div>
			</div>
			<div class="modal-foot">
				<button class="btn-cancel" onclick={() => (showCreate = false)}>Cancel</button>
				<button class="btn-confirm" onclick={createPlan} disabled={creating || !form.name}>
					{creating ? 'Creating…' : 'Create Plan'}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
	.p { max-width:1040px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:24px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }
	.add-btn { padding:6px 14px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; font-family:var(--font); white-space:nowrap; }
	.add-btn:hover { opacity:.88; }

	.cards-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(300px,1fr)); gap:14px; }
	.plan-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:18px; box-shadow:var(--shadow-sm); }
	.plan-card.plan-disabled { opacity:.65; }
	.sk-card { min-height:200px; }
	.plan-hdr { display:flex; justify-content:space-between; align-items:flex-start; margin-bottom:12px; gap:8px; }
	.plan-hdr-r { display:flex; align-items:center; gap:8px; flex-shrink:0; }
	.plan-name { font-size:16px; font-weight:800; color:var(--text); display:block; letter-spacing:-0.01em; }
	.plan-price { font-size:12px; font-weight:600; color:var(--text-3); display:block; margin-top:2px; }
	.disabled-badge { display:inline-flex; padding:2px 9px; border-radius:999px; font-size:10.5px; font-weight:700; background:var(--danger-soft); color:var(--danger); border:1px solid rgba(220,38,38,0.18); margin-bottom:10px; }
	.plan-grid { display:grid; grid-template-columns:1fr 1fr; gap:6px; }
	.plan-stat { display:flex; justify-content:space-between; padding:5px 8px; background:var(--surface-2); border-radius:5px; font-size:11.5px; }
	.stat-l { color:var(--text-3); }
	.stat-v { font-weight:700; color:var(--text); }

	.edit-btn {
		display:inline-flex; align-items:center; gap:4px;
		padding:3px 9px; height:26px; border-radius:var(--radius-sm);
		font-size:11px; font-weight:600; cursor:pointer;
		border:1px solid var(--border); background:var(--surface-2); color:var(--text-2);
		font-family:var(--font); transition:background .12s, color .12s, border-color .12s;
	}
	.edit-btn:hover { background:var(--accent); color:#000; border-color:var(--accent); }

	.toggle { position:relative; display:inline-flex; cursor:pointer; width:34px; height:20px; }
	.toggle input { opacity:0; width:0; height:0; }
	.toggle-track { position:absolute; inset:0; border-radius:999px; background:var(--border); transition:background .2s; }
	.toggle-track::after { content:''; position:absolute; left:2px; top:2px; width:16px; height:16px; border-radius:50%; background:#fff; transition:transform .2s; }
	.toggle input:checked + .toggle-track { background:var(--accent); }
	.toggle input:checked + .toggle-track::after { transform:translateX(14px); }

	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { display:flex; align-items:center; justify-content:center; padding:56px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.modal-bg { position:fixed; inset:0; z-index:200; background:rgba(0,0,0,0.45); display:flex; align-items:center; justify-content:center; padding:20px; backdrop-filter:blur(2px); }
	.modal { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); box-shadow:0 20px 60px rgba(0,0,0,0.25); width:100%; max-width:520px; max-height:90vh; overflow-y:auto; display:flex; flex-direction:column; }
	.modal-hdr { display:flex; align-items:center; justify-content:space-between; padding:16px 20px; border-bottom:1px solid var(--border); position:sticky; top:0; background:var(--surface); z-index:1; gap:10px; }
	.modal-title { font-size:14px; font-weight:700; color:var(--text); margin:0; }
	.modal-sub { font-size:11.5px; color:var(--text-3); display:block; margin-top:2px; }
	.modal-close { background:none; border:none; color:var(--text-3); font-size:14px; cursor:pointer; padding:4px; border-radius:4px; display:flex; align-items:center; justify-content:center; flex-shrink:0; }
	.modal-close:hover { color:var(--text); background:var(--surface-2); }
	.modal-body { padding:20px; }
	.modal-foot { display:flex; justify-content:flex-end; gap:8px; padding:14px 20px; border-top:1px solid var(--border); background:var(--surface-2); position:sticky; bottom:0; }

	.edit-err { padding:9px 12px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius-sm); font-size:12.5px; color:var(--danger); margin-bottom:14px; }

	.form-grid { display:grid; grid-template-columns:1fr 1fr; gap:14px; }
	.field { display:flex; flex-direction:column; gap:5px; }
	.toggle-field { flex-direction:row; align-items:center; gap:10px; padding-top:18px; }
	.lbl { font-size:11.5px; font-weight:600; color:var(--text-2); }
	.field-hint { font-size:10px; color:var(--text-3); font-weight:400; margin-left:4px; }
	.inp { height:34px; padding:0 10px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:100%; box-sizing:border-box; font-family:var(--font); transition:border-color .15s; }
	.inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.btn-cancel { padding:6px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); }
	.btn-cancel:hover:not(:disabled) { background:var(--surface-2); }
	.btn-cancel:disabled { opacity:.45; cursor:not-allowed; }
	.btn-confirm { padding:6px 16px; border-radius:var(--radius-sm); font-size:12px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; font-family:var(--font); }
	.btn-confirm:hover:not(:disabled) { opacity:.88; }
	.btn-confirm:disabled { opacity:.5; cursor:not-allowed; }

	@media (max-width: 480px) {
		.form-grid { grid-template-columns:1fr; }
		.p { padding:20px 16px; }
	}
</style>
