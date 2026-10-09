<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
	import { Button, FormField, TextField, Toggle, Badge, InlineAlert, Spinner } from '$lib/components/ui';
	import { Globe, Shield, ShieldOff, Dice5, Lock, Unlock, Zap } from '@lucide/svelte';

	interface EFnDomain {
		id: string; service_id: string; hostname: string;
		tls_enabled: boolean; cert_provider: string; port: number | null;
		traefik_router_name: string; created_at: string;
	}

	interface Props {
		orgId:     string;
		groupId:   string;
		onCreated: (domain: EFnDomain) => void;
	}

	let { orgId, groupId, onCreated }: Props = $props();

	const ADJECTIVES = [
		'brave','calm','dark','eager','fancy','gentle','happy','icy','jolly','keen',
		'lively','mighty','nimble','orange','proud','quiet','rapid','silky','tidy',
		'urban','vivid','wild','xenial','yellow','zesty','amber','bright','crisp',
	];
	const NOUNS = [
		'panda','tiger','wolf','eagle','hawk','bear','fox','deer','owl','lion',
		'whale','shark','raven','cobra','crane','gecko','lynx','moose','newt',
		'otter','quail','robin','snail','trout','viper','wasp','yak','zebra',
	];

	let serverIp       = $state('127.0.0.1');
	let serverIpPublic = $state(false);

	onMount(async () => {
		const res = await api.get<{ ip: string; is_public: boolean }>('/admin/host-ip');
		if (res.data) { serverIp = res.data.ip; serverIpPublic = res.data.is_public; }
	});

	function randomName(): string {
		const adj  = ADJECTIVES[Math.floor(Math.random() * ADJECTIVES.length)];
		const noun = NOUNS[Math.floor(Math.random() * NOUNS.length)];
		if (serverIpPublic) return `${adj}-${noun}.${serverIp}.nip.io`;
		return `${adj}-${noun}.traefik.me`;
	}

	let hostname     = $state('');
	let tlsEnabled   = $state(true);
	let certProvider = $state('letsencrypt');
	let customCert   = $state('');
	let portStr      = $state('');

	let isSubmitting = $state(false);
	let error        = $state('');

	let resolvedCertProvider = $derived(
		certProvider === 'custom' ? (customCert.trim() || 'letsencrypt') : certProvider
	);

	function roll() { hostname = randomName(); }

	async function handleSubmit(e: SubmitEvent) {
		e.preventDefault();
		if (!hostname.trim()) { error = 'Hostname is required.'; return; }
		error = '';
		isSubmitting = true;
		try {
			const portRaw  = String(portStr ?? '').trim();
			const parsedPort = portRaw ? parseInt(portRaw, 10) : null;
			const res = await api.post<EFnDomain>(
				`/orgs/${orgId}/edge-functions/groups/${groupId}/domains`,
				{
					hostname:      hostname.trim(),
					tls_enabled:   tlsEnabled,
					cert_provider: resolvedCertProvider,
					port:          parsedPort && !isNaN(parsedPort) ? parsedPort : null,
				}
			);
			if (res.error) { error = res.error.message; return; }
			if (res.data) { onCreated(res.data); uiStore.popPanel(); }
		} finally {
			isSubmitting = false;
		}
	}
</script>

