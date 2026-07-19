<script lang="ts">
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { api } from '$lib/api/client';
	import {
		Folder,
		File,
		ChevronRight,
		Search,
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
		CircleCheck,
		CircleX,
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
	let search = $state('');

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

	let filteredFolders = $derived(
		commonPrefixes.filter(p => {
			const name = p.replace(/\/$/, '').split('/').pop() ?? '';
			return !search || name.toLowerCase().includes(search.toLowerCase());
		})
	);

	let filteredFiles = $derived(
		objects.filter(o => {
			const name = o.key.split('/').pop() ?? '';
			return !search || name.toLowerCase().includes(search.toLowerCase());
		})
	);

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
			<button class="diag-btn" onclick={() => { diagOpen = !diagOpen; if (diagOpen && !diagResult) runDiagnostics(); }} title="Run storage diagnostics">
				<FlaskConical width="14" height="14" />
				Diagnostics
			</button>
			<button class="refresh-btn" onclick={() => selectedBucket ? loadList(currentPrefix) : loadBuckets()} title="Refresh">
				<RefreshCw width="14" height="14" class={(loading || bucketsLoading) ? 'spin' : ''} />
			</button>
		</div>
	</div>

	{#if diagOpen}
		<div class="diag-panel">
			<div class="diag-header">
				<span class="diag-title">Storage Diagnostics</span>
				<div style="display:flex;gap:8px;align-items:center;">
					<button class="diag-rerun" onclick={runDiagnostics} disabled={diagLoading}>
						<RefreshCw width="12" height="12" class={diagLoading ? 'spin' : ''} />
						Re-run
					</button>
					<button class="diag-close" onclick={() => diagOpen = false}><X width="14" height="14" /></button>
				</div>
			</div>
			{#if diagLoading && !diagResult}
				<div class="diag-loading"><Loader width="14" height="14" class="spin" /> Running storage probe…</div>
			{:else if diagError}
				<div class="diag-row diag-fail"><CircleX width="14" height="14" /> Request failed: {diagError}</div>
			{:else if diagResult}
				<div class="diag-rows">
					<div class="diag-row" class:diag-ok={diagResult.put_ok} class:diag-fail={!diagResult.put_ok}>
						{#if diagResult.put_ok}<CircleCheck width="14" height="14" />{:else}<CircleX width="14" height="14" />{/if}
						<span>PUT test object</span>
						{#if diagResult.put_error}<span class="diag-detail">{diagResult.put_error}</span>{/if}
					</div>
					<div class="diag-row" class:diag-ok={diagResult.exists_after_put} class:diag-fail={!diagResult.exists_after_put}>
						{#if diagResult.exists_after_put}<CircleCheck width="14" height="14" />{:else}<CircleX width="14" height="14" />{/if}
						<span>EXISTS check after PUT</span>
					</div>
					<div class="diag-row" class:diag-ok={!diagResult.list_error} class:diag-fail={!!diagResult.list_error}>
						{#if !diagResult.list_error}<CircleCheck width="14" height="14" />{:else}<CircleX width="14" height="14" />{/if}
						<span>LIST bucket root</span>
						{#if diagResult.list_error}<span class="diag-detail">{diagResult.list_error}</span>{/if}
					</div>
					{#if !diagResult.list_error}
						<div class="diag-info">
							Prefixes found: <span class="mono">{diagResult.list_prefixes.length > 0 ? diagResult.list_prefixes.join(', ') : '(none)'}</span>
							&nbsp;·&nbsp;
							Objects found: <span class="mono">{diagResult.list_objects.length > 0 ? diagResult.list_objects.join(', ') : '(none)'}</span>
						</div>
					{/if}
					<div class="diag-row" class:diag-ok={diagResult.delete_ok} class:diag-fail={!diagResult.delete_ok}>
						{#if diagResult.delete_ok}<CircleCheck width="14" height="14" />{:else}<CircleX width="14" height="14" />{/if}
						<span>DELETE test object</span>
					</div>
				</div>
				{#if diagResult.put_ok && !diagResult.list_error}
					{@const hasRealContent = diagResult.list_prefixes.some(p => p !== 'shipyard-storage-test/') || diagResult.list_objects.some(k => !k.startsWith('shipyard-storage-test/'))}
					<div class="diag-summary" class:diag-summary-warn={!hasRealContent}>
						{#if hasRealContent}
							Storage is working and has content. If the browser view is empty, try refreshing after selecting a bucket.
						{:else}
							S3 connection is healthy but the bucket has no artifact data yet. Trigger a static site, edge function, or Git-built service deployment to populate storage.
						{/if}
					</div>
				{/if}
			{/if}
		</div>
	{/if}

	{#if selectedBucket}
		<!-- ── File Browser View ── -->

		<!-- Breadcrumbs and Navigation bar -->
		<div class="nav-bar">
			<div class="breadcrumbs">
				<button class="crumb-btn" onclick={backToBuckets}>
					<Database width="14" height="14" style="margin-right: 4px;" />
					Buckets
				</button>
				<ChevronRight width="12" height="12" style="color: var(--text-3); flex-shrink: 0;" />
				<button class="crumb-btn" onclick={() => navigateTo('')}>
					<HardDrive width="14" height="14" style="margin-right: 4px;" />
					{selectedBucket}
				</button>
				{#each getBreadcrumbs(currentPrefix) as crumb}
					<ChevronRight width="12" height="12" style="color: var(--text-3); flex-shrink: 0;" />
					<button class="crumb-btn" onclick={() => navigateTo(crumb.prefix)}>{crumb.label}</button>
				{/each}
			</div>

			{#if currentPrefix}
				<button class="up-btn" onclick={goUp}>
					<ArrowLeft width="12" height="12" />
					Go Up
				</button>
			{/if}
		</div>

		<!-- Search & Toolbar -->
		<div class="toolbar">
			<label class="search">
				<Search width="14" height="14" style="position: absolute; left: 10px; color: var(--text-3); pointer-events: none;" />
				<input type="text" placeholder="Filter items in folder…" bind:value={search} />
			</label>
		</div>

		{#if loading && objects.length === 0 && commonPrefixes.length === 0}
			<div class="loading-wrap">
				<div class="sk-row"><div class="sk" style="width:30px;height:30px;border-radius:4px"></div><div class="sk" style="width:200px;height:14px"></div></div>
				<div class="sk-row"><div class="sk" style="width:30px;height:30px;border-radius:4px"></div><div class="sk" style="width:140px;height:14px"></div></div>
				<div class="sk-row"><div class="sk" style="width:30px;height:30px;border-radius:4px"></div><div class="sk" style="width:180px;height:14px"></div></div>
			</div>
		{:else if error}
			<div class="err-banner">{error}</div>
		{:else if filteredFolders.length === 0 && filteredFiles.length === 0}
			<div class="empty">No files or folders found here.</div>
		{:else}
			<div class="explorer">
				<div class="explorer-header">
					<span style="flex: 3;">Name</span>
					<span style="flex: 1; text-align: right;">Size</span>
					<span style="flex: 1.5; text-align: right;">Last Modified</span>
				</div>

				<div class="items-list">
					<!-- Folders -->
					{#each filteredFolders as folder}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="item-row folder-row" onclick={() => handleItemClick({ type: 'folder', path: folder })}>
							<div class="item-name" style="flex: 3;">
								<Folder style="color: var(--accent); flex-shrink: 0;" width="16" height="16" />
								<span class="mono">{folder.replace(currentPrefix, '')}</span>
							</div>
							<div class="item-size" style="flex: 1; text-align: right;">—</div>
							<div class="item-date" style="flex: 1.5; text-align: right;">—</div>
						</div>
					{/each}

					<!-- Files -->
					{#each filteredFiles as file}
						<!-- svelte-ignore a11y_click_events_have_key_events -->
						<!-- svelte-ignore a11y_no_static_element_interactions -->
						<div class="item-row file-row" onclick={() => handleItemClick({ type: 'file', path: file.key })}>
							<div class="item-name" style="flex: 3;">
								<File style="color: var(--text-3); flex-shrink: 0;" width="16" height="16" />
								<span class="mono trunc">{file.key.replace(currentPrefix, '')}</span>
							</div>
							<div class="item-size" style="flex: 1; text-align: right;">{fmtBytes(file.size)}</div>
							<div class="item-date" style="flex: 1.5; text-align: right;">
								{#if file.last_modified}
									<Calendar width="11" height="11" style="display:inline; margin-right:4px; vertical-align:-1px;" />
									{new Date(file.last_modified).toLocaleString()}
								{:else}
									—
								{/if}
							</div>
						</div>
					{/each}
				</div>
			</div>
		{/if}

	{:else}
		<!-- ── Bucket List View ── -->

		{#if bucketsLoading}
			<div class="loading-wrap">
				<div class="sk-row"><div class="sk" style="width:48px;height:48px;border-radius:8px"></div><div class="sk" style="width:220px;height:16px"></div></div>
			</div>
		{:else if bucketsError}
			<div class="err-banner">{bucketsError}</div>
		{:else if buckets.length === 0}
			<div class="empty">No storage backends configured.</div>
		{:else}
			<div class="bucket-grid">
				{#each buckets as bucket}
					<!-- svelte-ignore a11y_click_events_have_key_events -->
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="bucket-card" onclick={() => navigateToBucket(bucket)}>
						<div class="bucket-icon">
							{#if bucket.backend === 's3'}
								<Server width="22" height="22" />
							{:else}
								<HardDrive width="22" height="22" />
							{/if}
						</div>
						<div class="bucket-info">
							<div class="bucket-name mono">{bucket.bucket}</div>
							<div class="bucket-meta">
								<span class="badge">{bucket.backend === 's3' ? 'S3 / MinIO' : 'Local Disk'}</span>
								{#if bucket.endpoint}
									<span class="bucket-endpoint mono trunc">{bucket.endpoint}</span>
								{/if}
							</div>
						</div>
						<ChevronRight width="16" height="16" style="color: var(--text-3); flex-shrink: 0;" />
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
	.ttl { font-size: 20px; font-weight: 700; color: var(--text); margin: 0 0 4px; letter-spacing: -0.02em; }
	.sub { font-size: 13px; color: var(--text-3); margin: 0; }

	.refresh-btn { display: flex; align-items: center; justify-content: center; width: 34px; height: 34px; border-radius: var(--radius-sm); cursor: pointer; border: 1px solid var(--border); background: var(--surface); color: var(--text-2); transition: background .15s; }
	.refresh-btn:hover { background: var(--surface-2); }

	/* ── Bucket Grid ── */
	.bucket-grid { display: flex; flex-direction: column; gap: 10px; }

	.bucket-card { display: flex; align-items: center; gap: 16px; padding: 18px 20px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); cursor: pointer; transition: background .15s, border-color .15s; }
	.bucket-card:hover { background: var(--surface-2); border-color: var(--accent); }

	.bucket-icon { display: flex; align-items: center; justify-content: center; width: 44px; height: 44px; border-radius: 10px; background: var(--surface-2); border: 1px solid var(--border); color: var(--accent); flex-shrink: 0; }

	.bucket-info { flex: 1; min-width: 0; }
	.bucket-name { font-size: 15px; font-weight: 700; color: var(--text); margin-bottom: 6px; }
	.bucket-meta { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
	.badge { display: inline-block; padding: 2px 8px; background: var(--accent-soft, rgba(99,102,241,.12)); color: var(--accent); font-size: 11px; font-weight: 700; border-radius: 20px; letter-spacing: .04em; }
	.bucket-endpoint { font-size: 12px; color: var(--text-3); }
	/* Navigation and Breadcrumbs */
	.nav-bar { display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; background: var(--surface-2); border: 1px solid var(--border); border-radius: var(--radius); margin-bottom: 16px; min-height: 42px; box-sizing: border-box; }
	.breadcrumbs { display: flex; align-items: center; flex-wrap: wrap; gap: 4px; font-size: 13px; color: var(--text-2); }
	.crumb-btn { display: inline-flex; align-items: center; background: transparent; border: none; padding: 2px 6px; border-radius: 4px; color: var(--text-2); font-weight: 500; cursor: pointer; font-family: var(--font); font-size: 13px; transition: color .1s, background .1s; }
	.crumb-btn:hover { color: var(--accent); background: rgba(255,255,255,0.03); }

	.up-btn { display: inline-flex; align-items: center; gap: 6px; padding: 4px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 12px; font-weight: 600; color: var(--text-2); cursor: pointer; transition: background .15s; font-family: var(--font); }
	.up-btn:hover { background: var(--surface-2); color: var(--text); }

	/* Toolbar & Search */
	.toolbar { display: flex; align-items: center; margin-bottom: 16px; }
	.search { position: relative; display: flex; align-items: center; flex: 1; cursor: text; }
	.search input { height: 34px; padding: 0 10px 0 32px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 13px; color: var(--text); outline: none; width: 100%; transition: border-color .15s, box-shadow .15s; font-family: var(--font); }
	.search input::placeholder { color: var(--text-3); }
	.search input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-ring); }

	/* Explorer Table */
	.explorer { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); overflow: hidden; box-shadow: var(--shadow-sm); }
	.explorer-header { display: flex; align-items: center; padding: 10px 16px; background: var(--surface-2); border-bottom: 1px solid var(--border); font-size: 10.5px; font-weight: 700; color: var(--text-3); text-transform: uppercase; letter-spacing: .065em; }
	.items-list { display: flex; flex-direction: column; }

	.item-row { display: flex; align-items: center; padding: 11px 16px; border-bottom: 1px solid var(--border); transition: background .1s; cursor: pointer; font-size: 13px; color: var(--text-2); }
	.item-row:last-child { border-bottom: none; }
	.item-row:hover { background: var(--row-hover); }

	.item-name { display: flex; align-items: center; gap: 10px; font-family: var(--mono); color: var(--text); min-width: 0; }

	.item-size { color: var(--text-2); }
	.item-date { color: var(--text-3); font-size: 12px; }

	.mono { font-family: var(--mono); }
	.trunc { text-overflow: ellipsis; white-space: nowrap; overflow: hidden; }

	/* Loading Skeleton & Error/Empty */
	.loading-wrap { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); padding: 16px; display: flex; flex-direction: column; gap: 16px; }
	.sk-row { display: flex; align-items: center; gap: 12px; }
	.sk { background: var(--border); border-radius: 4px; animation: sk 1.3s ease-in-out infinite; }
	@keyframes sk { 0%, 100% { opacity: .5 } 50% { opacity: 1 } }

	.err-banner { padding: 12px 16px; background: var(--danger-soft); border: 1px solid rgba(220,38,38,0.2); border-radius: var(--radius); font-size: 13px; color: var(--danger); }
	.empty { padding: 56px; text-align: center; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); color: var(--text-3); font-size: 13px; }

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
	.diag-btn { display: inline-flex; align-items: center; gap: 6px; padding: 6px 12px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 12px; font-weight: 600; color: var(--text-2); cursor: pointer; transition: background .15s, color .15s; font-family: var(--font); }
	.diag-btn:hover { background: var(--surface-2); color: var(--text); }

	.diag-panel { background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius); margin-bottom: 20px; overflow: hidden; }
	.diag-header { display: flex; align-items: center; justify-content: space-between; padding: 10px 14px; background: var(--surface-2); border-bottom: 1px solid var(--border); }
	.diag-title { font-size: 12px; font-weight: 700; color: var(--text-2); text-transform: uppercase; letter-spacing: .05em; }
	.diag-rerun { display: inline-flex; align-items: center; gap: 5px; padding: 4px 10px; background: var(--surface); border: 1px solid var(--border); border-radius: var(--radius-sm); font-size: 11px; font-weight: 600; color: var(--text-2); cursor: pointer; font-family: var(--font); }
	.diag-rerun:hover { background: var(--surface-2); }
	.diag-rerun:disabled { opacity: .5; cursor: not-allowed; }
	.diag-close { display: flex; align-items: center; justify-content: center; width: 24px; height: 24px; background: none; border: none; cursor: pointer; color: var(--text-3); border-radius: 4px; }
	.diag-close:hover { background: var(--surface-2); color: var(--text-2); }

	.diag-loading { display: flex; align-items: center; gap: 8px; padding: 14px; font-size: 13px; color: var(--text-3); }

	.diag-rows { display: flex; flex-direction: column; padding: 8px 0; }
	.diag-row { display: flex; align-items: center; gap: 8px; padding: 7px 14px; font-size: 13px; }
	.diag-ok  { color: var(--ok, #22c55e); }
	.diag-fail { color: var(--danger, #ef4444); }
	.diag-detail { font-size: 11px; color: var(--text-3); font-family: var(--mono); word-break: break-all; }
	.diag-info { padding: 4px 14px 8px 36px; font-size: 12px; color: var(--text-3); }

	.diag-summary { padding: 10px 14px; font-size: 12px; border-top: 1px solid var(--border); color: var(--ok, #22c55e); background: rgba(34,197,94,.06); }
	.diag-summary-warn { color: var(--warn, #f59e0b); background: rgba(245,158,11,.06); }

	/* Responsiveness */
	@media (max-width: 768px) {
		.p { padding: 24px 16px; }
		.explorer-header { display: none; }
		.item-row { flex-direction: column; align-items: flex-start; gap: 4px; padding: 12px; }
		.item-name { width: 100%; }
		.item-size, .item-date { width: 100%; text-align: left !important; font-size: 11px; padding-left: 26px; }

		.overlay { justify-content: center; }
		.drawer { height: 100%; max-width: 100%; border-left: none; }

		.bucket-card { flex-wrap: wrap; }
	}
</style>
