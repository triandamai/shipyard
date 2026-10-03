<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { FileText } from '@lucide/svelte';
	import {
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

	interface TraefikFileResponse { content: string; path: string }
	interface TraefikDynamicResponse { dir: string; files: { name: string }[] }

	let dynamicDir      = $state<TraefikDynamicResponse | null>(null);
	let loading         = $state(true);
	let error           = $state('');
	let selectedFile    = $state<string | null>(null);
	let selectedContent = $state<TraefikFileResponse | null>(null);
	let fileLoading     = $state(false);
	let copied          = $state(false);
	let search          = $state('');

	async function load() {
		loading = true; error = '';
		const r = await api.get<TraefikDynamicResponse>('/settings/traefik/dynamic');
		if (r.data) dynamicDir = r.data;
		else error = r.error?.message ?? 'Failed to load dynamic directory';
		loading = false;
	}

	async function openFile(name: string) {
		selectedFile = name;
		fileLoading = true;
		selectedContent = null;
		const r = await api.get<TraefikFileResponse>(`/settings/traefik/dynamic/${encodeURIComponent(name)}`);
		if (r.data) selectedContent = r.data;
		fileLoading = false;
	}

	async function copyCode(text: string) {
		await navigator.clipboard.writeText(text);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	let filteredFiles = $derived(
		(dynamicDir?.files ?? []).filter(f => !search || f.name.toLowerCase().includes(search.toLowerCase()))
	);

	onMount(load);
</script>

{#if loading}
	<Skeleton variant="card" height="340px" />
{:else if error}
	<InlineAlert tone="error">{error}</InlineAlert>
{:else if dynamicDir}
	<div class="shell">
		<div class="file-list">
			<div class="fl-hdr">
				<span class="fl-path mono">{dynamicDir.dir}</span>
				<span class="count">{filteredFiles.length}</span>
			</div>
			<div class="search-wrap">
				<SearchInput bind:value={search} placeholder="Filter files…" />
			</div>
			<div class="fl-body">
				{#if filteredFiles.length === 0}
					<EmptyState message="No dynamic files." />
				{:else}
					<ActivityList>
						{#each filteredFiles as f (f.name)}
							<button
								type="button"
								class={selectedFile === f.name ? 'fl-item fl-sel' : 'fl-item'}
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
			{#if !selectedFile}
				<div class="fc-placeholder">Select a file to view</div>
			{:else if fileLoading}
				<div class="fc-placeholder"><Spinner size={20} /></div>
			{:else if selectedContent}
				<div class="fc-hdr">
					<span class="mono fc-name">{selectedContent.path}</span>
					{#if selectedContent.content}
						<Button variant="secondary" size="sm" onclick={() => copyCode(selectedContent!.content)}>
							{copied ? 'Copied!' : 'Copy'}
						</Button>
					{/if}
				</div>
				{#if selectedContent.content}
					<Card padding="0">
						<pre class="code">{selectedContent.content}</pre>
					</Card>
				{:else}
					<div class="fc-placeholder" style="padding:24px">File is empty.</div>
				{/if}
			{/if}
		</div>
	</div>
{:else}
	<EmptyState message="No dynamic config directory accessible." />
{/if}

<style>
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
	.fc-placeholder { display:flex; align-items:center; justify-content:center; flex:1; color:var(--text-dim); font-size:12.5px; padding:60px; }
	.code { margin:0; padding:16px; font-size:11.5px; line-height:1.65; color:var(--text-secondary); font-family:var(--font-mono); white-space:pre-wrap; word-break:break-all; overflow-x:auto; }

	.mono { font-family:var(--font-mono); }

	@media (max-width: 640px) {
		.shell { grid-template-columns: 1fr; }
		.file-list { border-right: none; border-bottom: 1px solid var(--border); }
	}
</style>