<div class="panel-wrap">
	<form class="form" onsubmit={handleSubmit}>

		<!-- context chip -->
		<div class="chip-row">
			<Badge tone="blue"><Zap size={11} />&nbsp;Routing to edge runtime · port 8000</Badge>
		</div>

		<!-- Hostname -->
		<FormField label="Hostname" for="efda-hostname">
			<div class="hostname-row">
				<div class="hostname-input mono-field">
					<TextField id="efda-hostname" type="text" placeholder="api.example.com" bind:value={hostname} required>
						{#snippet icon()}<Globe size={13} />{/snippet}
					</TextField>
				</div>
				<Button variant="secondary" size="icon" title="Generate random subdomain" aria-label="Generate random subdomain" onclick={roll}>
					<Dice5 size={15} />
				</Button>
			</div>
			<span class="form-hint">
				Use any domain you own, or click <Dice5 size={10} class="hint-icon" /> to generate a
				{#if serverIpPublic}
					<code class="mono">*.{serverIp}.nip.io</code> domain (→ server IP <code class="mono">{serverIp}</code>).
				{:else}
					<code class="mono">*.traefik.me</code> domain (→ <code class="mono">127.0.0.1</code>).
				{/if}
			</span>
		</FormField>

		<!-- SSL -->
		<FormField label="SSL / HTTPS">
			<div class="tls-row" class:tls-on={tlsEnabled}>
				<Toggle bind:checked={tlsEnabled} label="SSL / HTTPS" />
				<span class="tls-state">
					{#if tlsEnabled}
						<Shield size={13} />
						<span>Enabled — HTTPS</span>
					{:else}
						<ShieldOff size={13} />
						<span>Disabled — HTTP only</span>
					{/if}
				</span>
			</div>
		</FormField>

		<!-- Certificate provider (only when TLS on) -->
		{#if tlsEnabled}
			<FormField label="Certificate Provider">
				<div class="cert-options">
					{#each [
						{ value: 'letsencrypt', label: "Let's Encrypt", hint: 'Free ACME certificates (recommended)' },
						{ value: 'selfsigned',  label: 'Self-Signed',   hint: 'Auto-generated — browser will warn' },
						{ value: 'custom',      label: 'Custom resolver', hint: 'Named Traefik cert resolver' },
					] as opt (opt.value)}
						<button
							type="button"
							class="cert-option"
							class:active={certProvider === opt.value}
							onclick={() => certProvider = opt.value}
						>
							<span class="cert-opt-label">{opt.label}</span>
							<span class="cert-opt-hint">{opt.hint}</span>
						</button>
					{/each}
				</div>
				{#if certProvider === 'custom'}
					<div class="mono-field">
						<TextField type="text" aria-label="Custom certificate resolver" placeholder="my-resolver" bind:value={customCert} />
					</div>
					<span class="form-hint">Must match a <code class="mono">certificatesResolvers</code> key in your Traefik config.</span>
				{/if}
			</FormField>
		{/if}

		<!-- Port -->
		<FormField label="Container Port (optional)" for="efda-port">
			<div class="mono-field">
				<TextField id="efda-port" type="number" min="1" max="65535" placeholder="8000" bind:value={portStr}>
					{#snippet icon()}{#if tlsEnabled}<Lock size={13} />{:else}<Unlock size={13} />{/if}{/snippet}
				</TextField>
			</div>
			<span class="form-hint">
				Defaults to the edge runtime port (8000). Override only if you've mapped a different port.
			</span>
		</FormField>

		<!-- Route preview -->
		{#if hostname.trim()}
			<div class="preview-card">
				<span class="preview-label">Route preview</span>
				<code class="preview-route">
					{tlsEnabled ? 'https' : 'http'}://{hostname.trim()}{String(portStr ?? '').trim() ? ` → :${String(portStr ?? '').trim()}` : ' → :8000'}
				</code>
				{#if tlsEnabled}
					<Badge tone="green">{resolvedCertProvider}</Badge>
				{/if}
			</div>
		{/if}

		{#if error}
			<div role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
		{/if}

		<Button type="submit" disabled={isSubmitting || !hostname.trim()}>
			{#if isSubmitting}<Spinner size={12} tone="current" /> Adding…
			{:else}<Globe size={13} /> Add Domain{/if}
		</Button>
	</form>
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 16px; }

	.mono-field :global(input) { font-family: var(--font-mono); }
	.form-hint {
		font-size: 11px; color: var(--text-dim); line-height: 1.5;
		display: flex; align-items: center; gap: 3px; flex-wrap: wrap;
	}
	:global(.hint-icon) { color: var(--text-dim); }
	.mono {
		font-family: var(--font-mono); font-size: 10px;
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
	}
	.chip-row { align-self: flex-start; }

	.hostname-row { display: flex; gap: 6px; align-items: center; }
	.hostname-input { flex: 1; min-width: 0; }

	.tls-row { display: flex; align-items: center; gap: 10px; font-size: 13px; color: var(--text-muted); }
	.tls-row.tls-on { color: var(--accent-green); }
	.tls-state { display: inline-flex; align-items: center; gap: 6px; }

	.cert-options { display: flex; flex-direction: column; gap: 6px; }
	.cert-option {
		display: flex; flex-direction: column; gap: 2px; text-align: left;
		padding: 9px 12px; background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); cursor: pointer; font-family: var(--font-sans);
		transition: all var(--transition-fast);
	}
	.cert-option:hover { border-color: var(--accent); }
	.cert-option:focus-visible { outline: 2px solid var(--accent); outline-offset: 1px; }
	.cert-option.active {
		border-color: var(--accent);
		background: color-mix(in srgb, var(--accent) 6%, transparent);
	}
	.cert-opt-label { font-size: 13px; font-weight: 600; color: var(--text-primary); }
	.cert-opt-hint  { font-size: 11px; color: var(--text-dim); }

	.preview-card {
		display: flex; align-items: center; gap: 8px; flex-wrap: wrap;
		padding: 10px 12px;
		background: color-mix(in srgb, var(--accent) 5%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
		border-radius: var(--radius-sm);
	}
	.preview-label {
		font-size: 10px; font-weight: 600; color: var(--text-dim); flex-shrink: 0;
	}
	.preview-route {
		font-family: var(--font-mono); font-size: 12px; color: var(--accent);
		word-break: break-all; flex: 1;
	}
</style>
