<script lang="ts">
	import { onMount } from 'svelte';
	import { uiStore } from '$lib/stores/ui.store';
	import { api } from '$lib/api/client';
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
		<div class="context-chip">
			<Zap size={11} />
			<span>Routing to edge runtime · port 8000</span>
		</div>

		<!-- Hostname -->
		<div class="form-group">
			<label class="form-label" for="efda-hostname">Hostname</label>
			<div class="hostname-row">
				<div class="input-icon-wrap">
					<Globe size={13} class="input-icon" />
					<input
						id="efda-hostname"
						class="form-input with-icon font-mono"
						type="text"
						placeholder="api.example.com"
						bind:value={hostname}
						required
					/>
				</div>
				<button type="button" class="dice-btn" onclick={roll} title="Generate random subdomain">
					<Dice5 size={15} />
				</button>
			</div>
			<span class="form-hint">
				Use any domain you own, or click <Dice5 size={10} class="hint-icon" /> to generate a
				{#if serverIpPublic}
					<code class="mono">*.{serverIp}.nip.io</code> domain (→ server IP <code class="mono">{serverIp}</code>).
				{:else}
					<code class="mono">*.traefik.me</code> domain (→ <code class="mono">127.0.0.1</code>).
				{/if}
			</span>
		</div>

		<!-- SSL -->
		<div class="form-group">
			<label class="form-label">SSL / HTTPS</label>
			<button
				type="button"
				class="tls-toggle"
				class:tls-on={tlsEnabled}
				onclick={() => tlsEnabled = !tlsEnabled}
			>
				<div class="tls-track"><div class="tls-thumb"></div></div>
				{#if tlsEnabled}
					<Shield size={13} /><span>Enabled — HTTPS</span>
				{:else}
					<ShieldOff size={13} /><span>Disabled — HTTP only</span>
				{/if}
			</button>
		</div>

		<!-- Certificate provider -->
		{#if tlsEnabled}
			<div class="form-group">
				<label class="form-label" for="efda-cert">Certificate Provider</label>
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
					<input
						class="form-input font-mono"
						style="margin-top:8px"
						type="text"
						placeholder="my-resolver"
						bind:value={customCert}
					/>
					<span class="form-hint">Must match a <code class="mono">certificatesResolvers</code> key in your Traefik config.</span>
				{/if}
			</div>
		{/if}

		<!-- Port (optional override) -->
		<div class="form-group">
			<label class="form-label" for="efda-port">Container Port (optional)</label>
			<div class="input-icon-wrap">
				{#if tlsEnabled}<Lock size={13} class="input-icon" />{:else}<Unlock size={13} class="input-icon" />{/if}
				<input
					id="efda-port"
					class="form-input with-icon font-mono"
					type="number" min="1" max="65535"
					placeholder="8000"
					bind:value={portStr}
				/>
			</div>
			<span class="form-hint">
				Defaults to the edge runtime port (8000). Override only if you've mapped a different port.
			</span>
		</div>

		<!-- Route preview -->
		{#if hostname.trim()}
			<div class="preview-card">
				<span class="preview-label">Route preview</span>
				<code class="preview-route">
					{tlsEnabled ? 'https' : 'http'}://{hostname.trim()}{String(portStr ?? '').trim() ? ` → :${String(portStr ?? '').trim()}` : ' → :8000'}
				</code>
				{#if tlsEnabled}
					<span class="preview-cert-badge">{resolvedCertProvider}</span>
				{/if}
			</div>
		{/if}

		{#if error}
			<div class="error-msg">{error}</div>
		{/if}

		<button class="btn btn-primary submit-btn" type="submit" disabled={isSubmitting || !hostname.trim()}>
			{#if isSubmitting}<div class="btn-spinner"></div> Adding…
			{:else}<Globe size={13} /> Add Domain{/if}
		</button>
	</form>
</div>

<style>
	.panel-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.form { display: flex; flex-direction: column; gap: 16px; }
	.form-group { display: flex; flex-direction: column; gap: 6px; }

	.context-chip {
		display: inline-flex; align-items: center; gap: 6px;
		padding: 5px 10px; border-radius: 100px;
		background: color-mix(in srgb, var(--accent) 8%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
		color: var(--accent); font-size: 11px; font-weight: 500;
		align-self: flex-start;
	}

	.form-label {
		font-size: 11px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em;
	}
	.form-hint {
		font-size: 11px; color: var(--text-dim); line-height: 1.5;
		display: flex; align-items: center; gap: 3px; flex-wrap: wrap;
	}
	:global(.hint-icon) { color: var(--text-dim); }
	.mono {
		font-family: var(--font-mono); font-size: 10px;
		background: var(--bg-base); padding: 1px 4px; border-radius: 3px;
	}

	.hostname-row { display: flex; gap: 6px; align-items: stretch; }
	.hostname-row .input-icon-wrap { flex: 1; }

	.input-icon-wrap {
		position: relative; display: flex; align-items: center;
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); transition: border-color var(--transition-fast);
	}
	.input-icon-wrap:focus-within { border-color: var(--accent); }

	:global(.input-icon) {
		position: absolute; left: 10px; color: var(--text-dim); pointer-events: none; flex-shrink: 0;
	}

	.form-input {
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); color: var(--text-primary);
		font-size: 13px; font-family: var(--font-sans); padding: 8px 10px;
		outline: none; transition: border-color var(--transition-fast); width: 100%;
	}
	.form-input:focus { border-color: var(--accent); }
	.form-input.with-icon { background: transparent; border: none; padding-left: 32px; flex: 1; }
	.form-input.with-icon:focus { outline: none; }
	.font-mono { font-family: var(--font-mono); }

	.dice-btn {
		display: flex; align-items: center; justify-content: center; width: 36px; flex-shrink: 0;
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); cursor: pointer; color: var(--text-muted);
		transition: all var(--transition-fast);
	}
	.dice-btn:hover { border-color: var(--accent); color: var(--accent); }

	.tls-toggle {
		display: flex; align-items: center; gap: 8px;
		background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); padding: 9px 12px;
		cursor: pointer; font-size: 13px; font-family: var(--font-sans);
		color: var(--text-muted); transition: all var(--transition-fast); width: fit-content;
	}
	.tls-toggle:hover { border-color: var(--border-hover); }
	.tls-toggle.tls-on { border-color: rgba(16,185,129,0.4); color: #10B981; background: rgba(16,185,129,0.06); }

	.tls-track {
		width: 28px; height: 16px; border-radius: 99px;
		background: var(--text-dim); position: relative; transition: background var(--transition-fast); flex-shrink: 0;
	}
	.tls-on .tls-track { background: #10B981; }
	.tls-thumb {
		width: 12px; height: 12px; border-radius: 50%; background: white;
		position: absolute; top: 2px; left: 2px; transition: transform var(--transition-fast);
	}
	.tls-on .tls-thumb { transform: translateX(12px); }

	.cert-options { display: flex; flex-direction: column; gap: 6px; }
	.cert-option {
		display: flex; flex-direction: column; gap: 2px; text-align: left;
		padding: 9px 12px; background: var(--bg-elevated); border: 1px solid var(--border);
		border-radius: var(--radius-sm); cursor: pointer; font-family: var(--font-sans);
		transition: all var(--transition-fast);
	}
	.cert-option:hover { border-color: var(--accent); }
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
		font-size: 10px; font-weight: 600; color: var(--text-dim);
		text-transform: uppercase; letter-spacing: 0.06em; flex-shrink: 0;
	}
	.preview-route { font-family: var(--font-mono); font-size: 12px; color: var(--accent); word-break: break-all; flex: 1; }
	.preview-cert-badge {
		font-size: 10px; font-weight: 600; padding: 1px 7px; border-radius: 99px;
		background: rgba(16,185,129,0.1); color: #10B981; border: 1px solid rgba(16,185,129,0.25); flex-shrink: 0;
	}

	.error-msg {
		font-size: 12px; color: var(--accent-red); padding: 8px 10px;
		background: color-mix(in srgb, var(--accent-red) 10%, transparent);
		border: 1px solid color-mix(in srgb, var(--accent-red) 30%, transparent);
		border-radius: var(--radius-sm);
	}

	.submit-btn { display: flex; align-items: center; gap: 6px; justify-content: center; margin-top: 4px; }

	.btn-spinner {
		width: 12px; height: 12px; border: 2px solid rgba(255,255,255,0.3);
		border-top-color: white; border-radius: 50%; animation: spin 0.7s linear infinite;
	}

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
