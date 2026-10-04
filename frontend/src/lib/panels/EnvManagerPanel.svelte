<script lang="ts">
	import { onMount } from 'svelte';
	import { Eye, EyeOff, Plus, Trash2, Save, Code } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can, permProject } from '$lib/auth/permissions';
	import type { ServiceEnv } from '$lib/api/types';
	import { Button, TextField, Textarea, Checkbox, InlineAlert, Spinner, ConfirmDialog } from '$lib/components/ui';

	interface Props {
		serviceId?: string; // Optional for pre-creation mode
		projectId: string;
		serviceName?: string;
		// Local mode props
		initialEnvs?: Array<{ key: string; value: string; is_secret: boolean }>;
		onConfirm?: (envs: Array<{ key: string; value: string; is_secret: boolean }>) => void;
	}

	let { serviceId, projectId, serviceName = 'Service', initialEnvs = [], onConfirm }: Props = $props();

	let orgId = $derived($orgStore.activeOrg?.id ?? '');
	let canEnvWrite = $derived(
		!serviceId || can(
			$orgStore.myMembership?.role ?? null,
			$orgStore.myMembership?.permissions ?? [],
			permProject(orgId, projectId, 'env', 'write')
		)
	);

	// ── State ────────────────────────────────────────────────────────
	type Mode = 'list' | 'raw';
	let mode = $state<Mode>('list');
	let envs = $state<ServiceEnv[]>([]);
	let loading = $state(true);
	let saving = $state(false);
	let error = $state<string | null>(null);

	// Per-row editing state
	interface EditState {
		key: string;
		value: string;
		is_secret: boolean;
		revealed: boolean;
		revealing: boolean;
		dirty: boolean;
	}
	let edits = $state<Record<string, EditState>>({});

	// Add-new form
	let newKey = $state('');
	let newValue = $state('');
	let newIsSecret = $state(false);
	let showAddForm = $state(false);

	// Raw mode
	let rawText = $state('');
	let rawDirty = $state(false);

	// ── Helpers ──────────────────────────────────────────────────────
	function buildEdits(rows: ServiceEnv[]) {
		const next: Record<string, EditState> = {};
		for (const e of rows) {
			next[e.id] = {
				key: e.key,
				value: e.is_secret ? '***' : e.value_encrypted,
				is_secret: e.is_secret,
				revealed: false,
				revealing: false,
				dirty: false
			};
		}
		edits = next;
	}

	function toRaw(rows: ServiceEnv[]): string {
		return rows.map((e) =>
			e.is_secret
				? `# ${e.key}=*** (secret — reveal in list mode to edit)`
				: `${e.key}=${e.value_encrypted}`
		).join('\n');
	}

	function parseRaw(text: string): Array<{ key: string; value: string; is_secret: boolean }> {
		const result: Array<{ key: string; value: string; is_secret: boolean }> = [];
		for (const line of text.split('\n')) {
			const stripped = line.replace(/\r$/, '');
			if (!stripped || stripped.trimStart().startsWith('#')) continue;
			const eq = stripped.indexOf('=');
			if (eq === -1) continue;
			const key = stripped.slice(0, eq).trim();
			const value = stripped.slice(eq + 1);
			if (!key) continue;
			result.push({ key, value, is_secret: false });
		}
		return result;
	}

	function triggerConfirm() {
		if (onConfirm) {
			onConfirm(envs.map(e => ({
				key: e.key,
				value: e.value_encrypted,
				is_secret: e.is_secret
			})));
		}
	}

	// ── Load ─────────────────────────────────────────────────────────
	async function load() {
		loading = true;
		error = null;
		if (!serviceId) {
			envs = (initialEnvs || []).map((e, idx) => ({
				id: `local-${idx}`,
				service_id: '',
				key: e.key,
				value_encrypted: e.value,
				is_secret: e.is_secret,
				created_at: new Date().toISOString()
			}));
			buildEdits(envs);
			rawText = toRaw(envs);
		} else {
			const res = await api.getServiceEnvs(serviceId);
			if (res.error) {
				error = res.error.message;
			} else if (res.data) {
				envs = res.data.sort((a, b) => a.key.localeCompare(b.key));
				buildEdits(envs);
				rawText = toRaw(envs);
			}
		}
		loading = false;
	}

	// ── List mode actions ─────────────────────────────────────────────
	function setEdit(id: string, field: 'key' | 'value' | 'is_secret', val: string | boolean) {
		if (!edits[id]) return;
		(edits[id] as any)[field] = val;
		edits[id].dirty = true;
	}

	async function toggleReveal(id: string) {
		if (!edits[id]) return;
		if (edits[id].revealed) {
			edits[id].revealed = false;
			edits[id].value = '***';
			edits[id].dirty = false;
			return;
		}
		if (edits[id].value === '***') {
			if (!serviceId) {
				const row = envs.find(e => e.id === id);
				edits[id].value = row ? row.value_encrypted : '';
			} else {
				edits[id].revealing = true;
				const res = await api.revealEnv(projectId, serviceId, id);
				edits[id].revealing = false;
				if (res.error || !res.data) {
					error = res.error?.message ?? 'Failed to reveal secret';
					return;
				}
				edits[id].value = res.data.value;
			}
		}
		edits[id].revealed = true;
	}

	async function saveRow(id: string) {
		const edit = edits[id];
		if (!edit || !edit.dirty) return;
		if (edit.is_secret && edit.value === '***') {
			error = 'Reveal the secret value before saving changes to this row.';
			return;
		}
		saving = true;
		if (!serviceId) {
			envs = envs.map(e => {
				if (e.id === id) {
					return {
						...e,
						key: edit.key,
						value_encrypted: edit.value,
						is_secret: edit.is_secret
					};
				}
				return e;
			});
			buildEdits(envs);
			rawText = toRaw(envs);
			triggerConfirm();
		} else {
			const res = await api.upsertEnv(serviceId, {
				key: edit.key,
				value: edit.value,
				is_secret: edit.is_secret
			});
			if (res.error) {
				error = res.error.message;
			} else {
				await load();
			}
		}
		saving = false;
	}

	// Delete confirmation (was a native confirm()): the row id awaiting confirmation.
	let deleteId = $state<string | null>(null);
	let showDeleteConfirm = $state(false);

	function requestDelete(id: string) {
		deleteId = id;
		showDeleteConfirm = true;
	}

	async function deleteRow(id: string) {
		saving = true;
		if (!serviceId) {
			envs = envs.filter(e => e.id !== id);
			buildEdits(envs);
			rawText = toRaw(envs);
			triggerConfirm();
		} else {
			const res = await api.deleteEnv(serviceId, id);
			if (res.error) {
				error = res.error.message;
			} else {
				await load();
			}
		}
		saving = false;
	}

	async function addNew() {
		if (!newKey.trim()) return;
		saving = true;
		if (!serviceId) {
			envs.push({
				id: `local-${Date.now()}`,
				service_id: '',
				key: newKey.trim(),
				value_encrypted: newValue,
				is_secret: newIsSecret,
				created_at: new Date().toISOString()
			});
			envs.sort((a, b) => a.key.localeCompare(b.key));
			buildEdits(envs);
			rawText = toRaw(envs);
			newKey = '';
			newValue = '';
			newIsSecret = false;
			showAddForm = false;
			triggerConfirm();
		} else {
			const res = await api.upsertEnv(serviceId, {
				key: newKey.trim(),
				value: newValue,
				is_secret: newIsSecret
			});
			if (res.error) {
				error = res.error.message;
			} else {
				newKey = '';
				newValue = '';
				newIsSecret = false;
				showAddForm = false;
				await load();
			}
		}
		saving = false;
	}

	// ── Raw mode actions ──────────────────────────────────────────────
	function enterRaw() {
		rawText = toRaw(envs);
		rawDirty = false;
		mode = 'raw';
	}

	async function saveRaw() {
		const parsed = parseRaw(rawText);
		if (parsed.length === 0 && rawText.trim().length > 0) {
			error = 'Could not parse any KEY=VALUE pairs from the text.';
			return;
		}
		saving = true;
		if (!serviceId) {
			let nextEnvs: ServiceEnv[] = [];
			for (const item of parsed) {
				const existing = envs.find(e => e.key === item.key);
				if (existing) {
					nextEnvs.push({
						...existing,
						value_encrypted: item.value,
						is_secret: item.is_secret
					});
				} else {
					nextEnvs.push({
						id: `local-${Date.now()}-${Math.random()}`,
						service_id: '',
						key: item.key,
						value_encrypted: item.value,
						is_secret: item.is_secret,
						created_at: new Date().toISOString()
					});
				}
			}
			for (const existing of envs) {
				if (existing.is_secret && !nextEnvs.some(e => e.key === existing.key)) {
					if (rawText.includes(`# ${existing.key}=`)) {
						nextEnvs.push(existing);
					}
				}
			}
			envs = nextEnvs.sort((a, b) => a.key.localeCompare(b.key));
			buildEdits(envs);
			rawText = toRaw(envs);
			mode = 'list';
			triggerConfirm();
		} else {
			const res = await api.bulkSetEnvs(serviceId, parsed);
			if (res.error) {
				error = res.error.message;
			} else {
				await load();
				mode = 'list';
			}
		}
		saving = false;
	}

	onMount(load);

	let dirtyCount = $derived(Object.values(edits).filter((e) => e.dirty).length);
