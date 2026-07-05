<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { isOwnerRole } from '$lib/auth/permissions';
	import {
		Database, RefreshCw, Trash2, AlertTriangle, X, Loader2, TableProperties
	} from '@lucide/svelte';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { page } from '$app/state';

	let orgSlug = $derived(page.params.orgSlug ?? '');

	interface DbTable {
		name: string;
		row_count: number;
	}

	let myRole   = $derived($orgStore.myMembership?.role ?? null);
	let isOwner  = $derived(isOwnerRole(myRole));

	let tables   = $state<DbTable[]>([]);
	let loading  = $state(true);
	let error    = $state('');

	// Confirmation dialog state
	let confirmTable  = $state<DbTable | null>(null);
	let confirmInput  = $state('');
	let dropping      = $state(false);
	let dropError     = $state('');

	onMount(() => { if (isOwner) loadTables(); });

	async function loadTables() {
		loading = true;
		error = '';
		const res = await api.get<DbTable[]>('/admin/db/tables');
		if (res.error) {
			error = res.error.message;
		} else {
			tables = res.data ?? [];
		}
		loading = false;
	}

	function openConfirm(table: DbTable) {
		confirmTable = table;
		confirmInput = '';
		dropError = '';
	}

	function closeConfirm() {
		if (dropping) return;
		confirmTable = null;
		confirmInput = '';
		dropError = '';
	}

	async function dropTable() {
		if (!confirmTable || confirmInput !== confirmTable.name) return;
		dropping = true;
		dropError = '';
		const res = await api.delete(`/admin/db/tables/${encodeURIComponent(confirmTable.name)}`);
		if (res.error) {
			dropError = res.error.message;
			dropping = false;
			return;
		}
		tables = tables.filter(t => t.name !== confirmTable!.name);
		dropping = false;
		confirmTable = null;
	}

	function formatCount(n: number): string {
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
		if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
		return n.toString();
	}
</script>

<PermissionDeniedDialog
	open={!isOwner}
	message="Only organization owners can access database management."
	onDismiss={() => history.back()}
	onBack={() => history.back()}
/>

