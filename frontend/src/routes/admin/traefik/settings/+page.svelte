<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import LogViewerOverlay from '$lib/components/LogViewerOverlay.svelte';
	import { ScrollText } from '@lucide/svelte';
	import { Card, FormField, TextField, Button, InlineAlert, Skeleton } from '$lib/components/ui';

	interface TraefikSettings {
		main_domain: string;
		traefik_network: string;
		traefik_entrypoint_http: string;
		traefik_entrypoint_https: string;
		traefik_cert_resolver: string;
	}

	let settings    = $state<TraefikSettings>({
		main_domain: '',
		traefik_network: '',
		traefik_entrypoint_http: '',
		traefik_entrypoint_https: '',
		traefik_cert_resolver: '',
	});
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
			main_domain:                res.data.main_domain ?? '',
			traefik_network:            res.data.traefik_network ?? '',
			traefik_entrypoint_http:    res.data.traefik_entrypoint_http ?? '',
			traefik_entrypoint_https:   res.data.traefik_entrypoint_https ?? '',
			traefik_cert_resolver:      res.data.traefik_cert_resolver ?? '',
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
	<Card padding="18px">
		<div class="sk-wrap">
			{#each Array(5) as _}<Skeleton variant="row" height="34px" />{/each}
		</div>
	</Card>
{:else}
	<Card padding="18px">
		<form onsubmit={save}>
			<div class="row2">
				<FormField label="Main Domain" for="domain">
					<TextField id="domain" bind:value={settings.main_domain} placeholder="example.com" />
				</FormField>
				<FormField label="Traefik Network" for="network">
					<TextField id="network" bind:value={settings.traefik_network} placeholder="platform_proxy" />
				</FormField>
			</div>

			<div class="row2">
				<FormField label="HTTP Entrypoint" for="http-ep">
					<TextField id="http-ep" bind:value={settings.traefik_entrypoint_http} placeholder="web" />
				</FormField>
				<FormField label="HTTPS Entrypoint" for="https-ep">
					<TextField id="https-ep" bind:value={settings.traefik_entrypoint_https} placeholder="websecure" />
				</FormField>
			</div>

			<FormField label="Cert Resolver" for="cert">
				<TextField id="cert" bind:value={settings.traefik_cert_resolver} placeholder="letsencrypt" />
			</FormField>

			{#if saveError}<InlineAlert tone="error">{saveError}</InlineAlert>{/if}

			<div class="form-foot">
				<Button variant="secondary" size="sm" onclick={() => (showLogs = true)}>
					<ScrollText size={13} />
					Show Log Stream
				</Button>
				<Button type="submit" disabled={saving}>
					{#if saved}Saved{:else if saving}Saving…{:else}Save Changes{/if}
				</Button>
			</div>
		</form>
	</Card>

	<div class="tpl-section">
		<div class="tpl-hdr">
			<span class="mono tpl-title">Generated traefik.yml</span>
			<Button variant="secondary" size="sm" onclick={() => copyCode(traefikYaml)}>
				{copied ? 'Copied!' : 'Copy'}
			</Button>
		</div>
		<Card padding="0">
			<pre class="code">{traefikYaml}</pre>
		</Card>
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
	.sk-wrap { display:flex; flex-direction:column; gap:14px; }

	.row2 { display:grid; grid-template-columns:1fr 1fr; gap:12px; margin-bottom:14px; }

	.form-foot { display:flex; justify-content:space-between; align-items:center; margin-top:18px; padding-top:16px; border-top:1px solid var(--border); }

	.tpl-section { margin-top:16px; }
	.tpl-hdr { display:flex; align-items:center; justify-content:space-between; margin-bottom:10px; }
	.tpl-title { font-size:12px; color:var(--text-secondary); }
	.code { margin:0; padding:16px; font-size:11.5px; line-height:1.65; color:var(--text-secondary); font-family:var(--font-mono); white-space:pre-wrap; word-break:break-all; overflow-x:auto; }

	.mono { font-family:var(--font-mono); }

	@media (max-width: 640px) {
		.row2 { grid-template-columns: 1fr; }
		.form-foot { flex-direction:column; gap:10px; align-items:stretch; }
	}
</style>
