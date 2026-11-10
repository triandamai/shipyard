<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Table2, KeyRound, RefreshCw, Trash2, Layers } from '@lucide/svelte';
	import {
		SearchInput,
		ActivityList,
		ListRow,
		DataTable,
		Modal,
		ConfirmDialog,
		Button,
		Badge,
		FormField,
		TextField,
		Select,
		InlineAlert,
		EmptyState,
		Skeleton
	} from '$lib/components/ui';

	interface ColMeta {
		name: string;
		data_type: string;
		udt_name: string;
		is_nullable: boolean;
		is_primary_key: boolean;
	}
	interface RowsResponse {
		columns: ColMeta[];
		rows: (string | null)[][];
		total: number;
		page: number;
		per_page: number;
	}

	type DbType = 'postgres' | 'redis';
	let dbType = $state<DbType>('postgres');

	let tables = $state<string[]>([]);
	let tablesLoading = $state(true);
	let tablesError = $state('');

	let selected = $state<string | null>(null);
	let activeTab = $state<'columns' | 'rows'>('columns');

	// Columns sub-tab (client-mode DataTable, small fixed-per-table dataset).
	let columns = $state<ColMeta[]>([]);
	let colLoading = $state(false);
	let colError = $state('');

	// Rows sub-tab (server-mode DataTable). `rowsColumns`/`totalRows` are
	// captured from each `fetchRowsPage` response since DataTable's server
	// mode doesn't expose its fetched data back to the parent page (same
	// approach as Task 42's `totalProjects` capture).
	let rowsColumns = $state<ColMeta[]>([]);
	let totalRows = $state(0);
	// Bumped to force the Rows DataTable to remount (and refetch) after an
	// edit/delete mutation, since DataTable only re-fetches when its own
	// page/pageSize/search state changes — the `{#key}` remount technique is
	// the established workaround for an external forced refresh (Task 45 /
	// the `deployments` and `payments` pages).
	let rowsNonce = $state(0);

	let pkColIdx = $derived(rowsColumns.findIndex((c) => c.is_primary_key));
	let rowsTableColumns = $derived([
		...rowsColumns.map((c) => ({ key: c.name, label: c.name })),
		{ key: '__actions', label: '' }
	]);

	function rowKeyFor(r: (string | null)[]): string {
		return pkColIdx >= 0 ? String(r[pkColIdx] ?? '') : JSON.stringify(r);
	}

	// Redis state
	interface RedisInfo {
		key: string;
		value: string;
	}
	let redisInfo = $state<RedisInfo[]>([]);
	let redisLoading = $state(false);
	let redisError = $state('');

	async function loadRedisInfo() {
		redisLoading = true;
		redisError = '';
		const r = await api.get<RedisInfo[]>('/admin/redis/info');
		if (r.data) redisInfo = Array.isArray(r.data) ? r.data : [];
		else redisError = r.error?.message ?? 'Failed to load Redis info';
		redisLoading = false;
	}

	let search = $state('');
	let deleteTarget = $state<string | null>(null);

	let editModalOpen = $state(false);
	let editRow = $state<(string | null)[] | null>(null);
	let editDraft = $state<Record<string, string>>({});
	let saving = $state(false);
	let saveErr = $state('');

	let dropConfirmOpen = $state(false);

	interface TableItem {
		name: string;
		row_count: number;
		is_partition?: boolean;
		parent_table?: string;
	}

	let tableItems = $state<TableItem[]>([]);
	let hidePartitions = $state(true);

	let partitionCount = $derived(tableItems.filter((t) => t.is_partition).length);
	let parentPartitionCounts = $derived(
		tableItems.reduce((acc, t) => {
			if (t.parent_table) acc[t.parent_table] = (acc[t.parent_table] || 0) + 1;
			return acc;
		}, {} as Record<string, number>)
	);

	let visibleTables = $derived(
		tableItems.filter((t) => !hidePartitions || !t.is_partition)
	);

	let filteredTables = $derived(
		search.trim()
			? visibleTables.filter((t) => t.name.toLowerCase().includes(search.toLowerCase()))
			: visibleTables
	);

	function isLogTable(name: string | null): boolean {
		return !!name && (name === 'log_entries' || name.startsWith('log_entries_'));
	}

	// Clear logs modal state
	let clearLogsModalOpen = $state(false);
	let clearPreset = $state('older_than_7d');
	let customFrom = $state('');
	let customTo = $state('');
	let clearingLogs = $state(false);
	let clearError = $state('');
	let clearSuccess = $state('');

	async function executeClearLogs() {
		clearingLogs = true;
		clearError = '';
		clearSuccess = '';
		try {
			let payload: Record<string, any> = { preset: clearPreset };
			if (clearPreset === 'custom') {
				payload = {
					preset: 'custom',
					from: customFrom ? new Date(customFrom).toISOString() : undefined,
					to: customTo ? new Date(customTo).toISOString() : undefined
				};
			}
			const res = await api.post<{ partitions_dropped: number; rows_deleted: number; message: string }>(
				'/admin/db/logs/clear',
				payload
			);
			if (res.error) {
				clearError = res.error.message;
				return;
			}
			clearSuccess = res.data?.message || 'Logs successfully cleared.';
			await loadTables();
			if (isLogTable(selected)) {
				rowsNonce++;
				if (selected !== 'log_entries' && !tableItems.some((t) => t.name === selected)) {
					selected = 'log_entries';
					await loadColumns('log_entries');
				}
			}
			setTimeout(() => {
				if (!clearError) clearLogsModalOpen = false;
			}, 1500);
		} catch (e) {
			clearError = e instanceof Error ? e.message : 'Failed to clear logs.';
		} finally {
			clearingLogs = false;
		}
	}

	async function loadTables() {
		tablesLoading = true;
		tablesError = '';
		const r = await api.get<TableItem[]>('/admin/db/tables');
		if (r.data) {
			tableItems = Array.isArray(r.data)
				? r.data.map((t: any) => ({
						name: typeof t === 'string' ? t : (t.name ?? String(t)),
						row_count: typeof t === 'object' && t.row_count != null ? t.row_count : 0,
						is_partition: !!t.is_partition,
						parent_table: t.parent_table
				  }))
				: [];
			tables = tableItems.map((t) => t.name);
		} else {
			tablesError = r.error?.message ?? 'Failed to load tables';
		}
		tablesLoading = false;
	}

	async function selectTable(name: string) {
		selected = name;
		columns = [];
		colError = '';
		rowsColumns = [];
		totalRows = 0;
		activeTab = 'columns';
		await loadColumns(name);
	}

	async function loadColumns(name: string) {
		colLoading = true;
		colError = '';
		const r = await api.get<ColMeta[]>(`/admin/db/tables/${encodeURIComponent(name)}/columns`);
		if (r.data) columns = r.data;
		else colError = r.error?.message ?? 'Failed to load columns';
		colLoading = false;
	}

	// Server-mode `fetchPage` for the Rows DataTable. The backend
	// (`GET /admin/db/tables/:name/rows`) uses `page` (1-indexed, default 1)
	// and `per_page`, and filters via `search` (not `q`) — confirmed by
	// reading `TableRowsQuery`/`list_table_rows` in
	// `backend/crates/api/src/settings/mod.rs`. It already returns `total`
	// (a real `COUNT(*)` with the identical WHERE clause), so no backend fix
	// was needed here, unlike Tasks 45/46.
	async function fetchRowsPage(params: { page: number; pageSize: number; search: string }) {
		if (!selected) return { rows: [], total: 0 };
		const qs = new URLSearchParams({
			page: String(params.page + 1),
			per_page: String(params.pageSize),
			search: params.search
		});
		const res = await api.get<RowsResponse>(
			`/admin/db/tables/${encodeURIComponent(selected)}/rows?${qs}`
		);
		if (!res.data) throw new Error(res.error?.message ?? 'Failed to load rows');
		rowsColumns = res.data.columns;
		totalRows = res.data.total;
		return { rows: res.data.rows, total: res.data.total };
	}

	async function dropTable() {
		if (!deleteTarget) return;
		try {
			const r = await api.delete(`/admin/db/tables/${encodeURIComponent(deleteTarget)}`);
			if (!r.error) {
				tables = tables.filter((t) => t !== deleteTarget);
				if (selected === deleteTarget) {
					selected = null;
					columns = [];
					rowsColumns = [];
					totalRows = 0;
				}
			}
		} finally {
			deleteTarget = null;
		}
	}

	function startEdit(row: (string | null)[]) {
		editRow = row;
		// Every cell comes back from the backend already CAST to TEXT, and
		// every field here is a plain TextField (no `type="number"`) — so
		// `editDraft` never holds a real JS `number`, sidestepping the
		// Svelte 5 `<input type="number"> bind:value` auto-coercion footgun
		// entirely rather than needing to guard against it. The backend's
		// `update_table_row` already accepts/stringifies arbitrary
		// `serde_json::Value` updates regardless.
		editDraft = Object.fromEntries(rowsColumns.map((col, i) => [col.name, row[i] ?? '']));
		saveErr = '';
		editModalOpen = true;
	}

	async function saveEdit() {
		if (!editRow || !selected || pkColIdx < 0) return;
		saving = true;
		saveErr = '';
		try {
			const pkVal = String(editRow[pkColIdx] ?? '');
			const r = await api.patch(
				`/admin/db/tables/${encodeURIComponent(selected)}/rows/${encodeURIComponent(pkVal)}`,
				{ updates: editDraft }
			);
			if (r.error) {
				saveErr = r.error.message;
				return;
			}
			editModalOpen = false;
			editRow = null;
			rowsNonce++;
		} catch (e) {
			saveErr = e instanceof Error ? e.message : 'Failed to save row.';
		} finally {
			saving = false;
		}
	}

	async function deleteRow(row: (string | null)[]) {
		if (!selected || pkColIdx < 0) return;
		const pkVal = String(row[pkColIdx] ?? '');
		try {
			await api.delete(
				`/admin/db/tables/${encodeURIComponent(selected)}/rows/${encodeURIComponent(pkVal)}`
			);
		} finally {
			rowsNonce++;
		}
	}

	function cellVal(v: string | null | undefined): string {
		if (v === null || v === undefined) return '—';
		return v;
	}

	onMount(() => {
		loadTables();
		loadRedisInfo();
	});
