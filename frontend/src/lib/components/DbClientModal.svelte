<script lang="ts">
	import { onMount } from 'svelte';
	import { X, Database, Play, Loader2, AlertTriangle, Info, ChevronDown } from '@lucide/svelte';
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
		connected    = false;
		result       = null;
		queryError   = '';
		connectError = '';
	}

	// ── Close confirmation ────────────────────────────────────────────────────
	let showCloseConfirm = $state(false);

	function requestClose() {
		showCloseConfirm = true;
	}

	function confirmClose() {
		showCloseConfirm = false;
		onClose();
	}

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
				<span class="conn-badge connected">Connected</span>
			{/if}
		</div>
		<button class="icon-btn" onclick={requestClose} aria-label="Close"><X size={15} /></button>
	</div>

	{#if showCloseConfirm}
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="confirm-overlay" onclick={(e) => { if (e.target === e.currentTarget) showCloseConfirm = false; }} onkeydown={() => {}}>
			<div class="confirm-card">
				<p class="confirm-title">Close database client?</p>
				<p class="confirm-sub">Your connection and any unsaved query results will be lost.</p>
				<div class="confirm-actions">
					<button class="btn btn-ghost" onclick={() => showCloseConfirm = false}>Cancel</button>
					<button class="btn btn-danger" onclick={confirmClose}>Close</button>
				</div>
			</div>
		</div>
	{/if}

	{#if metaLoading}
		<div class="state-center">
			<Loader2 size={18} class="spin" />
			<span>Detecting database…</span>
		</div>
	{:else if metaError}
		<div class="state-center error">
			<AlertTriangle size={16} />
			<span>{metaError}</span>
		</div>
	{:else}
		<div class="modal-body">
			<!-- Connection panel -->
			{#if !connected}
				<section class="conn-section">
					{#if isDockerInternalHost}
						<div class="warn-notice">
							<AlertTriangle size={13} />
							<span>
								This service has no published port. The host <code>{host}</code> is a Docker-internal name — it only resolves when Shipyard itself runs inside Docker (production).
								To connect in dev, publish the port in the service settings first.
							</span>
						</div>
					{:else if meta?.detected}
						<div class="auto-detect-notice">
							<Info size={13} />
							<span>Auto-detected <strong>{meta.engine}</strong> at <code>{meta.host}:{meta.port}</code>. Enter credentials to connect.</span>
						</div>
					{/if}

					<div class="form-grid">
						<!-- Engine selector -->
						<div class="field">
							<label for="db-engine">Engine</label>
							<div class="select-wrap">
								<select id="db-engine" bind:value={engine} onchange={onEngineChange}>
									{#each ENGINE_OPTIONS as opt}
										<option value={opt.value}>{opt.label}</option>
									{/each}
								</select>
								<ChevronDown size={12} class="select-chevron" />
							</div>
						</div>

						<!-- Host + Port (all engines) -->
						<div class="field field-wide">
							<label for="db-host">Host</label>
							<input id="db-host" type="text" bind:value={host} placeholder="platform-uuid" spellcheck="false" />
						</div>

						<div class="field field-narrow">
							<label for="db-port">Port</label>
							<input id="db-port" type="number" bind:value={port} min="1" max="65535" />
						</div>

						<!-- Database / DB index (hide for Redis when not needed, relabel) -->
						{#if !isRedis || true}
							<div class="field {isSqlEngine || isMongo ? 'field-wide' : ''}">
								<label for="db-database">{dbFieldLabel}</label>
								<input
									id="db-database"
									type="text"
									bind:value={database}
									placeholder={isRedis ? '0' : 'mydb'}
									spellcheck="false"
								/>
							</div>
						{/if}

						<!-- Username — hidden for Redis (no username concept in basic Redis) -->
						{#if !isRedis}
							<div class="field">
								<label for="db-user">Username</label>
								<input id="db-user" type="text" bind:value={username}
									placeholder={isMongo ? 'admin' : 'postgres'}
									autocomplete="off" spellcheck="false" />
							</div>
						{/if}

						<!-- Password (all engines) -->
						<div class="field">
							<label for="db-pass">Password{isRedis ? ' (optional)' : ''}</label>
							<input id="db-pass" type="password" bind:value={password} autocomplete="new-password" />
						</div>
					</div>

					{#if connectError}
						<div class="error-banner">
							<AlertTriangle size={13} />
							<span>{connectError}</span>
						</div>
					{/if}

					<div class="conn-footer">
						<button class="btn btn-primary" onclick={connect} disabled={connecting}>
							{#if connecting}
								<Loader2 size={13} class="spin" />
								Connecting…
							{:else}
								<Play size={13} />
								Connect
							{/if}
						</button>
					</div>
				</section>
			{:else}
				<!-- Connected — show query editor -->
				<div class="editor-section">
					<!-- Connection status bar -->
					<div class="conn-bar">
						<span class="conn-detail">
							<span class="engine-badge {engine}">{engine}</span>
							<code>{host}:{port}</code>
							<span class="sep">·</span>
							<code>{database}</code>
							<span class="sep">·</span>
							<span>{username}</span>
						</span>
						<button class="btn-link" onclick={disconnect}>Disconnect</button>
					</div>

					<!-- Query editor (SQL / Redis command / MongoDB JSON) -->
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
							<button class="btn btn-primary btn-sm" onclick={runQuery} disabled={running || !sql.trim()}>
								{#if running}
									<Loader2 size={12} class="spin" />
									Running…
								{:else}
									<Play size={12} />
									Run
								{/if}
							</button>
						</div>
					</div>

					<!-- Results -->
					{#if queryError}
						<div class="error-banner">
							<AlertTriangle size={13} />
							<pre class="error-pre">{queryError}</pre>
						</div>
					{:else if result}
						<div class="results-section">
							<div class="results-meta">
								<span>{result.row_count} row{result.row_count !== 1 ? 's' : ''}</span>
								{#if result.truncated}
									<span class="truncated-badge">Limited to 1 000 rows</span>
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
			{/if}
		</div>
	{/if}
</div>

<style>
	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

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
		border-radius: 10px;
		box-shadow: 0 24px 64px rgba(0, 0, 0, 0.22);
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
		padding: 14px 18px;
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

	.conn-badge {
		font-size: 10px;
		font-weight: 600;
		padding: 2px 7px;
		border-radius: 10px;
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}
	.conn-badge.connected { background: #dcfce7; color: #15803d; }

	.icon-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 28px;
		height: 28px;
		border: none;
		background: transparent;
		color: var(--text-muted);
		cursor: pointer;
		border-radius: 4px;
		transition: background 0.12s;
	}
	.icon-btn:hover { background: var(--bg-muted); color: var(--text-primary); }

	.modal-body {
		flex: 1;
		overflow-y: auto;
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
	.state-center.error { color: #dc2626; }

	/* Connection form */
	.conn-section {
		padding: 20px;
		display: flex;
		flex-direction: column;
		gap: 16px;
	}

	.warn-notice {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		padding: 10px 14px;
		background: rgba(245, 158, 11, 0.08);
		border: 1px solid rgba(245, 158, 11, 0.3);
		border-radius: 6px;
		font-size: 13px;
		color: #92400e;
	}
	.warn-notice :global(svg) { flex-shrink: 0; margin-top: 1px; color: #d97706; }
	.warn-notice code { font-family: var(--font-mono, monospace); font-size: 12px; }

	.auto-detect-notice {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		padding: 10px 14px;
		background: rgba(59, 130, 246, 0.07);
		border: 1px solid rgba(59, 130, 246, 0.2);
		border-radius: 6px;
		font-size: 13px;
		color: var(--text-secondary, var(--text-muted));
	}
	.auto-detect-notice :global(svg) { flex-shrink: 0; margin-top: 1px; color: #3b82f6; }
	.auto-detect-notice code { font-family: var(--font-mono, monospace); font-size: 12px; }

	.form-grid {
		display: grid;
		grid-template-columns: 1fr 1fr 80px;
		gap: 12px;
	}

	.field { display: flex; flex-direction: column; gap: 5px; }
	.field-wide { grid-column: span 2; }
	.field-narrow { grid-column: span 1; }

	.field label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-muted);
		text-transform: uppercase;
		letter-spacing: 0.04em;
	}

	.field input,
	.field select {
		padding: 7px 10px;
		background: var(--bg-input, var(--bg-muted));
		border: 1px solid var(--border);
		border-radius: 6px;
		font-size: 13px;
		color: var(--text-primary);
		outline: none;
		font-family: inherit;
	}
	.field input:focus,
	.field select:focus { border-color: var(--accent); }

	.select-wrap { position: relative; }
	.select-wrap select { width: 100%; appearance: none; padding-right: 28px; }
	.select-wrap :global(.select-chevron) {
		position: absolute;
		right: 9px;
		top: 50%;
		transform: translateY(-50%);
		pointer-events: none;
		color: var(--text-muted);
	}

	.error-banner {
		display: flex;
		align-items: flex-start;
		gap: 8px;
		padding: 10px 14px;
		background: rgba(220, 38, 38, 0.06);
		border: 1px solid rgba(220, 38, 38, 0.25);
		border-radius: 6px;
		font-size: 13px;
		color: #dc2626;
	}
	.error-pre { margin: 0; font-family: var(--font-mono, monospace); font-size: 12px; white-space: pre-wrap; word-break: break-all; }

	.conn-footer { display: flex; justify-content: flex-end; }

	/* Buttons */
	.btn {
		display: inline-flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		border-radius: 6px;
		font-size: 13px;
		font-weight: 500;
		cursor: pointer;
		border: 1px solid transparent;
		transition: all 0.12s;
	}
	.btn:disabled { opacity: 0.5; cursor: not-allowed; }
	.btn-primary { background: var(--accent, #6366f1); color: #fff; border-color: var(--accent, #6366f1); }
	.btn-primary:hover:not(:disabled) { filter: brightness(1.08); }
	.btn-sm { padding: 5px 10px; font-size: 12px; }
	.btn-link {
		background: none;
		border: none;
		color: var(--text-muted);
		font-size: 12px;
		cursor: pointer;
		padding: 2px 4px;
	}
	.btn-link:hover { color: var(--text-primary); }

	/* Connected editor section */
	.editor-section {
		display: flex;
		flex-direction: column;
		gap: 0;
	}

	.conn-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 18px;
		background: var(--bg-muted);
		border-bottom: 1px solid var(--border);
		font-size: 12px;
		flex-shrink: 0;
	}

	.conn-detail {
		display: flex;
		align-items: center;
		gap: 6px;
		color: var(--text-muted);
	}
	.conn-detail code { font-family: var(--font-mono, monospace); }
	.sep { color: var(--border); }

	.engine-badge {
		font-size: 10px;
		font-weight: 700;
		padding: 2px 6px;
		border-radius: 4px;
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.engine-badge.postgres { background: #dbeafe; color: #1d4ed8; }
	.engine-badge.mysql    { background: #fef9c3; color: #a16207; }
	.engine-badge.mariadb  { background: #fce7f3; color: #9d174d; }
	.engine-badge.redis    { background: #fee2e2; color: #b91c1c; }
	.engine-badge.mongodb  { background: #dcfce7; color: #15803d; }

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
		background: var(--bg-base, #fafafa);
		border: none;
		outline: none;
		resize: vertical;
		font-family: var(--font-mono, 'Fira Mono', 'Consolas', monospace);
		font-size: 13px;
		color: var(--text-primary);
		line-height: 1.6;
		box-sizing: border-box;
	}

	.editor-actions {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 8px 14px;
		background: var(--bg-muted);
	}
	.editor-hint { font-size: 11px; color: var(--text-dim, var(--text-muted)); }

	/* Redis command: single-line style */
	.redis-editor {
		min-height: 44px;
		max-height: 44px;
		resize: none;
		font-size: 14px;
	}

	/* Results */
	.results-section {
		display: flex;
		flex-direction: column;
		flex: 1;
		overflow: hidden;
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

	.truncated-badge {
		padding: 1px 7px;
		background: rgba(245, 158, 11, 0.1);
		border: 1px solid rgba(245, 158, 11, 0.3);
		color: #92400e;
		border-radius: 10px;
		font-size: 11px;
	}

	.exec-time { margin-left: auto; color: var(--text-dim, var(--text-muted)); }

	.empty-result {
		padding: 24px 18px;
		color: var(--text-muted);
		font-size: 13px;
	}

	.table-scroll {
		overflow: auto;
		max-height: 320px;
	}

	.results-table {
		width: 100%;
		border-collapse: collapse;
		font-size: 12px;
		font-family: var(--font-mono, monospace);
	}

	.results-table th {
		position: sticky;
		top: 0;
		background: var(--bg-muted);
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

	.results-table tr:hover td { background: var(--bg-elevated, #f9fafb); }

	.null-cell { color: var(--text-dim, var(--text-muted)) !important; font-style: italic; }

	@media (max-width: 639px) {
		.form-grid { grid-template-columns: 1fr; }
		.field-wide, .field-narrow { grid-column: span 1; }
	}

	/* ── Close confirmation overlay ── */
	.confirm-overlay {
		position: absolute;
		inset: 0;
		background: rgba(0, 0, 0, 0.45);
		display: flex;
		align-items: center;
		justify-content: center;
		z-index: 10;
		border-radius: 10px;
	}

	.confirm-card {
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: 10px;
		padding: 24px 28px;
		width: min(340px, calc(100% - 40px));
		box-shadow: 0 16px 48px rgba(0, 0, 0, 0.2);
	}

	.confirm-title {
		margin: 0 0 8px;
		font-size: 15px;
		font-weight: 600;
		color: var(--text-primary);
	}

	.confirm-sub {
		margin: 0 0 20px;
		font-size: 13px;
		color: var(--text-muted);
		line-height: 1.5;
	}

	.confirm-actions {
		display: flex;
		justify-content: flex-end;
		gap: 8px;
	}

	.btn-ghost {
		background: transparent;
		border-color: var(--border);
		color: var(--text-secondary, var(--text-muted));
	}
	.btn-ghost:hover { background: var(--bg-muted); }

	.btn-danger {
		background: #dc2626;
		border-color: #dc2626;
		color: #fff;
	}
	.btn-danger:hover { background: #b91c1c; border-color: #b91c1c; }
</style>
