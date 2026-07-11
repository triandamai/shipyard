<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface ColMeta { name: string; data_type: string; udt_name: string; is_nullable: boolean; is_primary_key: boolean; }
	interface RowsResponse { columns: ColMeta[]; rows: (string | null)[][]; total: number; page: number; per_page: number; }

	type DbType = 'postgres' | 'redis';
	let dbType = $state<DbType>('postgres');

	let tables        = $state<string[]>([]);
	let tablesLoading = $state(true);
	let tablesError   = $state('');

	let selected      = $state<string | null>(null);
	let activeTab     = $state<'columns' | 'rows'>('columns');

	let columns       = $state<ColMeta[]>([]);
	let colLoading    = $state(false);
	let colError      = $state('');

	let rowsData      = $state<RowsResponse | null>(null);
	let rowsLoading   = $state(false);
	let rowsError     = $state('');
	let rowPage       = $state(1);
	const ROW_PER_PAGE = 50;

	// Redis state
	interface RedisInfo { key: string; value: string; }
	let redisInfo      = $state<RedisInfo[]>([]);
	let redisLoading   = $state(false);
	let redisError     = $state('');

	async function loadRedisInfo() {
		redisLoading = true; redisError = '';
		const r = await api.get<RedisInfo[]>('/admin/redis/info');
		if (r.data) redisInfo = Array.isArray(r.data) ? r.data : [];
		else redisError = r.error?.message ?? 'Failed to load Redis info';
		redisLoading = false;
	}

	function switchDb(t: DbType) {
		dbType = t;
	}

	let search        = $state('');
	let deleteTarget  = $state<string | null>(null);
	let deleting      = $state(false);

	let editRow       = $state<(string | null)[] | null>(null);
	let editDraft     = $state<Record<string, string>>({});
	let saving        = $state(false);
	let saveErr       = $state('');

	let filteredTables = $derived(
		search.trim()
			? tables.filter(t => t.toLowerCase().includes(search.toLowerCase()))
			: tables
	);

	let pkColIdx = $derived(rowsData?.columns.findIndex(c => c.is_primary_key) ?? -1);
	let totalRowPages = $derived(Math.ceil((rowsData?.total ?? 0) / ROW_PER_PAGE));

	async function loadTables() {
		tablesLoading = true;
		tablesError   = '';
		const r = await api.get<any[]>('/admin/db/tables');
		if (r.data) tables = r.data.map((t: any) => typeof t === 'string' ? t : t.name ?? String(t));
		else tablesError = r.error?.message ?? 'Failed to load tables';
		tablesLoading = false;
	}

	async function selectTable(name: string) {
		selected  = name;
		columns   = [];
		rowsData  = null;
		colError  = '';
		rowsError = '';
		rowPage   = 1;
		activeTab = 'columns';
		await loadColumns(name);
	}

	async function loadColumns(name: string) {
		colLoading = true;
		colError   = '';
		const r = await api.get<ColMeta[]>(`/admin/db/tables/${encodeURIComponent(name)}/columns`);
		if (r.data) columns = r.data;
		else colError = r.error?.message ?? 'Failed to load columns';
		colLoading = false;
	}

	async function loadRows(name: string, page = 1) {
		rowsLoading = true;
		rowsError   = '';
		const r = await api.get<RowsResponse>(`/admin/db/tables/${encodeURIComponent(name)}/rows?page=${page}&per_page=${ROW_PER_PAGE}`);
		if (r.data) { rowsData = r.data; rowPage = page; }
		else rowsError = r.error?.message ?? 'Failed to load rows';
		rowsLoading = false;
	}

	async function switchTab(t: 'columns' | 'rows') {
		activeTab = t;
		if (t === 'rows' && !rowsData && selected) await loadRows(selected);
	}

	async function dropTable() {
		if (!deleteTarget) return;
		deleting = true;
		const r = await api.delete(`/admin/db/tables/${encodeURIComponent(deleteTarget)}`);
		if (!r.error) {
			tables = tables.filter(t => t !== deleteTarget);
			if (selected === deleteTarget) { selected = null; columns = []; rowsData = null; }
		}
		deleteTarget = null;
		deleting = false;
	}

	function startEdit(row: (string | null)[]) {
		editRow = row;
		if (!rowsData) return;
		editDraft = Object.fromEntries(
			rowsData.columns.map((col, i) => [col.name, row[i] ?? ''])
		);
		saveErr = '';
	}

	async function saveEdit() {
		if (!editRow || !selected || !rowsData || pkColIdx < 0) return;
		saving = true;
		saveErr = '';
		const pkVal = editRow[pkColIdx] ?? '';
		const r = await api.patch(`/admin/db/tables/${encodeURIComponent(selected)}/rows/${encodeURIComponent(pkVal)}`, { updates: editDraft });
		if (r.error) { saveErr = r.error.message; saving = false; return; }
		await loadRows(selected, rowPage);
		editRow = null;
		saving = false;
	}

	async function deleteRow(row: (string | null)[]) {
		if (!selected || !rowsData || pkColIdx < 0) return;
		const pkVal = row[pkColIdx] ?? '';
		await api.delete(`/admin/db/tables/${encodeURIComponent(selected)}/rows/${encodeURIComponent(pkVal)}`);
		await loadRows(selected, rowPage);
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

{#if editRow}
	<!-- Edit row modal -->
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-back" onclick={() => (editRow = null)}>
		<div class="modal" onclick={(e) => e.stopPropagation()}>
			<div class="modal-hdr">
				<span class="modal-title">Edit Row</span>
				<button class="modal-close" onclick={() => (editRow = null)} aria-label="Close dialog">
					<svg viewBox="0 0 20 20" fill="currentColor" width="14" height="14" aria-hidden="true"><path fill-rule="evenodd" d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z" clip-rule="evenodd"/></svg>
				</button>
			</div>
			<div class="modal-body">
				{#each Object.keys(editDraft) as key}
					<div class="field">
						<label class="lbl" for="edit-{key}">{key}</label>
						<input id="edit-{key}" class="inp" bind:value={editDraft[key]} />
					</div>
				{/each}
				{#if saveErr}<div class="err-msg">{saveErr}</div>{/if}
			</div>
			<div class="modal-foot">
				<button class="btn-ghost" onclick={() => (editRow = null)}>Cancel</button>
				<button class="btn-primary" onclick={saveEdit} disabled={saving}>
					{saving ? 'Saving…' : 'Save'}
				</button>
			</div>
		</div>
	</div>
{/if}

{#if deleteTarget}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="modal-back" onclick={() => (deleteTarget = null)}>
		<div class="modal modal-sm" onclick={(e) => e.stopPropagation()}>
			<div class="modal-hdr">
				<span class="modal-title">Drop Table</span>
			</div>
			<div class="modal-body">
				<p class="confirm-text">Drop <strong class="mono">{deleteTarget}</strong>? This is irreversible.</p>
			</div>
			<div class="modal-foot">
				<button class="btn-ghost" onclick={() => (deleteTarget = null)}>Cancel</button>
				<button class="btn-danger" onclick={dropTable} disabled={deleting}>
					{deleting ? 'Dropping…' : 'Drop Table'}
				</button>
			</div>
		</div>
	</div>
{/if}

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Database</h1>
			<p class="sub">Inspect and manage system database tables and Redis.</p>
		</div>
	</header>

	<div class="db-tabs">
		<button class="db-tab" class:active={dbType === 'postgres'} onclick={() => switchDb('postgres')}>
			<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M5 4a3 3 0 00-3 3v6a3 3 0 003 3h10a3 3 0 003-3V7a3 3 0 00-3-3H5zm-1 9v-1h5v2H5a1 1 0 01-1-1zm7 1h4a1 1 0 001-1v-1h-5v2zm0-4h5V8h-5v2zM9 8H4v2h5V8z" clip-rule="evenodd"/></svg>
			PostgreSQL
		</button>
		<button class="db-tab" class:active={dbType === 'redis'} onclick={() => switchDb('redis')}>
			<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path d="M3 12v3c0 1.657 3.134 3 7 3s7-1.343 7-3v-3c0 1.657-3.134 3-7 3s-7-1.343-7-3z"/><path d="M3 7v3c0 1.657 3.134 3 7 3s7-1.343 7-3V7c0 1.657-3.134 3-7 3S3 8.657 3 7z"/><path d="M17 5c0 1.657-3.134 3-7 3S3 6.657 3 5s3.134-3 7-3 7 1.343 7 3z"/></svg>
			Redis
		</button>
	</div>

	{#if dbType === 'redis'}
		<div class="redis-panel">
			<div class="redis-hdr">
				<span class="redis-title">Redis Info</span>
				<button class="refresh-btn" onclick={loadRedisInfo}>
					<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
					Refresh
				</button>
			</div>
			{#if redisLoading}
				<div class="redis-body">{#each [0,1,2,3,4,5,6,7] as _}<div class="sk-row2"><div class="sk" style="width:140px;height:11px"></div><div class="sk" style="width:200px;height:11px"></div></div>{/each}</div>
			{:else if redisError}
				<div class="err-banner">{redisError}</div>
			{:else if redisInfo.length === 0}
				<div class="redis-empty">No Redis info available. Make sure the backend can connect to Redis.</div>
			{:else}
				<div class="redis-body">
					{#each redisInfo as item}
						<div class="redis-row">
							<span class="redis-key mono">{item.key}</span>
							<span class="redis-val mono">{item.value}</span>
						</div>
					{/each}
				</div>
			{/if}
		</div>
	{:else}
	<div class="shell">
		<!-- Table list -->
		<div class="tlist">
			<div class="tlist-hdr">
				<span class="tlist-title">Tables</span>
				<span class="count">{tables.length}</span>
			</div>
			<div class="search-wrap">
				<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="12" height="12"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
				<input class="search-inp" placeholder="Filter tables…" bind:value={search} />
			</div>
			<div class="tlist-body">
				{#if tablesLoading}
					{#each [0,1,2,3,4,5] as _}
						<div class="tlist-sk"><div class="sk" style="width:{60+Math.random()*80}px;height:11px"></div></div>
					{/each}
				{:else if tablesError}
					<div class="tlist-err">{tablesError}</div>
				{:else}
					{#each filteredTables as t}
						<button class="tlist-item" class:tlist-sel={selected === t} onclick={() => selectTable(t)}>
							<svg viewBox="0 0 20 20" fill="currentColor" width="11" height="11"><path fill-rule="evenodd" d="M5 4a3 3 0 00-3 3v6a3 3 0 003 3h10a3 3 0 003-3V7a3 3 0 00-3-3H5zm-1 9v-1h5v2H5a1 1 0 01-1-1zm7 1h4a1 1 0 001-1v-1h-5v2zm0-4h5V8h-5v2zM9 8H4v2h5V8z" clip-rule="evenodd"/></svg>
							{t}
						</button>
					{/each}
					{#if filteredTables.length === 0}
						<div class="tlist-empty">No tables found.</div>
					{/if}
				{/if}
			</div>
		</div>

		<!-- Detail panel -->
		<div class="detail">
			{#if !selected}
				<div class="detail-placeholder">Select a table to inspect</div>
			{:else}
				<div class="detail-hdr">
					<span class="detail-table mono">{selected}</span>
					<button class="btn-danger-sm" onclick={() => (deleteTarget = selected)} title="Drop table">
						<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M9 2a1 1 0 00-.894.553L7.382 4H4a1 1 0 000 2v10a2 2 0 002 2h8a2 2 0 002-2V6a1 1 0 100-2h-3.382l-.724-1.447A1 1 0 0011 2H9zM7 8a1 1 0 012 0v6a1 1 0 11-2 0V8zm5-1a1 1 0 00-1 1v6a1 1 0 102 0V8a1 1 0 00-1-1z" clip-rule="evenodd"/></svg>
						Drop
					</button>
				</div>

				<div class="tabs">
					<button class="tab" class:active={activeTab === 'columns'} onclick={() => switchTab('columns')}>Columns</button>
					<button class="tab" class:active={activeTab === 'rows'} onclick={() => switchTab('rows')}>Rows</button>
				</div>

				{#if activeTab === 'columns'}
					{#if colLoading}
						<div class="sk-wrap">{#each [0,1,2,3,4] as _}<div class="sk" style="height:34px;border-radius:var(--radius-sm)"></div>{/each}</div>
					{:else if colError}
						<div class="err-banner">{colError}</div>
					{:else}
						<div class="tbl">
							<div class="thead">
								<span style="flex:2">Column</span>
								<span style="flex:2">Type</span>
								<span style="flex:1">Nullable</span>
								<span style="flex:1">PK</span>
							</div>
							{#each columns as col}
								<div class="trow">
									<div class="mono" style="flex:2;font-size:12px">{col.name}</div>
									<div class="mono muted" style="flex:2;font-size:11.5px">{col.udt_name || col.data_type}</div>
									<div class="cell" style="flex:1">
										{#if col.is_nullable}
											<span class="badge-null">nullable</span>
										{:else}
											<span class="badge-nn">required</span>
										{/if}
									</div>
									<div class="cell" style="flex:1">
										{#if col.is_primary_key}
											<span class="badge-pk">PK</span>
										{/if}
									</div>
								</div>
							{/each}
							{#if columns.length === 0}
								<div class="tbl-empty">No columns.</div>
							{/if}
						</div>
						<!-- Mobile cards for columns -->
						<div class="card-list">
							{#each columns as col}
								<div class="m-card">
									<div class="m-card-title mono">{col.name}</div>
									<div class="m-card-row"><span class="m-card-key">Type</span><span class="mono">{col.udt_name || col.data_type}</span></div>
									<div class="m-card-row"><span class="m-card-key">Nullable</span><span>{col.is_nullable ? 'Yes' : 'No'}</span></div>
									<div class="m-card-row"><span class="m-card-key">PK</span><span>{col.is_primary_key ? 'Yes' : '-'}</span></div>
								</div>
							{/each}
						</div>
					{/if}

				{:else}
					{#if rowsLoading}
						<div class="sk-wrap">{#each [0,1,2,3] as _}<div class="sk" style="height:34px;border-radius:var(--radius-sm)"></div>{/each}</div>
					{:else if rowsError}
						<div class="err-banner">{rowsError}</div>
					{:else if rowsData}
						<div class="rows-meta">
							{rowsData.total} row{rowsData.total !== 1 ? 's' : ''} total
							{#if pkColIdx >= 0} &bull; PK: <span class="mono">{rowsData.columns[pkColIdx].name}</span>{/if}
						</div>
						{#if rowsData.rows.length === 0}
							<div class="detail-placeholder" style="padding:32px">Table is empty.</div>
						{:else}
							<div class="rows-scroll">
								<table class="rtbl">
									<thead>
										<tr>
											{#each rowsData.columns as col}
												<th class="rth">{col.name}</th>
											{/each}
											<th class="rth rth-act">Actions</th>
										</tr>
									</thead>
									<tbody>
										{#each rowsData.rows as row}
											<tr class="rtrow">
												{#each row as cell}
													<td class="rtd mono">{cellVal(cell)}</td>
												{/each}
												<td class="rtd rtd-act">
													<button class="act-btn" onclick={() => startEdit(row)}>Edit</button>
													<button class="act-btn act-del" onclick={() => deleteRow(row)}>Del</button>
												</td>
											</tr>
										{/each}
									</tbody>
								</table>
							</div>
							{#if totalRowPages > 1}
								<div class="pager">
									<button class="pg-btn" disabled={rowPage <= 1} onclick={() => selected && loadRows(selected, rowPage - 1)}>Prev</button>
									<span class="pg-info">Page {rowPage} of {totalRowPages}</span>
									<button class="pg-btn" disabled={rowPage >= totalRowPages} onclick={() => selected && loadRows(selected, rowPage + 1)}>Next</button>
								</div>
							{/if}
						{/if}
					{/if}
				{/if}
			{/if}
		</div>
	</div>
	{/if}
</div>

<style>
	.p { max-width:1100px; margin:0 auto; padding:40px 36px; }
	.hdr { margin-bottom:16px; }

	.db-tabs { display:flex; gap:2px; margin-bottom:16px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); padding:3px; width:fit-content; }
	.db-tab { display:flex; align-items:center; gap:6px; padding:5px 16px; border-radius:5px; font-size:12.5px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.db-tab.active { background:var(--surface); color:var(--text); box-shadow:0 1px 2px rgba(0,0,0,.07); }

	.redis-panel { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.redis-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 16px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.redis-title { font-size:12px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.06em; }
	.refresh-btn { display:flex; align-items:center; gap:5px; padding:5px 11px; height:28px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.refresh-btn:hover { background:var(--surface-2); }
	.redis-body { display:grid; grid-template-columns:1fr 1fr; }
	.redis-row { display:flex; align-items:baseline; gap:12px; padding:8px 16px; border-bottom:1px solid var(--border); }
	.redis-row:last-child { border-bottom:none; }
	.redis-key { font-size:12px; color:var(--text-3); min-width:180px; flex-shrink:0; }
	.redis-val { font-size:12px; color:var(--text); }
	.redis-empty { padding:40px; text-align:center; color:var(--text-3); font-size:13px; }
	.sk-row2 { display:flex; align-items:center; gap:20px; padding:11px 16px; border-bottom:1px solid var(--border); }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }

	.shell { display:grid; grid-template-columns:220px 1fr; gap:0; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); min-height:500px; }

	/* Table list */
	.tlist { border-right:1px solid var(--border); display:flex; flex-direction:column; }
	.tlist-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 12px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.tlist-title { font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.07em; }
	.count { font-size:10px; font-weight:700; background:var(--border); color:var(--text-3); padding:1px 6px; border-radius:999px; }
	.search-wrap { position:relative; display:flex; align-items:center; padding:8px; border-bottom:1px solid var(--border); }
	.si { position:absolute; left:16px; color:var(--text-3); pointer-events:none; }
	.search-inp { height:28px; padding:0 8px 0 26px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12px; color:var(--text); outline:none; width:100%; box-sizing:border-box; font-family:var(--font); transition:border-color .15s; }
	.search-inp::placeholder { color:var(--text-3); }
	.search-inp:focus { border-color:var(--accent); }
	.tlist-body { overflow-y:auto; flex:1; }
	.tlist-item { display:flex; align-items:center; gap:7px; padding:8px 12px; font-size:12px; font-family:var(--mono); color:var(--text-2); cursor:pointer; border:none; background:transparent; text-align:left; transition:background .1s, color .1s; width:100%; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
	.tlist-item:hover { background:var(--row-hover); color:var(--text); }
	.tlist-item.tlist-sel { background:var(--accent-soft); color:var(--accent); }
	.tlist-sk { padding:9px 12px; border-bottom:1px solid var(--border); }
	.tlist-err { padding:12px; font-size:12px; color:var(--danger); }
	.tlist-empty { padding:16px 12px; font-size:12px; color:var(--text-3); }

	/* Detail */
	.detail { display:flex; flex-direction:column; min-width:0; overflow:hidden; }
	.detail-placeholder { display:flex; align-items:center; justify-content:center; flex:1; color:var(--text-3); font-size:13px; padding:60px; }
	.detail-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; border-bottom:1px solid var(--border); background:var(--surface-2); flex-shrink:0; }
	.detail-table { font-size:13px; font-weight:600; color:var(--text); }

	.tabs { display:flex; gap:2px; padding:10px 14px 0; background:var(--surface); flex-shrink:0; }
	.tab { padding:5px 14px; border-radius:5px 5px 0 0; font-size:12.5px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); border-bottom:2px solid transparent; }
	.tab.active { color:var(--accent); border-bottom-color:var(--accent); }
	.tab:hover:not(.active) { color:var(--text); }

	.tbl { margin:12px 14px 0; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; }
	.thead { display:flex; align-items:center; gap:10px; padding:9px 14px; background:var(--surface-2); border-bottom:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.065em; }
	.trow { display:flex; align-items:center; gap:10px; padding:10px 14px; border-bottom:1px solid var(--border); transition:background .1s; }
	.trow:last-child { border-bottom:none; }
	.trow:hover { background:var(--row-hover); }
	.tbl-empty { padding:24px; text-align:center; color:var(--text-3); font-size:12.5px; }
	.cell { font-size:12.5px; color:var(--text-2); }
	.muted { color:var(--text-3); }
	.mono { font-family:var(--mono); color:var(--text); }

	.badge-null { display:inline-flex; padding:1px 7px; border-radius:999px; font-size:10px; font-weight:600; background:var(--warn-soft); color:var(--warn); border:1px solid rgba(180,83,9,0.18); }
	.badge-nn { display:inline-flex; padding:1px 7px; border-radius:999px; font-size:10px; font-weight:600; background:var(--ok-soft); color:var(--ok); border:1px solid rgba(22,163,74,0.18); }
	.badge-pk { display:inline-flex; padding:1px 7px; border-radius:999px; font-size:10px; font-weight:700; background:var(--accent-soft); color:var(--accent); border:1px solid var(--accent-ring); }

	.rows-meta { padding:10px 14px; font-size:11.5px; color:var(--text-3); border-bottom:1px solid var(--border); flex-shrink:0; }
	.rows-scroll { overflow-x:auto; flex:1; min-height:0; }
	.rtbl { border-collapse:collapse; width:100%; font-size:12px; }
	.rth { padding:8px 12px; text-align:left; background:var(--surface-2); border-bottom:1px solid var(--border); border-right:1px solid var(--border); font-size:10.5px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; white-space:nowrap; position:sticky; top:0; z-index:1; }
	.rth-act { border-right:none; }
	.rtrow:hover { background:var(--row-hover); }
	.rtd { padding:8px 12px; border-bottom:1px solid var(--border); border-right:1px solid var(--border); color:var(--text-2); font-family:var(--mono); font-size:11.5px; white-space:nowrap; max-width:200px; overflow:hidden; text-overflow:ellipsis; }
	.rtd-act { border-right:none; white-space:nowrap; }
	.act-btn { padding:3px 9px; border-radius:var(--radius-sm); font-size:11px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; margin-right:4px; }
	.act-btn:hover { background:var(--surface-2); }
	.act-del { color:var(--danger); border-color:rgba(220,38,38,0.25); }
	.act-del:hover { background:var(--danger-soft); }

	.pager { display:flex; align-items:center; gap:10px; padding:12px 14px; justify-content:center; border-top:1px solid var(--border); flex-shrink:0; }
	.pg-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.pg-btn:hover:not(:disabled) { background:var(--surface-2); }
	.pg-btn:disabled { opacity:.4; cursor:not-allowed; }
	.pg-info { font-size:12px; color:var(--text-3); }

	.sk-wrap { display:flex; flex-direction:column; gap:8px; padding:14px; }
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.err-banner { margin:12px 14px; padding:10px 12px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius-sm); font-size:12.5px; color:var(--danger); }

	/* Modals */
	.modal-back { position:fixed; inset:0; background:rgba(0,0,0,0.5); z-index:100; display:flex; align-items:center; justify-content:center; padding:20px; }
	.modal { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); width:100%; max-width:500px; box-shadow:var(--shadow-md); display:flex; flex-direction:column; max-height:80vh; }
	.modal-sm { max-width:360px; }
	.modal-hdr { display:flex; align-items:center; justify-content:space-between; padding:16px 20px; border-bottom:1px solid var(--border); flex-shrink:0; }
	.modal-title { font-size:14px; font-weight:700; color:var(--text); }
	.modal-close { display:flex; align-items:center; justify-content:center; width:28px; height:28px; border-radius:6px; border:none; background:var(--surface-2); color:var(--text-3); cursor:pointer; transition:background .15s; }
	.modal-close:hover { background:var(--border); color:var(--text); }
	.modal-body { padding:20px; overflow-y:auto; flex:1; display:flex; flex-direction:column; gap:12px; }
	.modal-foot { padding:14px 20px; border-top:1px solid var(--border); display:flex; justify-content:flex-end; gap:8px; flex-shrink:0; }

	.field { display:flex; flex-direction:column; gap:5px; }
	.lbl { font-size:11.5px; font-weight:600; color:var(--text-2); }
	.inp { height:34px; padding:0 10px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:100%; box-sizing:border-box; font-family:var(--mono); transition:border-color .15s, box-shadow .15s; }
	.inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.confirm-text { font-size:13px; color:var(--text-2); margin:0; line-height:1.5; }

	.err-msg { padding:8px 10px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius-sm); font-size:12px; color:var(--danger); }

	.btn-primary { padding:7px 18px; height:34px; border-radius:var(--radius-sm); font-size:12.5px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; transition:opacity .15s; font-family:var(--font); }
	.btn-primary:hover:not(:disabled) { opacity:.88; }
	.btn-primary:disabled { opacity:.5; cursor:not-allowed; }
	.btn-ghost { padding:7px 14px; height:34px; border-radius:var(--radius-sm); font-size:12.5px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); font-family:var(--font); transition:background .15s; }
	.btn-ghost:hover { background:var(--surface-2); }
	.btn-danger { padding:7px 18px; height:34px; border-radius:var(--radius-sm); font-size:12.5px; font-weight:600; cursor:pointer; border:1px solid var(--danger); background:var(--danger); color:#fff; font-family:var(--font); transition:opacity .15s; }
	.btn-danger:hover:not(:disabled) { opacity:.88; }
	.btn-danger:disabled { opacity:.5; cursor:not-allowed; }
	.btn-danger-sm { display:flex; align-items:center; gap:5px; padding:5px 11px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:600; cursor:pointer; border:1px solid rgba(220,38,38,0.3); background:var(--danger-soft); color:var(--danger); font-family:var(--font); transition:background .15s; }
	.btn-danger-sm:hover { background:rgba(220,38,38,0.14); }

	/* Mobile cards */
	.card-list { display:none; margin:12px 14px 0; }
	.m-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:14px; margin-bottom:8px; }
	.m-card-title { font-size:13px; font-weight:600; color:var(--text); margin-bottom:8px; }
	.m-card-row { display:flex; justify-content:space-between; align-items:center; padding:4px 0; border-bottom:1px solid var(--border); font-size:12.5px; color:var(--text-2); }
	.m-card-row:last-child { border-bottom:none; }
	.m-card-key { font-size:11px; font-weight:600; color:var(--text-3); text-transform:uppercase; letter-spacing:.05em; }

	@media (max-width: 640px) {
		.p { padding:16px 12px; }
		.shell { grid-template-columns:1fr; grid-template-rows:auto 1fr; }
		.tlist { border-right:none; border-bottom:1px solid var(--border); max-height:200px; }
		.tbl { display:none; }
		.card-list { display:block; }
		.rows-scroll { overflow-x:auto; }
		.redis-body { grid-template-columns:1fr; }
		.redis-row { flex-direction:column; align-items:flex-start; gap:3px; }
		.redis-key { min-width:unset; }
		.db-tabs { width:100%; }
		.db-tab { flex:1; justify-content:center; }
	}
</style>
