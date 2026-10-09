<script lang="ts">
	import { onDestroy } from 'svelte';
	import { Terminal } from '@xterm/xterm';
	import { FitAddon } from '@xterm/addon-fit';
	import '@xterm/xterm/css/xterm.css';
	import { api } from '$lib/api/client';
	import StatusDot from '$lib/components/ui/StatusDot.svelte';

	interface Props {
		serviceId: string;
		/** Optional — when provided, shows a small status pill next to the
		 * Logs/Shell tabs while the dev server is still starting, so the
		 * loading state is visible without having to read the log text. */
		sandboxStatus?: 'stopped' | 'starting' | 'running';
	}

	let { serviceId, sandboxStatus }: Props = $props();

	// 'logs' (default): a read-only tail of the dev server's own stdout/stderr
	// — what most people opening the terminal actually want to see. 'shell'
	// is the full interactive PTY, needed for Custom mode's scaffold-by-hand
	// workflow or any other manual command.
	let mode = $state<'logs' | 'shell'>('logs');

	let term: Terminal | null = null;
	let fitAddon: FitAddon | null = null;
	let ws: WebSocket | null = null;
	let resizeObs: ResizeObserver | null = null;
	let pingTimer: ReturnType<typeof setInterval> | null = null;
	let connState = $state<'connecting' | 'connected' | 'error'>('connecting');
	let errorMsg = $state<string>('');

	function mountTerminal(el: HTMLDivElement) {
		term = new Terminal({
			cursorBlink: true,
			fontSize: 13,
			fontFamily: 'Menlo, Monaco, "Courier New", monospace',
			theme: {
				background: '#141619',
				foreground: '#C3C6CA',
				cursor: '#F26B1D',
				black: '#3A3E44',
				red: '#F87171',
				green: '#4ADE80',
				yellow: '#FBBF24',
				blue: '#60A5FA',
				magenta: '#C4B5FD',
				cyan: '#5EEAD4',
				white: '#C3C6CA',
				brightBlack: '#8B9097',
				brightWhite: '#ECEDEE'
			}
		});

		fitAddon = new FitAddon();
		term.loadAddon(fitAddon);
		term.open(el);

		term.onData((data) => {
			if (mode === 'shell' && ws?.readyState === WebSocket.OPEN) {
				const encoded = new TextEncoder().encode(data);
				ws.send(encoded.buffer);
			}
		});
		term.onResize(({ cols, rows }) => {
			if (mode === 'shell' && ws?.readyState === WebSocket.OPEN) {
				ws.send(JSON.stringify({ type: 'resize', cols, rows }));
			}
		});

		requestAnimationFrame(() => {
			if (!fitAddon) return;
			fitAddon.fit();
			resizeObs = new ResizeObserver(() => fitAddon?.fit());
			resizeObs.observe(el);
			connect();
		});
	}

	function teardownSocket() {
		if (pingTimer) clearInterval(pingTimer);
		pingTimer = null;
		ws?.close();
		ws = null;
	}

	function switchMode(next: 'logs' | 'shell') {
		if (mode === next) return;
		teardownSocket();
		mode = next;
		term?.reset();
		connState = 'connecting';
		connect();
	}

	async function connect() {
		if (mode === 'shell') {
			await connectShell();
		} else {
			await connectLogs();
		}
	}

	async function connectShell() {
		const tokenRes = await api.mintSandboxExecToken(serviceId);
		if (!tokenRes.data) {
			errorMsg = tokenRes.error?.message ?? 'Failed to get exec token';
			connState = 'error';
			return;
		}
		const token = tokenRes.data.token;
		if (!term) return;
		const { cols, rows } = term;
		const wsProto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
		const wsUrl =
			`${wsProto}//${window.location.host}/api/apps/${serviceId}/exec` +
			`?token=${encodeURIComponent(token)}` +
			`&cmd=/bin/sh` +
			`&cols=${cols}&rows=${rows}`;

		ws = new WebSocket(wsUrl);
		ws.binaryType = 'arraybuffer';

		ws.onopen = () => {
			connState = 'connected';
			term!.focus();
			startPing();
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
			connState = 'error';
		};
		ws.onclose = () => {
			if (connState === 'connected') term?.writeln('\r\n\x1b[33m[Session closed]\x1b[0m');
		};
	}

	async function connectLogs() {
		const tokenRes = await api.mintSandboxExecToken(serviceId);
		if (!tokenRes.data) {
			errorMsg = tokenRes.error?.message ?? 'Failed to get exec token';
			connState = 'error';
			return;
		}
		const token = tokenRes.data.token;
		const wsProto = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
		const wsUrl =
			`${wsProto}//${window.location.host}/api/apps/${serviceId}/logs` +
			`?token=${encodeURIComponent(token)}` +
			`&tail=200`;

		ws = new WebSocket(wsUrl);

		ws.onopen = () => {
			connState = 'connected';
			startPing();
		};
		ws.onmessage = (evt) => {
			if (typeof evt.data !== 'string') return;
			try {
				const msg = JSON.parse(evt.data);
				if (msg.type === 'error') {
					term!.writeln(`\r\n\x1b[31mError: ${msg.message}\x1b[0m`);
					return;
				}
			} catch {
				// Not JSON — a plain log line. xterm needs \r\n for a real line
				// break; the log line itself only ever carries a bare \n.
				term!.write(evt.data.replace(/\n/g, '\r\n') + '\r\n');
			}
		};
		ws.onerror = () => {
			errorMsg = 'WebSocket connection failed.';
			connState = 'error';
		};
		ws.onclose = () => {
			if (connState === 'connected') term?.writeln('\r\n\x1b[33m[Log stream closed]\x1b[0m');
		};
	}

	function startPing() {
		// A quiet shell or a dev server that's momentarily silent produces zero
		// traffic — long enough with none and a reverse proxy (or any
		// idle-timeout layer between here and the container) closes the
		// connection even though the session is still alive. Both endpoints
		// recognize and drop this control message rather than acting on it.
		pingTimer = setInterval(() => {
			if (ws?.readyState === WebSocket.OPEN) {
				ws.send(JSON.stringify({ type: 'ping' }));
			}
		}, 30_000);
	}

	function cleanup() {
		teardownSocket();
		resizeObs?.disconnect();
		term?.dispose();
		term = null;
		fitAddon = null;
	}

	onDestroy(cleanup);
