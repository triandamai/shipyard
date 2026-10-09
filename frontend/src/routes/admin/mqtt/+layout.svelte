<script lang="ts">
	import { page } from '$app/state';
	import { onDestroy } from 'svelte';
	import { RefreshCw } from '@lucide/svelte';

	let { children } = $props();

	const tabs = [
		{ label: 'Clients', href: '/admin/mqtt/clients' },
		{ label: 'Subscriptions', href: '/admin/mqtt/subscriptions' },
		{ label: 'Topics', href: '/admin/mqtt/topics' }
	];

	let logLines = $state<string[]>([]);
	let logConnected = $state(false);
	let logEs: EventSource | null = null;
	let logEl = $state<HTMLDivElement | null>(null);

	function openMqttLogs() {
		closeMqttLogs();
		logLines = [];
		logConnected = false;
		const es = new EventSource('/api/admin/mqtt/logs/stream');
		es.onopen = () => { logConnected = true; };
		es.onmessage = (e) => {
			logLines = [...logLines.slice(-499), e.data];
			requestAnimationFrame(() => { if (logEl) logEl.scrollTop = logEl.scrollHeight; });
		};
		es.onerror = () => { logConnected = false; };
		logEs = es;
	}

	function closeMqttLogs() {
		logEs?.close(); logEs = null; logConnected = false;
	}

	function refreshChild() {
		window.location.reload();
	}

	onDestroy(closeMqttLogs);
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">MQTT Broker</h1>
			<p class="sub">Platform-wide broker monitoring — clients, subscriptions, topics.</p>
		</div>
		<button class="refresh-btn" onclick={refreshChild}>
			<RefreshCw size={13} />
			Refresh
		</button>
	</header>

	<div class="tabs">
		{#each tabs as t}
			<a class="tab" class:active={page.url.pathname === t.href} href={t.href}>
				{t.label}
			</a>
		{/each}
	</div>

	{@render children()}

	<div class="log-section">
		<div class="log-hdr">
			<span class="log-title">Broker Logs</span>
			{#if !logEs}
				<button class="log-connect-btn" onclick={openMqttLogs}>Connect</button>
			{:else}
				<div style="display:flex;gap:6px;align-items:center">
					<span class="conn-dot" class:conn-ok={logConnected}></span>
					<span style="font-size:11.5px;color:var(--text-muted)">{logConnected ? 'Live' : 'Connecting…'}</span>
					<button class="copy-btn" onclick={openMqttLogs}>Reconnect</button>
					<button class="copy-btn" onclick={() => { logLines = []; }}>Clear</button>
					<button class="copy-btn" onclick={closeMqttLogs}>Disconnect</button>
				</div>
			{/if}
		</div>
		{#if logEs}
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
</div>

<style>
	.p { max-width:1000px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:20px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text-primary); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-muted); margin:0; }
	.refresh-btn { display:flex; align-items:center; gap:6px; padding:6px 12px; height:32px; border-radius:var(--radius-md); font-size:12px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--bg-surface); color:var(--text-secondary); transition:background .15s; font-family:var(--font-sans); }
	.refresh-btn:hover { background:var(--bg-elevated); }

	.tabs { display:flex; gap:2px; margin-bottom:14px; background:var(--bg-elevated); border:1px solid var(--border); border-radius:var(--radius-md); padding:3px; width:fit-content; }
	.tab { display:flex; align-items:center; gap:6px; padding:5px 14px; border-radius:5px; font-size:12.5px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-secondary); transition:background .15s, color .15s; font-family:var(--font-sans); text-decoration:none; }
	.tab.active { background:var(--bg-surface); color:var(--text-primary); box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.tab:hover:not(.active) { color:var(--text-primary); }

	.log-section { margin-top:28px; }
	.log-hdr { display:flex; align-items:center; justify-content:space-between; gap:12px; margin-bottom:10px; }
	.log-title { font-size:13px; font-weight:700; color:var(--text-primary); letter-spacing:-0.01em; }
	.log-connect-btn { padding:5px 14px; height:28px; border-radius:var(--radius-md); font-size:12px; font-weight:600; cursor:pointer; border:none; background:var(--accent); color: var(--accent-fg); transition:opacity .15s; font-family:var(--font-sans); }
	.log-connect-btn:hover { opacity:.88; }
	.copy-btn { padding:3px 10px; height:24px; border-radius:var(--radius-md); font-size:11.5px; font-weight:500; cursor:pointer; border:1px solid var(--border); background:var(--bg-surface); color:var(--text-secondary); font-family:var(--font-sans); transition:background .15s; }
	.copy-btn:hover { background:var(--bg-elevated); }
	.conn-dot { width:7px; height:7px; border-radius:50%; background:var(--text-muted); transition:background .3s; }
	.conn-dot.conn-ok { background:var(--accent-green); }
	.log-shell { background:var(--terminal-bg); border:1px solid rgba(255,255,255,.08); border-radius:var(--radius-lg); overflow:hidden; }
	.log-body { height:320px; overflow-y:auto; padding:12px 14px; font-family:var(--font-mono); font-size:11.5px; line-height:1.6; }
	.log-line { color:var(--terminal-fg); white-space:pre-wrap; word-break:break-all; }
	.log-empty { color:var(--terminal-dim); font-style:italic; }

	@media (max-width: 640px) {
		.p { padding:20px 12px; }
		.tabs { width:100%; overflow-x:auto; white-space:nowrap; }
	}
</style>
