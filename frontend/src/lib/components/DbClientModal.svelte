<script lang="ts">
	import { onMount } from 'svelte';
	import { X, Database, Play, AlertTriangle, Info, RefreshCw } from '@lucide/svelte';
	import { Button, Badge, Select, TextField, FormField, InlineAlert, Spinner, ConfirmDialog } from '$lib/components/ui';
	import { api } from '$lib/api/client';
	import type { DbEngine, DbMeta, DbQueryResult } from '$lib/api/types';

	interface Props {
		serviceId: string;
		onClose: () => void;
	}

	let { serviceId, onClose }: Props = $props();

	// ── Connection form state ─────────────────────────────────────────────────
	let meta        = $state<DbMeta | null>(null);
	let metaLoading = $state(true);
	let metaError   = $state('');

	let engine   = $state<DbEngine>('postgres');
	let host     = $state('');
	let port     = $state(5432);
	let database = $state('');
	let username = $state('');
	let password = $state('');

	// ── Query state ───────────────────────────────────────────────────────────
	let connected    = $state(false);
	let connecting   = $state(false);
	let connectError = $state('');

	let sql          = $state('');
	let running      = $state(false);
	let result       = $state<DbQueryResult | null>(null);
	let queryError   = $state('');

	// ── Schema browser ────────────────────────────────────────────────────────
	let browserItems   = $state<string[]>([]);
	let browserLoading = $state(false);
	let browserError   = $state('');
	let selectedItem   = $state<string | null>(null);

	const ENGINE_OPTIONS: { value: DbEngine; label: string; defaultPort: number }[] = [
		{ value: 'postgres', label: 'PostgreSQL', defaultPort: 5432 },
		{ value: 'mysql',    label: 'MySQL',      defaultPort: 3306 },
		{ value: 'mariadb',  label: 'MariaDB',    defaultPort: 3306 },
		{ value: 'redis',    label: 'Redis',      defaultPort: 6379 },
		{ value: 'mongodb',  label: 'MongoDB',    defaultPort: 27017 },
	];

	const ENGINE_PLACEHOLDER: Record<DbEngine, string> = {
		postgres: 'SELECT * FROM users LIMIT 10;',
		mysql:    'SELECT * FROM users LIMIT 10;',
		mariadb:  'SELECT * FROM users LIMIT 10;',
		redis:    'KEYS *',
		mongodb:  '{\n  "collection": "users",\n  "filter": {},\n  "sort": { "_id": -1 },\n  "limit": 100\n}',
	};

	// Which engines use the SQL-style form (database + username required)
	const isSqlEngine = $derived(engine === 'postgres' || engine === 'mysql' || engine === 'mariadb');
	const isRedis    = $derived(engine === 'redis');
	const isMongo    = $derived(engine === 'mongodb');

	// Label overrides per engine
	const dbFieldLabel  = $derived(isRedis ? 'DB Index (0–15)' : 'Database');
	const queryLabel    = $derived(isRedis ? 'Redis Command' : isMongo ? 'Query (JSON)' : 'SQL Query');
	const queryHint     = $derived(isRedis ? 'Enter to run' : isMongo ? 'Ctrl+Enter to run' : 'Ctrl+Enter to run');
	const browserLabel  = $derived(isMongo ? 'Collections' : isRedis ? 'Keys' : 'Tables');

	// True when the auto-detected host is a Docker-internal name (not an IP).
	// In dev the backend runs on the host so Docker DNS won't resolve — the user
	// must publish the port or connect to the container's IP manually.
	let isDockerInternalHost = $derived(
		!!host && !host.match(/^\d+\.\d+\.\d+\.\d+$/) && host !== 'localhost'
	);

	onMount(async () => {
		const res = await api.getDbMeta(serviceId);
		metaLoading = false;
		if (res.error) {
			metaError = res.error.message;
			return;
		}
		meta = res.data;
		if (meta) {
			if (meta.engine)   engine   = meta.engine;
			if (meta.host)     host     = meta.host;
			if (meta.port)     port     = meta.port;
			if (meta.username) username = meta.username;
			if (meta.password) password = meta.password;
			if (meta.database) database = meta.database;
		}
	});

	function onEngineChange() {
		const opt = ENGINE_OPTIONS.find(o => o.value === engine);
		if (opt) port = opt.defaultPort;
	}

	async function connect() {
		if (!host.trim()) { connectError = 'Host is required.'; return; }
		if (isSqlEngine && !database.trim()) { connectError = 'Database name is required.'; return; }
		if (isSqlEngine && !username.trim()) { connectError = 'Username is required.'; return; }
		if (isMongo && !database.trim())     { connectError = 'Database name is required.'; return; }

		connecting   = true;
		connectError = '';

		const testSql =
			isRedis ? 'PING' :
			isMongo ? JSON.stringify({ $ping: true }) :
			'SELECT 1';

		const res = await api.runDbQuery(serviceId, {
			engine, host: host.trim(), port,
			database: database.trim(),
			username: username.trim(),
			password,
			sql: testSql,
		});
		connecting = false;
		if (res.error) { connectError = res.error.message; return; }
		connected = true;
		sql = ENGINE_PLACEHOLDER[engine];
		loadBrowser();
	}

	async function runQuery() {
		if (!sql.trim() || running) return;
		running    = true;
		queryError = '';
		result     = null;

		const res = await api.runDbQuery(serviceId, {
			engine, host: host.trim(), port, database: database.trim(),
			username: username.trim(), password, sql: sql.trim(),
		});
		running = false;

		if (res.error) {
			queryError = res.error.message;
			return;
		}
		result = res.data;
	}

	function handleKeydown(e: KeyboardEvent) {
		// Redis: Enter runs (it's a single-line command)
		if (isRedis && e.key === 'Enter' && !e.shiftKey) {
			e.preventDefault();
			runQuery();
			return;
		}
		// SQL / MongoDB: Ctrl+Enter or Cmd+Enter runs
		if (!isRedis && (e.ctrlKey || e.metaKey) && e.key === 'Enter') {
			e.preventDefault();
			runQuery();
		}
	}

	function disconnect() {
		connected      = false;
		result         = null;
		queryError     = '';
		connectError   = '';
		browserItems   = [];
		browserError   = '';
		browserLoading = false;
		selectedItem   = null;
	}

	async function loadBrowser() {
		browserLoading = true;
		browserError   = '';
		browserItems   = [];
		selectedItem   = null;

		const listSql =
			engine === 'postgres'
				? "SELECT table_name FROM information_schema.tables WHERE table_schema = 'public' ORDER BY table_name"
				: engine === 'mysql' || engine === 'mariadb'
					? 'SHOW TABLES'
					: engine === 'mongodb'
						? JSON.stringify({ '$listCollections': true })
						: 'KEYS *';

		const res = await api.runDbQuery(serviceId, {
			engine, host: host.trim(), port, database: database.trim(),
			username: username.trim(), password, sql: listSql,
		});
		browserLoading = false;
		if (res.error) { browserError = res.error.message; return; }
		browserItems = (res.data?.rows ?? [])
			.map(row => String(row[0] ?? ''))
			.filter(Boolean)
			.slice(0, 300);
	}

	function fillQuery(name: string) {
		selectedItem = name;
		if (isSqlEngine) {
			const quoted = engine === 'postgres' ? `"${name}"` : `\`${name}\``;
			sql = `SELECT * FROM ${quoted} LIMIT 100;`;
		} else if (isMongo) {
			sql = `{\n  "collection": "${name}",\n  "filter": {},\n  "sort": { "_id": -1 },\n  "limit": 100\n}`;
		} else {
			sql = `GET ${name}`;
		}
	}

	// ── Close confirmation ────────────────────────────────────────────────────
	let showCloseConfirm = $state(false);

	function requestClose() {
		showCloseConfirm = true;
	}

	function confirmClose() {
		onClose();
	}

	const ENGINE_TONE: Record<DbEngine, 'blue' | 'yellow' | 'neutral' | 'red' | 'green'> = {
		postgres: 'blue',
		mysql:    'yellow',
		mariadb:  'neutral',
		redis:    'red',
		mongodb:  'green',
	};

	function formatCell(val: unknown): string {
		if (val === null || val === undefined) return 'NULL';
		if (typeof val === 'object') return JSON.stringify(val);
		return String(val);
	}

	function isCellNull(val: unknown) {
		return val === null || val === undefined;
	}
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="backdrop" onclick={requestClose} onkeydown={() => {}}></div>

