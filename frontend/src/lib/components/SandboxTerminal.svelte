<script lang="ts">
	import { onDestroy } from 'svelte';
	import { Terminal } from '@xterm/xterm';
	import { FitAddon } from '@xterm/addon-fit';
	import '@xterm/xterm/css/xterm.css';
	import { api } from '$lib/api/client';

	interface Props {
		serviceId: string;
	}

	let { serviceId }: Props = $props();

	let term: Terminal | null = null;
	let fitAddon: FitAddon | null = null;
	let ws: WebSocket | null = null;
	let resizeObs: ResizeObserver | null = null;
	let connState = $state<'connecting' | 'connected' | 'error'>('connecting');
	let errorMsg = $state<string>('');

	function mountTerminal(el: HTMLDivElement) {
		// Start the async initialization without awaiting
		initTerminal(el);
	}

	async function initTerminal(el: HTMLDivElement) {
		const tokenRes = await api.mintSandboxExecToken(serviceId);
		if (!tokenRes.data) {
			errorMsg = tokenRes.error?.message ?? 'Failed to get exec token';
			connState = 'error';
			return;
		}
		const token = tokenRes.data.token;

		term = new Terminal({
			cursorBlink: true,
			fontSize: 13,
			fontFamily: 'Menlo, Monaco, "Courier New", monospace',
			theme: {
				background: '#0d1117',
				foreground: '#e6edf3',
				cursor: '#58a6ff',
				black: '#484f58',
				red: '#ff7b72',
				green: '#3fb950',
				yellow: '#d29922',
				blue: '#58a6ff',
				magenta: '#bc8cff',
				cyan: '#39d353',
				white: '#b1bac4',
				brightBlack: '#6e7681',
				brightWhite: '#f0f6fc'
			}
		});

		fitAddon = new FitAddon();
		term.loadAddon(fitAddon);
		term.open(el);

		requestAnimationFrame(() => {
			if (!term || !fitAddon) return;
			fitAddon.fit();
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

			term.onData((data) => {
				if (ws?.readyState === WebSocket.OPEN) {
					const encoded = new TextEncoder().encode(data);
					ws.send(encoded.buffer);
				}
			});
			term.onResize(({ cols, rows }) => {
				if (ws?.readyState === WebSocket.OPEN) {
					ws.send(JSON.stringify({ type: 'resize', cols, rows }));
				}
			});

			resizeObs = new ResizeObserver(() => fitAddon?.fit());
			resizeObs.observe(el);
		});
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

<div class="sandbox-terminal">
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
		background: #0d1117;
	}
	.term-container {
		flex: 1;
		min-height: 0;
		padding: 4px;
	}
	.error {
		color: #ff7b72;
		font-size: 12px;
		padding: 8px;
	}
</style>