</script>

<div class="env-panel">
	<!-- Header -->
	<div class="env-header">
		<div class="header-left">
			<span class="header-title">Environment Variables</span>
			<span class="header-sub">{serviceName}</span>
		</div>
		<div class="header-actions">
			{#if mode === 'list'}
				<Button variant="ghost" size="sm" onclick={enterRaw} title="Edit as raw text">
					<Code size={13} />
					Raw
				</Button>
				{#if canEnvWrite}
					<Button variant="secondary" size="sm" onclick={() => { showAddForm = !showAddForm; }}>
						<Plus size={13} />
						Add
					</Button>
				{/if}
			{:else}
				<Button variant="ghost" size="sm" onclick={() => { mode = 'list'; }}>Cancel</Button>
				<Button variant="primary" size="sm" disabled={saving} onclick={saveRaw}>
					<Save size={13} />
					{saving ? 'Saving…' : 'Save All'}
				</Button>
			{/if}
		</div>
	</div>

	{#if error}
		<div class="env-error" role="alert">
			<InlineAlert tone="error">
				<span class="env-error-row">
					<span>{error}</span>
					<button class="dismiss" aria-label="Dismiss error" onclick={() => { error = null; }}>✕</button>
				</span>
			</InlineAlert>
		</div>
	{/if}

	<!-- Loading -->
	{#if loading}
		<div class="env-loading">
			<Spinner size={16} />
			<span>Loading variables…</span>
		</div>

	<!-- Raw mode -->
	{:else if mode === 'raw'}
		<div class="raw-section">
			<div class="raw-hint">
				One variable per line: <span class="font-mono">KEY=value</span>. Lines starting with <span class="font-mono">#</span> are ignored. Secret variables are shown as comments and are <strong>not overwritten</strong> — reveal them in list mode to change their values.
			</div>
			<div class="raw-field">
				<Textarea
					bind:value={rawText}
					oninput={() => { rawDirty = true; }}
					rows={20}
					wrap="off"
					placeholder={'DATABASE_URL=postgres://...\nSECRET_KEY=my-secret'}
					spellcheck={false}
					{...{ autocorrect: 'off' }}
					autocapitalize="off"
				/>
			</div>
		</div>

	<!-- List mode -->
	{:else}
		<!-- Add-new form -->
		{#if showAddForm}
			<div class="add-form">
				<div class="add-key mono-field">
					<TextField
						placeholder="KEY"
						bind:value={newKey}
						onkeydown={(e) => { if (e.key === 'Enter') addNew(); }}
					/>
				</div>
				<div class="add-value mono-field">
					<TextField
						placeholder="value"
						type={newIsSecret ? 'password' : 'text'}
						bind:value={newValue}
						onkeydown={(e) => { if (e.key === 'Enter') addNew(); }}
					/>
				</div>
				<span title="Mark as secret"><Checkbox bind:checked={newIsSecret} label="Secret" /></span>
				<Button variant="primary" size="sm" disabled={saving || !newKey.trim() || !canEnvWrite} onclick={addNew}>
					{saving ? '…' : 'Add'}
				</Button>
				<Button variant="ghost" size="sm" onclick={() => { showAddForm = false; }}>Cancel</Button>
			</div>
		{/if}

		<!-- Env list -->
		{#if envs.length === 0 && !showAddForm}
			<div class="env-empty">
				<span>No environment variables yet.</span>
				{#if canEnvWrite}
					<Button variant="secondary" size="sm" onclick={() => { showAddForm = true; }}>
						<Plus size={13} />
						Add first variable
					</Button>
				{/if}
			</div>
		{:else}
			<div class="env-list">
				{#each envs as env (env.id)}
					{@const edit = edits[env.id]}
					{#if edit}
						<div class="env-row" class:dirty={edit.dirty}>
							<!-- Key -->
							<div class="env-key mono-field">
								<TextField
									value={edit.key}
									oninput={(e) => setEdit(env.id, 'key', (e.target as HTMLInputElement).value)}
									placeholder="KEY"
									aria-label="Variable name"
								/>
							</div>

							<!-- Value -->
							<div class="value-wrap">
								<div class="env-value mono-field">
									<TextField
										type={edit.is_secret && !edit.revealed ? 'password' : 'text'}
										value={edit.value}
										oninput={(e) => setEdit(env.id, 'value', (e.target as HTMLInputElement).value)}
										placeholder="value"
										aria-label="Variable value"
									/>
								</div>
								{#if edit.is_secret}
									<Button
										variant="ghost"
										size="icon"
										onclick={() => toggleReveal(env.id)}
										disabled={edit.revealing}
										title={edit.revealed ? 'Hide' : 'Reveal'}
										aria-label={edit.revealed ? 'Hide value' : 'Reveal value'}
									>
										{#if edit.revealing}
											<Spinner size={12} tone="current" />
										{:else if edit.revealed}
											<EyeOff size={13} />
										{:else}
											<Eye size={13} />
										{/if}
									</Button>
								{/if}
							</div>

							<!-- Secret toggle -->
							<span class="secret-toggle" title="Mark as secret">
								<Checkbox
									checked={edit.is_secret}
									onchange={(checked) => setEdit(env.id, 'is_secret', checked)}
								/>
							</span>

							<!-- Save / Delete -->
							{#if edit.dirty}
								<Button
									variant="primary"
									size="icon"
									disabled={saving}
									onclick={() => saveRow(env.id)}
									title="Save"
									aria-label="Save variable"
								>
									<Save size={12} />
								</Button>
							{/if}
							<Button
								variant="ghost"
								size="icon"
								onclick={() => requestDelete(env.id)}
								title={canEnvWrite ? 'Delete' : 'Insufficient permissions'}
								aria-label="Delete variable"
								disabled={!canEnvWrite}
							>
								<Trash2 size={12} />
							</Button>
						</div>
					{/if}
				{/each}
			</div>

			{#if dirtyCount > 0}
				<div class="dirty-banner">
					{dirtyCount} unsaved change{dirtyCount === 1 ? '' : 's'} — click the save icon on each row to apply.
				</div>
			{/if}
		{/if}
	{/if}
</div>

<ConfirmDialog
	bind:open={showDeleteConfirm}
	title="Delete variable"
	message="Delete this environment variable?"
	confirmLabel="Delete"
	onConfirm={async () => {
		if (deleteId) await deleteRow(deleteId);
		deleteId = null;
	}}
/>

<style>
	.env-panel {
		display: flex;
		flex-direction: column;
		height: 100%;
	}

	.env-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 14px 16px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.header-left {
		display: flex;
		flex-direction: column;
		gap: 2px;
	}

	.header-title {
		font-size: 14px;
		font-weight: 700;
		color: var(--text-primary);
	}

	.header-sub {
		font-size: 11px;
		color: var(--text-muted);
	}

	.header-actions {
		display: flex;
		gap: 6px;
		align-items: center;
	}

	.env-error {
		padding: 8px 16px;
		flex-shrink: 0;
	}

	.env-error-row {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 8px;
	}

	.dismiss {
		background: transparent;
		border: none;
		color: inherit;
		cursor: pointer;
		font-size: 14px;
		padding: 0 2px;
	}

	.env-loading {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 24px;
		color: var(--text-muted);
		font-size: 13px;
	}

	/* Monospace + compact text for variable inputs (TextField owns the rest) */
	.mono-field :global(input) {
		font-family: var(--font-mono);
		font-size: 12px;
	}

	/* Add form */
	.add-form {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		flex-shrink: 0;
	}

	.add-key { width: 110px; flex-shrink: 0; }
	.add-value { flex: 1; min-width: 80px; }

	/* List */
	.env-list {
		flex: 1;
		overflow-y: auto;
	}

	.env-row {
		display: flex;
		align-items: center;
		flex-wrap: wrap;
		gap: 6px;
		padding: 7px 16px;
		border-bottom: 1px solid var(--border);
		transition: background var(--transition-fast);
	}

	.env-row:hover {
		background: var(--bg-elevated);
	}

	.env-row.dirty {
		background: var(--accent-blue-muted);
	}

	.env-key {
		width: 150px;
		flex-shrink: 0;
	}

	.value-wrap {
		flex: 1;
		min-width: 120px;
		display: flex;
		align-items: center;
		gap: 4px;
	}

	.env-value { flex: 1; min-width: 0; }

	.secret-toggle {
		display: flex;
		align-items: center;
		flex-shrink: 0;
	}

	@media (max-width: 639px) {
		.env-key { width: 100%; }
	}

	.env-empty {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 16px;
		flex: 1;
		padding: 40px 24px;
		color: var(--text-muted);
		font-size: 13px;
		text-align: center;
	}

	.dirty-banner {
		padding: 8px 16px;
		font-size: 11px;
		color: var(--accent-blue);
		background: var(--accent-blue-muted);
		border-top: 1px solid var(--border);
		text-align: center;
		flex-shrink: 0;
	}

	/* Raw mode */
	.raw-section {
		flex: 1;
		display: flex;
		flex-direction: column;
		padding: 12px 16px;
		gap: 10px;
		overflow: hidden;
	}

	.raw-hint {
		font-size: 12px;
		color: var(--text-muted);
		padding: 8px 12px;
		background: var(--bg-elevated);
		border-radius: var(--radius-sm);
		border: 1px solid var(--border);
		line-height: 1.5;
	}

	.raw-field {
		flex: 1;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}

	.raw-field :global(textarea) {
		flex: 1;
		resize: none;
		font-family: var(--font-mono);
		font-size: 12px;
		line-height: 1.6;
		padding: 12px;
		background: var(--bg-base);
		white-space: pre;
		overflow: auto;
	}
</style>
