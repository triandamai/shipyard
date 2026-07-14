<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';

	interface TraefikFileResponse { content: string; path: string }
	interface TraefikDynamicResponse { dir: string; files: { name: string }[] }

	let dynamicDir    = $state<TraefikDynamicResponse | null>(null);
	let loading       = $state(true);
	let error         = $state('');
	let selectedFile  = $state<string | null>(null);
	let selectedContent = $state<TraefikFileResponse | null>(null);
	let fileLoading   = $state(false);
	let copied = $state(false);

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

	onMount(load);
</script>

{#if loading}
	<div class="card sk-wrap"><div class="sk" style="height:80px"></div></div>
{:else if error}
	<div class="err">{error}</div>
{:else if dynamicDir}
	<div class="dyn-shell">
		<div class="file-list">
			<div class="file-list-hdr">{dynamicDir.dir}</div>
			{#each dynamicDir.files as f}
				<button class="file-item" class:file-sel={selectedFile === f.name} onclick={() => openFile(f.name)}>
					<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd"/></svg>
					{f.name}
				</button>
			{/each}
			{#if dynamicDir.files.length === 0}
				<div class="file-empty">No dynamic files.</div>
			{/if}
		</div>
		<div class="file-content">
			{#if fileLoading}
				<div class="fc-center"><div class="mini-spin"></div></div>
			{:else if selectedContent}
				<div class="tpl-hdr">
					<span class="tpl-title mono">{selectedContent.path}</span>
					<button class="copy-btn" onclick={() => copyCode(selectedContent!.content)}>
						{copied ? 'Copied!' : 'Copy'}
					</button>
				</div>
				<pre class="code" style="border-top-left-radius:0;border-top-right-radius:0">{selectedContent.content}</pre>
			{:else}
				<div class="fc-center" style="color:var(--text-3);font-size:12.5px">Select a file to view</div>
			{/if}
		</div>
	</div>
{:else}
	<div class="empty">No dynamic config directory accessible.</div>
{/if}

<style>
	.card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:24px; box-shadow:var(--shadow-sm); }
	.sk-wrap { display:flex; flex-direction:column; gap:16px; }
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.dyn-shell { display:grid; grid-template-columns:220px 1fr; gap:0; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); min-height:240px; }
	.file-list { border-right:1px solid var(--border); display:flex; flex-direction:column; }
	.file-list-hdr { padding:9px 12px; font-size:10px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.06em; border-bottom:1px solid var(--border); background:var(--surface-2); font-family:var(--mono); word-break:break-all; }
	.file-item { display:flex; align-items:center; gap:7px; padding:8px 12px; font-size:12px; color:var(--text-2); cursor:pointer; border:none; background:transparent; text-align:left; transition:background .1s, color .1s; width:100%; font-family:var(--mono); }
	.file-item:hover { background:var(--row-hover); color:var(--text); }
	.file-item.file-sel { background:var(--accent-soft); color:var(--accent); }
	.file-empty { padding:16px 12px; font-size:12px; color:var(--text-3); }
	.file-content { display:flex; flex-direction:column; min-width:0; }
	.fc-center { display:flex; align-items:center; justify-content:center; flex:1; padding:40px; }

	.tpl-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.tpl-title { font-size:12px; font-weight:600; color:var(--text-2); }
	.copy-btn { padding:4px 11px; border-radius:var(--radius-sm); font-size:11px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.copy-btn:hover { background:var(--accent); border-color:var(--accent); color:#000; }
	.code { margin:0; padding:16px; font-size:11.5px; line-height:1.65; color:var(--text-2); font-family:var(--mono); white-space:pre-wrap; word-break:break-all; overflow-x:auto; }
	.mono { font-family:var(--mono); }

	.err { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }
	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.mini-spin { display:inline-block; width:18px; height:18px; border:2px solid var(--border-2); border-top-color:var(--accent); border-radius:50%; animation:spin .7s linear infinite; }
	@keyframes spin { to { transform:rotate(360deg); } }

	@media (max-width: 640px) {
		.dyn-shell { grid-template-columns: 1fr; }
		.file-list { border-right: none; border-bottom: 1px solid var(--border); }
	}
</style>