</script>

<Modal bind:open={editModalOpen} title="Edit Row">
	{#if editRow}
		{#each rowsColumns as col (col.name)}
			<FormField label={col.name} for="edit-{col.name}">
				<TextField id="edit-{col.name}" bind:value={editDraft[col.name]} />
			</FormField>
		{/each}
		{#if saveErr}<InlineAlert tone="error">{saveErr}</InlineAlert>{/if}
	{/if}
	{#snippet footer()}
		<Button variant="ghost" onclick={() => (editModalOpen = false)}>Cancel</Button>
		<Button variant="primary" disabled={saving} onclick={saveEdit}>
			{saving ? 'Saving…' : 'Save'}
		</Button>
	{/snippet}
</Modal>

<ConfirmDialog
	bind:open={dropConfirmOpen}
	title={deleteTarget ? `Drop table "${deleteTarget}"?` : 'Drop table'}
	message={deleteTarget
		? `This will permanently delete the "${deleteTarget}" table and all of its data. This is irreversible.`
		: ''}
	confirmLabel="Drop Table"
	onConfirm={dropTable}
/>

<Modal bind:open={clearLogsModalOpen} title="Clear Observability Logs">
	<div class="clear-logs-body">
		<p class="clear-logs-desc">
			Clear observability log entries and permanently drop expired daily partition tables (<span class="db-mono">log_entries_y*</span>) to reclaim disk space.
		</p>

		<FormField label="Range Preset" for="log-preset">
			<Select
				id="log-preset"
				bind:value={clearPreset}
				options={[
					{ value: 'older_than_24h', label: 'Older than 24 hours' },
					{ value: 'older_than_7d', label: 'Older than 7 days (Recommended)' },
					{ value: 'older_than_14d', label: 'Older than 14 days' },
					{ value: 'older_than_30d', label: 'Older than 30 days' },
					{ value: 'older_than_90d', label: 'Older than 90 days' },
					{ value: 'custom', label: 'Custom Date Range…' },
					{ value: 'all', label: 'All logs (Drop all daily partitions)' }
				]}
			/>
		</FormField>

		{#if clearPreset === 'custom'}
			<div class="clear-logs-custom-range">
				<FormField label="From (UTC)" for="log-from">
					<TextField id="log-from" type="datetime-local" bind:value={customFrom} />
				</FormField>
				<FormField label="To (UTC)" for="log-to">
					<TextField id="log-to" type="datetime-local" bind:value={customTo} />
				</FormField>
			</div>
		{/if}

		<InlineAlert tone={clearPreset === 'all' ? 'error' : 'info'}>
			{#if clearPreset === 'all'}
				Warning: All existing daily log partition tables and records will be dropped. Current and tomorrow's partitions will be recreated automatically for ongoing logging.
			{:else if clearPreset === 'custom'}
				Partitions fully within this date range will be dropped instantaneously. Partial days and default partition rows will be deleted.
			{:else}
				Daily partition tables older than the selected threshold will be dropped, freeing disk space immediately without table bloat.
			{/if}
		</InlineAlert>

		{#if clearError}
			<InlineAlert tone="error">{clearError}</InlineAlert>
		{/if}
		{#if clearSuccess}
			<InlineAlert tone="success">{clearSuccess}</InlineAlert>
		{/if}
	</div>
	{#snippet footer()}
		<Button variant="ghost" onclick={() => (clearLogsModalOpen = false)}>Cancel</Button>
		<Button variant="danger" disabled={clearingLogs} onclick={executeClearLogs}>
			{clearingLogs ? 'Clearing…' : 'Clear Logs'}
		</Button>
	{/snippet}
</Modal>

<div class="db-page">
	<header class="db-hdr">
		<h1 class="db-ttl">Database</h1>
		<p class="db-sub">Inspect and manage system database tables and Redis.</p>
	</header>

	<div class="tabs">
		<button class="tab" class:active={dbType === 'postgres'} onclick={() => (dbType = 'postgres')}>
			PostgreSQL
		</button>
		<button class="tab" class:active={dbType === 'redis'} onclick={() => (dbType = 'redis')}>
			Redis
		</button>
	</div>

	{#if dbType === 'redis'}
		<div class="db-redis-toolbar">
			<Button variant="secondary" size="icon" onclick={loadRedisInfo}>
				<RefreshCw size={13} />
			</Button>
		</div>
		{#if redisLoading}
			<div class="db-redis-skeleton">
				{#each Array(6) as _}<Skeleton variant="row" />{/each}
			</div>
		{:else if redisError}
			<InlineAlert tone="error">{redisError}</InlineAlert>
		{:else if redisInfo.length === 0}
			<EmptyState
				message="No Redis info available."
				sub="Make sure the backend can connect to Redis."
			/>
		{:else}
			<ActivityList>
				{#each redisInfo as item (item.key)}
					<ListRow iconTone="red" title={item.key} meta={item.value}>
						{#snippet icon()}<KeyRound size={13} />{/snippet}
					</ListRow>
				{/each}
			</ActivityList>
		{/if}
	{:else}
		<div class="db-shell">
			<!-- Table list -->
			<div class="db-tlist">
				<div class="db-tlist-hdr">
					<div class="db-tlist-hdr-left">
						<span class="db-tlist-title">Tables</span>
						<Badge tone="neutral">{visibleTables.length}</Badge>
						{#if partitionCount > 0}
							<button
								type="button"
								class="db-part-btn"
								class:active={!hidePartitions}
								onclick={() => (hidePartitions = !hidePartitions)}
								title={hidePartitions ? `Show ${partitionCount} partition tables` : 'Group partition tables'}
							>
								<Layers size={11} />
								<span>{hidePartitions ? `+${partitionCount} parts` : 'Hide parts'}</span>
							</button>
						{/if}
					</div>
					<Button variant="ghost" size="sm" onclick={() => (clearLogsModalOpen = true)} title="Clear logs with range">
						<Trash2 size={11} />
						Clear Logs
					</Button>
				</div>
				<div class="db-tlist-search">
					<SearchInput bind:value={search} placeholder="Filter tables…" />
				</div>
				<div class="db-tlist-body">
					{#if tablesLoading}
						<div class="db-tlist-skeleton">
							{#each Array(6) as _}<Skeleton variant="row" height="30px" />{/each}
						</div>
					{:else if tablesError}
						<InlineAlert tone="error">{tablesError}</InlineAlert>
					{:else if filteredTables.length === 0}
						<EmptyState message="No tables found." />
					{:else}
						<ActivityList>
							{#each filteredTables as t (t.name)}
								<button
									type="button"
									class={selected === t.name ? 'db-tlist-item db-tlist-item-sel' : 'db-tlist-item'}
									onclick={() => selectTable(t.name)}
								>
									<ListRow iconTone={t.is_partition ? 'yellow' : 'blue'} title={t.name}>
										{#snippet icon()}<Table2 size={13} />{/snippet}
										{#snippet trailing()}
											{#if parentPartitionCounts[t.name]}
												<Badge tone="neutral">{parentPartitionCounts[t.name]} parts</Badge>
											{:else if t.is_partition}
												<Badge tone="yellow">part</Badge>
											{/if}
										{/snippet}
									</ListRow>
								</button>
							{/each}
						</ActivityList>
					{/if}
				</div>
			</div>

			<!-- Detail panel -->
			<div class="db-detail">
				{#if !selected}
					<div class="db-detail-placeholder">Select a table to inspect</div>
				{:else}
					<div class="db-detail-hdr">
						<div class="db-detail-hdr-left">
							<span class="db-detail-table db-mono">{selected}</span>
							{#if isLogTable(selected)}
								<Badge tone="blue">Logs</Badge>
							{/if}
						</div>
						<div class="db-detail-hdr-actions">
							{#if isLogTable(selected)}
								<Button
									variant="secondary"
									size="sm"
									onclick={() => (clearLogsModalOpen = true)}
								>
									<Trash2 size={12} />
									Clear Logs
								</Button>
							{/if}
							<Button
								variant="danger-outline"
								size="sm"
								onclick={() => {
									deleteTarget = selected;
									dropConfirmOpen = true;
								}}
							>
								<Trash2 size={12} />
								Drop
							</Button>
						</div>
					</div>

					<div class="tabs db-subtabs">
						<button class="tab" class:active={activeTab === 'columns'} onclick={() => (activeTab = 'columns')}>
							Columns
						</button>
						<button class="tab" class:active={activeTab === 'rows'} onclick={() => (activeTab = 'rows')}>
							Rows
						</button>
					</div>

					{#if activeTab === 'columns'}
						{#if colError}
							<InlineAlert tone="error">{colError}</InlineAlert>
						{:else}
							<DataTable
								items={columns}
								rowKey={(c) => c.name}
								searchable={false}
								columns={[
									{ key: 'name', label: 'Column' },
									{ key: 'type', label: 'Type' },
									{ key: 'nullable', label: 'Nullable' },
									{ key: 'pk', label: 'PK' }
								]}
								emptyMessage={colLoading ? 'Loading…' : 'No columns.'}
							>
								{#snippet row(c)}
									<tr>
										<td class="db-mono">{c.name}</td>
										<td class="db-mono db-dim">{c.udt_name || c.data_type}</td>
										<td>
											{#if c.is_nullable}
												<Badge tone="yellow">nullable</Badge>
											{:else}
												<Badge tone="green">required</Badge>
											{/if}
										</td>
										<td>
											{#if c.is_primary_key}<Badge tone="blue">PK</Badge>{/if}
										</td>
									</tr>
								{/snippet}
							</DataTable>
						{/if}
					{:else}
						<div class="db-rows-meta">
							{totalRows} row{totalRows !== 1 ? 's' : ''} total
							{#if pkColIdx >= 0}
								&bull; PK: <span class="db-mono">{rowsColumns[pkColIdx].name}</span>
							{/if}
						</div>
						{#key selected + '-' + rowsNonce}
							<DataTable
								fetchPage={fetchRowsPage}
								rowKey={rowKeyFor}
								columns={rowsTableColumns}
								emptyMessage="Table is empty."
							>
								{#snippet row(r)}
									<tr>
										{#each r as cell}
											<td class="db-mono db-cell-trunc">{cellVal(cell)}</td>
										{/each}
										<td class="db-row-actions">
											<Button variant="ghost" size="sm" onclick={() => startEdit(r)}>Edit</Button>
											<Button variant="danger-outline" size="sm" onclick={() => deleteRow(r)}>
												Delete
											</Button>
										</td>
									</tr>
								{/snippet}
							</DataTable>
						{/key}
					{/if}
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.db-page { max-width: 1100px; margin: 0 auto; padding: 40px 36px; width: 100%; min-width: 0; }
	.db-hdr { margin-bottom: 16px; }
	.db-ttl { font-size: 20px; font-weight: 700; color: var(--text-primary); margin: 0 0 4px; letter-spacing: -0.02em; }
	.db-sub { font-size: 12.5px; color: var(--text-muted); margin: 0; }

	/* Tab-bar pattern — copied verbatim from the design-system memory's
	   "Detail Panel Anatomy" section, reused for both the Postgres/Redis
	   switcher and the Columns/Rows sub-tabs. */
	.tabs { display: flex; gap: 2px; border-bottom: 1px solid var(--border); margin-bottom: 16px; overflow-x: auto; scrollbar-width: none; }
	.tab { padding: 7px 12px; font-size: 12px; font-weight: 500; color: var(--text-muted); background: none; border: none; border-bottom: 2px solid transparent; cursor: pointer; margin-bottom: -1px; white-space: nowrap; font-family: var(--font-sans); }
	.tab:hover { color: var(--text-primary); }
	.tab.active { color: var(--accent); border-bottom-color: var(--accent); }

	.db-subtabs { margin: 14px 0 12px; }

	.db-redis-toolbar { display: flex; justify-content: flex-end; margin-bottom: 10px; }
	.db-redis-skeleton { display: flex; flex-direction: column; gap: 8px; }

	.db-shell { display: grid; grid-template-columns: 240px 1fr; gap: 16px; align-items: start; }

	.clear-logs-body { display: flex; flex-direction: column; gap: 14px; }
	.clear-logs-desc { font-size: 13px; color: var(--text-muted); margin: 0; line-height: 1.5; }
	.clear-logs-custom-range { display: grid; grid-template-columns: 1fr 1fr; gap: 12px; }

	/* Table list */
	.db-tlist { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-lg); overflow: hidden; display: flex; flex-direction: column; max-height: 640px; }
	.db-tlist-hdr { display: flex; align-items: center; justify-content: space-between; padding: 10px 14px; border-bottom: 1px solid var(--border); background: var(--bg-elevated); gap: 6px; }
	.db-tlist-hdr-left { display: flex; align-items: center; gap: 6px; }
	.db-tlist-title { font-size: 10.5px; font-weight: 700; color: var(--text-dim); }
	.db-part-btn {
		display: inline-flex;
		align-items: center;
		gap: 3px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		padding: 2px 6px;
		font-size: 10px;
		font-weight: 500;
		color: var(--text-muted);
		cursor: pointer;
		font-family: var(--font-sans);
		transition: all var(--transition-fast);
	}
	.db-part-btn:hover { color: var(--text-primary); border-color: var(--border-hover); }
	.db-part-btn.active { background: var(--accent-subtle); color: var(--accent); border-color: var(--accent); }

	.db-tlist-search { padding: 8px; border-bottom: 1px solid var(--border); }
	.db-tlist-body { overflow-y: auto; flex: 1; padding: 6px 10px; }
	.db-tlist-skeleton { display: flex; flex-direction: column; gap: 6px; padding: 4px 2px; }
	.db-tlist-item { display: block; width: 100%; background: none; border: none; padding: 0; cursor: pointer; text-align: left; font-family: var(--font-sans); border-radius: var(--radius-sm); }
	.db-tlist-item :global(.ui-list-row-title) { font-family: var(--font-mono); }
	.db-tlist-item-sel :global(.ui-list-row-title) { color: var(--accent); }
	.db-tlist-item:hover { background: var(--bg-hover); }

	/* Detail panel */
	.db-detail { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 14px 16px; min-height: 500px; min-width: 0; }
	.db-detail-placeholder { display: flex; align-items: center; justify-content: center; min-height: 460px; color: var(--text-dim); font-size: 13px; }
	.db-detail-hdr { display: flex; align-items: center; justify-content: space-between; margin-bottom: 12px; gap: 8px; flex-wrap: wrap; }
	.db-detail-hdr-left { display: flex; align-items: center; gap: 8px; }
	.db-detail-hdr-actions { display: flex; align-items: center; gap: 8px; }
	.db-detail-table { font-size: 13px; font-weight: 600; color: var(--text-primary); }

	.db-rows-meta { font-size: 11.5px; color: var(--text-dim); margin-bottom: 10px; }
	.db-mono { font-family: var(--font-mono); color: var(--text-primary); }
	.db-dim { color: var(--text-dim); }
	.db-cell-trunc { max-width: 220px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.db-row-actions { white-space: nowrap; }

	@media (max-width: 760px) {
		.db-page { padding: 16px 12px; }
		.db-shell { grid-template-columns: 1fr; }
		.db-tlist { max-height: 260px; }
		.clear-logs-custom-range { grid-template-columns: 1fr; }
		.tabs { width: 100%; max-width: 100%; -webkit-overflow-scrolling: touch; }
	}
</style>
