<script lang="ts">
	import { onDestroy } from 'svelte';
	import { Terminal } from '@xterm/xterm';
	import { FitAddon } from '@xterm/addon-fit';
	import { X, Loader2, AlertCircle, Terminal as TermIcon } from '@lucide/svelte';

	interface Replica {
		id: string;
		slot: number | null;
		container_id: string | null;
		status: string;
		image: string;
	}

	interface Props {
		projectId: string;
		serviceId: string;
		serviceName: string;
		onClose: () => void;
	}

	let { projectId, serviceId, serviceName, onClose }: Props = $props();

	type PanelState = 'loading' | 'pick' | 'connecting' | 'connected' | 'error';

	let state      = $state<PanelState>('loading');
	let replicas   = $state<Replica[]>([]);
	let errorMsg   = $state('');
	let termEl     = $state<HTMLDivElement | null>(null);
	let term: Terminal | null = null;
	let fitAddon: FitAddon | null = null;
	let ws: WebSocket | null = null;
	let resizeObs: ResizeObserver | null = null;

	// Load replicas on mount
	$effect(() => {
		loadReplicas();
		return () => cleanup();
	});

	// Mount terminal once the element is available and we're in 'connecting' state
	$effect(() => {
		if (termEl && state === 'connecting') {
			mountTerminal(termEl);
		}
	});

	async function loadReplicas() {
		state = 'loading';
		try {
			const res = await fetch(`/api/projects/${projectId}/services/${serviceId}/replicas`);
			const json = await res.json();
			const tasks: Replica[] = (json.data ?? []).filter((t: Replica) => t.container_id);
			if (tasks.length === 0) {
				errorMsg = 'No running containers found for this service.';
				state = 'error';
				return;
			}
			if (tasks.length === 1) {
				startExec(tasks[0].container_id!);
			} else {
				replicas = tasks;
				state = 'pick';
			}
		} catch (e) {
			errorMsg = String(e);
			state = 'error';
		}
	}

	let _pendingContainerId = '';
	let _pendingToken = '';

	async function startExec(containerId: string) {
		state = 'connecting';
		// Fetch a short-lived exec token via the proxy (uses the session cookie).
		// This avoids needing the JS access token which may not be in the store
		// after a page refresh (auth is maintained via an httponly cookie).
		try {
			const res = await fetch(
				`/api/projects/${projectId}/services/${serviceId}/exec/token`,
				{ method: 'POST' }
			);
			const json = await res.json();
			if (!res.ok || !json.data?.token) {
				throw new Error(json.error?.message ?? 'Failed to get exec token');
			}
			_pendingContainerId = containerId;
			_pendingToken = json.data.token;
		} catch (e) {
			errorMsg = String(e);
			state = 'error';
		}
	}

	function mountTerminal(el: HTMLDivElement) {
		term = new Terminal({
			cursorBlink: true,
			fontSize: 13,
			fontFamily: 'Menlo, Monaco, "Courier New", monospace',
			theme: {
				background:  '#0d1117',
				foreground:  '#e6edf3',
				cursor:      '#58a6ff',
				black:       '#484f58',
				red:         '#ff7b72',
				green:       '#3fb950',
				yellow:      '#d29922',
				blue:        '#58a6ff',
				magenta:     '#bc8cff',
				cyan:        '#39d353',
				white:       '#b1bac4',
				brightBlack: '#6e7681',
				brightWhite: '#f0f6fc',
			},
		});

		fitAddon = new FitAddon();
		term.loadAddon(fitAddon);
		term.open(el);
		fitAddon.fit();

		const { cols, rows } = term;
		// Use the page's own host so the WebSocket goes through the SvelteKit
		// server (server.js) which tunnels /api/* upgrades to the backend —
		// same trust boundary as the HTTP proxy in hooks.server.ts.
		const wsProto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
		const wsUrl = `${wsProto}//${window.location.host}/api/projects/${projectId}/services/${serviceId}/exec`
			+ `?token=${encodeURIComponent(_pendingToken)}`
			+ `&container_id=${encodeURIComponent(_pendingContainerId)}`
			+ `&cmd=/bin/sh`
			+ `&cols=${cols}&rows=${rows}`;

		ws = new WebSocket(wsUrl);
		ws.binaryType = 'arraybuffer';

		ws.onopen = () => {
			state = 'connected';
			term!.focus();
		};

		ws.onmessage = (evt) => {
			if (evt.data instanceof ArrayBuffer) {
				term!.write(new Uint8Array(evt.data));
			} else {
				try {
					const msg = JSON.parse(evt.data as string);
					if (msg.type === 'error') {
						term!.writeln(`\r\n\x1b[31mError: ${msg.message}\x1b[0m`);
					}
				} catch {}
			}
		};

		ws.onerror = () => {
			errorMsg = 'WebSocket connection failed.';
			state = 'error';
		};

		ws.onclose = () => {
			if (state === 'connected') {
				term?.writeln('\r\n\x1b[33m[Session closed]\x1b[0m');
			}
		};

		// Send keystrokes as binary
		term.onData((data) => {
			if (ws?.readyState === WebSocket.OPEN) {
				const encoded = new TextEncoder().encode(data);
				ws.send(encoded.buffer);
			}
		});

		// Send resize events
		term.onResize(({ cols, rows }) => {
			if (ws?.readyState === WebSocket.OPEN) {
				ws.send(JSON.stringify({ type: 'resize', cols, rows }));
			}
		});

		// Fit on container resize
		resizeObs = new ResizeObserver(() => fitAddon?.fit());
		resizeObs.observe(el);
	}

	function cleanup() {
		resizeObs?.disconnect();
		ws?.close();
		term?.dispose();
		ws = null;
		term = null;
		fitAddon = null;
	}

	onDestroy(cleanup);
