<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { api } from '$lib/api/client';

	interface TraefikSettings {
		main_domain?: string;
		traefik_network?: string;
		traefik_entrypoint_http?: string;
		traefik_entrypoint_https?: string;
		traefik_cert_resolver?: string;
	}
	interface TraefikFileResponse { content: string; path: string }
	interface TraefikDynamicResponse { dir: string; files: { name: string }[] }

	let settings    = $state<TraefikSettings>({});
	let loading     = $state(true);
	let saving      = $state(false);
	let saved       = $state(false);
	let saveError   = $state('');

	type Tab = 'settings' | 'static' | 'dynamic' | 'logs';
	let activeTab  = $state<Tab>('settings');
	let staticFile = $state<TraefikFileResponse | null>(null);
	let staticLoading = $state(false);
	let dynamicDir    = $state<TraefikDynamicResponse | null>(null);
	let dynamicLoading = $state(false);
	let selectedFile  = $state<string | null>(null);
	let selectedContent = $state<TraefikFileResponse | null>(null);
	let fileLoading   = $state(false);
	let copied = $state(false);

	let network      = $derived(settings.traefik_network        || 'platform_proxy');
	let httpEp       = $derived(settings.traefik_entrypoint_http  || 'web');
	let httpsEp      = $derived(settings.traefik_entrypoint_https || 'websecure');
	let certResolver = $derived(settings.traefik_cert_resolver   || 'letsencrypt');
	let domain       = $derived(settings.main_domain             || 'example.com');

	async function load() {
		const res = await api.get<TraefikSettings>('/settings');
		if (res.data) settings = {
			main_domain:                res.data.main_domain,
			traefik_network:            res.data.traefik_network,
			traefik_entrypoint_http:    res.data.traefik_entrypoint_http,
			traefik_entrypoint_https:   res.data.traefik_entrypoint_https,
			traefik_cert_resolver:      res.data.traefik_cert_resolver,
		};
		loading = false;
	}

	async function save(e: SubmitEvent) {
		e.preventDefault();
		saving = true; saved = false; saveError = '';
		const res = await api.put('/settings', settings);
		if (res.error) saveError = res.error.message;
		else { saved = true; setTimeout(() => (saved = false), 3000); }
		saving = false;
	}

	async function loadStatic() {
		staticLoading = true;
		const r = await api.get<TraefikFileResponse>('/settings/traefik/static');
		if (r.data) staticFile = r.data;
		staticLoading = false;
	}

	async function loadDynamic() {
		dynamicLoading = true;
		const r = await api.get<TraefikDynamicResponse>('/settings/traefik/dynamic');
		if (r.data) dynamicDir = r.data;
		dynamicLoading = false;
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

	let logLines = $state<string[]>([]);
	let logConnected = $state(false);
	let logEs: EventSource | null = null;
	let logEl = $state<HTMLDivElement | null>(null);

	function openLogStream() {
		if (logEs) { logEs.close(); logEs = null; }
		logLines = [];
		logConnected = false;
		const es = new EventSource('/api/admin/traefik/logs/stream');
		es.onopen = () => { logConnected = true; };
		es.onmessage = (e) => {
			logLines = [...logLines.slice(-499), e.data];
			requestAnimationFrame(() => { if (logEl) logEl.scrollTop = logEl.scrollHeight; });
		};
		es.onerror = () => { logConnected = false; };
		logEs = es;
	}

	function closeLogStream() {
		logEs?.close();
		logEs = null;
		logConnected = false;
	}

	async function switchTab(t: Tab) {
		activeTab = t;
		if (t === 'static'  && !staticFile)  await loadStatic();
		if (t === 'dynamic' && !dynamicDir)  await loadDynamic();
		if (t !== 'logs') closeLogStream();
	}

	onDestroy(() => closeLogStream());

	let traefikYaml = $derived(`# Traefik v3 — Static Configuration
api:
  dashboard: true
  insecure: false

entryPoints:
  ${httpEp}:
    address: ":80"
    http:
      redirections:
        entryPoint:
          to: ${httpsEp}
          scheme: https
  ${httpsEp}:
    address: ":443"

providers:
  docker:
    swarmMode: true
    exposedByDefault: false
    network: ${network}

certificatesResolvers:
  ${certResolver}:
    acme:
      email: "admin@${domain}"
      storage: /letsencrypt/acme.json
      httpChallenge:
        entryPoint: ${httpEp}

log:
  level: INFO`);

	onMount(load);
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Traefik</h1>
			<p class="sub">Proxy settings, static config, and dynamic files.</p>
		</div>
	</header>

	<div class="tabs">
		<button class="tab" class:active={activeTab === 'settings'} onclick={() => switchTab('settings')}>Settings</button>
		<button class="tab" class:active={activeTab === 'static'}   onclick={() => switchTab('static')}>Static File</button>
		<button class="tab" class:active={activeTab === 'dynamic'}  onclick={() => switchTab('dynamic')}>Dynamic Dir</button>
		<button class="tab" class:active={activeTab === 'logs'}     onclick={() => switchTab('logs')}>Log Stream</button>
	</div>

	{#if activeTab === 'settings'}
		{#if loading}
			<div class="card sk-wrap">
				{#each [0,1,2,3,4] as _}
					<div class="sk-row"><div class="sk sk-label"></div><div class="sk sk-input"></div></div>
				{/each}
			</div>
		{:else}
			<form class="card" onsubmit={save}>
				<div class="field">
					<label class="lbl" for="domain">Main Domain</label>
					<input id="domain" class="inp" bind:value={settings.main_domain} placeholder="example.com" />
				</div>
				<div class="field">
					<label class="lbl" for="network">Traefik Network</label>
					<input id="network" class="inp" bind:value={settings.traefik_network} placeholder="platform_proxy" />
				</div>
				<div class="row2">
					<div class="field">
						<label class="lbl" for="http-ep">HTTP Entrypoint</label>
						<input id="http-ep" class="inp" bind:value={settings.traefik_entrypoint_http} placeholder="web" />
					</div>
					<div class="field">
						<label class="lbl" for="https-ep">HTTPS Entrypoint</label>
						<input id="https-ep" class="inp" bind:value={settings.traefik_entrypoint_https} placeholder="websecure" />
					</div>
				</div>
				<div class="field">
					<label class="lbl" for="cert">Cert Resolver</label>
					<input id="cert" class="inp" bind:value={settings.traefik_cert_resolver} placeholder="letsencrypt" />
				</div>
				{#if saveError}
					<div class="err-msg">{saveError}</div>
				{/if}
				<div class="form-foot">
					<button type="submit" class="btn-primary" disabled={saving}>
						{#if saved}Saved{:else if saving}Saving…{:else}Save Changes{/if}
					</button>
				</div>
			</form>

			<div class="tpl-card">
				<div class="tpl-hdr">
					<span class="tpl-title">Generated traefik.yml</span>
					<button class="copy-btn" onclick={() => copyCode(traefikYaml)}>
						{copied ? 'Copied!' : 'Copy'}
					</button>
				</div>
				<pre class="code">{traefikYaml}</pre>
			</div>
		{/if}

	{:else if activeTab === 'static'}
		{#if staticLoading}
			<div class="card sk-wrap"><div class="sk" style="height:200px"></div></div>
		{:else if staticFile}
			<div class="tpl-card">
				<div class="tpl-hdr">
					<span class="tpl-title mono">{staticFile.path}</span>
					<button class="copy-btn" onclick={() => copyCode(staticFile!.content)}>
						{copied ? 'Copied!' : 'Copy'}
					</button>
				</div>
				<pre class="code">{staticFile.content}</pre>
			</div>
		{:else}
			<div class="empty">No static config found on server.</div>
		{/if}

	{:else if activeTab === 'logs'}
		<div class="log-shell">
			<div class="log-hdr">
				<div class="log-hdr-l">
					<span class="conn-dot" class:conn-ok={logConnected}></span>
					<span class="log-title">{logConnected ? 'Live' : logEs ? 'Connecting…' : 'Not connected'}</span>
				</div>
				<div style="display:flex;gap:6px">
					{#if !logEs}
						<button class="t-btn t-btn-connect" onclick={openLogStream}>Connect</button>
					{:else}
						<button class="t-btn" onclick={openLogStream}>Reconnect</button>
						<button class="t-btn" onclick={closeLogStream}>Disconnect</button>
					{/if}
					<button class="t-btn" onclick={() => { logLines = []; }}>Clear</button>
				</div>
			</div>
			<div class="log-body" bind:this={logEl}>
				{#if !logEs && logLines.length === 0}
					<div class="log-empty">Click <strong>Connect</strong> to start streaming Traefik access logs.</div>
				{:else if logLines.length === 0}
					<div class="log-empty">Waiting for log entries…</div>
				{:else}
					{#each logLines as line}
						<div class="log-line">{line}</div>
					{/each}
				{/if}
			</div>
		</div>

	{:else if activeTab === 'dynamic'}
		{#if dynamicLoading}
			<div class="card sk-wrap"><div class="sk" style="height:80px"></div></div>
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
	{/if}
</div>

<style>
	.p { max-width:860px; margin:0 auto; padding:40px 36px; }

	.hdr { margin-bottom:20px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }

	.tabs { display:flex; gap:2px; margin-bottom:16px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); padding:3px; width:fit-content; }
	.tab { padding:5px 14px; border-radius:5px; font-size:12.5px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.tab.active { background:var(--surface); color:var(--text); box-shadow:var(--shadow-sm); }
	.tab:hover:not(.active) { color:var(--text); }

	.card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); padding:24px; box-shadow:var(--shadow-sm); }
	.sk-wrap { display:flex; flex-direction:column; gap:16px; }
	.sk { background:var(--border); border-radius:4px; animation:sk 1.3s ease-in-out infinite; }
	.sk-label { width:90px; height:11px; }
	.sk-input { height:34px; border-radius:var(--radius-sm); flex:1; }
	.sk-row { display:flex; align-items:center; gap:12px; }
	@keyframes sk { 0%,100%{opacity:.5} 50%{opacity:1} }

	.field { display:flex; flex-direction:column; gap:5px; margin-bottom:14px; }
	.field:last-of-type { margin-bottom:0; }
	.row2 { display:grid; grid-template-columns:1fr 1fr; gap:12px; }
	.lbl { font-size:11.5px; font-weight:600; color:var(--text-2); }
	.inp { height:34px; padding:0 10px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); font-size:12.5px; color:var(--text); outline:none; width:100%; box-sizing:border-box; font-family:var(--font); transition:border-color .15s, box-shadow .15s; }
	.inp:focus { border-color:var(--accent); box-shadow:0 0 0 3px var(--accent-ring); }

	.err-msg { padding:8px 12px; background:var(--danger-soft); border:1px solid rgba(220,38,38,0.2); border-radius:var(--radius-sm); font-size:12px; color:var(--danger); margin-top:10px; }

	.form-foot { display:flex; justify-content:flex-end; margin-top:20px; padding-top:16px; border-top:1px solid var(--border); }
	.btn-primary { padding:7px 18px; height:34px; border-radius:var(--radius-sm); font-size:12.5px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; transition:opacity .15s; font-family:var(--font); }
	.btn-primary:hover:not(:disabled) { opacity:.88; }
	.btn-primary:disabled { opacity:.5; cursor:not-allowed; }

	.tpl-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); margin-top:16px; }
	.tpl-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.tpl-title { font-size:12px; font-weight:600; color:var(--text-2); }
	.copy-btn { padding:4px 11px; border-radius:var(--radius-sm); font-size:11px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.copy-btn:hover { background:var(--accent); border-color:var(--accent); color:#000; }
	.code { margin:0; padding:16px; font-size:11.5px; line-height:1.65; color:var(--text-2); font-family:var(--mono); white-space:pre-wrap; word-break:break-all; overflow-x:auto; }
	.mono { font-family:var(--mono); }

	.dyn-shell { display:grid; grid-template-columns:220px 1fr; gap:0; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); min-height:240px; }
	.file-list { border-right:1px solid var(--border); display:flex; flex-direction:column; }
	.file-list-hdr { padding:9px 12px; font-size:10px; font-weight:700; color:var(--text-3); text-transform:uppercase; letter-spacing:.06em; border-bottom:1px solid var(--border); background:var(--surface-2); font-family:var(--mono); word-break:break-all; }
	.file-item { display:flex; align-items:center; gap:7px; padding:8px 12px; font-size:12px; color:var(--text-2); cursor:pointer; border:none; background:transparent; text-align:left; transition:background .1s, color .1s; width:100%; font-family:var(--mono); }
	.file-item:hover { background:var(--row-hover); color:var(--text); }
	.file-item.file-sel { background:var(--accent-soft); color:var(--accent); }
	.file-empty { padding:16px 12px; font-size:12px; color:var(--text-3); }
	.file-content { display:flex; flex-direction:column; min-width:0; }
	.fc-center { display:flex; align-items:center; justify-content:center; flex:1; padding:40px; }

	.empty { padding:48px; text-align:center; background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); color:var(--text-3); font-size:13px; }

	.mini-spin { display:inline-block; width:18px; height:18px; border:2px solid var(--border-2); border-top-color:var(--accent); border-radius:50%; animation:spin .7s linear infinite; }
	@keyframes spin { to { transform:rotate(360deg); } }

	.log-shell { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); }
	.log-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.log-hdr-l { display:flex; align-items:center; gap:7px; }
	.conn-dot { display:inline-block; width:7px; height:7px; border-radius:50%; background:var(--text-4); flex-shrink:0; }
	.conn-dot.conn-ok { background:var(--ok); box-shadow:0 0 0 2px var(--ok-soft); }
	.log-title { font-size:12px; font-weight:600; color:var(--text-2); }
	.log-body { height:420px; overflow-y:auto; padding:10px 14px; background:#0d0d0d; scrollbar-width:thin; scrollbar-color:rgba(255,255,255,0.1) transparent; }
	.log-line { font-size:11.5px; font-family:var(--mono); color:rgba(255,255,255,0.75); line-height:1.55; white-space:pre-wrap; word-break:break-all; }
	.log-empty { font-size:12px; color:rgba(255,255,255,0.3); padding:20px 0; }
	.t-btn { padding:4px 11px; border-radius:var(--radius-sm); font-size:11px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.t-btn:hover { background:var(--surface-2); }
	.t-btn-connect { background:var(--accent); color:#000; border-color:var(--accent); }
	.t-btn-connect:hover { opacity:.88; }
</style>