{#if isOwner}
	<div class="db-page">
		<div class="section-header">
			<div class="section-title">
				<Database size={18} />
				<div>
					<h2>Database Tables</h2>
					<p>All tables in the Shipyard platform database. Dropped tables are permanent and unrecoverable.</p>
				</div>
			</div>
			<button class="btn btn-secondary btn-sm icon-btn" onclick={loadTables} disabled={loading}>
				<RefreshCw size={13} class={loading ? 'spin' : ''} />
				Refresh
			</button>
		</div>

		{#if error}
			<div class="alert alert-error">
				<AlertTriangle size={14} />
				{error}
			</div>
		{/if}

		{#if loading}
			<div class="loading-state">
				<Loader2 size={20} class="spin" />
				<span>Loading tables…</span>
			</div>
		{:else if tables.length === 0}
			<div class="empty-state">
				<TableProperties size={32} />
				<p>No tables found in the public schema.</p>
			</div>
		{:else}
			<div class="warning-banner">
				<AlertTriangle size={14} />
				<span>Dropping a table is irreversible and removes all its data. Proceed with extreme caution.</span>
			</div>

			<div class="table-list">
				<div class="table-header">
					<span class="col-name">Table name</span>
					<span class="col-rows">Rows</span>
					<span class="col-action"></span>
				</div>
				{#each tables as table (table.name)}
					<div class="table-row">
						<span class="col-name">
							<code class="table-name">{table.name}</code>
						</span>
						<span class="col-rows">{formatCount(table.row_count)}</span>
						<span class="col-action">
							<button
								class="btn btn-danger-outline btn-sm icon-btn"
								onclick={() => openConfirm(table)}
								title="Drop table"
							>
								<Trash2 size={13} />
								Drop
							</button>
						</span>
					</div>
				{/each}
			</div>

			<p class="table-count">{tables.length} table{tables.length !== 1 ? 's' : ''}</p>
		{/if}
	</div>

	{#if confirmTable}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="modal-backdrop" onclick={closeConfirm} onkeydown={() => {}}></div>
		<div class="modal" role="dialog" aria-modal="true" aria-label="Confirm drop table">
			<div class="modal-header">
				<div class="modal-title">
					<Trash2 size={16} />
					<span>Drop table <code>{confirmTable.name}</code></span>
				</div>
				<button class="close-btn" onclick={closeConfirm} disabled={dropping}>
					<X size={15} />
				</button>
			</div>

			<div class="modal-body">
				<div class="danger-notice">
					<AlertTriangle size={16} />
					<div>
						<strong>This action is permanent.</strong>
						<p>All data in <code>{confirmTable.name}</code> will be deleted and cannot be recovered. Any tables that depend on it will also be affected (CASCADE).</p>
					</div>
				</div>

				<p class="confirm-label">Type <strong>{confirmTable.name}</strong> to confirm:</p>
				<input
					class="confirm-input"
					type="text"
					placeholder={confirmTable.name}
					bind:value={confirmInput}
					disabled={dropping}
					autocomplete="off"
					spellcheck="false"
				/>

				{#if dropError}
					<p class="drop-error">{dropError}</p>
				{/if}
			</div>

			<div class="modal-footer">
				<button class="btn btn-secondary" onclick={closeConfirm} disabled={dropping}>Cancel</button>
				<button
					class="btn btn-danger"
					onclick={dropTable}
					disabled={dropping || confirmInput !== confirmTable.name}
				>
					{#if dropping}
						<Loader2 size={13} class="spin" />
						Dropping…
					{:else}
						<Trash2 size={13} />
						Drop table
					{/if}
				</button>
			</div>
		</div>
	{/if}
{/if}

<style>
	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.db-page {
		display: flex;
		flex-direction: column;
		gap: 20px;
		max-width: 800px;
	}

	.section-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
		gap: 16px;
	}

	.section-title {
		display: flex;
		align-items: flex-start;
		gap: 12px;
		color: var(--text-muted);
	}

	.section-title h2 {
		font-size: 15px;
		font-weight: 600;
		color: var(--text-primary);
		margin: 0 0 3px;
	}

	.section-title p {
		font-size: 13px;
		color: var(--text-muted);
		margin: 0;
	}

	.icon-btn {
		display: flex;
		align-items: center;
		gap: 6px;
	}

	.alert {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 14px;
		border-radius: 6px;
		font-size: 13px;
	}

	.alert-error {
		background: #fef2f2;
		color: #dc2626;
		border: 1px solid #fecaca;
	}

	.loading-state, .empty-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		padding: 48px;
		color: var(--text-muted);
		font-size: 13px;
	}

	.warning-banner {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 10px 14px;
		background: #fffbeb;
		border: 1px solid #fde68a;
		border-radius: 6px;
		font-size: 13px;
		color: #92400e;
	}

	.table-list {
		border: 1px solid var(--border);
		border-radius: 8px;
		overflow: hidden;
	}

	.table-header {
		display: grid;
		grid-template-columns: 1fr 100px 100px;
		padding: 9px 16px;
		background: var(--bg-elevated, #f9fafb);
		border-bottom: 1px solid var(--border);
		font-size: 11px;
		font-weight: 600;
		text-transform: uppercase;
		letter-spacing: 0.05em;
		color: var(--text-muted);
	}

	.table-row {
		display: grid;
		grid-template-columns: 1fr 100px 100px;
		align-items: center;
		padding: 10px 16px;
		border-bottom: 1px solid var(--border);
		font-size: 13px;
	}

	.table-row:last-child { border-bottom: none; }
	.table-row:hover { background: var(--bg-elevated, #f9fafb); }

	.col-name { display: flex; align-items: center; min-width: 0; }
	.col-rows { color: var(--text-muted); font-variant-numeric: tabular-nums; }
	.col-action { display: flex; justify-content: flex-end; }

	code.table-name {
		font-family: var(--font-mono, monospace);
		font-size: 12px;
		color: var(--text-primary);
		background: var(--bg-base);
		padding: 2px 6px;
		border-radius: 4px;
		border: 1px solid var(--border);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.table-count {
		font-size: 12px;
		color: var(--text-muted);
		margin: 0;
		text-align: right;
	}

	/* Danger variant */
	.btn-danger-outline {
		background: transparent;
		border: 1px solid #fca5a5;
		color: #dc2626;
	}
	.btn-danger-outline:hover:not(:disabled) {
		background: #fee2e2;
		border-color: #dc2626;
	}

	/* Modal */
	.modal-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.45);
		z-index: 100;
	}

	.modal {
		position: fixed;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(480px, calc(100vw - 32px));
		background: var(--bg-surface, #fff);
		border: 1px solid var(--border);
		border-radius: 10px;
		box-shadow: 0 20px 60px rgba(0, 0, 0, 0.18);
		z-index: 101;
		display: flex;
		flex-direction: column;
	}

	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 16px 20px;
		border-bottom: 1px solid var(--border);
	}

	.modal-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.modal-title code {
		font-family: var(--font-mono, monospace);
		font-size: 13px;
		color: #dc2626;
	}

	.close-btn {
		background: none;
		border: none;
		color: var(--text-muted);
		cursor: pointer;
		padding: 4px;
		border-radius: 4px;
		display: flex;
		align-items: center;
	}
	.close-btn:hover:not(:disabled) { color: var(--text-primary); }

	.modal-body {
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.danger-notice {
		display: flex;
		gap: 12px;
		padding: 14px;
		background: #fef2f2;
		border: 1px solid #fecaca;
		border-radius: 6px;
		color: #dc2626;
		font-size: 13px;
	}

	.danger-notice :global(svg) { flex-shrink: 0; margin-top: 1px; }

	.danger-notice strong { display: block; margin-bottom: 4px; }
	.danger-notice p { margin: 0; color: #7f1d1d; }
	.danger-notice code { font-family: var(--font-mono, monospace); font-size: 12px; }

	.confirm-label {
		font-size: 13px;
		color: var(--text-secondary, var(--text-muted));
		margin: 0;
	}

	.confirm-input {
		width: 100%;
		padding: 8px 12px;
		font-size: 13px;
		font-family: var(--font-mono, monospace);
		border: 1px solid var(--border);
		border-radius: 6px;
		background: var(--bg-base);
		color: var(--text-primary);
		box-sizing: border-box;
		outline: none;
	}
	.confirm-input:focus { border-color: #dc2626; box-shadow: 0 0 0 2px #fee2e2; }

	.drop-error {
		font-size: 13px;
		color: #dc2626;
		margin: 0;
	}

	.modal-footer {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
		padding: 16px 20px;
		border-top: 1px solid var(--border);
	}

	.btn-danger {
		background: #dc2626;
		border: 1px solid #dc2626;
		color: #fff;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.btn-danger:hover:not(:disabled) { background: #b91c1c; border-color: #b91c1c; }
	.btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