</script>

<div class="exec-backdrop" onclick={onClose} role="none"></div>

<div class="exec-panel" role="dialog" aria-label="Terminal — {serviceName}">
	<div class="exec-header">
		<div class="exec-title">
			<TermIcon size={14} />
			<span>Terminal — <strong>{serviceName}</strong></span>
		</div>
		<button class="close-btn" onclick={onClose} aria-label="Close terminal">
			<X size={15} />
		</button>
	</div>

	<div class="exec-body">
		{#if state === 'loading'}
			<div class="exec-center">
				<Loader2 size={20} class="spin" />
				<span>Finding containers…</span>
			</div>

		{:else if state === 'error'}
			<div class="exec-center error">
				<AlertCircle size={20} />
				<span>{errorMsg}</span>
				<button class="btn btn-secondary btn-sm" onclick={loadReplicas}>Retry</button>
			</div>

		{:else if state === 'pick'}
			<div class="pick-list">
				<p class="pick-hint">Multiple replicas found — select a container:</p>
				{#each replicas as r}
					<button class="pick-item" onclick={() => startExec(r.container_id!)}>
						<span class="pick-slot">Replica {r.slot ?? '?'}</span>
						<span class="pick-status" class:running={r.status === 'running'}>{r.status}</span>
						<code class="pick-id">{r.container_id!.slice(0, 12)}</code>
					</button>
				{/each}
			</div>

		{:else}
			<!-- connecting or connected — terminal renders here -->
			{#if state === 'connecting'}
				<div class="term-overlay">
					<Loader2 size={16} class="spin" />
					<span>Connecting…</span>
				</div>
			{/if}
			<div class="term-wrap" bind:this={termEl}></div>
		{/if}
	</div>
</div>

<style>
	@import '@xterm/xterm/css/xterm.css';

	:global(.spin) { animation: spin 0.8s linear infinite; }
	@keyframes spin { to { transform: rotate(360deg); } }

	.exec-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		z-index: 70;
	}

	.exec-panel {
		position: fixed;
		right: 0;
		top: 0;
		width: min(900px, 100vw);
		height: 100vh;
		background: #0d1117;
		border-left: 1px solid #30363d;
		display: flex;
		flex-direction: column;
		z-index: 71;
		box-shadow: -8px 0 32px rgba(0, 0, 0, 0.4);
	}

	.exec-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 16px;
		height: 44px;
		border-bottom: 1px solid #30363d;
		background: #161b22;
		flex-shrink: 0;
	}

	.exec-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: #8b949e;
	}
	.exec-title strong { color: #e6edf3; }

	.close-btn {
		background: none;
		border: none;
		color: #8b949e;
		cursor: pointer;
		padding: 4px;
		border-radius: 4px;
		display: flex;
		align-items: center;
		transition: color 0.15s;
	}
	.close-btn:hover { color: #e6edf3; }

	.exec-body {
		flex: 1;
		display: flex;
		flex-direction: column;
		overflow: hidden;
		position: relative;
	}

	.exec-center {
		display: flex;
		flex-direction: column;
		align-items: center;
		justify-content: center;
		gap: 12px;
		height: 100%;
		color: #8b949e;
		font-size: 13px;
	}
	.exec-center.error { color: #f85149; }

	.pick-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 24px;
	}
	.pick-hint {
		font-size: 13px;
		color: #8b949e;
		margin: 0 0 8px;
	}
	.pick-item {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 16px;
		background: #161b22;
		border: 1px solid #30363d;
		border-radius: 6px;
		cursor: pointer;
		text-align: left;
		transition: border-color 0.15s;
		font-family: inherit;
	}
	.pick-item:hover { border-color: #58a6ff; }
	.pick-slot { font-size: 13px; color: #e6edf3; font-weight: 600; flex: 1; }
	.pick-status { font-size: 11px; color: #8b949e; }
	.pick-status.running { color: #3fb950; }
	.pick-id { font-size: 11px; color: #8b949e; font-family: monospace; }

	.term-wrap {
		flex: 1;
		padding: 8px;
		overflow: hidden;
	}
	.term-wrap :global(.xterm) { height: 100%; }
	.term-wrap :global(.xterm-viewport) { border-radius: 0; }

	.term-overlay {
		position: absolute;
		inset: 0;
		display: flex;
		align-items: center;
		justify-content: center;
		gap: 8px;
		color: #8b949e;
		font-size: 13px;
		background: #0d1117;
		z-index: 1;
	}

	.btn-sm { font-size: 12px; padding: 5px 12px; }
</style>
