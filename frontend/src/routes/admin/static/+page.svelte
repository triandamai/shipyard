<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import LogViewerOverlay from '$lib/components/LogViewerOverlay.svelte';
	import { FileText, RefreshCw, ScrollText } from '@lucide/svelte';

	import {
		PageHeader,
		SearchInput,
		ActivityList,
		ListRow,
		Card,
		Button,
		EmptyState,
		InlineAlert,
		Skeleton,
		Spinner
	} from '$lib/components/ui';
	import CodeEditor from '$lib/components/CodeEditor.svelte';
	import { authStore } from '$lib/stores/auth.store';

	interface NginxConfEntry { name: string }
	interface NginxConfList { dir: string; files: NginxConfEntry[]; error?: string; }
	interface NginxConfFile { name: string; content?: string; exists: boolean; error?: string; }

	let confList      = $state<NginxConfList | null>(null);
	let loading       = $state(true);
	let listError     = $state('');
	let selected      = $state<string | null>(null);
	let fileContent   = $state<NginxConfFile | null>(null);
	let editorContent = $state('');
	let loadingFile   = $state(false);
	let saving        = $state(false);
	let fileError     = $state('');
	let successMsg    = $state('');
	let copied        = $state(false);
	let search        = $state('');

	const canManage = $derived(
		$authStore.user?.is_superadmin === true ||
		($authStore.user?.staff_permissions ?? []).includes('shipyard:admin:infra:static:manage')
	);

	let hasChanges = $derived(
		fileContent?.content !== undefined && editorContent !== fileContent.content
	);

	async function loadList() {
		loading = true;
		listError = '';
		const res = await api.get<NginxConfList>('/admin/nginx-static/confs');
		if (res.data) {
			confList = res.data;
			if (res.data.error) listError = res.data.error;
		} else {
			listError = res.error?.message ?? 'Failed to load';
		}
		loading = false;
	}

	async function openFile(name: string) {
		selected = name;
		fileContent = null;
		editorContent = '';
		loadingFile = true;
		fileError = '';
		successMsg = '';
		const res = await api.get<NginxConfFile>(`/admin/nginx-static/confs/${encodeURIComponent(name)}`);
		if (res.data) {
			fileContent = res.data;
			editorContent = res.data.content ?? '';
			if (res.data.error) fileError = res.data.error;
		} else {
			fileError = res.error?.message ?? 'Failed to read file';
		}
		loadingFile = false;
	}

	async function saveFile() {
		if (!canManage || !selected || saving) return;
		saving = true;
		fileError = '';
		successMsg = '';
		const res = await api.put<NginxConfFile>(
			`/admin/nginx-static/confs/${encodeURIComponent(selected)}`,
			{ content: editorContent }
		);
		if (res.data) {
			fileContent = res.data;
			editorContent = res.data.content ?? '';
			successMsg = 'Nginx configuration saved and reloaded.';
			setTimeout(() => { successMsg = ''; }, 3000);
		} else {
			fileError = res.error?.message ?? 'Failed to save configuration';
		}
		saving = false;
	}

	async function copyContent() {
		if (!editorContent) return;
		await navigator.clipboard.writeText(editorContent);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	let filteredFiles = $derived(
		(confList?.files ?? []).filter(f => !search || f.name.toLowerCase().includes(search.toLowerCase()))
	);

	let showLogs = $state(false);

	onMount(loadList);
</script>

<div class="p">
	<PageHeader title="Static Sites" subtitle="Platform nginx configuration files for static site deployments.">
		{#snippet actions()}
			<Button variant="secondary" size="sm" onclick={() => (showLogs = true)}>
				<ScrollText size={13} />
				Show Log Stream
			</Button>
			<Button variant="secondary" size="sm" onclick={loadList}>
				<RefreshCw size={13} />
				Refresh
			</Button>
		{/snippet}
	</PageHeader>

	{#if loading}
		<Skeleton variant="card" height="340px" />
	{:else if listError}
		<InlineAlert tone="error">{listError}</InlineAlert>
	{:else}
		<div class="shell">
			<div class="file-list">
				<div class="fl-hdr">
					<span class="fl-path mono">{confList?.dir ?? ''}</span>
					<span class="count">{filteredFiles.length}</span>
				</div>
				<div class="search-wrap">
					<SearchInput bind:value={search} placeholder="Filter files…" />
				</div>
				<div class="fl-body">
					{#if filteredFiles.length === 0}
						<EmptyState message="No config files." />
					{:else}
						<ActivityList>
							{#each filteredFiles as f (f.name)}
								<button
									type="button"
									class={selected === f.name ? 'fl-item fl-sel' : 'fl-item'}
									onclick={() => openFile(f.name)}
								>
									<ListRow iconTone="blue" title={f.name}>
										{#snippet icon()}<FileText size={13} />{/snippet}
									</ListRow>
								</button>
							{/each}
						</ActivityList>
					{/if}
				</div>
			</div>

			<div class="file-content">
				{#if !selected}
					<div class="fc-placeholder">Select a config file to view</div>
				{:else if loadingFile}
					<div class="fc-placeholder"><Spinner size={20} /></div>
				{:else if fileContent}
					{#if fileError}
						<div class="alert-row">
							<InlineAlert tone="error">{fileError}</InlineAlert>
						</div>
					{/if}
					{#if successMsg}
						<div class="alert-row">
							<InlineAlert tone="success">{successMsg}</InlineAlert>
						</div>
					{/if}
					<div class="fc-hdr">
						<span class="mono fc-name">{fileContent.name}</span>
						<div class="fc-actions">
							{#if fileContent.content !== undefined}
								<Button variant="secondary" size="sm" onclick={copyContent}>
									{copied ? 'Copied!' : 'Copy'}
								</Button>
								{#if canManage}
									<Button
										variant="primary"
										size="sm"
										onclick={saveFile}
										disabled={saving || !hasChanges}
									>
										{saving ? 'Saving…' : 'Save & Reload'}
									</Button>
								{/if}
							{/if}
						</div>
					</div>
					{#if fileContent.content !== undefined}
						<CodeEditor
							value={editorContent}
							onChange={(v) => { editorContent = v; }}
							onRun={saveFile}
							language="nginx"
							readonly={!canManage}
							height="420px"
						/>
					{:else}
						<div class="fc-placeholder" style="padding:24px">File is empty.</div>
					{/if}
				{/if}
			</div>
		</div>
	{/if}
</div>

<LogViewerOverlay
	open={showLogs}
	title="Nginx Static site logs"
	subtitle="Live stream from shipyard-nginx-static logs"
	streamUrl="/api/admin/nginx-static/logs/stream"
	onClose={() => (showLogs = false)}
	fetchFn={async () => []}
/>

<style>
	.p { max-width:1040px; margin:0 auto; padding:40px 36px; }

	.shell { display:grid; grid-template-columns:260px 1fr; background:var(--bg-surface); border:1px solid var(--border); border-radius:var(--radius-lg); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); min-height:340px; }

	.file-list { border-right:1px solid var(--border); display:flex; flex-direction:column; }
	.fl-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 12px; border-bottom:1px solid var(--border); background:var(--bg-elevated); gap:6px; }
	.fl-path { font-size:10.5px; color:var(--text-dim); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
	.count { font-size:10px; font-weight:700; background:var(--border); color:var(--text-dim); padding:1px 6px; border-radius:999px; flex-shrink:0; }
	.search-wrap { padding:8px; border-bottom:1px solid var(--border); }
	.fl-body { overflow-y:auto; flex:1; padding:6px 10px; }
	.fl-item { display:block; width:100%; background:none; border:none; padding:0; cursor:pointer; text-align:left; font-family:var(--font-sans); border-radius:var(--radius-sm); }
	.fl-item :global(.ui-list-row-title) { font-family:var(--font-mono); }
	.fl-item.fl-sel :global(.ui-list-row-title) { color:var(--accent); }
	.fl-item:hover { background:var(--bg-hover); }

	.file-content { display:flex; flex-direction:column; min-width:0; padding:14px 16px; }
	.fc-hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:10px; }
	.fc-name { font-size:12px; color:var(--text-secondary); }
	.fc-actions { display:flex; align-items:center; gap:8px; }
	.alert-row { margin-bottom:12px; }
	.fc-placeholder { display:flex; align-items:center; justify-content:center; flex:1; color:var(--text-dim); font-size:12.5px; padding:60px; }

	.mono { font-family:var(--font-mono); }

	@media (max-width: 640px) {
		.shell { grid-template-columns: 1fr; }
		.file-list { border-right: none; border-bottom: 1px solid var(--border); }
	}
</style>
