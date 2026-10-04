<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { orgStore } from '$lib/stores/org.store';
	import { Tag, ChevronRight, RefreshCw, FolderOpen, Trash2 } from '@lucide/svelte';
	import { onMount } from 'svelte';
	import { Button, Badge, Card, ConfirmDialog, DataTable, EmptyState, Skeleton } from '$lib/components/ui';

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
	let deleteError = $state('');

	function openDeleteDialog() {
		deleteError = '';
		showDeleteDialog = true;
	}

	async function confirmDelete() {
		deleteError = '';
		const res = await api.delete(`/orgs/${orgId}/registry/namespaces/${nsId}/repos/${encodeURIComponent(repo)}`);
		if (res.error) {
			deleteError = res.error.message ?? 'Delete failed.';
			return false;
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

	const kindMeta: Record<string, { label: string; tone: 'blue' | 'green' | 'yellow' | 'neutral' }> = {
		docker_image:  { label: 'Image',   tone: 'blue'    },
		static_bundle: { label: 'Static',  tone: 'green'   },
		edge_function: { label: 'Edge Fn', tone: 'yellow'  },
		build_cache:   { label: 'Cache',   tone: 'neutral' },
	};
	function kindLabel(k: string) { return kindMeta[k]?.label ?? k; }
	function kindTone(k: string) { return kindMeta[k]?.tone ?? 'neutral'; }

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
			<Button variant="secondary" size="icon" title="Refresh" aria-label="Refresh" onclick={load}>
				<RefreshCw size={13} />
			</Button>
			<Button variant="danger-outline" size="sm" title="Delete repository" onclick={openDeleteDialog}>
				<Trash2 size={13} />
				Delete repository
			</Button>
		</div>
	</div>

	{#if loading}
		<div class="skel-list">
			{#each [1,2,3] as _}<Skeleton variant="row" />{/each}
		</div>
	{:else if tags.length === 0}
		<EmptyState message="No tags in this repository.">
			{#snippet icon()}<Tag size={32} />{/snippet}
		</EmptyState>
	{:else}
		<DataTable
			items={tags}
			rowKey={(t: ArtTag) => t.id}
			searchable={false}
			columns={[
				{ key: 'tag', label: 'Tag' },
				{ key: 'kind', label: 'Kind' },
				{ key: 'size_bytes', label: 'Size' },
				{ key: 'manifest_digest', label: 'Digest' },
				{ key: 'pushed_at', label: 'Pushed' }
			]}
		>
			{#snippet row(t: ArtTag)}
				<tr>
					<td><span class="tag-pill"><Badge tone="blue">{t.tag}</Badge></span></td>
					<td><Badge tone={kindTone(t.kind)}>{kindLabel(t.kind)}</Badge></td>
					<td class="muted">{fmtBytes(t.size_bytes)}</td>
					<td><span class="digest mono">{t.manifest_digest.slice(0, 23)}…</span></td>
					<td class="muted">{timeAgo(t.pushed_at)}</td>
				</tr>
			{/snippet}
		</DataTable>

		<Card>
			<div class="pull-cmd">
				<span class="pull-label">Pull command</span>
				<code class="pull-code">docker pull registry.{page.url.hostname.split('.').slice(1).join('.')}/{nsSlug}/{repo}:&lt;tag&gt;</code>
			</div>
		</Card>
	{/if}
</div>

<ConfirmDialog
	bind:open={showDeleteDialog}
	title="Delete repository"
	message="This will permanently delete all tags and artifacts in {repo}. This action cannot be undone. Blobs may still be referenced by other manifests and will not be garbage-collected automatically."
	confirmLabel="Delete repository"
	confirmText={repo}
	error={deleteError}
	onConfirm={confirmDelete}
/>

<style>
	.browser { padding: 20px 32px 40px; display: flex; flex-direction: column; gap: 16px; max-width: 1000px; }
	.topbar { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
	.topbar-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
	.breadcrumb { display: flex; align-items: center; gap: 4px; flex-wrap: wrap; }
	.bc-item { display: flex; align-items: center; gap: 5px; font-size: 13px; font-weight: 500; color: var(--text-muted); padding: 3px 5px; border-radius: var(--radius-sm); text-decoration: none; transition: color var(--transition-fast), background var(--transition-fast); }
	.bc-item:is(a):hover { color: var(--text-primary); background: var(--bg-hover); }
	.bc-item.bc-active { color: var(--text-primary); cursor: default; }
	:global(.bc-sep) { color: var(--border); flex-shrink: 0; }
	.skel-list { display: flex; flex-direction: column; gap: 6px; }
	.muted { color: var(--text-muted); font-size: 12px; white-space: nowrap; }
	.mono { font-family: var(--font-mono); font-size: 12px; }
	.tag-pill { font-family: var(--font-mono); }
	.digest { font-size: 11px; color: var(--text-muted); }
	.pull-cmd { display: flex; flex-direction: column; gap: 6px; }
	.pull-label { font-size: 11px; font-weight: 600; color: var(--text-muted); text-transform: uppercase; letter-spacing: 0.04em; }
	.pull-code { font-size: 12px; font-family: var(--font-mono); color: var(--text-primary); word-break: break-all; }
</style>