<div class="modal" role="dialog" aria-modal="true" aria-label="Database Client">
	<!-- Header -->
	<div class="modal-header">
		<div class="modal-title">
			<Database size={15} />
			<span>Database Client</span>
			{#if connected}
				<Badge tone="green">Connected</Badge>
			{/if}
		</div>
		<Button variant="ghost" size="icon" onclick={requestClose} aria-label="Close"><X size={15} /></Button>
	</div>

	{#if metaLoading}
		<div class="state-center">
			<Spinner size={18} />
			<span>Detecting database…</span>
		</div>
	{:else if metaError}
		<div class="state-pad" role="alert">
			<InlineAlert tone="error">
				<span class="alert-row"><AlertTriangle size={16} /><span>{metaError}</span></span>
			</InlineAlert>
		</div>
	{:else}
		<div class="modal-body">
			<!-- Connection panel -->
			{#if !connected}
				<section class="conn-section">
					{#if isDockerInternalHost}
						<InlineAlert tone="warning">
							<span class="alert-row">
								<AlertTriangle size={13} />
								<span>
									This service has no published port. The host <code>{host}</code> is a Docker-internal name — it only resolves when Shipyard itself runs inside Docker (production).
									To connect in dev, publish the port in the service settings first.
								</span>
							</span>
						</InlineAlert>
					{:else if meta?.detected}
						<InlineAlert tone="info">
							<span class="alert-row">
								<Info size={13} />
								<span>
									Auto-detected <strong>{meta.engine}</strong> at <code>{meta.host}:{meta.port}</code>.
									{#if meta.password}Credentials prefilled from the service — just click Connect.{:else}Enter credentials to connect.{/if}
								</span>
							</span>
						</InlineAlert>
					{/if}

					<div class="form-grid">
						<!-- Engine selector -->
						<div class="field">
							<FormField label="Engine" for="db-engine">
								<Select
									id="db-engine"
									bind:value={() => engine, (v) => { engine = v as DbEngine; onEngineChange(); }}
									options={ENGINE_OPTIONS.map((o) => ({ value: o.value, label: o.label }))}
								/>
							</FormField>
						</div>

						<!-- Host + Port (all engines) -->
						<div class="field field-wide">
							<FormField label="Host" for="db-host">
								<TextField id="db-host" type="text" bind:value={host} placeholder="platform-uuid" spellcheck="false" />
							</FormField>
						</div>

						<div class="field field-narrow">
							<FormField label="Port" for="db-port">
								<!-- port stays a number (native number-input binding); TextField types its value as string -->
								<TextField id="db-port" type="number" bind:value={() => port as unknown as string, (v) => { port = v as unknown as number; }} min="1" max="65535" />
							</FormField>
						</div>

						<!-- Database / DB index (hide for Redis when not needed, relabel) -->
						{#if !isRedis || true}
							<div class="field {isSqlEngine || isMongo ? 'field-wide' : ''}">
								<FormField label={dbFieldLabel} for="db-database">
									<TextField
										id="db-database"
										type="text"
										bind:value={database}
										placeholder={isRedis ? '0' : 'mydb'}
										spellcheck="false"
									/>
								</FormField>
							</div>
						{/if}

						<!-- Username — hidden for Redis (no username concept in basic Redis) -->
						{#if !isRedis}
							<div class="field">
								<FormField label="Username" for="db-user">
									<TextField id="db-user" type="text" bind:value={username}
										placeholder={isMongo ? 'admin' : 'postgres'}
										autocomplete="off" spellcheck="false" />
								</FormField>
							</div>
						{/if}

						<!-- Password (all engines) -->
						<div class="field">
							<FormField label="Password{isRedis ? ' (optional)' : ''}" for="db-pass">
								<TextField id="db-pass" type="password" bind:value={password} autocomplete="new-password" />
							</FormField>
						</div>
					</div>

					{#if connectError}
						<div role="alert">
							<InlineAlert tone="error">
								<span class="alert-row"><AlertTriangle size={13} /><span>{connectError}</span></span>
							</InlineAlert>
						</div>
					{/if}

					<div class="conn-footer">
						<Button variant="primary" onclick={connect} disabled={connecting}>
							{#if connecting}
								<Spinner size={13} tone="current" />
								Connecting…
							{:else}
								<Play size={13} />
								Connect
							{/if}
						</Button>
					</div>
				</section>
			{:else}
				<!-- Connected — show schema browser + query editor -->
				<div class="editor-section">
					<!-- Connection status bar (full width) -->
					<div class="conn-bar">
						<span class="conn-detail">
							<Badge tone={ENGINE_TONE[engine]}><span class="engine-label">{engine}</span></Badge>
							<code>{host}:{port}</code>
							<span class="sep">·</span>
							<code>{database}</code>
							{#if username}
								<span class="sep">·</span>
								<span>{username}</span>
							{/if}
						</span>
						<Button variant="ghost" size="sm" onclick={disconnect}>Disconnect</Button>
					</div>

					<!-- Two-column: schema sidebar + editor/results -->
					<div class="browser-layout">
						<!-- Schema browser sidebar -->
						<div class="browser-sidebar">
							<div class="browser-header">
								<span>{browserLabel}</span>
								<Button variant="ghost" size="icon" onclick={loadBrowser} title="Refresh" aria-label="Refresh {browserLabel}">
									<RefreshCw size={11} />
								</Button>
							</div>
							{#if browserLoading}
								<div class="browser-state">
									<Spinner size={14} />
								</div>
							{:else if browserError}
								<div class="browser-state browser-err" title={browserError}>
									<AlertTriangle size={13} />
									<span>Failed to load</span>
								</div>
							{:else if browserItems.length === 0}
								<div class="browser-state">
									<span>No {browserLabel.toLowerCase()}</span>
								</div>
							{:else}
								<div class="browser-list">
									{#each browserItems as item}
										<button
											class="browser-item"
											class:active={selectedItem === item}
											onclick={() => fillQuery(item)}
											title={item}
										>{item}</button>
									{/each}
								</div>
							{/if}
						</div>

						<!-- Right: editor + results -->
						<div class="browser-main">
							<!-- Query editor -->
							<div class="editor-wrap">
								<div class="editor-label">{queryLabel}</div>
								<textarea
									class="sql-editor"
									class:redis-editor={isRedis}
									bind:value={sql}
									onkeydown={handleKeydown}
									placeholder={ENGINE_PLACEHOLDER[engine]}
									spellcheck="false"
									autocomplete="off"
								></textarea>
								<div class="editor-actions">
									<span class="editor-hint">{queryHint}</span>
									<Button variant="primary" size="sm" onclick={runQuery} disabled={running || !sql.trim()}>
										{#if running}
											<Spinner size={12} tone="current" />
											Running…
										{:else}
											<Play size={12} />
											Run
										{/if}
									</Button>
								</div>
							</div>

							<!-- Results -->
							{#if queryError}
								<div class="result-error" role="alert">
									<InlineAlert tone="error">
										<span class="alert-row"><AlertTriangle size={13} /><pre class="error-pre">{queryError}</pre></span>
									</InlineAlert>
								</div>
							{:else if result}
								<div class="results-section">
									<div class="results-meta">
										<span>{result.row_count} row{result.row_count !== 1 ? 's' : ''}</span>
										{#if result.truncated}
											<Badge tone="yellow">Limited to 1 000 rows</Badge>
										{/if}
										<span class="exec-time">{result.execution_time_ms}ms</span>
									</div>

									{#if result.columns.length === 0}
										<div class="empty-result">Query executed successfully — no rows returned.</div>
									{:else}
										<div class="table-scroll">
											<table class="results-table">
												<thead>
													<tr>
														{#each result.columns as col}
															<th>{col}</th>
														{/each}
													</tr>
												</thead>
												<tbody>
													{#each result.rows as row}
														<tr>
															{#each row as cell}
																<td class:null-cell={isCellNull(cell)}>{formatCell(cell)}</td>
															{/each}
														</tr>
													{/each}
												</tbody>
											</table>
										</div>
									{/if}
								</div>
							{/if}
						</div>
					</div>
				</div>
			{/if}
		</div>
	{/if}
</div>

<!-- Sibling of .modal (not a child): .modal's transform would otherwise contain the dialog's fixed positioning. -->
<ConfirmDialog
	bind:open={showCloseConfirm}
	title="Close database client?"
	message="Your connection and any unsaved query results will be lost."
	confirmLabel="Close"
	onConfirm={confirmClose}
/>

<style>
	.backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		z-index: 300;
	}

	.modal {
		position: fixed;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		width: min(880px, calc(100vw - 32px));
		max-height: calc(100vh - 48px);
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-lg);
		box-shadow: var(--shadow-lg);
		z-index: 301;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		isolation: isolate;
	}

	.modal-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 10px 18px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.modal-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 14px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.modal-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
	}

	/* Loading / error */
	.state-center {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 10px;
		height: 200px;
		color: var(--text-muted);
		font-size: 13px;
	}
	.state-pad { padding: 20px; }

	.alert-row { display: flex; align-items: flex-start; gap: 8px; }
	.alert-row :global(svg) { flex-shrink: 0; margin-top: 1px; }
	.alert-row code { font-family: var(--font-mono); font-size: 12px; }

	/* Connection form */
	.conn-section {
		flex: 1;
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 16px;
		overflow-y: auto;
	}

	.form-grid {
		display: grid;
		grid-template-columns: 1fr 1fr 80px;
		gap: 12px;
	}

	.field { min-width: 0; }
	.field-wide { grid-column: span 2; }
	.field-narrow { grid-column: span 1; }

	.error-pre { margin: 0; font-family: var(--font-mono); font-size: 12px; white-space: pre-wrap; word-break: break-all; }

	.conn-footer { display: flex; justify-content: flex-end; }

	/* Connected editor section */
	.editor-section {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		min-height: 0;
	}

	.conn-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 18px;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		font-size: 12px;
		flex-shrink: 0;
		gap: 8px;
	}

	.conn-detail {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--text-muted);
		min-width: 0;
		flex-wrap: wrap;
	}
	.conn-detail code { font-family: var(--font-mono); }
	.sep { color: var(--border-hover); }
	.engine-label { text-transform: uppercase; letter-spacing: 0.05em; font-size: 10px; }

	.editor-wrap {
		display: flex;
		flex-direction: column;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.editor-label {
		padding: 6px 18px 0;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.sql-editor {
		width: 100%;
		min-height: 100px;
		max-height: 220px;
		padding: 12px 18px;
		background: var(--bg-base);
		border: none;
		outline: none;
		resize: vertical;
		font-family: var(--font-mono);
		font-size: 13px;
		color: var(--text-primary);
		line-height: 1.6;
		box-sizing: border-box;
	}
	.sql-editor:focus-visible { box-shadow: inset 0 0 0 2px var(--accent-muted), inset 0 2px 0 var(--accent); }

	.editor-actions {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 14px;
		background: var(--bg-elevated);
	}
	.editor-hint { font-size: 11px; color: var(--text-dim); }

	/* Redis command: single-line style */
	.redis-editor {
		min-height: 44px;
		max-height: 44px;
		resize: none;
		font-size: 14px;
	}

	/* Results */
	.result-error { padding: 12px 18px; }

	.results-section {
		display: flex;
		flex-direction: column;
		flex: 1;
		overflow: hidden;
		min-height: 0;
	}

	.results-meta {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 18px;
		font-size: 12px;
		color: var(--text-muted);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	.exec-time { margin-left: auto; color: var(--text-dim); }

	.empty-result {
		padding: 24px 18px;
		color: var(--text-muted);
		font-size: 13px;
	}

	.table-scroll {
		overflow: auto;
		flex: 1;
		min-height: 0;
	}

	.results-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
		font-family: var(--font-mono);
	}

	.results-table th {
		position: sticky;
		top: 0;
		background: var(--bg-elevated);
		padding: 6px 14px;
		text-align: left;
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.04em;
		white-space: nowrap;
		border-bottom: 1px solid var(--border);
	}

	.results-table td {
		padding: 5px 14px;
		border-bottom: 1px solid var(--border);
		color: var(--text-primary);
		white-space: nowrap;
		max-width: 320px;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.results-table tr:hover td { background: var(--bg-hover); }

	.null-cell { color: var(--text-dim) !important; font-style: italic; }

	/* ── Schema browser ── */
	.browser-layout {
		flex: 1;
		display: flex;
		overflow: hidden;
		min-height: 0;
	}

	.browser-sidebar {
		width: 180px;
		flex-shrink: 0;
		border-right: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		background: var(--bg-elevated);
	}

	.browser-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 2px 4px 2px 12px;
		border-bottom: 1px solid var(--border);
		font-size: 10px;
		font-weight: 700;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.06em;
		flex-shrink: 0;
	}

	.browser-state {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 6px;
		flex: 1;
		padding: 16px;
		color: var(--text-muted);
		font-size: 11px;
		text-align: center;
	}
	.browser-state.browser-err { color: var(--accent-red); }

	.browser-list {
		flex: 1;
		overflow-y: auto;
		padding: 4px 0;
	}

	.browser-item {
		display: block;
		width: 100%;
		padding: 5px 12px;
		font-size: 12px;
		font-family: var(--font-mono);
		color: var(--text-primary);
		background: transparent;
		border: none;
		text-align: left;
		cursor: pointer;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
		transition: background var(--transition-fast);
	}
	.browser-item:hover { background: var(--bg-hover); }
	.browser-item:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
	.browser-item.active {
		background: var(--accent-muted);
		color: var(--accent);
	}

	.browser-main {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		min-width: 0;
	}

	@media (max-width: 639px) {
		.form-grid { grid-template-columns: 1fr; }
		.field-wide, .field-narrow { grid-column: span 1; }
	}
</style>
