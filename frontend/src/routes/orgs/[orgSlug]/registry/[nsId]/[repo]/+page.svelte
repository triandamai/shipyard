<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { Tag, ChevronRight, RefreshCw, FolderOpen, Trash2 } from '@lucide/svelte';
	import { onMount } from 'svelte';

	let orgId   = $derived($orgStore.activeOrg?.id ?? '');
	let orgSlug = $derived(page.params.orgSlug ?? '');
	let nsId    = $derived(page.params.nsId ?? '');
	let repo    = $derived(decodeURIComponent(page.params.repo ?? ''));

	type ArtTag = {
		id: string;
		tag: string;
		kind: string;
		size_bytes: number;
		manifest_digest: string;
		metadata: Record<string, unknown>;
		pushed_at: string;
	};

	let nsSlug  = $state('');
	let tags: ArtTag[] = $state([]);
	let loading = $state(false);

	// ── Delete dialog ─────────────────────────────────────────────────────────
	let showDeleteDialog = $state(false);
	let deleteConfirmText = $state('');
	let deleting = $state(false);
	let deleteError = $state('');

	function openDeleteDialog() {
		deleteConfirmText = '';
		deleteError = '';
		showDeleteDialog = true;
	}

	function closeDeleteDialog() {
		if (deleting) return;
		showDeleteDialog = false;
	}

	async function confirmDelete() {
		if (deleteConfirmText !== repo) return;
		deleting = true;
		deleteError = '';
		const res = await api.delete(`/orgs/${orgId}/registry/namespaces/${nsId}/repos/${encodeURIComponent(repo)}`);
		deleting = false;
		if (res.error) {
			deleteError = res.error.message ?? 'Delete failed.';
			return;
		}
		await goto(`/orgs/${orgSlug}/registry/${nsId}`);
	}

	async function load() {
		if (!orgId || !nsId || !repo) return;
		loading = true;

		// Load namespace slug for breadcrumb
		const nsRes = await api.get(`/orgs/${orgId}/registry/namespaces?page=1&per_page=200`);
		const nsList = nsRes.data?.items ?? nsRes.data ?? [];
		const ns = nsList.find((n: { id: string; slug: string }) => n.id === nsId);
		if (ns) nsSlug = ns.slug;

		const res = await api.get(
			`/orgs/${orgId}/registry/namespaces/${nsId}/repos/${encodeURIComponent(repo)}/tags`
		);
		tags    = res.data ?? [];
		loading = false;
	}

	onMount(load);
	$effect(() => { if (orgId && nsId && repo) load(); });

	const kindMeta: Record<string, { label: string; color: string }> = {
		docker_image:  { label: 'Image',   color: '#3b82f6' },
		static_bundle: { label: 'Static',  color: '#8b5cf6' },
		edge_function: { label: 'Edge Fn', color: '#f59e0b' },
		build_cache:   { label: 'Cache',   color: '#6b7280' },
	};
	function kindLabel(k: string) { return kindMeta[k]?.label ?? k; }
	function kindColor(k: string) { return kindMeta[k]?.color ?? '#6b7280'; }

	function fmtBytes(n: number) {
		if (!n) return '0 B';
		if (n < 1024) return `${n} B`;
		if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
		if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
		return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
	}

	function timeAgo(d: string | null) {
		if (!d) return '—';
		const s = Math.floor((Date.now() - new Date(d).getTime()) / 1000);
		if (s < 60)    return 'just now';
		if (s < 3600)  return `${Math.floor(s / 60)}m ago`;
		if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
		return `${Math.floor(s / 86400)}d ago`;
	}
</script>

