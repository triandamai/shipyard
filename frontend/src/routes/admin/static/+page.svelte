<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api/client';

	interface NginxConfEntry { name: string }
	interface NginxConfList { dir: string; files: NginxConfEntry[]; error?: string; }
	interface NginxConfFile { name: string; content?: string; exists: boolean; error?: string; }

	let confList    = $state<NginxConfList | null>(null);
	let loading     = $state(true);
	let listError   = $state('');
	let selected    = $state<string | null>(null);
	let fileContent = $state<NginxConfFile | null>(null);
	let loadingFile = $state(false);
	let copied      = $state(false);
	let search      = $state('');

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
		loadingFile = true;
		const res = await api.get<NginxConfFile>(`/admin/nginx-static/confs/${encodeURIComponent(name)}`);
		if (res.data) fileContent = res.data;
		loadingFile = false;
	}

	async function copyContent() {
		if (!fileContent?.content) return;
		await navigator.clipboard.writeText(fileContent.content);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

	let filteredFiles = $derived(
		(confList?.files ?? []).filter(f => !search || f.name.toLowerCase().includes(search.toLowerCase()))
	);

	let logLines = $state<string[]>([]);
	let logConnected = $state(false);
	let logEs: EventSource | null = null;
	let logEl = $state<HTMLDivElement | null>(null);
	let showLogs = $state(false);

	function openLogStream() {
		logEs?.close();
		logLines = [];
		logConnected = false;
		const es = new EventSource('/api/admin/nginx-static/logs/stream');
		es.onopen = () => { logConnected = true; };
		es.onmessage = (e) => {
			logLines = [...logLines.slice(-499), e.data];
			requestAnimationFrame(() => { if (logEl) logEl.scrollTop = logEl.scrollHeight; });
		};
		es.onerror = () => { logConnected = false; };
		logEs = es;
	}

	function toggleLogs() {
		if (logEs) {
			logEs.close(); logEs = null; logConnected = false; showLogs = false;
		} else {
			showLogs = true;
			openLogStream();
		}
	}

	onMount(loadList);
	onDestroy(() => { logEs?.close(); });
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Static Sites</h1>
			<p class="sub">Platform nginx configuration files for static site deployments.</p>
		</div>
		<button class="refresh-btn" onclick={loadList}>
			<svg viewBox="0 0 20 20" fill="currentColor" width="13" height="13"><path fill-rule="evenodd" d="M4 2a1 1 0 011 1v2.101a7.002 7.002 0 0111.601 2.566 1 1 0 11-1.885.666A5.002 5.002 0 005.999 7H9a1 1 0 010 2H4a1 1 0 01-1-1V3a1 1 0 011-1zm.008 9.057a1 1 0 011.276.61A5.002 5.002 0 0014.001 13H11a1 1 0 110-2h5a1 1 0 011 1v5a1 1 0 11-2 0v-2.101a7.002 7.002 0 01-11.601-2.566 1 1 0 01.61-1.276z" clip-rule="evenodd"/></svg>
			Refresh
		</button>
	</header>

	{#if loading}
		<div class="shell"><div class="sk" style="height:200px"></div></div>
	{:else if listError}
		<div class="err-banner">{listError}</div>
	{:else}
		<div class="shell">
			<div class="file-list">
				<div class="fl-hdr">
					<span class="fl-path mono">{confList?.dir ?? ''}</span>
					<span class="count">{filteredFiles.length}</span>
				</div>
				<div class="search-wrap">
					<svg viewBox="0 0 20 20" fill="currentColor" class="si" width="12" height="12"><path fill-rule="evenodd" d="M8 4a4 4 0 100 8 4 4 0 000-8zM2 8a6 6 0 1110.89 3.476l4.817 4.817a1 1 0 01-1.414 1.414l-4.816-4.816A6 6 0 012 8z" clip-rule="evenodd"/></svg>
					<input class="search-inp" placeholder="Filter files…" bind:value={search} />
				</div>
				<div class="fl-body">
					{#each filteredFiles as f}
						<button class="fl-item" class:fl-sel={selected === f.name} onclick={() => openFile(f.name)}>
							<svg viewBox="0 0 20 20" fill="currentColor" width="12" height="12"><path fill-rule="evenodd" d="M4 4a2 2 0 012-2h4.586A2 2 0 0112 2.586L15.414 6A2 2 0 0116 7.414V16a2 2 0 01-2 2H6a2 2 0 01-2-2V4z" clip-rule="evenodd"/></svg>
							{f.name}
						</button>
					{/each}
					{#if filteredFiles.length === 0}
						<div class="fl-empty">No config files.</div>
					{/if}
				</div>
			</div>

			<div class="file-content">
				{#if !selected}
					<div class="fc-placeholder">Select a config file to view</div>
				{:else if loadingFile}
					<div class="fc-placeholder"><div class="mini-spin"></div></div>
				{:else if fileContent}
					<div class="fc-hdr">
						<span class="mono fc-name">{fileContent.name}</span>
						{#if fileContent.content}
							<button class="copy-btn" onclick={copyContent}>{copied ? 'Copied!' : 'Copy'}</button>
						{/if}
					</div>
					{#if fileContent.error}
						<div class="fc-err">{fileContent.error}</div>
					{:else if fileContent.content}
						<pre class="code">{fileContent.content}</pre>
					{:else}
						<div class="fc-placeholder" style="padding:24px">File is empty.</div>
					{/if}
				{/if}
			</div>
		</div>
	{/if}

	<!-- Nginx Log Stream — below conf section -->
	<div class="log-section-hdr">
		<span class="log-section-title">Nginx Log Stream</span>
		{#if !logEs}
			<button class="log-connect-btn" onclick={toggleLogs}>Connect</button>
		{:else}
			<div style="display:flex;gap:6px;align-items:center">
				<span class="conn-dot" class:conn-ok={logConnected}></span>
				<span style="font-size:11.5px;color:var(--text-3)">{logConnected ? 'Live' : 'Connecting…'}</span>
				<button class="copy-btn" onclick={openLogStream}>Reconnect</button>
				<button class="copy-btn" onclick={() => { logLines = []; }}>Clear</button>
				<button class="copy-btn" onclick={toggleLogs}>Disconnect</button>
			</div>
		{/if}
	</div>
	{#if showLogs}
		<div class="log-shell">
			<div class="log-body" bind:this={logEl}>
				{#if logLines.length === 0}
					<div class="log-empty">Waiting for log entries…</div>
				{:else}
					{#each logLines as line}
						<div class="log-line">{line}</div>
					{/each}
				{/if}
			</div>
		</div>
	{/if}
</div>

<style>
	.p { max-width:1040px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:20px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }
	.refresh-btn { display:flex; align-items:center; gap:6px; padding:6px 12px; height:32px; border-radius:var(--radius-sm); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.refresh-btn:hover { background:var(--surface-2); }
	.refresh-btn.log-active { background:var(--accent-soft); color:var(--accent); border-color:var(--accent-ring); }

	.log-section-hdr { display:flex; align-items:center; justify-content:space-between; margin-top:24px; margin-bottom:10px; }
	.log-section-title { font-size:13px; font-weight:700; color:var(--text); }
	.log-connect-btn { padding:5px 14px; border-radius:var(--radius-sm); font-size:11.5px; font-weight:600; cursor:pointer; border:none; background:var(--accent); color:#000; font-family:var(--font); }
	.log-connect-btn:hover { opacity:.88; }
	.log-shell { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); margin-bottom:20px; }
	.log-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.conn-dot { display:inline-block; width:7px; height:7px; border-radius:50%; background:var(--text-4); flex-shrink:0; }
	.conn-dot.conn-ok { background:var(--ok); box-shadow:0 0 0 2px var(--ok-soft); }
	.log-title { font-size:12px; font-weight:600; color:var(--text-2); }
	.log-body { height:340px; overflow-y:auto; padding:10px 14px; background:#0d0d0d; scrollbar-width:thin; scrollbar-color:rgba(255,255,255,0.1) transparent; }
	.log-line { font-size:11.5px; font-family:var(--mono); color:rgba(255,255,255,0.75); line-height:1.55; white-space:pre-wrap; word-break:break-all; }
	.log-empty { font-size:12px; color:rgba(255,255,255,0.3); padding:20px 0; }

	.shell { display:grid; grid-template-columns:260px 1fr; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:0 1px 2px rgba(0,0,0,.07); min-height:340px; }

	.file-list { border-right:1px solid var(--border); display:flex; flex-direction:column; }
	.fl-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 12px; border-bottom:1px solid var(--border); background:var(--surface-2); gap:6px; }
	.fl-path { font-size:10.5px; color:var(--text-3); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
	.count { font-size:10px; font-weight:700; background:var(--border); color:var(--text-3); padding:1px 6px; border-radius:999px; flex-shrink:0; }
	.search-wrap { position:relative; display:flex; align-items:center; padding:8px 8px; border-bottom:1px solid var(--border); }
	.si { position:absolute; left:16px; color:var(--text-3); pointer-events:none; }
	.search-inp { height:28px; padding:0 8px 0 26px; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12px; color:var(--text); outline:none; width:100%; box-sizing:border-box; font-family:var(--font); transition:border-color .15s; }
	.search-inp::placeholder { color:var(--text-3); }
	.search-inp:focus { border-color:var(--accent); }
	.fl-body { overflow-y:auto; flex:1; }
	.fl-item { display:flex; align-items:center; gap:7px; padding:8px 12px; font-size:12px; font-family:var(--mono); color:var(--text-2); cursor:pointer; border:none; background:transparent; text-align:left; transition:background .1s, color .1s; width:100%; }
	.fl-item:hover { background:var(--row-hover); color:var(--text); }
	.fl-item.fl-sel { background:var(--accent-soft); color:var(--accent); }
	.fl-empty { padding:16px 12px; font-size:12px; color:var(--text-3); }

	.file-content { display:flex; flex-direction:column; min-width:0; }
	.fc-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.fc-name { font-size:12px; color:var(--text-2); }
	.copy-btn { padding:4px 11px; border-radius:var(--radius-sm); font-size:11px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.copy-btn:hover { background:var(--accent); border-color:var(--accent); color:#000; }
	.fc-placeholder { display:flex; align-items:center; justify-content:center; flex:1; color:var(--text-3); font-size:12.5px; padding:60px; }
	.fc-err { padding:16px; color:var(--danger); font-size:12.5px; }
	.code { margin:0; padding:16px; font-size:11.5px; line-height:1.65; color:var(--text-2); font-family:var(--mono); white-space:pre-wrap; word-break:break-all; overflow-x:auto; flex:1; }

	.err-banner { padding:11px 14px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius); font-size:13px; color:var(--danger); }

	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; width:100%; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.mono { font-family:var(--mono); }
	.mini-spin { display:inline-block; width:18px; height:18px; border:2px solid var(--border-2); border-top-color:var(--accent); border-radius:50%; animation:spin .7s linear infinite; }
	@keyframes spin { to { transform:rotate(360deg); } }
</style>
