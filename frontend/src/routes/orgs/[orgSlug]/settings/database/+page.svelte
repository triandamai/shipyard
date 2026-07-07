<script lang="ts">
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { isOwnerRole } from '$lib/auth/permissions';
	import {
		Database, RefreshCw, Trash2, AlertTriangle, X, Loader2,
		TableProperties, ChevronLeft, ChevronRight, Search, Edit2,
		Check, CornerDownLeft, Minus, PlugZap, Unplug
	} from '@lucide/svelte';
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
	let browseTable   = $state<DbTable | null>(null);
	let columns       = $state<ColMeta[]>([]);
	let rows          = $state<(string | number | boolean | null)[][]>([]);
	let totalRows     = $state(0);
	let browsePage    = $state(1);
	let perPage       = 50;
	let rowSearch     = $state('');
	let rowSearchDebounce: ReturnType<typeof setTimeout>;
	let loadingRows   = $state(false);
	let rowsError     = $state('');

	async function openBrowser(table: DbTable) {
		browseTable = table;
		columns = [];
		rows = [];
		rowSearch = '';
		browsePage = 1;
		await fetchRows();
	}

	function closeBrowser() {
		browseTable = null;
		columns = [];
		rows = [];
		rowSearch = '';
		browsePage = 1;
	}

	async function fetchRows() {
		if (!browseTable) return;
		loadingRows = true;
		rowsError = '';
		const params = new URLSearchParams({
			search: rowSearch.trim(),
			page: String(browsePage),
			per_page: String(perPage),
		});
		const res = await api.get<RowsResponse>(
			`/admin/db/tables/${encodeURIComponent(browseTable.name)}/rows?${params}`
		);
		if (res.error) {
			rowsError = res.error.message;
		} else if (res.data) {
			columns   = res.data.columns;
			rows      = res.data.rows;
			totalRows = res.data.total;
		}
		loadingRows = false;
	}

	function onRowSearchInput() {
		clearTimeout(rowSearchDebounce);
		rowSearchDebounce = setTimeout(() => { browsePage = 1; fetchRows(); }, 350);
	}

	function prevPage() { if (browsePage > 1) { browsePage--; fetchRows(); } }
	function nextPage() {
		if (browsePage < Math.ceil(totalRows / perPage)) { browsePage++; fetchRows(); }
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
			await fetchRows();
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

	async function doDeleteRow() {
		if (!browseTable || !deleteRowPk) return;
		deletingRow = true;
		deleteRowError = '';
		const res = await api.delete(
			`/admin/db/tables/${encodeURIComponent(browseTable.name)}/rows/${encodeURIComponent(deleteRowPk)}`
		);
		deletingRow = false;
		if (res.error) {
			deleteRowError = res.error.message;
		} else {
			deleteRowPk = null;
			await fetchRows();
		}
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

	async function dropTable() {
		if (!confirmTable || confirmInput !== confirmTable.name) return;
		dropping = true;
		dropError = '';
		const res = await api.delete(`/admin/db/tables/${encodeURIComponent(confirmTable.name)}`);
		if (res.error) { dropError = res.error.message; dropping = false; return; }
		if (browseTable?.name === confirmTable.name) closeBrowser();
		tables = tables.filter(t => t.name !== confirmTable!.name);
		dropping = false;
		confirmTable = null;
	}

	// ── Derived ────────────────────────────────────────────────────────────
	let totalPages = $derived(Math.max(1, Math.ceil(totalRows / perPage)));
	let pkCol      = $derived(columns.find(c => c.is_primary_key));
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
				<button class="btn btn-secondary btn-sm icon-btn" onclick={refreshTables} disabled={loadingTables}>
					<RefreshCw size={13} class={loadingTables ? 'spin' : ''} />
					Refresh
				</button>
				<button class="btn btn-ghost btn-sm icon-btn" onclick={disconnect} title="Disconnect">
					<Unplug size={13} />
					Disconnect
				</button>
			{/if}
		</div>
	</div>

	{#if tableError}
		<div class="alert alert-error"><AlertTriangle size={14} />{tableError}</div>
	{/if}

	<!-- ── Not connected ── -->
	{#if !isDbConnected}
		<div class="connect-state">
			<div class="connect-icon"><Database size={36} /></div>
			<h3>Not connected</h3>
			<p>Connect to browse and manage the platform database tables.</p>
			{#if connectError}
				<div class="connect-error"><AlertTriangle size={13} />{connectError}</div>
			{/if}
			<button class="btn btn-primary icon-btn connect-btn" onclick={connect} disabled={connecting}>
				{#if connecting}
					<Loader2 size={14} class="spin" />Connecting…
				{:else}
					<PlugZap size={14} />Connect to database
				{/if}
			</button>
		</div>
	{:else}

	<div class="layout" class:has-browser={browseTable !== null}>

		<!-- ── Left: table list ── -->
		<div class="table-panel">
			<div class="table-search-row">
				<div class="search-box">
					<Search size={12} />
					<input
						class="search-input"
						type="text"
						placeholder="Filter tables…"
						bind:value={tableSearch}
					/>
				</div>
				<span class="table-count">{filteredTables.length} table{filteredTables.length !== 1 ? 's' : ''}</span>
			</div>

			{#if loadingTables}
				<div class="list-loading"><Loader2 size={16} class="spin" /><span>Loading…</span></div>
			{:else if filteredTables.length === 0}
				<div class="list-empty"><TableProperties size={24} /><p>No tables.</p></div>
			{:else}
				<div class="table-list">
					{#each filteredTables as table (table.name)}
						<button
							class="table-row"
							class:active={browseTable?.name === table.name}
							onclick={() => openBrowser(table)}
						>
							<span class="trow-name">{table.name}</span>
							<span class="trow-count">{formatCount(table.row_count)}</span>
							<span class="trow-actions">
								<span
									class="drop-btn"
									title="Drop table"
									role="button"
									tabindex="0"
									onclick={(e) => openConfirm(table, e)}
									onkeydown={(e) => e.key === 'Enter' && openConfirm(table, e as unknown as MouseEvent)}
								>
									<Trash2 size={11} />
								</span>
							</span>
						</button>
					{/each}
				</div>
			{/if}
		</div>

		<!-- ── Right: table browser ── -->
		{#if browseTable}
		<div class="browser-panel">
			<div class="browser-header">
				<div class="browser-title">
					<button class="back-btn" onclick={closeBrowser} title="Back to list">
						<ChevronLeft size={14} />
					</button>
					<code class="tname">{browseTable.name}</code>
					<span class="row-badge">{totalRows.toLocaleString()} rows</span>
				</div>
				<div class="browser-controls">
					<div class="search-box">
						<Search size={12} />
						<input
							class="search-input"
							type="text"
							placeholder="Search all columns…"
							bind:value={rowSearch}
							oninput={onRowSearchInput}
						/>
					</div>
					<div class="pagination">
						<button class="pg-btn" onclick={prevPage} disabled={browsePage <= 1 || loadingRows}>
							<ChevronLeft size={13} />
						</button>
						<span class="pg-label">{browsePage} / {totalPages}</span>
						<button class="pg-btn" onclick={nextPage} disabled={browsePage >= totalPages || loadingRows}>
							<ChevronRight size={13} />
						</button>
					</div>
					<button class="btn btn-ghost btn-xs icon-btn" onclick={fetchRows} disabled={loadingRows}>
						<RefreshCw size={12} class={loadingRows ? 'spin' : ''} />
					</button>
				</div>
			</div>

			{#if rowsError}
				<div class="alert alert-error" style="margin:8px 12px"><AlertTriangle size={13} />{rowsError}</div>
			{/if}

			{#if loadingRows && rows.length === 0}
				<div class="browser-loading"><Loader2 size={18} class="spin" /><span>Loading rows…</span></div>
			{:else if rows.length === 0}
				<div class="browser-empty"><p>No rows{rowSearch ? ' matching your search' : ''}.</p></div>
			{:else}
				<div class="rows-wrap" class:loading={loadingRows}>
					<table class="rows-table">
						<thead>
							<tr>
								{#each columns as col}
									<th class:pk={col.is_primary_key} title="{col.data_type} · {col.is_nullable ? 'nullable' : 'not null'}">
										{col.name}
										{#if col.is_primary_key}<span class="pk-badge">PK</span>{/if}
									</th>
								{/each}
								<th class="action-col"></th>
							</tr>
						</thead>
						<tbody>
							{#each rows as row, ri (ri)}
								<tr onclick={() => openEdit(row)} class="data-row">
									{#each row as cell}
										<td class:null-cell={cell === null || cell === undefined}>
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
											<span
												class="del-row-btn"
												title="Delete row"
												role="button"
												tabindex="0"
												onclick={(e) => confirmDeleteRow(row, e)}
												onkeydown={(e) => e.key === 'Enter' && confirmDeleteRow(row, e as unknown as MouseEvent)}
											><Minus size={10} /></span>
										</span>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			{/if}

			<div class="browser-footer">
				<span>Showing {Math.min((browsePage - 1) * perPage + 1, totalRows)}–{Math.min(browsePage * perPage, totalRows)} of {totalRows.toLocaleString()}</span>
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
{#if editRow}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-backdrop" onclick={closeEdit} onkeydown={() => {}}></div>
	<div class="modal edit-modal" role="dialog" aria-modal="true">
		<div class="modal-header">
			<div class="modal-title">
				<Edit2 size={14} />
				<span>Edit row — <code>{browseTable?.name}</code></span>
				{#if pkCol}
					<span class="pk-label">{pkCol.name} = {editRow[columns.findIndex(c => c.is_primary_key)]}</span>
				{/if}
			</div>
			<button class="close-btn" onclick={closeEdit} disabled={saving}><X size={15} /></button>
		</div>
		<div class="modal-body edit-body">
			{#each columns as col, i}
				<div class="field-row" class:pk-field={col.is_primary_key}>
					<label class="field-label" for="edit-{col.name}">
						<span class="field-name">{col.name}</span>
						<span class="field-type">{col.udt_name}</span>
						{#if col.is_primary_key}<span class="pk-badge">PK</span>{/if}
					</label>
					{#if col.is_primary_key}
						<div class="field-value-ro">
							{#if editRow[i] === null || editRow[i] === undefined}
								<em class="null-val">null</em>
							{:else}
								{editRow[i]}
							{/if}
						</div>
					{:else}
						<input
							id="edit-{col.name}"
							class="field-input"
							type="text"
							bind:value={editValues[col.name]}
							disabled={saving}
							placeholder={col.is_nullable ? 'null' : ''}
						/>
					{/if}
				</div>
			{/each}
		</div>
		{#if saveError}
			<div class="modal-notice modal-notice-error"><AlertTriangle size={13} />{saveError}</div>
		{/if}
		{#if saveSuccess}
			<div class="modal-notice modal-notice-success"><Check size={13} />Saved!</div>
		{/if}
		<div class="modal-footer">
			<button class="btn btn-secondary" onclick={closeEdit} disabled={saving}>Cancel</button>
			<button class="btn btn-primary icon-btn" onclick={saveEdit} disabled={saving}>
				{#if saving}<Loader2 size={13} class="spin" />Saving…{:else}<CornerDownLeft size={13} />Save changes{/if}
			</button>
		</div>
	</div>
{/if}

<!-- ── Delete row confirm modal ── -->
{#if deleteRowPk !== null}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-backdrop" onclick={cancelDeleteRow} onkeydown={() => {}}></div>
	<div class="modal" role="dialog" aria-modal="true">
		<div class="modal-header">
			<div class="modal-title"><Trash2 size={14} /><span>Delete row</span></div>
			<button class="close-btn" onclick={cancelDeleteRow} disabled={deletingRow}><X size={15} /></button>
		</div>
		<div class="modal-body">
			<div class="danger-notice">
				<AlertTriangle size={16} />
				<div>
					<strong>This action is permanent.</strong>
					<p>Row with {pkCol?.name} = <code>{deleteRowPk}</code> in <code>{browseTable?.name}</code> will be deleted.</p>
				</div>
			</div>
			{#if deleteRowError}<p class="drop-error">{deleteRowError}</p>{/if}
		</div>
		<div class="modal-footer">
			<button class="btn btn-secondary" onclick={cancelDeleteRow} disabled={deletingRow}>Cancel</button>
			<button class="btn btn-danger icon-btn" onclick={doDeleteRow} disabled={deletingRow}>
				{#if deletingRow}<Loader2 size={13} class="spin" />Deleting…{:else}<Trash2 size={13} />Delete row{/if}
			</button>
		</div>
	</div>
{/if}

<!-- ── Drop table confirm modal ── -->
{#if confirmTable}
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-backdrop" onclick={closeConfirm} onkeydown={() => {}}></div>
	<div class="modal" role="dialog" aria-modal="true">
		<div class="modal-header">
			<div class="modal-title"><Trash2 size={16} /><span>Drop <code>{confirmTable.name}</code></span></div>
			<button class="close-btn" onclick={closeConfirm} disabled={dropping}><X size={15} /></button>
		</div>
		<div class="modal-body">
			<div class="danger-notice">
				<AlertTriangle size={16} />
				<div>
					<strong>This action is permanent.</strong>
					<p>All data in <code>{confirmTable.name}</code> will be deleted and cannot be recovered.</p>
				</div>
			</div>
			<p class="confirm-label">Type <strong>{confirmTable.name}</strong> to confirm:</p>
			<input class="confirm-input" type="text" placeholder={confirmTable.name}
				bind:value={confirmInput} disabled={dropping} autocomplete="off" spellcheck="false" />
			{#if dropError}<p class="drop-error">{dropError}</p>{/if}
		</div>
		<div class="modal-footer">
			<button class="btn btn-secondary" onclick={closeConfirm} disabled={dropping}>Cancel</button>
			<button class="btn btn-danger icon-btn" onclick={dropTable}
				disabled={dropping || confirmInput !== confirmTable.name}>
				{#if dropping}<Loader2 size={13} class="spin" />Dropping…{:else}<Trash2 size={13} />Drop table{/if}
			</button>
		</div>
	</div>
{/if}

{/if}

<style>
	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.db-page { display: flex; flex-direction: column; gap: 16px; height: 100%; }

	/* Header */
	.page-header {
		display: flex; align-items: flex-start; justify-content: space-between; gap: 16px;
	}
	.header-title { display: flex; align-items: flex-start; gap: 12px; color: var(--text-muted); }
	.header-title h2 { font-size: 15px; font-weight: 600; color: var(--text-primary); margin: 0 0 3px; }
	.header-title p  { font-size: 13px; color: var(--text-muted); margin: 0; }
	.header-actions { display: flex; align-items: center; gap: 6px; }
	.icon-btn { display: flex; align-items: center; gap: 6px; }

	.alert {
		display: flex; align-items: center; gap: 8px;
		padding: 10px 14px; border-radius: 6px; font-size: 13px;
	}
	.alert-error {
		background: var(--accent-red-muted); color: var(--accent-red);
		border: 1px solid color-mix(in srgb, var(--accent-red) 30%, transparent);
	}

	/* Connect state */
	.connect-state {
		flex: 1; display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 12px; color: var(--text-muted); text-align: center;
	}
	.connect-icon { color: var(--text-muted); opacity: 0.5; }
	.connect-state h3 { font-size: 15px; font-weight: 600; color: var(--text-primary); margin: 0; }
	.connect-state p  { font-size: 13px; margin: 0; max-width: 320px; }
	.connect-error {
		display: flex; align-items: center; gap: 6px; font-size: 12px;
		color: var(--accent-red); background: var(--accent-red-muted);
		border: 1px solid color-mix(in srgb, var(--accent-red) 25%, transparent);
		padding: 8px 12px; border-radius: 6px;
	}
	.connect-btn { margin-top: 4px; }

	/* Layout */
	.layout {
		display: grid;
		grid-template-columns: 260px 1fr;
		gap: 12px;
		flex: 1;
		min-height: 0;
	}

	/* Table list panel */
	.table-panel {
		display: flex; flex-direction: column; gap: 8px;
		border: 1px solid var(--border); border-radius: 8px; overflow: hidden;
		background: var(--bg-surface);
	}
	.table-search-row {
		display: flex; align-items: center; gap: 8px;
		padding: 10px 12px; border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
	}
	.table-count { font-size: 11px; color: var(--text-muted); white-space: nowrap; }

	.search-box {
		display: flex; align-items: center; gap: 6px; flex: 1;
		background: var(--bg-base); border: 1px solid var(--border);
		border-radius: 5px; padding: 4px 8px;
	}
	.search-box :global(svg) { color: var(--text-muted); flex-shrink: 0; }
	.search-input {
		border: none; background: none; outline: none; font-size: 12px;
		color: var(--text-primary); width: 100%; min-width: 0;
	}

	.list-loading, .list-empty {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 8px; padding: 32px; color: var(--text-muted); font-size: 13px;
	}

	.table-list { overflow-y: auto; flex: 1; }
	.table-row {
		display: grid; grid-template-columns: 1fr auto auto;
		align-items: center; padding: 8px 12px; gap: 8px;
		font-size: 12px; cursor: pointer; width: 100%; text-align: left;
		background: none; border: none; border-bottom: 1px solid var(--border);
		color: var(--text-primary); transition: background 0.1s;
	}
	.table-row:last-child { border-bottom: none; }
	.table-row:hover { background: var(--bg-hover); }
	.table-row.active { background: var(--accent-muted); color: var(--text-primary); }
	.trow-name { font-family: var(--font-mono); font-size: 12px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.trow-count { font-size: 11px; color: var(--text-muted); font-variant-numeric: tabular-nums; }
	.trow-actions { display: flex; align-items: center; }
	.drop-btn {
		display: flex; align-items: center; padding: 3px;
		border-radius: 4px; color: var(--text-muted); cursor: pointer;
		transition: color 0.15s, background 0.15s;
	}
	.drop-btn:hover { color: var(--accent-red); background: var(--accent-red-muted); }

	/* Browser panel */
	.browser-panel {
		display: flex; flex-direction: column;
		border: 1px solid var(--border); border-radius: 8px; overflow: hidden;
		background: var(--bg-surface); min-height: 0;
	}
	.browser-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 10px 14px; border-bottom: 1px solid var(--border);
		background: var(--bg-elevated); gap: 12px; flex-shrink: 0;
	}
	.browser-title { display: flex; align-items: center; gap: 8px; min-width: 0; }
	.back-btn {
		display: flex; align-items: center; background: none; border: none;
		cursor: pointer; color: var(--text-muted); padding: 3px; border-radius: 4px;
	}
	.back-btn:hover { color: var(--text-primary); background: var(--bg-hover); }
	.tname { font-size: 13px; font-family: var(--font-mono); color: var(--text-primary); }
	.row-badge {
		font-size: 11px; color: var(--text-muted);
		background: var(--bg-base); border: 1px solid var(--border);
		padding: 1px 6px; border-radius: 10px;
	}
	.browser-controls { display: flex; align-items: center; gap: 8px; }
	.pagination { display: flex; align-items: center; gap: 4px; }
	.pg-btn {
		display: flex; align-items: center; background: none;
		border: 1px solid var(--border); border-radius: 4px;
		cursor: pointer; padding: 3px 6px; color: var(--text-muted);
	}
	.pg-btn:hover:not(:disabled) { color: var(--text-primary); background: var(--bg-hover); }
	.pg-btn:disabled { opacity: 0.4; cursor: not-allowed; }
	.pg-label { font-size: 12px; color: var(--text-muted); min-width: 48px; text-align: center; }

	.browser-loading, .browser-empty {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 8px; padding: 48px; color: var(--text-muted); font-size: 13px; flex: 1;
	}

	.rows-wrap { flex: 1; overflow: auto; min-height: 0; }
	.rows-wrap.loading { opacity: 0.6; pointer-events: none; }
	.rows-table { width: 100%; border-collapse: collapse; font-size: 12px; table-layout: auto; }
	.rows-table thead { position: sticky; top: 0; z-index: 1; }
	.rows-table th {
		background: var(--bg-elevated); border-bottom: 2px solid var(--border);
		padding: 7px 10px; text-align: left; font-size: 11px; font-weight: 600;
		color: var(--text-muted); white-space: nowrap;
		text-transform: uppercase; letter-spacing: 0.04em;
	}
	.rows-table th.pk { color: var(--accent); }
	.rows-table .action-col { width: 48px; padding: 0 6px; }
	.pk-badge {
		display: inline-block; font-size: 9px; font-weight: 700;
		color: var(--accent); background: var(--accent-muted);
		border-radius: 3px; padding: 0 4px; margin-left: 4px; vertical-align: middle;
	}
	.data-row { cursor: pointer; }
	.data-row:hover { background: var(--bg-hover); }
	.data-row:hover .edit-hint,
	.data-row:hover .del-row-btn { opacity: 1; }
	.rows-table td {
		padding: 6px 10px; border-bottom: 1px solid var(--border);
		max-width: 260px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
		vertical-align: middle;
	}
	.null-val { color: var(--text-muted); font-style: italic; font-size: 11px; }
	.cell-val { color: var(--text-primary); }
	.row-actions { display: flex; align-items: center; gap: 4px; }
	.edit-hint { opacity: 0; color: var(--text-muted); display: flex; align-items: center; }
	.del-row-btn {
		opacity: 0; display: flex; align-items: center; padding: 2px;
		border-radius: 3px; color: var(--text-muted); cursor: pointer;
		transition: color 0.15s, background 0.15s;
	}
	.del-row-btn:hover { color: var(--accent-red); background: var(--accent-red-muted); }

	.browser-footer {
		display: flex; align-items: center; justify-content: space-between;
		padding: 8px 14px; border-top: 1px solid var(--border);
		font-size: 11px; color: var(--text-muted); flex-shrink: 0;
		background: var(--bg-elevated);
	}
	.click-hint { font-style: italic; display: flex; align-items: center; gap: 4px; }
	.browser-empty-state {
		display: flex; flex-direction: column; align-items: center; justify-content: center;
		gap: 10px; color: var(--text-muted); font-size: 13px;
		border: 1px dashed var(--border); border-radius: 8px;
	}

	/* Edit modal */
	.edit-modal { width: min(580px, calc(100vw - 32px)); max-height: 80vh; }
	.edit-body { overflow-y: auto; max-height: 60vh; display: flex; flex-direction: column; gap: 8px; }
	.field-row {
		display: grid; grid-template-columns: 180px 1fr; align-items: center;
		gap: 8px; padding: 6px 0; border-bottom: 1px solid var(--border);
	}
	.field-row:last-child { border-bottom: none; }
	.pk-field { background: var(--bg-elevated); border-radius: 4px; padding: 6px 4px; }
	.field-label { display: flex; align-items: center; gap: 6px; min-width: 0; }
	.field-name { font-size: 12px; font-weight: 600; font-family: var(--font-mono); color: var(--text-primary); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.field-type { font-size: 10px; color: var(--text-muted); flex-shrink: 0; }
	.field-value-ro { font-size: 12px; font-family: var(--font-mono); color: var(--text-muted); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.field-input {
		width: 100%; padding: 5px 8px; font-size: 12px; font-family: var(--font-mono);
		border: 1px solid var(--border); border-radius: 5px;
		background: var(--bg-base); color: var(--text-primary); outline: none;
		box-sizing: border-box;
	}
	.field-input:focus { border-color: var(--accent); box-shadow: 0 0 0 2px var(--accent-muted); }
	.modal-notice {
		display: flex; align-items: center; gap: 8px;
		margin: 0 20px; padding: 8px 12px; border-radius: 6px; font-size: 12px;
	}
	.modal-notice-error {
		background: var(--accent-red-muted);
		border: 1px solid color-mix(in srgb, var(--accent-red) 25%, transparent);
		color: var(--accent-red);
	}
	.modal-notice-success {
		background: var(--accent-green-muted);
		border: 1px solid color-mix(in srgb, var(--accent-green) 25%, transparent);
		color: var(--accent-green);
	}
	.pk-label { font-size: 11px; color: var(--text-muted); font-family: var(--font-mono); }

	/* Shared modal */
	.modal-backdrop { position: fixed; inset: 0; background: rgba(0,0,0,0.55); z-index: 100; }
	.modal {
		position: fixed; top: 50%; left: 50%; transform: translate(-50%, -50%);
		width: min(480px, calc(100vw - 32px));
		background: var(--bg-surface); border: 1px solid var(--border);
		border-radius: 10px; box-shadow: 0 20px 60px rgba(0,0,0,0.3);
		z-index: 101; display: flex; flex-direction: column;
	}
	.modal-header {
		display: flex; align-items: center; justify-content: space-between;
		padding: 14px 18px; border-bottom: 1px solid var(--border);
	}
	.modal-title {
		display: flex; align-items: center; gap: 8px;
		font-size: 14px; font-weight: 600; color: var(--text-primary);
	}
	.modal-title code { font-family: var(--font-mono); font-size: 13px; color: var(--accent-red); }
	.close-btn {
		background: none; border: none; color: var(--text-muted); cursor: pointer;
		padding: 4px; border-radius: 4px; display: flex; align-items: center;
	}
	.close-btn:hover:not(:disabled) { color: var(--text-primary); }
	.modal-body { padding: 16px 20px; display: flex; flex-direction: column; gap: 14px; }
	.modal-footer { display: flex; justify-content: flex-end; gap: 8px; padding: 14px 18px; border-top: 1px solid var(--border); }
	.danger-notice {
		display: flex; gap: 12px; padding: 14px;
		background: var(--accent-red-muted);
		border: 1px solid color-mix(in srgb, var(--accent-red) 25%, transparent);
		border-radius: 6px; color: var(--accent-red); font-size: 13px;
	}
	.danger-notice :global(svg) { flex-shrink: 0; margin-top: 1px; }
	.danger-notice strong { display: block; margin-bottom: 4px; }
	.danger-notice p { margin: 0; opacity: 0.85; }
	.danger-notice code { font-family: var(--font-mono); font-size: 12px; }
	.confirm-label { font-size: 13px; color: var(--text-muted); margin: 0; }
	.confirm-input {
		width: 100%; padding: 8px 12px; font-size: 13px; font-family: var(--font-mono);
		border: 1px solid var(--border); border-radius: 6px;
		background: var(--bg-base); color: var(--text-primary);
		box-sizing: border-box; outline: none;
	}
	.confirm-input:focus { border-color: var(--accent-red); box-shadow: 0 0 0 2px var(--accent-red-muted); }
	.drop-error { font-size: 13px; color: var(--accent-red); margin: 0; }
	.btn-danger {
		background: var(--accent-red); border: 1px solid var(--accent-red); color: #fff;
		display: flex; align-items: center; gap: 6px;
	}
	.btn-danger:hover:not(:disabled) { opacity: 0.88; }
	.btn-danger:disabled { opacity: 0.5; cursor: not-allowed; }
</style>