</script>

<div class="sandbox-terminal">
	<div class="terminal-mode-tabs">
		<button class="mode-tab" class:active={mode === 'logs'} onclick={() => switchMode('logs')}>
			Logs
		</button>
		<button class="mode-tab" class:active={mode === 'shell'} onclick={() => switchMode('shell')}>
			Shell
		</button>
		{#if sandboxStatus === 'starting'}
			<span class="status-pill">
				<StatusDot status="pending" />
				Starting…
			</span>
		{/if}
	</div>
	{#if connState === 'error'}
		<p class="error">{errorMsg}</p>
	{/if}
	<div class="term-container" use:mountTerminal></div>
</div>

<style>
	.sandbox-terminal {
		display: flex;
		flex-direction: column;
		height: 100%;
		background: var(--terminal-bg);
	}
	.terminal-mode-tabs {
		display: flex;
		gap: 2px;
		padding: 4px 4px 0;
		flex-shrink: 0;
	}
	.mode-tab {
		padding: 4px 12px;
		font-size: 11px;
		font-weight: 600;
		background: transparent;
		border: none;
		border-radius: 4px 4px 0 0;
		color: var(--terminal-dim);
		cursor: pointer;
	}
	.mode-tab:hover {
		color: var(--terminal-fg);
		background: rgba(255, 255, 255, 0.05);
	}
	.mode-tab.active {
		color: var(--terminal-blue);
		background: color-mix(in srgb, var(--terminal-blue) 10%, transparent);
	}
	.status-pill {
		display: flex;
		align-items: center;
		gap: 5px;
		margin-left: auto;
		margin-right: 8px;
		padding: 3px 8px;
		font-size: 11px;
		font-weight: 500;
		color: var(--terminal-yellow);
		background: color-mix(in srgb, var(--terminal-yellow) 12%, transparent);
		border-radius: 999px;
	}
	.term-container {
		flex: 1;
		min-height: 0;
		padding: 4px;
	}
	.error {
		color: var(--terminal-red);
		font-size: 12px;
		padding: 8px;
	}
</style>
