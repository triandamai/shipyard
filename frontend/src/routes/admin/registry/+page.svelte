<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Package, HardDrive, Layers, Archive, RefreshCw, Search, Trash2, ChevronRight, ChevronDown, X, AlertTriangle } from '@lucide/svelte';

	// ── Types ─────────────────────────────────────────────────────────────────────
	interface NsRow {
		id: string;
		slug: string;
		artifact_count: number;
		total_size: number;
		last_pushed: string | null;
		created_at?: string;
	}
	interface RepoRow {
		repo: string;
		kind: string;
		tag_count: number;
		total_size: number;
		last_pushed: string;
	}

	// ── State ─────────────────────────────────────────────────────────────────────
	let stats: {
		total_blobs: number;
		total_blob_size: number;
		total_artifacts: number;
		total_namespaces: number;
		hostname: string;
		storage_type: string;
	} | null = $state(null);

	let namespaces   = $state<NsRow[]>([]);
	let loading      = $state(true);
	let searchQ      = $state('');
	let searching    = $state(false);

	// Expanded namespace → repos
	let expandedNs   = $state<string | null>(null);
	let nsRepos      = $state<RepoRow[]>([]);
	let reposLoading = $state(false);

	// Delete confirmation
	let confirmDelete = $state<{ type: 'ns'; nsId: string; slug: string } | { type: 'repo'; nsId: string; repo: string } | null>(null);
	let deleting      = $state(false);
	let deleteError   = $state('');

	// ── Helpers ───────────────────────────────────────────────────────────────────
	function fmtBytes(n: number): string {
		if (!n) return '0 B';
		if (n < 1024) return `${n} B`;
		if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
		if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
		return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
	}
	function timeAgo(dateStr: string | null): string {
		if (!dateStr) return '—';
		const s = Math.floor((Date.now() - new Date(dateStr).getTime()) / 1000);
		if (s < 60)    return 'just now';
		if (s < 3600)  return `${Math.floor(s / 60)}m ago`;
		if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
		return `${Math.floor(s / 86400)}d ago`;
	}
	function kindColor(kind: string): string {
		if (kind === 'docker_image')   return '#3b82f6';
		if (kind === 'static_bundle')  return '#22c55e';
		if (kind === 'edge_function')  return '#f59e0b';
		return '#8b5cf6';
	}

	// ── Data loading ──────────────────────────────────────────────────────────────
	async function load() {
		loading = true;
		const [sRes, nRes] = await Promise.all([
			api.get('/admin/registry/stats'),
			api.get('/admin/registry/namespaces'),
		]);
		stats      = sRes.data ?? null;
		namespaces = nRes.data ?? [];
		loading    = false;
	}

	async function search() {
		if (!searchQ.trim()) { return load(); }
		searching = true;
		const res = await api.get(`/admin/registry/namespaces/search?q=${encodeURIComponent(searchQ.trim())}`);
		if (res.data) namespaces = res.data;
		searching = false;
	}

	let searchTimeout: ReturnType<typeof setTimeout>;
	function onSearchInput() {
		clearTimeout(searchTimeout);
		searchTimeout = setTimeout(search, 280);
	}

	async function toggleNs(ns: NsRow) {
		if (expandedNs === ns.id) { expandedNs = null; nsRepos = []; return; }
		expandedNs   = ns.id;
		reposLoading = true;
		nsRepos      = [];
		const res = await api.get(`/admin/registry/namespaces/${ns.id}/repos`);
		nsRepos      = res.data ?? [];
		reposLoading = false;
	}

	// ── Delete ────────────────────────────────────────────────────────────────────
	async function executeDelete() {
		if (!confirmDelete) return;
		deleting    = true;
		deleteError = '';
		let res;
		if (confirmDelete.type === 'ns') {
			res = await api.delete(`/admin/registry/namespaces/${confirmDelete.nsId}`);
		} else {
			res = await api.delete(`/admin/registry/namespaces/${confirmDelete.nsId}/repos/${encodeURIComponent(confirmDelete.repo)}`);
		}
		if (res.error) {
			deleteError = res.error.message;
			deleting    = false;
			return;
		}
		confirmDelete = null;
		deleting      = false;
		// Reload
		if (expandedNs && confirmDelete === null) {
			// repo deleted — refresh repos
			const nId = expandedNs;
			nsRepos = [];
			reposLoading = true;
			const r = await api.get(`/admin/registry/namespaces/${nId}/repos`);
			nsRepos = r.data ?? [];
			reposLoading = false;
		}
		await load();
	}

	onMount(load);
