<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { Package, HardDrive, Layers, Archive, RefreshCw, Search, Trash2, ChevronRight, ChevronDown } from '@lucide/svelte';
	import {
		PageHeader,
		StatCard,
		SectionLabel,
		TextField,
		Button,
		Badge,
		ActivityList,
		ListRow,
		EmptyState,
		InlineAlert,
		Spinner,
		Skeleton,
		ConfirmDialog
	} from '$lib/components/ui';

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

	let namespaces = $state<NsRow[]>([]);
	let loading    = $state(true);
	let searchQ    = $state('');
	let searching  = $state(false);

	// Expanded namespaces → lazily loaded repos. Follows the Set<string> expand
	// pattern established in Tasks 31/35/36/48: a Set of expanded row ids,
	// replaced (never mutated) so Svelte 5 picks up the change. Repos for each
	// expanded namespace are fetched once and cached by namespace id.
	let expanded        = $state(new Set<string>());
	let nsRepos         = $state<Record<string, RepoRow[]>>({});
	let reposLoadingFor = $state(new Set<string>());

	// Delete confirmation (namespace or repo)
	let deleteTarget = $state<{ type: 'ns'; nsId: string; slug: string } | { type: 'repo'; nsId: string; repo: string } | null>(null);
	let confirmOpen  = $state(false);
	let deleteError  = $state('');

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
	function kindTone(kind: string): 'blue' | 'green' | 'yellow' | 'red' {
		if (kind === 'docker_image')  return 'blue';
		if (kind === 'static_bundle') return 'green';
		if (kind === 'edge_function') return 'yellow';
		return 'red';
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

	// Debounced search — preserves the original 280ms setTimeout debounce.
	// TextField (unlike a raw <input>) exposes no oninput prop, so the
	// debounce is re-wired onto an $effect watching the bound value instead —
	// the same approach the audit log page uses for its org-ID filter.
	let searchDebounce: ReturnType<typeof setTimeout> | undefined;
	let firstSearchRun = true;
	$effect(() => {
		searchQ;
		if (firstSearchRun) { firstSearchRun = false; return; }
		clearTimeout(searchDebounce);
		searchDebounce = setTimeout(search, 280);
		return () => clearTimeout(searchDebounce);
	});

	async function loadRepos(nsId: string) {
		const startLoading = new Set(reposLoadingFor);
		startLoading.add(nsId);
		reposLoadingFor = startLoading;

		const res = await api.get(`/admin/registry/namespaces/${nsId}/repos`);
		nsRepos = { ...nsRepos, [nsId]: res.data ?? [] };

		const doneLoading = new Set(reposLoadingFor);
		doneLoading.delete(nsId);
		reposLoadingFor = doneLoading;
	}

	function toggleExpand(ns: NsRow) {
		const next = new Set(expanded);
		if (next.has(ns.id)) {
			next.delete(ns.id);
		} else {
			next.add(ns.id);
			// Lazily load this namespace's repos the first time it is expanded.
			if (!(ns.id in nsRepos)) loadRepos(ns.id);
		}
		expanded = next;
	}

	// ── Delete ────────────────────────────────────────────────────────────────────
	function askDeleteNs(ns: NsRow) {
		deleteError = '';
		deleteTarget = { type: 'ns', nsId: ns.id, slug: ns.slug };
		confirmOpen = true;
	}
	function askDeleteRepo(ns: NsRow, r: RepoRow) {
		deleteError = '';
		deleteTarget = { type: 'repo', nsId: ns.id, repo: r.repo };
		confirmOpen = true;
	}

	async function executeDelete() {
		if (!deleteTarget) return;
		const target = deleteTarget;

		const res = target.type === 'ns'
			? await api.delete(`/admin/registry/namespaces/${target.nsId}`)
			: await api.delete(`/admin/registry/namespaces/${target.nsId}/repos/${encodeURIComponent(target.repo)}`);

		if (res.error) {
			deleteError = res.error.message;
			return;
		}

		if (target.type === 'ns') {
			const next = new Set(expanded);
			next.delete(target.nsId);
			expanded = next;
			const { [target.nsId]: _dropped, ...rest } = nsRepos;
			nsRepos = rest;
		} else {
			// Repo deleted — refresh that namespace's (still expanded) repo list.
			await loadRepos(target.nsId);
		}

		deleteTarget = null;
		await load();
	}

	onMount(load);
</script>

<div class="page">
	<PageHeader title="Registry" subtitle="Artifact storage — blobs, manifests, namespaces">
		{#snippet actions()}
			<Button variant="secondary" size="icon" onclick={load}>
				<RefreshCw size={14} />
			</Button>
		{/snippet}
	</PageHeader>

	{#if deleteError}
		<InlineAlert tone="error">{deleteError}</InlineAlert>
	{/if}

	{#if loading}
		<div class="reg-grid4">
			{#each Array(4) as _}<Skeleton variant="card" height="88px" />{/each}
		</div>
	{:else if stats}
		<!-- Stat cards -->
		<div class="reg-grid4">
			<StatCard tone="blue" value={stats.total_blobs.toLocaleString()} label="Total Blobs">
				{#snippet icon()}<Layers size={16} />{/snippet}
			</StatCard>
			<StatCard tone="yellow" value={fmtBytes(stats.total_blob_size)} label="Blob Storage Used">
				{#snippet icon()}<HardDrive size={16} />{/snippet}
			</StatCard>
			<StatCard tone="green" value={stats.total_artifacts.toLocaleString()} label="Artifacts (tags)">
				{#snippet icon()}<Package size={16} />{/snippet}
			</StatCard>
			<StatCard tone="red" value={stats.total_namespaces.toLocaleString()} label="Namespaces">
				{#snippet icon()}<Archive size={16} />{/snippet}
			</StatCard>
		</div>

		<!-- Config -->
		<div class="reg-config">
			<div class="reg-config-row">
				<span class="reg-config-key">Hostname</span>
				<code class="reg-config-val">{stats.hostname || '(not set)'}</code>
			</div>
			<div class="reg-config-row">
				<span class="reg-config-key">Storage Backend</span>
				<Badge tone={stats.storage_type === 's3' ? 'blue' : 'neutral'}>
					{stats.storage_type === 's3' ? 'S3 / MinIO' : 'Local Disk'}
				</Badge>
			</div>
			{#if stats.hostname}
				<div class="reg-config-row">
					<span class="reg-config-key">Login Command</span>
					<code class="reg-config-val">docker login {stats.hostname}</code>
				</div>
			{/if}
		</div>

		<!-- Namespace search + list -->
		<div class="reg-section">
			<div class="reg-section-header">
				<SectionLabel>Namespaces</SectionLabel>
				<div class="reg-search-wrap">
					<TextField bind:value={searchQ} placeholder="Search by slug…">
						{#snippet icon()}<Search size={13} />{/snippet}
					</TextField>
					{#if searching}<Spinner size={13} />{/if}
				</div>
			</div>

			{#if namespaces.length === 0}
				<EmptyState
					message={searchQ ? `No namespaces matching "${searchQ}"` : 'No namespaces yet.'}
					sub={searchQ ? undefined : 'Artifacts are registered here when the build engine first pushes to a project.'}
				/>
			{:else}
				<ActivityList>
					{#each namespaces as ns (ns.id)}
						<div class="reg-ns-block">
							<!-- svelte-ignore a11y_click_events_have_key_events -->
							<!-- svelte-ignore a11y_no_static_element_interactions -->
							<div
								class="reg-ns-toggle"
								role="button"
								tabindex="0"
								onclick={() => toggleExpand(ns)}
								onkeydown={(e) => {
									if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); toggleExpand(ns); }
								}}
							>
								<ListRow
									iconTone="blue"
									title={ns.slug}
									meta="{ns.artifact_count} artifact{ns.artifact_count !== 1 ? 's' : ''} · {fmtBytes(ns.total_size)}{ns.last_pushed ? ` · ${timeAgo(ns.last_pushed)}` : ''}"
								>
									{#snippet icon()}
										{#if expanded.has(ns.id)}<ChevronDown size={14} />{:else}<ChevronRight size={14} />{/if}
									{/snippet}
									{#snippet trailing()}
										<button
											type="button"
											class="reg-delete-btn"
											title="Delete namespace"
											onclick={(e) => { e.stopPropagation(); askDeleteNs(ns); }}
										>
											<Trash2 size={13} />
										</button>
									{/snippet}
								</ListRow>
							</div>

							{#if expanded.has(ns.id)}
								<div class="reg-repo-sublist">
									{#if reposLoadingFor.has(ns.id)}
										<div class="reg-repo-loading"><Spinner size={13} /> Loading repositories…</div>
									{:else if (nsRepos[ns.id] ?? []).length === 0}
										<EmptyState message="No repositories in this namespace." />
									{:else}
										<ActivityList>
											{#each nsRepos[ns.id] as r (r.repo)}
												<ListRow
													iconTone={kindTone(r.kind)}
													title={r.repo}
													meta="{r.kind} · {r.tag_count} tag{r.tag_count !== 1 ? 's' : ''} · {fmtBytes(r.total_size)} · {timeAgo(r.last_pushed)}"
												>
													{#snippet icon()}<Package size={12} />{/snippet}
													{#snippet trailing()}
														<button
															type="button"
															class="reg-delete-btn"
															title="Delete repository"
															onclick={() => askDeleteRepo(ns, r)}
														>
															<Trash2 size={13} />
														</button>
													{/snippet}
												</ListRow>
											{/each}
										</ActivityList>
									{/if}
								</div>
							{/if}
						</div>
					{/each}
				</ActivityList>
			{/if}
		</div>
	{/if}
</div>

<ConfirmDialog
	bind:open={confirmOpen}
	title={deleteTarget?.type === 'ns'
		? `Delete namespace "${deleteTarget.slug}"?`
		: deleteTarget?.type === 'repo'
			? `Delete repository "${deleteTarget.repo}"?`
			: 'Delete'}
	message={deleteTarget?.type === 'ns'
		? 'This will permanently remove the namespace and all artifacts inside it. Blob data will be cleaned up by the next GC run. This cannot be undone.'
		: deleteTarget?.type === 'repo'
			? `All tags for "${deleteTarget.repo}" will be permanently deleted. This cannot be undone.`
			: ''}
	confirmLabel="Delete"
	onConfirm={executeDelete}
/>

<style>
	.page {
		padding: 28px 32px 40px;
		display: flex;
		flex-direction: column;
		gap: 24px;
		max-width: 900px;
	}

	/* ── Stat cards ── */
	.reg-grid4 { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 12px; }

	/* ── Config box ── */
	.reg-config { background: var(--bg-surface); border: 1px solid var(--border); border-radius: var(--radius-lg); padding: 0 16px; }
	.reg-config-row { display: flex; align-items: center; gap: 12px; padding: 11px 0; border-bottom: 1px solid var(--border); font-size: 13px; }
	.reg-config-row:last-child { border-bottom: none; }
	.reg-config-key { width: 140px; flex-shrink: 0; color: var(--text-secondary); font-weight: 500; }
	.reg-config-val { font-family: var(--font-mono); font-size: 12px; color: var(--text-primary); background: var(--bg-elevated); padding: 2px 8px; border-radius: var(--radius-sm); }

	/* ── Section ── */
	.reg-section { display: flex; flex-direction: column; gap: 10px; }
	.reg-section-header { display: flex; align-items: center; justify-content: space-between; gap: 12px; }

	/* ── Search ── */
	.reg-search-wrap { display: flex; align-items: center; gap: 8px; min-width: 220px; }

	/* ── Namespace rows ── */
	.reg-ns-block + .reg-ns-block { margin-top: 0; }
	.reg-ns-toggle { cursor: pointer; border-radius: var(--radius-sm); }
	.reg-ns-toggle:hover { background: var(--bg-hover); }
	.reg-ns-toggle:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }

	/* ── Delete button ── */
	.reg-delete-btn {
		display: flex; align-items: center; justify-content: center;
		width: 26px; height: 26px; border: none; background: transparent;
		color: var(--text-muted); cursor: pointer; border-radius: var(--radius-sm);
		flex-shrink: 0; transition: background var(--transition-fast), color var(--transition-fast);
	}
	.reg-delete-btn:hover { background: var(--accent-red-muted); color: var(--accent-red); }

	/* ── Repo sublist (expanded) ── */
	.reg-repo-sublist { margin: 2px 0 8px 40px; }

	@media (max-width: 768px) {
		.page { padding: 16px; }
	}
	@media (max-width: 600px) {
		.reg-config-row { flex-direction: column; align-items: flex-start; gap: 4px; }
		.reg-config-key { width: auto; }
		.reg-repo-sublist { margin-left: 16px; }
	}
</style>