<div class="browser">
	<div class="topbar">
		<nav class="breadcrumb" aria-label="Registry navigation">
			<a class="bc-item" href="/orgs/{orgSlug}/registry">
				<FolderOpen size={13} /> Namespaces
			</a>
			<ChevronRight size={12} class="bc-sep" />
			<a class="bc-item" href="/orgs/{orgSlug}/registry/{nsId}">
				{nsSlug || nsId}
			</a>
			<ChevronRight size={12} class="bc-sep" />
			<span class="bc-item bc-active"><Tag size={13} /> {repo}</span>
		</nav>
		<div class="topbar-actions">
			<button class="icon-btn" onclick={load} title="Refresh" aria-label="Refresh">
				<RefreshCw size={13} />
			</button>
			<button class="delete-btn" onclick={openDeleteDialog} title="Delete repository">
				<Trash2 size={13} />
				Delete repository
			</button>
		</div>
	</div>

	{#if loading}
		<div class="skel-list">
			{#each [1,2,3] as _}<div class="skel"></div>{/each}
		</div>
	{:else if tags.length === 0}
		<div class="empty">
			<Tag size={32} />
			<p>No tags in this repository.</p>
		</div>
	{:else}
		<div class="table-wrap">
			<table class="table">
				<thead>
					<tr>
						<th>Tag</th>
						<th>Kind</th>
						<th class="num-col">Size</th>
						<th>Digest</th>
						<th class="num-col">Pushed</th>
					</tr>
				</thead>
				<tbody>
					{#each tags as t}
						<tr>
							<td><span class="tag-pill">{t.tag}</span></td>
							<td>
								<span class="kind-badge" style="--kc:{kindColor(t.kind)}">
									{kindLabel(t.kind)}
								</span>
							</td>
							<td class="num-col muted">{fmtBytes(t.size_bytes)}</td>
							<td><span class="digest mono">{t.manifest_digest.slice(0, 23)}…</span></td>
							<td class="num-col muted">{timeAgo(t.pushed_at)}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<div class="pull-cmd">
			<span class="pull-label">Pull command</span>
			<code class="pull-code">docker pull registry.{page.url.hostname.split('.').slice(1).join('.')}/{nsSlug}/{repo}:&lt;tag&gt;</code>
		</div>
	{/if}
</div>

{#if showDeleteDialog}
	<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
	<div class="overlay" onclick={closeDeleteDialog}>
		<div class="dialog" onclick={(e) => e.stopPropagation()} role="dialog" aria-modal="true"
			aria-labelledby="dlg-title">
			<div class="dlg-header">
				<div class="dlg-icon-wrap">
					<Trash2 size={18} />
				</div>
				<div>
					<h2 id="dlg-title">Delete repository</h2>
					<p class="dlg-sub">This will permanently delete all tags and artifacts in <strong>{repo}</strong>.</p>
				</div>
			</div>

			<div class="dlg-body">
				<p class="dlg-warning">
					This action <strong>cannot be undone</strong>. Blobs may still be referenced by
					other manifests and will not be garbage-collected automatically.
				</p>
				<label class="dlg-label" for="confirm-input">
					Type <code class="inline-code">{repo}</code> to confirm
				</label>
				<input
					id="confirm-input"
					class="dlg-input"
					type="text"
					bind:value={deleteConfirmText}
					placeholder={repo}
					autocomplete="off"
					onkeydown={(e) => e.key === 'Enter' && deleteConfirmText === repo && confirmDelete()}
				/>
				{#if deleteError}
					<p class="dlg-error">{deleteError}</p>
				{/if}
			</div>

			<div class="dlg-footer">
				<button class="btn-cancel" onclick={closeDeleteDialog} disabled={deleting}>
					Cancel
				</button>
				<button class="btn-delete" onclick={confirmDelete}
					disabled={deleteConfirmText !== repo || deleting}>
					{#if deleting}
						<span class="spinner"></span> Deleting…
					{:else}
						<Trash2 size={13} /> Delete repository
					{/if}
				</button>
			</div>
		</div>
	</div>
{/if}

<style>
.browser { padding: 20px 32px 40px; display: flex; flex-direction: column; gap: 16px; max-width: 1000px; }
.topbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.topbar-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
.breadcrumb { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
.bc-item { display: flex; align-items: center; gap: 5px; font-size: 13px; font-weight: 500; color: var(--text-muted); padding: 3px 5px; border-radius: 5px; text-decoration: none; background: none; border: none; transition: color var(--transition-fast), background var(--transition-fast); }
.bc-item:is(a):hover { color: var(--text-primary); background: var(--surface-2); }
.bc-item.bc-active { color: var(--text-primary); cursor: default; }
:global(.bc-sep) { color: var(--border); flex-shrink: 0; }
.icon-btn { display: flex; align-items: center; justify-content: center; width: 30px; height: 30px; border-radius: 7px; border: 1px solid var(--border); background: var(--surface); color: var(--text-muted); cursor: pointer; flex-shrink: 0; transition: background var(--transition-fast), color var(--transition-fast); }
.icon-btn:hover { background: var(--surface-2); color: var(--text-primary); }
.delete-btn { display: flex; align-items: center; gap: 6px; padding: 0 12px; height: 30px; border-radius: 7px; border: 1px solid rgba(239,68,68,0.35); background: rgba(239,68,68,0.07); color: #ef4444; font-size: 12px; font-weight: 500; cursor: pointer; white-space: nowrap; transition: background var(--transition-fast), border-color var(--transition-fast); }
.delete-btn:hover { background: rgba(239,68,68,0.14); border-color: rgba(239,68,68,0.55); }
.table-wrap { border: 1px solid var(--border); border-radius: 10px; overflow: hidden; }
.table { width: 100%; border-collapse: collapse; font-size: 13px; }
.table th { background: var(--surface-2); border-bottom: 1px solid var(--border); padding: 8px 14px; text-align: left; font-size: 11px; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; white-space: nowrap; }
.table td { padding: 10px 14px; border-bottom: 1px solid var(--border); color: var(--text-primary); vertical-align: middle; }
.table tr:last-child td { border-bottom: none; }
.num-col { text-align: right; white-space: nowrap; }
.muted { color: var(--text-muted); font-size: 12px; }
.mono { font-family: var(--font-mono, monospace); font-size: 12px; }
.kind-badge { display: inline-flex; align-items: center; font-size: 11px; font-weight: 600; padding: 2px 8px; border-radius: 999px; background: color-mix(in srgb, var(--kc) 12%, transparent); color: var(--kc); border: 1px solid color-mix(in srgb, var(--kc) 25%, transparent); white-space: nowrap; }
.tag-pill { display: inline-block; font-size: 11px; font-weight: 600; font-family: var(--font-mono, monospace); background: rgba(99,102,241,0.1); border: 1px solid rgba(99,102,241,0.2); color: #6366f1; border-radius: 999px; padding: 2px 9px; }
.digest { font-size: 11px; color: var(--text-muted); }
.pull-cmd { display: flex; flex-direction: column; gap: 6px; padding: 14px 16px; border: 1px solid var(--border); border-radius: 8px; background: var(--surface); }
.pull-label { font-size: 11px; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
.pull-code { font-size: 12px; font-family: var(--font-mono, monospace); color: var(--text-primary); word-break: break-all; }
.empty { display: flex; flex-direction: column; align-items: center; gap: 10px; padding: 60px 20px; color: var(--text-muted); background: var(--surface); border: 1px dashed var(--border); border-radius: 10px; text-align: center; }
.empty p { font-size: 14px; font-weight: 600; margin: 0; color: var(--text-primary); }
.skel-list { display: flex; flex-direction: column; gap: 6px; }
.skel { height: 44px; background: var(--surface); border: 1px solid var(--border); border-radius: 8px; animation: pulse 1.5s ease-in-out infinite; }
@keyframes pulse { 0%,100%{opacity:1} 50%{opacity:0.4} }

/* ── Delete dialog ── */
.overlay {
	position: fixed; inset: 0;
	background: rgba(0,0,0,0.55);
	backdrop-filter: blur(3px);
	display: flex; align-items: center; justify-content: center;
	z-index: 200;
	padding: 16px;
}
.dialog {
	background: var(--surface);
	border: 1px solid var(--border);
	border-radius: 14px;
	width: 100%;
	max-width: 440px;
	box-shadow: 0 20px 60px rgba(0,0,0,0.4);
	display: flex; flex-direction: column;
	overflow: hidden;
}
.dlg-header {
	display: flex; align-items: flex-start; gap: 14px;
	padding: 20px 20px 16px;
	border-bottom: 1px solid var(--border);
}
.dlg-icon-wrap {
	width: 38px; height: 38px; border-radius: 10px; flex-shrink: 0;
	display: flex; align-items: center; justify-content: center;
	background: rgba(239,68,68,0.1);
	color: #ef4444;
	border: 1px solid rgba(239,68,68,0.2);
}
.dlg-header h2 { font-size: 15px; font-weight: 700; margin: 0 0 4px; color: var(--text-primary); }
.dlg-sub { font-size: 13px; color: var(--text-muted); margin: 0; line-height: 1.45; }
.dlg-sub strong { color: var(--text-primary); }
.dlg-body { padding: 16px 20px; display: flex; flex-direction: column; gap: 12px; }
.dlg-warning {
	font-size: 13px; color: var(--text-muted);
	background: rgba(239,68,68,0.06);
	border: 1px solid rgba(239,68,68,0.18);
	border-radius: 8px;
	padding: 10px 12px;
	line-height: 1.5;
}
.dlg-warning strong { color: #ef4444; }
.dlg-label { font-size: 12px; font-weight: 500; color: var(--text-muted); }
.inline-code {
	font-family: var(--font-mono, monospace);
	font-size: 11px;
	background: var(--surface-2);
	border: 1px solid var(--border);
	border-radius: 4px;
	padding: 1px 5px;
	color: var(--text-primary);
}
.dlg-input {
	width: 100%; padding: 8px 11px;
	font-size: 13px; font-family: var(--font-mono, monospace);
	background: var(--surface-2);
	border: 1px solid var(--border);
	border-radius: 8px;
	color: var(--text-primary);
	outline: none;
	transition: border-color var(--transition-fast);
}
.dlg-input:focus { border-color: #ef4444; }
.dlg-error { font-size: 12px; color: #ef4444; margin: 0; }
.dlg-footer {
	display: flex; align-items: center; justify-content: flex-end; gap: 8px;
	padding: 14px 20px;
	border-top: 1px solid var(--border);
}
.btn-cancel {
	padding: 7px 16px; border-radius: 8px;
	font-size: 13px; font-weight: 500;
	background: var(--surface-2); border: 1px solid var(--border);
	color: var(--text-muted); cursor: pointer;
	transition: background var(--transition-fast), color var(--transition-fast);
}
.btn-cancel:hover:not(:disabled) { color: var(--text-primary); background: var(--surface-3, var(--surface-2)); }
.btn-cancel:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-delete {
	display: flex; align-items: center; gap: 6px;
	padding: 7px 16px; border-radius: 8px;
	font-size: 13px; font-weight: 600;
	background: #ef4444; border: none;
	color: #fff; cursor: pointer;
	transition: background var(--transition-fast), opacity var(--transition-fast);
}
.btn-delete:hover:not(:disabled) { background: #dc2626; }
.btn-delete:disabled { opacity: 0.45; cursor: not-allowed; }
.spinner {
	width: 13px; height: 13px;
	border: 2px solid rgba(255,255,255,0.35);
	border-top-color: #fff;
	border-radius: 50%;
	animation: spin 0.7s linear infinite;
	display: inline-block;
}
@keyframes spin { to { transform: rotate(360deg); } }
</style>