</script>

<!-- Delete confirmation modal -->
{#if confirmDelete}
<div class="modal-backdrop" role="dialog" aria-modal="true">
	<div class="modal">
		<div class="modal-header">
			<AlertTriangle size={16} style="color:#ef4444" />
			<h3>
				{confirmDelete.type === 'ns'
					? `Delete namespace "${confirmDelete.slug}"?`
					: `Delete repository "${confirmDelete.repo}"?`}
			</h3>
			<button class="icon-btn" onclick={() => (confirmDelete = null)} disabled={deleting}><X size={14} /></button>
		</div>
		<div class="modal-body">
			<p>
				{#if confirmDelete.type === 'ns'}
					This will permanently remove the namespace and <strong>all artifacts</strong> inside it.
					Blob data will be cleaned up by the next GC run.
				{:else}
					All tags for <code>{confirmDelete.repo}</code> will be permanently deleted.
				{/if}
				This cannot be undone.
			</p>
			{#if deleteError}<p class="delete-error">{deleteError}</p>{/if}
		</div>
		<div class="modal-footer">
			<button class="btn-danger" onclick={executeDelete} disabled={deleting}>
				{deleting ? 'Deleting…' : 'Yes, delete'}
			</button>
			<button class="btn-ghost" onclick={() => (confirmDelete = null)} disabled={deleting}>Cancel</button>
		</div>
	</div>
</div>
{/if}

<div class="page">
	<div class="page-header">
		<div>
			<h1>Registry</h1>
			<p>Artifact storage — blobs, manifests, namespaces</p>
		</div>
		<button class="ftr-btn" onclick={load} title="Refresh">
			<RefreshCw size={14} />
		</button>
	</div>

	{#if loading}
		<div class="loading-row">
			{#each [1,2,3,4] as _}<div class="skel"></div>{/each}
		</div>
	{:else if stats}
		<!-- Stat cards -->
		<div class="cards">
			<div class="card">
				<div class="card-icon blue"><Layers size={16} /></div>
				<div class="card-body">
					<div class="card-val">{stats.total_blobs.toLocaleString()}</div>
					<div class="card-label">Total Blobs</div>
				</div>
			</div>
			<div class="card">
				<div class="card-icon purple"><HardDrive size={16} /></div>
				<div class="card-body">
					<div class="card-val">{fmtBytes(stats.total_blob_size)}</div>
					<div class="card-label">Blob Storage Used</div>
				</div>
			</div>
			<div class="card">
				<div class="card-icon green"><Package size={16} /></div>
				<div class="card-body">
					<div class="card-val">{stats.total_artifacts.toLocaleString()}</div>
					<div class="card-label">Artifacts (tags)</div>
				</div>
			</div>
			<div class="card">
				<div class="card-icon orange"><Archive size={16} /></div>
				<div class="card-body">
					<div class="card-val">{stats.total_namespaces.toLocaleString()}</div>
					<div class="card-label">Namespaces</div>
				</div>
			</div>
		</div>

		<!-- Config -->
		<div class="config-box">
			<div class="config-row">
				<span class="config-key">Hostname</span>
				<code class="config-val">{stats.hostname || '(not set)'}</code>
			</div>
			<div class="config-row">
				<span class="config-key">Storage Backend</span>
				<span class="badge badge--{stats.storage_type === 's3' ? 'blue' : 'grey'}">
					{stats.storage_type === 's3' ? 'S3 / MinIO' : 'Local Disk'}
				</span>
			</div>
			{#if stats.hostname}
				<div class="config-row">
					<span class="config-key">Login Command</span>
					<code class="config-val">docker login {stats.hostname}</code>
				</div>
			{/if}
		</div>

		<!-- Namespace search + list -->
		<div class="section">
			<div class="section-header">
				<h2>Namespaces</h2>
				<div class="search-wrap">
					<Search size={13} class="search-icon" />
					<input
						class="search-input"
						type="text"
						placeholder="Search by slug…"
						bind:value={searchQ}
						oninput={onSearchInput}
					/>
					{#if searching}
						<div class="spin-sm"></div>
					{/if}
				</div>
			</div>

			{#if namespaces.length === 0}
				<div class="empty">
					{searchQ ? `No namespaces matching "${searchQ}"` : 'No namespaces yet. Artifacts are registered here when the build engine first pushes to a project.'}
				</div>
			{:else}
				<div class="ns-list">
					{#each namespaces as ns (ns.id)}
						<div class="ns-row">
							<div class="ns-main">
								<button class="ns-expand" onclick={() => toggleNs(ns)}>
									{#if expandedNs === ns.id}
										<ChevronDown size={14} />
									{:else}
										<ChevronRight size={14} />
									{/if}
								</button>
								<div class="ns-info">
									<span class="ns-slug mono">{ns.slug}</span>
									<span class="ns-meta">
										{ns.artifact_count} artifact{ns.artifact_count !== 1 ? 's' : ''}
										· {fmtBytes(ns.total_size)}
										{#if ns.last_pushed}· {timeAgo(ns.last_pushed)}{/if}
									</span>
								</div>
								<button
									class="delete-btn"
									title="Delete namespace"
									onclick={() => (confirmDelete = { type: 'ns', nsId: ns.id, slug: ns.slug })}
								>
									<Trash2 size={13} />
								</button>
							</div>

							{#if expandedNs === ns.id}
								<div class="repo-list">
									{#if reposLoading}
										<div class="repo-loading">Loading repositories…</div>
									{:else if nsRepos.length === 0}
										<div class="repo-empty">No repositories in this namespace.</div>
									{:else}
										{#each nsRepos as r (r.repo)}
											<div class="repo-row">
												<div class="repo-kind-dot" style="background:{kindColor(r.kind)}"></div>
												<div class="repo-info">
													<span class="repo-name mono">{r.repo}</span>
													<span class="repo-meta">
														<span class="kind-pill" style="color:{kindColor(r.kind)}">{r.kind}</span>
														· {r.tag_count} tag{r.tag_count !== 1 ? 's' : ''}
														· {fmtBytes(r.total_size)}
														· {timeAgo(r.last_pushed)}
													</span>
												</div>
												<button
													class="delete-btn"
													title="Delete repository"
													onclick={() => (confirmDelete = { type: 'repo', nsId: ns.id, repo: r.repo })}
												>
													<Trash2 size={13} />
												</button>
											</div>
										{/each}
									{/if}
								</div>
							{/if}
						</div>
					{/each}
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	.page {
		padding: 28px 32px 40px;
		display: flex;
		flex-direction: column;
		gap: 24px;
		max-width: 900px;
	}
	.page-header {
		display: flex;
		align-items: flex-start;
		justify-content: space-between;
	}
	.page-header h1 { font-size: 20px; font-weight: 700; margin: 0 0 2px; letter-spacing: -0.02em; color: var(--text); }
	.page-header p  { font-size: 13px; color: var(--text-2); margin: 0; }

	.ftr-btn {
		display: flex; align-items: center; justify-content: center;
		width: 32px; height: 32px;
		border-radius: 7px; border: 1px solid var(--border);
		background: var(--surface); color: var(--text-2); cursor: pointer;
	}
	.ftr-btn:hover { background: var(--surface-2); color: var(--text); }

	/* ── Stat cards ── */
	.cards { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }
	.card {
		display: flex; align-items: center; gap: 12px; padding: 16px;
		background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius);
		min-width: 0;
	}
	.card-body { min-width: 0; }
	.card-icon { width: 36px; height: 36px; border-radius: 8px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; }
	.card-icon.blue   { background: rgba(59,130,246,0.12); color: #3b82f6; }
	.card-icon.purple { background: rgba(139,92,246,0.12); color: #8b5cf6; }
	.card-icon.green  { background: rgba(34,197,94,0.12);  color: #22c55e; }
	.card-icon.orange { background: rgba(249,115,22,0.12); color: #f97316; }
	.card-val   { font-size: 19px; font-weight: 700; color: var(--text); line-height: 1.1; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.card-label { font-size: 11px; color: var(--text-3); margin-top: 3px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	/* ── Config box ── */
	.config-box { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 0 16px; }
	.config-row { display: flex; align-items: center; gap: 12px; padding: 11px 0; border-bottom: 1px solid var(--border); font-size: 13px; }
	.config-row:last-child { border-bottom: none; }
	.config-key  { width: 140px; flex-shrink: 0; color: var(--text-2); font-weight: 500; }
	.config-val  { font-family: var(--mono); font-size: 12px; color: var(--text); }
	code.config-val { background: var(--surface-2); padding: 2px 8px; border-radius: 5px; }
	.badge { display: inline-flex; align-items: center; font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: 999px; }
	.badge--blue { background: rgba(59,130,246,0.1); color: #3b82f6; border: 1px solid rgba(59,130,246,0.2); }
	.badge--grey { background: var(--surface-2); color: var(--text-2); border: 1px solid var(--border); }

	/* ── Section ── */
	.section { display: flex; flex-direction: column; gap: 10px; }
	.section-header { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
	.section-header h2 { font-size: 14px; font-weight: 600; margin: 0; color: var(--text); }

	/* ── Search ── */
	.search-wrap {
		position: relative; display: flex; align-items: center; gap: 6px;
		background: var(--surface); border: 1px solid var(--border);
		border-radius: 7px; padding: 0 10px; height: 32px; min-width: 220px;
	}
	.search-wrap :global(.search-icon) { color: var(--text-2); flex-shrink: 0; }
	.search-input {
		background: transparent; border: none; outline: none;
		font-size: 13px; color: var(--text); flex: 1; min-width: 0;
	}
	.search-input::placeholder { color: var(--text-3); }
	.spin-sm {
		width: 12px; height: 12px; flex-shrink: 0;
		border: 2px solid var(--border); border-top-color: var(--accent);
		border-radius: 50%; animation: spin 0.6s linear infinite;
	}

	/* ── Namespace list ── */
	.ns-list { display: flex; flex-direction: column; gap: 0; border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; }
	.ns-row { border-bottom: 1px solid var(--border); }
	.ns-row:last-child { border-bottom: none; }

	.ns-main {
		display: flex; align-items: center; gap: 8px;
		padding: 10px 12px; background: var(--surface);
		transition: background 0.1s;
	}
	.ns-main:hover { background: var(--surface-2); }

	.ns-expand {
		display: flex; align-items: center; justify-content: center;
		width: 22px; height: 22px; flex-shrink: 0;
		border: none; background: transparent; color: var(--text-2);
		cursor: pointer; border-radius: 4px;
	}
	.ns-expand:hover { background: var(--surface-3, var(--border)); }

	.ns-info { flex: 1; display: flex; flex-direction: column; gap: 1px; min-width: 0; }
	.ns-slug { font-size: 13px; font-weight: 500; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
	.ns-meta { font-size: 11px; color: var(--text-2); }

	/* ── Delete button ── */
	.delete-btn {
		display: flex; align-items: center; justify-content: center;
		width: 28px; height: 28px; border: none; background: transparent;
		color: var(--text-2); cursor: pointer; border-radius: 5px;
		flex-shrink: 0; transition: background 0.1s, color 0.1s;
	}
	.delete-btn:hover { background: rgba(239,68,68,.1); color: #ef4444; }

	/* ── Repo list (expanded) ── */
	.repo-list { border-top: 1px solid var(--border); background: var(--surface-2, rgba(0,0,0,.02)); }
	.repo-loading, .repo-empty { font-size: 12px; color: var(--text-2); padding: 10px 20px; }
	.repo-row {
		display: flex; align-items: center; gap: 10px;
		padding: 8px 12px 8px 44px; border-bottom: 1px solid var(--border);
		transition: background 0.1s;
	}
	.repo-row:last-child { border-bottom: none; }
	.repo-row:hover { background: var(--row-hover, rgba(0,0,0,.03)); }
	.repo-kind-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }
	.repo-info { flex: 1; display: flex; flex-direction: column; gap: 1px; min-width: 0; }
	.repo-name { font-size: 12px; font-weight: 500; color: var(--text); }
	.repo-meta { font-size: 11px; color: var(--text-2); display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
	.kind-pill { font-size: 10px; font-weight: 600; }

	/* ── Empty ── */
	.empty {
		font-size: 13px; color: var(--text-3);
		background: var(--surface); border: 1px dashed var(--border);
		border-radius: var(--radius); padding: 24px 20px; text-align: center;
	}

	/* ── Loading skeleton ── */
	.loading-row { display: flex; gap: 12px; }
	.skel { flex: 1; height: 80px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); animation: pulse 1.5s ease-in-out infinite; }

	/* ── Delete modal ── */
	.modal-backdrop {
		position: fixed; inset: 0; background: rgba(0,0,0,.65);
		display: flex; align-items: center; justify-content: center; z-index: 300; padding: 16px;
	}
	.modal {
		background: var(--surface, #ffffff);
		border: 1px solid var(--border, #e3e3e3);
		border-radius: 10px; width: 100%; max-width: 420px; overflow: hidden;
		box-shadow: 0 8px 40px rgba(0,0,0,.45);
		color: var(--text, #111);
	}
	.modal-header {
		display: flex; align-items: center; gap: 10px;
		padding: 14px 16px; border-bottom: 1px solid var(--border, #e3e3e3);
		background: var(--surface, #ffffff);
	}
	.modal-header h3 { flex: 1; margin: 0; font-size: 14px; font-weight: 600; color: var(--text, #111); }
	.modal-body {
		padding: 16px; font-size: 13px; color: var(--text-2, #555); line-height: 1.6;
		background: var(--surface, #ffffff);
	}
	.modal-body p { margin: 0 0 8px; }
	.modal-body p:last-child { margin: 0; }
	.modal-body strong { color: var(--text, #111); }
	.modal-body code { font-family: var(--mono); font-size: 12px; background: var(--surface-2, #f0f0f0); padding: 1px 5px; border-radius: 4px; color: var(--text, #111); }
	.modal-footer {
		display: flex; gap: 8px; padding: 12px 16px; border-top: 1px solid var(--border, #e3e3e3);
		background: var(--surface, #ffffff);
	}
	.icon-btn { display: flex; align-items: center; justify-content: center; width: 28px; height: 28px; border: none; background: transparent; color: var(--text-2, #555); cursor: pointer; border-radius: 4px; margin-left: auto; }
	.icon-btn:hover { background: var(--surface-2, #f0f0f0); }

	.btn-danger {
		padding: 6px 14px; background: #ef4444; color: #fff;
		border: none; border-radius: 6px; font-size: 13px; font-weight: 500; cursor: pointer;
	}
	.btn-danger:disabled { opacity: 0.6; cursor: not-allowed; }
	.btn-ghost {
		padding: 6px 14px; background: transparent; color: var(--text-2, #555);
		border: 1px solid var(--border, #e3e3e3); border-radius: 6px; font-size: 13px; cursor: pointer;
	}
	.delete-error { color: #ef4444; font-size: 12px; margin-top: 8px; }

	.mono { font-family: var(--mono); }

	@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:0.5} }
	@keyframes spin   { to { transform: rotate(360deg); } }

	@media (max-width: 768px) {
		.page  { padding: 16px; }
	}
	@media (max-width: 600px) {
		.config-row { flex-direction: column; align-items: flex-start; gap: 4px; }
		.config-key { width: auto; }
		.ns-main { flex-wrap: wrap; }
		.ns-info { width: 100%; order: 2; margin-top: 4px; }
		.delete-btn { order: 1; margin-left: auto; }
	}
</style>
