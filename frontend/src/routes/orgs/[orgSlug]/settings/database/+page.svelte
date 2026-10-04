<script lang="ts">
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { isOwnerRole } from '$lib/auth/permissions';
	import {
		Database, RefreshCw, Trash2, TableProperties, ChevronLeft, Edit2,
		Check, CornerDownLeft, Minus, PlugZap, Unplug
	} from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import Badge from '$lib/components/ui/Badge.svelte';
	import DataTable from '$lib/components/ui/DataTable.svelte';
	import Modal from '$lib/components/ui/Modal.svelte';
	import ConfirmDialog from '$lib/components/ui/ConfirmDialog.svelte';
	import FormField from '$lib/components/ui/FormField.svelte';
	import TextField from '$lib/components/ui/TextField.svelte';
	import SearchInput from '$lib/components/ui/SearchInput.svelte';
	import InlineAlert from '$lib/components/ui/InlineAlert.svelte';
	import EmptyState from '$lib/components/ui/EmptyState.svelte';
	import Spinner from '$lib/components/ui/Spinner.svelte';
	import PermissionDeniedDialog from '$lib/components/PermissionDeniedDialog.svelte';
	import { page } from '$app/state';

	let orgSlug = $derived(page.params.orgSlug ?? '');

	// ── Types ──────────────────────────────────────────────────────────────
	interface DbTable { name: string; row_count: number; }
	interface ColMeta {
		name: string; data_type: string; udt_name: string;
		is_nullable: boolean; is_primary_key: boolean;
	}
	interface RowsResponse {
		columns: ColMeta[];
		rows: (string | number | boolean | null)[][];
		total: number;
		page: number;
		per_page: number;
	}

	// ── Permissions ────────────────────────────────────────────────────────
	let myRole  = $derived($orgStore.myMembership?.role ?? null);
	let isOwner = $derived(isOwnerRole(myRole));

	// ── Connection state ───────────────────────────────────────────────────
	let isDbConnected  = $state(false);
	let connecting     = $state(false);
	let connectError   = $state('');

	async function connect() {
		connecting = true;
		connectError = '';
		const res = await api.get<DbTable[]>('/admin/db/tables');
		connecting = false;
		if (res.error) {
			connectError = res.error.message;
			return;
		}
		tables = res.data ?? [];
		isDbConnected = true;
		loadingTables = false;
	}

	function disconnect() {
		isDbConnected = false;
		tables = [];
		closeBrowser();
		connectError = '';
	}

	// ── Table list ─────────────────────────────────────────────────────────
	let tables        = $state<DbTable[]>([]);
	let loadingTables = $state(false);
	let tableError    = $state('');
	let tableSearch   = $state('');

	let filteredTables = $derived(
		tableSearch.trim()
			? tables.filter(t => t.name.toLowerCase().includes(tableSearch.trim().toLowerCase()))
			: tables
	);

	async function refreshTables() {
		loadingTables = true;
		tableError = '';
		const res = await api.get<DbTable[]>('/admin/db/tables');
		if (res.error) tableError = res.error.message;
		else tables = res.data ?? [];
		loadingTables = false;
	}

	function formatCount(n: number) {
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
		if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
		return n.toString();
	}

	// ── Browse handlers ────────────────────────────────────────────────────
	type Cell = string | number | boolean | null;
	let browseTable   = $state<DbTable | null>(null);
	let columns       = $state<ColMeta[]>([]);
	let totalRows     = $state(0);
	const perPage     = 50;
	// Bumped to make the rows DataTable refetch its current page (refresh button, after edit/delete).
	let refreshTick   = $state(0);

	function openBrowser(table: DbTable) {
		browseTable = table;
		columns = [];
		totalRows = 0;
	}

	function closeBrowser() {
		browseTable = null;
		columns = [];
		totalRows = 0;
	}

	// Server-mode fetch for the rows DataTable (endpoint already returns `total`).
	async function fetchRowsPage(params: { page: number; pageSize: number; search: string }) {
		// Read synchronously so DataTable's effect re-runs this fetch when refreshTick changes
		// while keeping its current page and search.
		void refreshTick;
		if (!browseTable) return { rows: [] as Cell[][], total: 0 };
		const qs = new URLSearchParams({
			search: params.search.trim(),
			page: String(params.page + 1),
			per_page: String(params.pageSize),
		});
		const res = await api.get<RowsResponse>(
			`/admin/db/tables/${encodeURIComponent(browseTable.name)}/rows?${qs}`
		);
		if (!res.data) throw new Error(res.error?.message ?? 'Failed to load rows');
		columns   = res.data.columns;
		totalRows = res.data.total;
		return { rows: res.data.rows, total: res.data.total };
	}

	// ── Edit modal ─────────────────────────────────────────────────────────
	let editRow    = $state<(string | number | boolean | null)[] | null>(null);
	let editValues = $state<Record<string, string>>({});
	let saving     = $state(false);
	let saveError  = $state('');
	let saveSuccess = $state(false);

	function openEdit(row: (string | number | boolean | null)[]) {
		editRow = row;
		saveError = '';
		saveSuccess = false;
		const vals: Record<string, string> = {};
		columns.forEach((col, i) => {
			const v = row[i];
			vals[col.name] = v === null || v === undefined ? '' : String(v);
		});
		editValues = vals;
	}

	function closeEdit() { editRow = null; saveError = ''; saveSuccess = false; }

	async function saveEdit() {
		if (!browseTable || !editRow) return;
		const pkCol = columns.find(c => c.is_primary_key);
		if (!pkCol) { saveError = 'No primary key found for this table'; return; }

		const pkIdx   = columns.findIndex(c => c.is_primary_key);
		const pkValue = String(editRow[pkIdx] ?? '');

		const updates: Record<string, string> = {};
		columns.forEach((col, i) => {
			if (col.is_primary_key) return;
			const original = editRow![i] === null || editRow![i] === undefined ? '' : String(editRow![i]);
			if (editValues[col.name] !== original) updates[col.name] = editValues[col.name];
		});

		if (Object.keys(updates).length === 0) { closeEdit(); return; }

		saving = true;
		saveError = '';
		const res = await api.patch(
			`/admin/db/tables/${encodeURIComponent(browseTable.name)}/rows/${encodeURIComponent(pkValue)}`,
			{ updates }
		);
		saving = false;
		if (res.error) {
			saveError = res.error.message;
		} else {
			saveSuccess = true;
			refreshTick++;
			setTimeout(() => { closeEdit(); }, 600);
		}
	}

	// ── Delete row confirm ─────────────────────────────────────────────────
	let deleteRowPk    = $state<string | null>(null);
	let deletingRow    = $state(false);
	let deleteRowError = $state('');

	function confirmDeleteRow(row: (string | number | boolean | null)[], e: MouseEvent) {
		e.stopPropagation();
		if (!pkCol) return;
		const pkIdx = columns.findIndex(c => c.is_primary_key);
		deleteRowPk   = String(row[pkIdx] ?? '');
		deleteRowError = '';
	}

	function cancelDeleteRow() { if (!deletingRow) { deleteRowPk = null; deleteRowError = ''; } }

	// Returns false on failure so ConfirmDialog stays open and shows the error.
	async function doDeleteRow(): Promise<boolean> {
		if (!browseTable || !deleteRowPk) return false;
		deletingRow = true;
		deleteRowError = '';
		const res = await api.delete(
			`/admin/db/tables/${encodeURIComponent(browseTable.name)}/rows/${encodeURIComponent(deleteRowPk)}`
		);
		deletingRow = false;
		if (res.error) {
			deleteRowError = res.error.message;
			return false;
		}
		deleteRowPk = null;
		refreshTick++;
		return true;
	}

	// ── Drop confirm ───────────────────────────────────────────────────────
	let confirmTable = $state<DbTable | null>(null);
	let confirmInput = $state('');
	let dropping     = $state(false);
	let dropError    = $state('');

	function openConfirm(table: DbTable, e: MouseEvent) {
		e.stopPropagation();
		confirmTable = table;
		confirmInput = '';
		dropError = '';
	}
	function closeConfirm() { if (!dropping) { confirmTable = null; confirmInput = ''; dropError = ''; } }

	// Returns false on failure so ConfirmDialog stays open (with its typed text) and shows the error.
	async function dropTable(): Promise<boolean> {
		if (!confirmTable) return false;
		dropping = true;
		dropError = '';
		const res = await api.delete(`/admin/db/tables/${encodeURIComponent(confirmTable.name)}`);
		if (res.error) { dropError = res.error.message; dropping = false; return false; }
		if (browseTable?.name === confirmTable.name) closeBrowser();
		tables = tables.filter(t => t.name !== confirmTable!.name);
		dropping = false;
		confirmTable = null;
		return true;
	}

	// ── Derived ────────────────────────────────────────────────────────────
	let pkCol      = $derived(columns.find(c => c.is_primary_key));
	let rowsTableColumns = $derived([
		...columns.map(c => ({ key: c.name, label: c.name })),
		{ key: '__actions', label: '' }
	]);
	function rowKeyFor(r: Cell[]): string {
		const i = columns.findIndex(c => c.is_primary_key);
		return i >= 0 ? String(r[i] ?? '') : JSON.stringify(r);
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

	<!-- ── Header ── -->
	<div class="page-header">
		<div class="header-title">
			<Database size={18} />
			<div>
				<h2>Database</h2>
				<p>Browse, edit, and manage tables in the platform database.</p>
			</div>
		</div>
		<div class="header-actions">
			{#if isDbConnected}
				<Button variant="secondary" size="sm" onclick={refreshTables} disabled={loadingTables}>
					{#if loadingTables}<Spinner size={12} tone="current" />{:else}<RefreshCw size={13} />{/if}
					Refresh
				</Button>
				<Button variant="ghost" size="sm" onclick={disconnect} title="Disconnect">
					<Unplug size={13} />
					Disconnect
				</Button>
			{/if}
		</div>
	</div>

	{#if tableError}
		<div role="alert"><InlineAlert tone="error">{tableError}</InlineAlert></div>
	{/if}

	<!-- ── Not connected ── -->
	{#if !isDbConnected}
		<div class="connect-state">
			<EmptyState message="Not connected" sub="Connect to browse and manage the platform database tables.">
				{#snippet icon()}<Database size={36} />{/snippet}
			</EmptyState>
			{#if connectError}
				<div role="alert"><InlineAlert tone="error">{connectError}</InlineAlert></div>
			{/if}
			<Button variant="primary" onclick={connect} disabled={connecting}>
				{#if connecting}
					<Spinner size={14} tone="current" />Connecting…
				{:else}
					<PlugZap size={14} />Connect to database
				{/if}
			</Button>
		</div>
	{:else}

	<div class="layout">

		<!-- ── Left: table list ── -->
		<div class="table-panel">
			<div class="table-search-row">
				<div class="search-box">
					<SearchInput bind:value={tableSearch} placeholder="Filter tables…" />
				</div>
				<span class="table-count">{filteredTables.length} table{filteredTables.length !== 1 ? 's' : ''}</span>
			</div>

			{#if loadingTables}
				<div class="list-loading"><Spinner size={16} /><span>Loading…</span></div>
			{:else if filteredTables.length === 0}
				<EmptyState message="No tables.">
					{#snippet icon()}<TableProperties size={24} />{/snippet}
				</EmptyState>
			{:else}
				<div class="table-list">
					{#each filteredTables as table (table.name)}
						<div class={browseTable?.name === table.name ? 'table-row active' : 'table-row'}>
							<button type="button" class="trow-main" onclick={() => openBrowser(table)}>
								<span class="trow-name">{table.name}</span>
								<span class="trow-count">{formatCount(table.row_count)}</span>
							</button>
							<Button
								variant="ghost"
								size="icon"
								title="Drop table"
								aria-label="Drop table {table.name}"
								onclick={(e) => openConfirm(table, e)}
							>
								<Trash2 size={11} />
							</Button>
						</div>
					{/each}
				</div>
			{/if}
		</div>

		<!-- ── Right: table browser ── -->
		{#if browseTable}
		<div class="browser-panel">
			<div class="browser-header">
				<div class="browser-title">
					<Button variant="ghost" size="icon" onclick={closeBrowser} title="Back to list" aria-label="Back to list">
						<ChevronLeft size={14} />
					</Button>
					<code class="tname">{browseTable.name}</code>
					<Badge tone="neutral">{totalRows.toLocaleString()} rows</Badge>
				</div>
				<Button variant="ghost" size="icon" onclick={() => refreshTick++} title="Refresh rows" aria-label="Refresh rows">
					<RefreshCw size={12} />
				</Button>
			</div>

			<div class="browser-body">
				{#key browseTable.name}
					<DataTable
						fetchPage={fetchRowsPage}
						rowKey={rowKeyFor}
						columns={rowsTableColumns}
						pageSize={perPage}
						emptyMessage="No rows."
					>
						{#snippet row(r: Cell[])}
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<tr class="data-row" onclick={() => openEdit(r)}>
								{#each r as cell}
									<td>
										{#if cell === null || cell === undefined}
											<span class="null-val">null</span>
										{:else}
											<span class="cell-val">{String(cell)}</span>
										{/if}
									</td>
								{/each}
								<td class="action-col">
									<span class="row-actions">
										<span class="edit-hint"><Edit2 size={10} /></span>
										<Button
											variant="ghost"
											size="icon"
											title="Delete row"
											aria-label="Delete row"
											onclick={(e) => confirmDeleteRow(r, e)}
										><Minus size={10} /></Button>
									</span>
								</td>
							</tr>
						{/snippet}
					</DataTable>
				{/key}
			</div>

			<div class="browser-footer">
				<span class="click-hint">Click row to edit · <Minus size={9} /> to delete</span>
			</div>
		</div>
		{:else}
		<div class="browser-empty-state">
			<TableProperties size={32} />
			<p>Select a table to browse its rows.</p>
		</div>
		{/if}

	</div>
	{/if}
</div>

<!-- ── Edit modal ── -->
<Modal
	bind:open={() => editRow !== null, (v) => { if (!v) closeEdit(); }}
	title="Edit row — {browseTable?.name ?? ''}"
>
	{#if editRow}
		{#if pkCol}
			<span class="pk-label">{pkCol.name} = {editRow[columns.findIndex(c => c.is_primary_key)]}</span>
		{/if}
		<div class="edit-body">
			{#each columns as col, i}
				<FormField
					label={col.name}
					for="edit-{col.name}"
					hint="{col.udt_name}{col.is_primary_key ? ' · primary key' : ''}"
				>
					{#if col.is_primary_key}
						<TextField id="edit-{col.name}" value={editRow[i] === null || editRow[i] === undefined ? 'null' : String(editRow[i])} readonly />
					{:else}
						<TextField
							id="edit-{col.name}"
							bind:value={editValues[col.name]}
							disabled={saving}
							placeholder={col.is_nullable ? 'null' : ''}
						/>
					{/if}
				</FormField>
			{/each}
		</div>
		{#if saveError}
			<div role="alert"><InlineAlert tone="error">{saveError}</InlineAlert></div>
		{/if}
		{#if saveSuccess}
			<div role="status"><InlineAlert tone="success"><Check size={13} /> Saved!</InlineAlert></div>
		{/if}
	{/if}
	{#snippet footer()}
		<Button variant="secondary" onclick={closeEdit} disabled={saving}>Cancel</Button>
		<Button variant="primary" onclick={saveEdit} disabled={saving}>
			{#if saving}<Spinner size={13} tone="current" />Saving…{:else}<CornerDownLeft size={13} />Save changes{/if}
		</Button>
	{/snippet}
</Modal>

<!-- ── Delete row confirm ── -->
<ConfirmDialog
	bind:open={() => deleteRowPk !== null, (v) => { if (!v) cancelDeleteRow(); }}
	title="Delete row"
	message="This action is permanent. Row with {pkCol?.name} = {deleteRowPk} in {browseTable?.name} will be deleted."
	confirmLabel="Delete row"
	error={deleteRowError}
	onConfirm={doDeleteRow}
/>

<!-- ── Drop table confirm ── -->
<ConfirmDialog
	bind:open={() => confirmTable !== null, (v) => { if (!v) closeConfirm(); }}
	title="Drop {confirmTable?.name ?? ''}"
	message="This action is permanent. All data in {confirmTable?.name} will be deleted and cannot be recovered."
	confirmLabel="Drop table"
	confirmText={confirmTable?.name}
	error={dropError}
	onConfirm={dropTable}
/>

{/if}

<style>
	.db-page { display: flex; flex-direction: column; gap: 16px; height: 100%; }

	/* Header */
	.page-header {
		display: flex; align-items: flex-start; justify-content: space-between; gap: 16px;
	}
	.header-title { display: flex; align-items: flex-start; gap: 12px; color: var(--text-muted); }
	.header-title h2 { font-size: 15px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.header-title p  { font-size: 13px; color: var(--text-muted); margin: 0; }
	.header-actions { display: flex; align-items: center; gap: 6px; }

	/* Connect state */
	.connect-state {
		flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 12px; color: var(--text-muted); text-align: center;
	}

	/* Layout */
	.layout {
		display: grid;
		grid-template-columns: 260px minmax(0, 1fr);
		gap: 12px;
		flex: 1;
		min-height: 0;
	}

	/* Table list panel */
	.table-panel {
		display: flex; flex-direction: column; gap: 8px;
		border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden;
		background: var(--bg-surface);
	}
	.table-search-row {
		display: flex; align-items: center; gap: 8px;
		padding: 10px 12px; border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.search-box { flex: 1; min-width: 0; }
	.table-count { font-size: 11px; color: var(--text-muted); white-space: nowrap; }

	.list-loading {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 8px; padding: 32px; color: var(--text-muted); font-size: 13px;
	}

	.table-list { overflow-y: auto; flex: 1; }
	.table-row {
		display: flex; align-items: center; gap: 4px; padding-right: 6px;
		border-bottom: 1px solid var(--border);
		transition: background var(--transition-fast);
	}
	.table-row:last-child { border-bottom: none; }
	.table-row:hover { background: var(--bg-hover); }
	.table-row.active { background: var(--accent-muted); }
	.trow-main {
		flex: 1; min-width: 0; display: grid; grid-template-columns: 1fr auto;
		align-items: center; gap: 8px; padding: 8px 6px 8px 12px;
		font-size: 12px; cursor: pointer; text-align: left;
		background: none; border: none; color: var(--text-primary); font-family: var(--font-sans);
	}
	.trow-name { font-family: var(--font-mono); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.trow-count { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }

	/* Browser panel */
	.browser-panel {
		display: flex; flex-direction: column;
		border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden;
		background: var(--bg-surface); min-height: 0; min-width: 0;
	}
	.browser-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 10px 14px; border-bottom: 1px solid var(--border);
		background: var(--bg-elevated); gap: 12px; flex-shrink: 0;
	}
	.browser-title { display: flex; align-items: center; gap: 8px; min-width: 0; }
	.tname { font-size: 13px; font-family: var(--font-mono); color: var(--text-primary); }
	.browser-body { flex: 1; min-height: 0; overflow: auto; }
	.browser-body :global(.ui-data-table) { border: none; border-radius: 0; }

	.data-row { cursor: pointer; }
	.data-row td { max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	/* Sticky so the delete control stays reachable when the table scrolls sideways on narrow screens. */
	.data-row .action-col { width: 48px; padding: 0 6px; position: sticky; right: 0; background: var(--bg-surface); }
	.data-row:hover .action-col { background: var(--bg-hover); }
	.data-row:hover .edit-hint { opacity: 1; }
	.null-val { color: var(--text-muted); font-style: italic; font-size: 11px; }
	.cell-val { color: var(--text-primary); }
	.row-actions { display: flex; align-items: center; gap: 4px; }
	.edit-hint { opacity: 0; color: var(--text-muted); display: flex; align-items: center; }

	.browser-footer {
		display: flex; align-items: center; justify-content: flex-end;
		padding: 8px 14px; border-top: 1px solid var(--border);
		font-size: 11px; color: var(--text-muted); flex-shrink: 0;
		background: var(--bg-elevated);
	}
	.click-hint { font-style: italic; display: flex; align-items: center; gap: 4px; }
	.browser-empty-state {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; color: var(--text-muted); font-size: 13px; min-height: 200px;
		border: 1px dashed var(--border); border-radius: var(--radius-lg);
	}

	/* Edit modal */
	.edit-body { overflow-y: auto; max-height: 55vh; display: flex; flex-direction: column; gap: 10px; }
	.pk-label { font-size: 11px; color: var(--text-muted); font-family: var(--font-mono); }

	@media (max-width: 760px) {
		.layout { grid-template-columns: minmax(0, 1fr); }
		.table-panel { max-height: 260px; }
	}
</style>
