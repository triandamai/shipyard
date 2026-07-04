<script lang="ts">
	import { onMount } from 'svelte';
	import { Eye, EyeOff, Plus, Trash2, Save, Code } from '@lucide/svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { can } from '$lib/auth/permissions';
	import type { ServiceEnv } from '$lib/api/types';

	interface Props {
		serviceId: string;
		projectId: string;
		serviceName?: string;
	}

	let { serviceId, projectId, serviceName = 'Service' }: Props = $props();

	let canEnvWrite = $derived(can($orgStore.myMembership?.role ?? null, $orgStore.myMembership?.permissions ?? [], 'env:write'));

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
			// Strip only \r (Windows line endings) — do not trim the line itself
			const stripped = line.replace(/\r$/, '');
			if (!stripped || stripped.trimStart().startsWith('#')) continue;
			const eq = stripped.indexOf('=');
			if (eq === -1) continue;
			const key = stripped.slice(0, eq).trim();
			// Value: everything after the first `=`, no trimming so spaces/special chars are preserved
			const value = stripped.slice(eq + 1);
			if (!key) continue;
			result.push({ key, value, is_secret: false });
		}
		return result;
	}

	// ── Load ─────────────────────────────────────────────────────────
	async function load() {
		loading = true;
		error = null;
		const res = await api.getServiceEnvs(serviceId);
		if (res.error) {
			error = res.error.message;
		} else if (res.data) {
			envs = res.data.sort((a, b) => a.key.localeCompare(b.key));
			buildEdits(envs);
			rawText = toRaw(envs);
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
			// hide: mask again
			edits[id].revealed = false;
			edits[id].value = '***';
			edits[id].dirty = false;
			return;
		}
		// reveal: fetch real value if still masked
		if (edits[id].value === '***') {
			edits[id].revealing = true;
			const res = await api.revealEnv(projectId, serviceId, id);
			edits[id].revealing = false;
			if (res.error || !res.data) {
				error = res.error?.message ?? 'Failed to reveal secret';
				return;
			}
			edits[id].value = res.data.value;
		}
		edits[id].revealed = true;
	}

	async function saveRow(id: string) {
		const edit = edits[id];
		if (!edit || !edit.dirty) return;
		// Unrevealed secret — value is still masked. Don't overwrite the stored
		// secret with an empty string. Reveal first, then save.
		if (edit.is_secret && edit.value === '***') {
			error = 'Reveal the secret value before saving changes to this row.';
			return;
		}
		saving = true;
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
		saving = false;
	}

	async function deleteRow(id: string) {
		if (!confirm('Delete this environment variable?')) return;
		saving = true;
		const res = await api.deleteEnv(serviceId, id);
		if (res.error) {
			error = res.error.message;
		} else {
			await load();
		}
		saving = false;
	}

	async function addNew() {
		if (!newKey.trim()) return;
		saving = true;
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
		const res = await api.bulkSetEnvs(serviceId, parsed);
		if (res.error) {
			error = res.error.message;
		} else {
			await load();
			mode = 'list';
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
				<button class="btn btn-ghost btn-sm" onclick={enterRaw} title="Edit as raw text">
					<Code size={13} />
					Raw
				</button>
				{#if canEnvWrite}
				<button
					class="btn btn-secondary btn-sm"
					onclick={() => { showAddForm = !showAddForm; }}
				>
					<Plus size={13} />
					Add
				</button>
				{/if}
			{:else}
				<button class="btn btn-ghost btn-sm" onclick={() => { mode = 'list'; }}>
					Cancel
				</button>
				<button
					class="btn btn-primary btn-sm"
					disabled={saving}
					onclick={saveRaw}
				>
					<Save size={13} />
					{saving ? 'Saving…' : 'Save All'}
				</button>
			{/if}
		</div>
	</div>

	{#if error}
		<div class="env-error">{error}<button class="dismiss" onclick={() => { error = null; }}>✕</button></div>
	{/if}

	<!-- Loading -->
	{#if loading}
		<div class="env-loading">
			<div class="spinner-sm"></div>
			<span>Loading variables…</span>
		</div>

	<!-- Raw mode -->
	{:else if mode === 'raw'}
		<div class="raw-section">
			<div class="raw-hint">
				One variable per line: <span class="font-mono">KEY=value</span>. Lines starting with <span class="font-mono">#</span> are ignored. Secret variables are shown as comments and are <strong>not overwritten</strong> — reveal them in list mode to change their values.
			</div>
			<textarea
				class="raw-textarea font-mono"
				bind:value={rawText}
				oninput={() => { rawDirty = true; }}
				rows={20}
				wrap="off"
				placeholder="DATABASE_URL=postgres://...&#10;SECRET_KEY=my-secret"
				spellcheck="false"
				autocorrect="off"
				autocapitalize="off"
			></textarea>
		</div>

	<!-- List mode -->
	{:else}
		<!-- Add-new form -->
		{#if showAddForm}
			<div class="add-form">
				<input
					class="input add-key"
					placeholder="KEY"
					bind:value={newKey}
					onkeydown={(e) => { if (e.key === 'Enter') addNew(); }}
				/>
				<input
					class="input add-value"
					placeholder="value"
					type={newIsSecret ? 'password' : 'text'}
					bind:value={newValue}
					onkeydown={(e) => { if (e.key === 'Enter') addNew(); }}
				/>
				<label class="secret-toggle" title="Mark as secret">
					<input type="checkbox" bind:checked={newIsSecret} />
					<span>Secret</span>
				</label>
				<button class="btn btn-primary btn-sm" disabled={saving || !newKey.trim() || !canEnvWrite} onclick={addNew}>
					{saving ? '…' : 'Add'}
				</button>
				<button class="btn btn-ghost btn-sm" onclick={() => { showAddForm = false; }}>
					Cancel
				</button>
			</div>
		{/if}

		<!-- Env list -->
		{#if envs.length === 0 && !showAddForm}
			<div class="env-empty">
				<span>No environment variables yet.</span>
				{#if canEnvWrite}
				<button class="btn btn-secondary btn-sm" onclick={() => { showAddForm = true; }}>
					<Plus size={13} />
					Add first variable
				</button>
				{/if}
			</div>
		{:else}
			<div class="env-list">
				{#each envs as env (env.id)}
					{@const edit = edits[env.id]}
					{#if edit}
						<div class="env-row" class:dirty={edit.dirty}>
							<!-- Key -->
							<input
								class="input env-key font-mono"
								value={edit.key}
								oninput={(e) => setEdit(env.id, 'key', (e.target as HTMLInputElement).value)}
								placeholder="KEY"
							/>

							<!-- Value -->
							<div class="value-wrap">
								<input
									class="input env-value font-mono"
									type={edit.is_secret && !edit.revealed ? 'password' : 'text'}
									value={edit.value}
									oninput={(e) => setEdit(env.id, 'value', (e.target as HTMLInputElement).value)}
									placeholder="value"
								/>
								{#if edit.is_secret}
									<button
										class="reveal-btn"
										onclick={() => toggleReveal(env.id)}
										disabled={edit.revealing}
										title={edit.revealed ? 'Hide' : 'Reveal'}
									>
										{#if edit.revealing}
											<span class="reveal-spinner"></span>
										{:else if edit.revealed}
											<EyeOff size={13} />
										{:else}
											<Eye size={13} />
										{/if}
									</button>
								{/if}
							</div>

							<!-- Secret toggle -->
							<label class="secret-toggle" title="Mark as secret">
								<input
									type="checkbox"
									checked={edit.is_secret}
									onchange={(e) => setEdit(env.id, 'is_secret', (e.target as HTMLInputElement).checked)}
								/>
							</label>

							<!-- Save / Delete -->
							{#if edit.dirty}
								<button
									class="btn btn-primary btn-sm row-action"
									disabled={saving}
									onclick={() => saveRow(env.id)}
									title="Save"
								>
									<Save size={12} />
								</button>
							{/if}
							<button
								class="btn btn-ghost btn-sm row-action danger"
								onclick={() => deleteRow(env.id)}
								title={canEnvWrite ? 'Delete' : 'Insufficient permissions'}
								disabled={!canEnvWrite}
							>
								<Trash2 size={12} />
							</button>
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
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 16px;
		background: var(--accent-red-muted);
		border-bottom: 1px solid var(--accent-red);
		font-size: 12px;
		color: var(--accent-red);
		flex-shrink: 0;
	}

	.dismiss {
		background: transparent;
		border: none;
		color: var(--accent-red);
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

	.spinner-sm {
		width: 16px;
		height: 16px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
	}

	@keyframes spin { to { transform: rotate(360deg); } }

	/* Add form */
	.add-form {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		flex-shrink: 0;
	}

	.add-key { width: 140px; flex-shrink: 0; }
	.add-value { flex: 1; }

	/* List */
	.env-list {
		flex: 1;
		overflow-y: auto;
	}

	.env-row {
		display: flex;
		align-items: center;
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
		font-size: 12px;
		padding: 5px 8px;
	}

	.value-wrap {
		flex: 1;
		position: relative;
		display: flex;
		align-items: center;
	}

	.env-value {
		flex: 1;
		font-size: 12px;
		padding: 5px 28px 5px 8px;
	}

	.reveal-btn {
		position: absolute;
		right: 6px;
		background: transparent;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		padding: 2px;
		display: flex;
		align-items: center;
		transition: color var(--transition-fast);
	}

	.reveal-btn:hover { color: var(--text-primary); }
	.reveal-btn:disabled { opacity: 0.5; cursor: default; }
	.reveal-spinner {
		width: 12px;
		height: 12px;
		border: 2px solid var(--border);
		border-top-color: var(--text-muted);
		border-radius: 50%;
		animation: spin 0.6s linear infinite;
		display: inline-block;
	}
	@keyframes spin { to { transform: rotate(360deg); } }

	.secret-toggle {
		display: flex;
		align-items: center;
		gap: 4px;
		font-size: 11px;
		color: var(--text-muted);
		cursor: pointer;
		user-select: none;
		white-space: nowrap;
		flex-shrink: 0;
	}

	.secret-toggle input { accent-color: var(--accent); }
	.secret-toggle span { font-size: 11px; }

	.row-action {
		padding: 4px 6px;
		flex-shrink: 0;
	}

	.row-action.danger {
		color: var(--text-dim);
	}

	.row-action.danger:hover {
		color: var(--accent-red);
		background: var(--accent-red-muted);
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

	.raw-textarea {
		flex: 1;
		width: 100%;
		background: var(--bg-base);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		color: var(--text-primary);
		font-size: 12px;
		line-height: 1.6;
		padding: 12px;
		resize: none;
		outline: none;
		transition: border-color var(--transition-fast);
		white-space: pre;
		overflow-x: auto;
		overflow-y: auto;
		box-sizing: border-box;
	}

	.raw-textarea:focus {
		border-color: var(--accent);
		box-shadow: 0 0 0 3px var(--accent-muted);
	}
</style>
