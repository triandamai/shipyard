<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import { Card, Button, InlineAlert, Skeleton, EmptyState, DataTable, ListRow } from '$lib/components/ui';
	import {
		Folder,
		File,
		ChevronRight,
		ArrowLeft,
		HardDrive,
		Calendar,
		X,
		Download,
		RefreshCw,
		Copy,
		Check,
		Database,
		Server,
		FlaskConical,
		CheckCircle2,
		XCircle,
		Loader,
	} from '@lucide/svelte';

	interface StorageObject {
		key: string;
		size: number;
		last_modified: string | null;
	}

	interface ListResult {
		objects: StorageObject[];
		common_prefixes: string[];
	}

	interface BucketInfo {
		backend: string;
		bucket: string;
		endpoint: string;
	}

	// Bucket state
	let buckets = $state<BucketInfo[]>([]);
	let bucketsLoading = $state(false);
	let bucketsError = $state('');

	// URL-driven: bucket param selects the "active" bucket; prefix navigates within it.
	let selectedBucket = $derived(page.url.searchParams.get('bucket') ?? '');
	let currentPrefix = $derived(page.url.searchParams.get('prefix') ?? '');

	let objects = $state<StorageObject[]>([]);
	let commonPrefixes = $state<string[]>([]);
	let loading = $state(false);
	let error = $state('');

	// Diagnostics
	interface DiagResult {
		put_ok: boolean;
		put_error: string | null;
		exists_after_put: boolean;
		list_objects: string[];
		list_prefixes: string[];
		list_error: string | null;
		delete_ok: boolean;
	}
	let diagOpen = $state(false);
	let diagLoading = $state(false);
	let diagResult = $state<DiagResult | null>(null);
	let diagError = $state('');

	async function runDiagnostics() {
		diagLoading = true;
		diagResult = null;
		diagError = '';
		const r = await api.get<DiagResult>('/admin/storage/test');
		if (r.data) {
			diagResult = r.data;
		} else {
			diagError = r.error?.message ?? 'Diagnostics request failed';
		}
		diagLoading = false;
	}

	// Preview overlay state
	let previewKey = $state<string | null>(null);
	let previewLoading = $state(false);
	let previewContent = $state<string | null>(null);
	let previewError = $state('');
	let previewIsImage = $derived(
		previewKey ? /\.(png|jpe?g|gif|svg|webp|ico)$/i.test(previewKey) : false
	);
	let copied = $state(false);

	async function loadBuckets() {
		bucketsLoading = true;
		bucketsError = '';
		const r = await api.get<BucketInfo[]>('/admin/storage/buckets');
		if (r.data) {
			buckets = r.data;
		} else {
			bucketsError = r.error?.message ?? 'Failed to load storage buckets';
		}
		bucketsLoading = false;
	}

	async function loadList(prefix: string) {
		loading = true;
		error = '';
		objects = [];
		commonPrefixes = [];
		const qs = new URLSearchParams({ delimiter: '/' });
		if (prefix) qs.set('prefix', prefix);
		const r = await api.get<ListResult>(`/admin/storage/list?${qs.toString()}`);
		if (r.data) {
			objects = r.data.objects;
			commonPrefixes = r.data.common_prefixes;
		} else {
			error = r.error?.message ?? 'Failed to list storage objects';
		}
		loading = false;
	}

	function navigateToBucket(bucket: BucketInfo) {
		const url = new URL(window.location.href);
		url.searchParams.set('bucket', bucket.bucket);
		url.searchParams.delete('prefix');
		goto(url.toString(), { keepFocus: true });
		// Explicitly load — don't rely solely on $effect since $derived→$effect
		// chaining can miss the first navigation on the same page.
		loadList('');
	}

	function navigateTo(prefix: string) {
		const url = new URL(window.location.href);
		if (prefix) {
			url.searchParams.set('prefix', prefix);
		} else {
			url.searchParams.delete('prefix');
		}
		goto(url.toString(), { keepFocus: true });
		loadList(prefix);
	}

	function backToBuckets() {
		const url = new URL(window.location.href);
		url.searchParams.delete('bucket');
		url.searchParams.delete('prefix');
		goto(url.toString(), { keepFocus: true });
		loadBuckets();
	}

	// Single effect: re-runs whenever page.url changes (direct read — no $derived indirection).
	// Handles the initial load and back/forward navigation.
	$effect(() => {
		const bucket = page.url.searchParams.get('bucket');
		const prefix = page.url.searchParams.get('prefix') ?? '';
		if (bucket) {
			loadList(prefix);
		} else {
			loadBuckets();
		}
	});

	function goUp() {
		if (!currentPrefix) return;
		const parts = currentPrefix.replace(/\/$/, '').split('/');
		parts.pop();
		const parent = parts.length > 0 ? parts.join('/') + '/' : '';
		navigateTo(parent);
	}

	function handleItemClick(item: { type: 'folder' | 'file'; path: string }) {
		if (item.type === 'folder') {
			navigateTo(item.path);
		} else {
			openPreview(item.path);
		}
	}

	async function openPreview(key: string) {
		previewKey = key;
		previewError = '';
		previewContent = null;
		copied = false;

		if (previewIsImage) {
			previewContent = `/api/admin/storage/preview?key=${encodeURIComponent(key)}`;
			return;
		}

		previewLoading = true;
		try {
			const token = localStorage.getItem('shipyard_token');
			const response = await fetch(`/api/admin/storage/preview?key=${encodeURIComponent(key)}`, {
				headers: token ? { Authorization: `Bearer ${token}` } : {}
			});

			if (!response.ok) {
				throw new Error(`HTTP error ${response.status}`);
			}

			const text = await response.text();
			previewContent = text;
		} catch (e: any) {
			previewError = e.message ?? 'Failed to load preview';
		} finally {
			previewLoading = false;
		}
	}

	function closePreview() {
		previewKey = null;
		previewContent = null;
		previewError = '';
	}

	function copyToClipboard() {
		if (!previewContent) return;
		navigator.clipboard.writeText(previewContent);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	function fmtBytes(n: number): string {
		if (!n) return '0 B';
		if (n < 1024) return `${n} B`;
		if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
		return `${(n / 1024 / 1024).toFixed(1)} MB`;
	}

	function getBreadcrumbs(prefix: string) {
		const parts = prefix.replace(/\/$/, '').split('/').filter(Boolean);
		let acc = '';
		return parts.map(p => {
			acc += p + '/';
			return { label: p, prefix: acc };
		});
	}

	// Unified folder+file row model for the DataTable (client mode). Search
	// filtering is now handled by DataTable's own built-in `searchFields`
	// matching, replacing the page's previous hand-rolled `search` filter —
	// the old page already had its own search input over the same in-memory
	// per-directory listing, so this is a straightforward swap (Ruling 10).
	interface BrowserRow {
		type: 'folder' | 'file';
		path: string;
		name: string;
		size: number | null;
		lastModified: string | null;
	}

	let browserRows = $derived<BrowserRow[]>([
		...commonPrefixes.map((folder) => ({
			type: 'folder' as const,
			path: folder,
			name: folder.replace(currentPrefix, ''),
			size: null,
			lastModified: null,
		})),
		...objects.map((file) => ({
			type: 'file' as const,
			path: file.key,
			name: file.key.replace(currentPrefix, ''),
			size: file.size,
			lastModified: file.last_modified,
		})),
	]);

</script>

<svelte:head>
	<title>S3 Storage Browser — Shipyard Admin</title>
</svelte:head>

<div class="p">
	<div class="hdr">
		<div>
			<h1 class="ttl">Storage Browser</h1>
			<p class="sub">Registry content store explorer (S3 / Local storage backend)</p>
		</div>
		<div style="display:flex;gap:8px;align-items:center;">
			<Button variant="secondary" size="sm" onclick={() => { diagOpen = !diagOpen; if (diagOpen && !diagResult) runDiagnostics(); }}>
				<FlaskConical size={14} />
				Diagnostics
			</Button>
			<Button variant="secondary" size="icon" onclick={() => selectedBucket ? loadList(currentPrefix) : loadBuckets()}>
				<RefreshCw size={14} class={(loading || bucketsLoading) ? 'spin' : ''} />
			</Button>
		</div>
	</div>

	{#if diagOpen}
		<Card padding="0">
			<div class="diag-header">
				<span class="diag-title">Storage Diagnostics</span>
				<div style="display:flex;gap:8px;align-items:center;">
					<Button variant="secondary" size="sm" onclick={runDiagnostics} disabled={diagLoading}>
						<RefreshCw size={12} class={diagLoading ? 'spin' : ''} />
						Re-run
					</Button>
					<Button variant="ghost" size="icon" onclick={() => diagOpen = false}><X size={14} /></Button>
				</div>
			</div>
			{#if diagLoading && !diagResult}
				<div class="diag-loading"><Loader size={14} class="spin" /> Running storage probe…</div>
			{:else if diagError}
				<div class="diag-body">
					<InlineAlert tone="error">Request failed: {diagError}</InlineAlert>
				</div>
			{:else if diagResult}
				{@const d = diagResult}
				<div class="diag-rows">
					<ListRow
						iconTone={d.put_ok ? 'green' : 'red'}
						title="PUT test object"
						meta={d.put_error ?? undefined}
					>
						{#snippet icon()}
							{#if d.put_ok}<CheckCircle2 size={14} />{:else}<XCircle size={14} />{/if}
						{/snippet}
					</ListRow>
					<ListRow
						iconTone={d.exists_after_put ? 'green' : 'red'}
						title="EXISTS check after PUT"
					>
						{#snippet icon()}
							{#if d.exists_after_put}<CheckCircle2 size={14} />{:else}<XCircle size={14} />{/if}
						{/snippet}
					</ListRow>
					<ListRow
						iconTone={!d.list_error ? 'green' : 'red'}
						title="LIST bucket root"
						meta={d.list_error ?? undefined}
					>
						{#snippet icon()}
							{#if !d.list_error}<CheckCircle2 size={14} />{:else}<XCircle size={14} />{/if}
						{/snippet}
					</ListRow>
					{#if !d.list_error}
						<div class="diag-info">
							Prefixes found: <span class="mono">{d.list_prefixes.length > 0 ? d.list_prefixes.join(', ') : '(none)'}</span>
							&nbsp;·&nbsp;
							Objects found: <span class="mono">{d.list_objects.length > 0 ? d.list_objects.join(', ') : '(none)'}</span>
						</div>
					{/if}
					<ListRow
						iconTone={d.delete_ok ? 'green' : 'red'}
						title="DELETE test object"
					>
						{#snippet icon()}
							{#if d.delete_ok}<CheckCircle2 size={14} />{:else}<XCircle size={14} />{/if}
						{/snippet}
					</ListRow>
				</div>
				{#if d.put_ok && !d.list_error}
					{@const hasRealContent = d.list_prefixes.some(p => p !== 'shipyard-storage-test/') || d.list_objects.some(k => !k.startsWith('shipyard-storage-test/'))}
					<div class="diag-body">
						<InlineAlert tone={hasRealContent ? 'success' : 'warning'}>
							{#if hasRealContent}
								Storage is working and has content. If the browser view is empty, try refreshing after selecting a bucket.
							{:else}
								S3 connection is healthy but the bucket has no artifact data yet. Trigger a static site, edge function, or Git-built service deployment to populate storage.
							{/if}
						</InlineAlert>
					</div>
				{/if}
			{/if}
		</Card>
	{/if}

	{#if selectedBucket}
		<!-- ── File Browser View ── -->

		<!-- Breadcrumbs and Navigation bar — page-local, not a shared component
		     (a one-off `/`-separated link trail specific to this page) -->
		<div class="nav-bar">
			<div class="breadcrumbs">
				<button class="crumb-btn" onclick={backToBuckets}>
					<Database size={14} style="margin-right: 4px;" />
					Buckets
				</button>
				<ChevronRight size={12} class="crumb-sep" />
				<button class="crumb-btn" onclick={() => navigateTo('')}>
					<HardDrive size={14} style="margin-right: 4px;" />
					{selectedBucket}
				</button>
				{#each getBreadcrumbs(currentPrefix) as crumb}
					<ChevronRight size={12} class="crumb-sep" />
					<button class="crumb-btn" onclick={() => navigateTo(crumb.prefix)}>{crumb.label}</button>
				{/each}
			</div>

			{#if currentPrefix}
				<Button variant="secondary" size="sm" onclick={goUp}>
					<ArrowLeft size={12} />
					Go Up
				</Button>
			{/if}
		</div>

		{#if error}
			<InlineAlert tone="error">{error}</InlineAlert>
		{:else}
			<DataTable
				items={browserRows}
				rowKey={(r) => `${r.type}:${r.path}`}
				searchFields={['name']}
				columns={[
					{ key: 'name', label: 'Name', width: '55%' },
					{ key: 'size', label: 'Size', width: '20%' },
					{ key: 'lastModified', label: 'Last Modified', width: '25%' }
				]}
				emptyMessage="No files or folders found here."
			>
				{#snippet row(item)}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<tr class="browser-row" onclick={() => handleItemClick({ type: item.type, path: item.path })}>
						<td class="browser-name">
							{#if item.type === 'folder'}
								<Folder size={16} class="browser-icon-folder" />
							{:else}
								<File size={16} class="browser-icon-file" />
							{/if}
							<span class="mono trunc">{item.name}</span>
						</td>
						<td class="browser-dim">{item.type === 'folder' ? '—' : fmtBytes(item.size ?? 0)}</td>
						<td class="browser-dim">
							{#if item.lastModified}
								<Calendar size={11} style="display:inline; margin-right:4px; vertical-align:-1px;" />
								{new Date(item.lastModified).toLocaleString()}
							{:else}
								—
							{/if}
						</td>
					</tr>
				{/snippet}
			</DataTable>
		{/if}

	{:else}
		<!-- ── Bucket List View ── -->

		{#if bucketsLoading}
			<div class="bucket-grid-skeleton">
				<Skeleton variant="card" />
				<Skeleton variant="card" />
			</div>
		{:else if bucketsError}
			<InlineAlert tone="error">{bucketsError}</InlineAlert>
		{:else if buckets.length === 0}
			<EmptyState message="No storage backends configured." />
		{:else}
			<div class="bucket-grid">
				{#each buckets as bucket}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="bucket-tile" onclick={() => navigateToBucket(bucket)}>
						<Card padding="16px">
							<div class="bucket-card-inner">
								<div class="bucket-icon">
									{#if bucket.backend === 's3'}
										<Server size={20} />
									{:else}
										<HardDrive size={20} />
									{/if}
								</div>
								<div class="bucket-fields">
									<div class="bucket-field">
										<span class="bucket-field-label">Backend</span>
										<span class="bucket-field-value">{bucket.backend === 's3' ? 'S3 / MinIO' : 'Local Disk'}</span>
									</div>
									<div class="bucket-field">
										<span class="bucket-field-label">Bucket</span>
										<span class="bucket-field-value mono trunc">{bucket.bucket}</span>
									</div>
									{#if bucket.endpoint}
										<div class="bucket-field">
											<span class="bucket-field-label">Endpoint</span>
											<span class="bucket-field-value mono trunc">{bucket.endpoint}</span>
										</div>
									{/if}
								</div>
								<ChevronRight size={16} class="bucket-chevron" />
							</div>
						</Card>
					</div>
				{/each}
			</div>
		{/if}
	{/if}
</div>

<!-- Preview Drawer / Side Panel Overlay -->
{#if previewKey}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<!-- svelte-ignore a11y_no_static_element_interactions -->
	<div class="overlay" onclick={closePreview}>
		<!-- svelte-ignore a11y_click_events_have_key_events -->
		<!-- svelte-ignore a11y_no_static_element_interactions -->
		<div class="drawer" onclick={(e) => e.stopPropagation()}>
			<div class="drawer-header">
				<div class="drawer-title-area">
					<span class="drawer-meta-label">FILE PREVIEW</span>
					<h3 class="drawer-title mono trunc" title={previewKey}>{previewKey.split('/').pop()}</h3>
				</div>
				<div class="drawer-actions">
					{#if previewContent && !previewIsImage}
						<button class="drawer-btn" onclick={copyToClipboard} title="Copy Content">
							{#if copied}
								<Check width="14" height="14" style="color:var(--ok)" />
							{:else}
								<Copy width="14" height="14" />
							{/if}
						</button>
					{/if}
					<a class="drawer-btn" href={`/api/admin/storage/preview?key=${encodeURIComponent(previewKey)}`} download={previewKey.split('/').pop()} target="_blank" title="Download File">
						<Download width="14" height="14" />
					</a>
					<button class="drawer-btn close-btn" onclick={closePreview}>
						<X width="16" height="16" />
					</button>
				</div>
			</div>

			<div class="drawer-body">
				{#if previewLoading}
					<div class="preview-loading">
						<div class="spinner"></div>
						<span>Loading file content…</span>
					</div>
				{:else if previewError}
					<div class="preview-error">
						<h3>Failed to render preview</h3>
						<p>{previewError}</p>
					</div>
				{:else if previewIsImage}
					<div class="preview-image-container">
						<!-- svelte-ignore a11y_missing_attribute -->
						<img src={previewContent} class="preview-image" />
					</div>
				{:else if previewContent !== null}
					<div class="preview-text-container">
						<pre class="preview-code mono">{previewContent}</pre>
					</div>
				{:else}
					<div class="preview-binary">
						<File width="48" height="48" style="color: var(--text-3); margin-bottom: 12px;" />
						<h3>Binary File</h3>
						<p>Previews are not supported for this file type.</p>
						<a class="download-link" href={`/api/admin/storage/preview?key=${encodeURIComponent(previewKey)}`} download={previewKey.split('/').pop()} target="_blank">
							<Download width="14" height="14" style="margin-right: 6px;" />
							Download File
						</a>
					</div>
				{/if}
			</div>
		</div>
	</div>
{/if}

<style>
	.p { max-width: 1100px; margin: 0 auto; padding: 40px 36px; box-sizing: border-box; }
	.hdr { display: flex; align-items: center; justify-content: space-between; margin-bottom: 24px; }
	.ttl { font-size: 20px; font-weight: 700; color: var(--text-primary); margin: 0 0 4px; letter-spacing: -0.02em; }
	.sub { font-size: 13px; color: var(--text-dim); margin: 0; }

	/* ── Bucket Grid ── */
	.bucket-grid-skeleton { display: flex; flex-direction: column; gap: 10px; }
	.bucket-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(280px, 1fr)); gap: 12px; }

	.bucket-tile { cursor: pointer; }
	.bucket-tile :global(.ui-card) { transition: background var(--transition-fast), border-color var(--transition-fast); }
	.bucket-tile:hover :global(.ui-card) { background: var(--bg-hover); border-color: var(--accent); }

	.bucket-card-inner { display: flex; align-items: center; gap: 14px; }

	.bucket-icon { display: flex; align-items: center; justify-content: center; width: 40px; height: 40px; border-radius: var(--radius-md); background: var(--bg-elevated); border: 1px solid var(--border); color: var(--accent); flex-shrink: 0; }

	.bucket-fields { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
	.bucket-field { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
	.bucket-field-label { font-size: 10px; font-weight: 700; color: var(--text-dim); text-transform: uppercase; letter-spacing: .05em; width: 58px; flex-shrink: 0; }
	.bucket-field-value { font-size: 12.5px; color: var(--text-primary); font-weight: 500; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

	:global(.bucket-chevron) { color: var(--text-dim); flex-shrink: 0; }
	/* Navigation and Breadcrumbs */
	.nav-bar { display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; background: var(--bg-elevated); border: 1px solid var(--border); border-radius: var(--radius-md); margin-bottom: 16px; min-height: 42px; box-sizing: border-box; }
	.breadcrumbs { display: flex; align-items: center; flex-wrap: wrap; gap: 4px; font-size: 13px; color: var(--text-secondary); }
	.crumb-btn { display: inline-flex; align-items: center; background: transparent; border: none; padding: 2px 6px; border-radius: 4px; color: var(--text-secondary); font-weight: 500; cursor: pointer; font-family: var(--font-sans); font-size: 13px; transition: color var(--transition-fast), background var(--transition-fast); }
	.crumb-btn:hover { color: var(--accent); background: var(--bg-hover); }
	:global(.crumb-sep) { color: var(--text-dim); flex-shrink: 0; }

	/* Browser DataTable rows */
	.browser-row { cursor: pointer; }
	.browser-name { display: flex; align-items: center; gap: 10px; min-width: 0; }
	:global(.browser-icon-folder) { color: var(--accent); flex-shrink: 0; }
	:global(.browser-icon-file) { color: var(--text-dim); flex-shrink: 0; }
	.browser-dim { color: var(--text-muted); font-size: 12px; }

	.mono { font-family: var(--font-mono); }
	.trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; }

	/* Preview Overlay & Drawer */
	.overlay { position: fixed; top: 0; left: 0; right: 0; bottom: 0; background: rgba(0, 0, 0, 0.6); backdrop-filter: blur(4px); z-index: 1000; display: flex; justify-content: flex-end; }
	.drawer { width: 100%; max-width: 600px; height: 100%; background: var(--surface); border-left: 1px solid var(--border); display: flex; flex-direction: column; box-shadow: -10px 0 30px rgba(0, 0, 0, 0.4); animation: slideIn 0.25s cubic-bezier(0.16, 1, 0.3, 1); }

	@keyframes slideIn {
		from { transform: translateX(100%); }
		to { transform: translateX(0); }
	}

	.drawer-header { display: flex; align-items: center; justify-content: space-between; padding: 20px 24px; border-bottom: 1px solid var(--border); background: var(--surface-2); }
	.drawer-title-area { display: flex; flex-direction: column; gap: 2px; min-width: 0; flex: 1; }
	.drawer-meta-label { font-size: 9.5px; font-weight: 700; color: var(--accent); letter-spacing: 0.08em; }
	.drawer-title { font-size: 16px; font-weight: 700; color: var(--text); margin: 0; }

	.drawer-actions { display: flex; align-items: center; gap: 8px; flex-shrink: 0; }
	.drawer-btn { display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; border-radius: var(--radius-sm); border: 1px solid var(--border); background: var(--surface); color: var(--text-2); cursor: pointer; transition: background .15s, color .15s; }
	.drawer-btn:hover { background: var(--surface-2); color: var(--text); }
	.close-btn:hover { background: var(--danger-soft); color: var(--danger); border-color: rgba(220,38,38,0.2); }

	.drawer-body { flex: 1; overflow-y: auto; padding: 24px; display: flex; flex-direction: column; }

	/* Preview Types */
	.preview-loading { display: flex; flex-direction: column; align-items: center; justify-content: center; flex: 1; gap: 12px; color: var(--text-3); font-size: 13px; }
	.spinner { width: 24px; height: 24px; border: 2.5px solid var(--border); border-top-color: var(--accent); border-radius: 50%; animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.preview-error { text-align: center; margin: auto; max-width: 320px; }
	.preview-error h3 { font-size: 15px; color: var(--danger); margin: 0 0 8px; }
	.preview-error p { font-size: 13px; color: var(--text-3); margin: 0; line-height: 1.5; }

	.preview-image-container { display: flex; align-items: center; justify-content: center; background: rgba(0, 0, 0, 0.25); border: 1px solid var(--border); border-radius: var(--radius); padding: 16px; overflow: auto; max-height: 100%; box-sizing: border-box; }
	.preview-image { max-width: 100%; max-height: 480px; object-fit: contain; border-radius: var(--radius-sm); }

	.preview-text-container { background: rgba(0, 0, 0, 0.25); border: 1px solid var(--border); border-radius: var(--radius); overflow: auto; flex: 1; max-height: 100%; }
	.preview-code { margin: 0; padding: 16px; font-size: 12px; line-height: 1.6; color: var(--text-2); white-space: pre; font-family: var(--mono); }

	.preview-binary { display: flex; flex-direction: column; align-items: center; justify-content: center; margin: auto; text-align: center; max-width: 320px; }
	.preview-binary h3 { font-size: 15px; color: var(--text); margin: 0 0 6px; }
	.preview-binary p { font-size: 13px; color: var(--text-3); margin: 0 0 20px; line-height: 1.5; }

	.download-link { display: inline-flex; align-items: center; padding: 8px 16px; background: var(--accent); color: #000; font-size: 13px; font-weight: 600; text-decoration: none; border-radius: var(--radius-sm); transition: opacity .15s; }
	.download-link:hover { opacity: 0.9; }

	/* Diagnostics panel */
	.diag-header { display: flex; align-items: center; justify-content: space-between; padding: 10px 16px; background: var(--bg-elevated); border-bottom: 1px solid var(--border); }
	.diag-title { font-size: 12px; font-weight: 700; color: var(--text-secondary); text-transform: uppercase; letter-spacing: .05em; }

	.diag-loading { display: flex; align-items: center; gap: 8px; padding: 14px 16px; font-size: 13px; color: var(--text-muted); }

	.diag-rows { display: flex; flex-direction: column; padding: 4px 16px; }
	.diag-info { padding: 4px 16px 12px 46px; font-size: 12px; color: var(--text-muted); }
	.diag-body { padding: 12px 16px; }

	/* Responsiveness */
	@media (max-width: 768px) {
		.p { padding: 24px 16px; }

		.overlay { justify-content: center; }
		.drawer { height: 100%; max-width: 100%; border-left: none; }

		.bucket-grid { grid-template-columns: 1fr; }
	}
</style>
