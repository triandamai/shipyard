<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import LogViewerOverlay from '$lib/components/LogViewerOverlay.svelte';

	interface TraefikSettings {
		main_domain?: string;
		traefik_network?: string;
		traefik_entrypoint_http?: string;
		traefik_entrypoint_https?: string;
		traefik_cert_resolver?: string;
	}

	let settings    = $state<TraefikSettings>({});
	let loading     = $state(true);
	let saving      = $state(false);
	let saved       = $state(false);
	let saveError   = $state('');
	let copied      = $state(false);
	let showLogs    = $state(false);

	let network      = $derived(settings.traefik_network        || 'platform_proxy');
	let httpEp       = $derived(settings.traefik_entrypoint_http  || 'web');
	let httpsEp      = $derived(settings.traefik_entrypoint_https || 'websecure');
	let certResolver = $derived(settings.traefik_cert_resolver   || 'letsencrypt');
	let domain       = $derived(settings.main_domain             || 'example.com');

	async function load() {
		loading = true;
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

	async function copyCode(text: string) {
		await navigator.clipboard.writeText(text);
		copied = true;
		setTimeout(() => (copied = false), 2000);
	}

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
			<button type="button" class="btn-secondary" onclick={() => (showLogs = true)}>
				Show Log Stream
			</button>
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

<LogViewerOverlay
	open={showLogs}
	title="Traefik Access Logs"
	subtitle="Live HTTP traffic log stream"
	streamUrl="/api/admin/traefik/logs/stream"
	fetchFn={async () => []}
	onClose={() => (showLogs = false)}
/>

<style>
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

	.form-foot { display:flex; justify-content:space-between; align-items:center; margin-top:20px; padding-top:16px; border-top:1px solid var(--border); }
	.btn-primary { padding:7px 18px; height:34px; border-radius:var(--radius-sm); font-size:12.5px; font-weight:600; cursor:pointer; border:1px solid var(--accent); background:var(--accent); color:#000; transition:opacity .15s; font-family:var(--font); }
	.btn-primary:hover:not(:disabled) { opacity:.88; }
	.btn-primary:disabled { opacity:.5; cursor:not-allowed; }

	.btn-secondary { padding:7px 18px; height:34px; border-radius:var(--radius-sm); font-size:12.5px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s; font-family:var(--font); }
	.btn-secondary:hover { background:var(--surface-2); }

	.tpl-card { background:var(--surface); border:1px solid var(--border); border-radius:var(--radius); overflow:hidden; box-shadow:var(--shadow-sm); margin-top:16px; }
	.tpl-hdr { display:flex; align-items:center; justify-content:space-between; padding:10px 14px; border-bottom:1px solid var(--border); background:var(--surface-2); }
	.tpl-title { font-size:12px; font-weight:600; color:var(--text-2); }
	.copy-btn { padding:4px 11px; border-radius:var(--radius-sm); font-size:11px; font-weight:600; cursor:pointer; border:1px solid var(--border); background:var(--surface); color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); }
	.copy-btn:hover { background:var(--accent); border-color:var(--accent); color:#000; }
	.code { margin:0; padding:16px; font-size:11.5px; line-height:1.65; color:var(--text-2); font-family:var(--mono); white-space:pre-wrap; word-break:break-all; overflow-x:auto; }

	@media (max-width: 640px) {
		.row2 { grid-template-columns: 1fr; }
		.form-foot { flex-direction:column; gap:10px; width:100%; }
		.btn-primary, .btn-secondary { width:100%; text-align:center; }
	}
</style>
