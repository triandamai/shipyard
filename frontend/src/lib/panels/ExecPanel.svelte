<script lang="ts">
	import { onDestroy } from 'svelte';
	import { Terminal } from '@xterm/xterm';
	import { FitAddon } from '@xterm/addon-fit';
	import { X, AlertCircle, Terminal as TermIcon } from '@lucide/svelte';
	import { Button, Badge, Spinner, ConfirmDialog } from '$lib/components/ui';

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

	// Mount terminal when termEl becomes available and _mountPending is set.
	// _mountPending is intentionally non-reactive so that subsequent state changes
	// (connecting → connected) do NOT re-run this effect and trigger cleanup(),
	// which was previously disposing the terminal the moment the WS connected.
	$effect(() => {
		const el = termEl; // reactive dependency: fires when the div is bound
		if (!el || !_mountPending) return;
		_mountPending = false; // consume the flag — prevents double-mount in dev mode
		mountTerminal(el);
		// No cleanup return here: onDestroy(cleanup) handles component teardown.
		// The only time we want effect-cleanup is during dev double-invocation,
		// but the _mountPending = false gate already prevents a second mount.
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
	// Non-reactive gate: set to true once by startExec, consumed once by the
	// termEl effect. Non-reactive so state changes (connecting→connected) do
	// NOT re-trigger the effect and accidentally call cleanup().
	let _mountPending = false;

	async function startExec(containerId: string) {
		state = 'loading';
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
			_mountPending = true;
			// Switching state to 'connecting' renders the term-wrap div, which
			// triggers the termEl binding → the effect below fires.
			state = 'connecting';
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

		// Defer fit + WebSocket open to the next animation frame so the browser
		// has finished layout before we read dimensions. Calling fitAddon.fit()
		// synchronously after term.open() gives 0×0 because the element has not
		// been painted yet, which means cols=0/rows=0 → blank canvas.
		requestAnimationFrame(() => {
			if (!term || !fitAddon) return; // cleaned up before frame fired

			fitAddon.fit();

			const { cols, rows } = term;
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

			// Refit whenever the panel is resized
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

	// ── Close confirmation ────────────────────────────────────────────────────────
	let showCloseConfirm = $state(false);

	function requestClose() {
		showCloseConfirm = true;
	}

	function confirmClose() {
		showCloseConfirm = false;
		onClose();
	}
</script>

<div class="exec-backdrop" onclick={requestClose} role="none"></div>

<div class="exec-panel" role="dialog" aria-label="Terminal — {serviceName}">
	<div class="exec-header">
		<div class="exec-title">
			<TermIcon size={14} />
			<span>Terminal — <strong>{serviceName}</strong></span>
		</div>
		<Button variant="ghost" size="icon" onclick={requestClose} aria-label="Close terminal">
			<X size={15} />
		</Button>
	</div>

	<div class="exec-body">
		{#if state === 'loading'}
			<div class="exec-center">
				<Spinner size={20} />
				<span>Finding containers…</span>
			</div>

		{:else if state === 'error'}
			<div class="exec-center error">
				<AlertCircle size={20} />
				<span>{errorMsg}</span>
				<Button variant="secondary" size="sm" onclick={loadReplicas}>Retry</Button>
			</div>

		{:else if state === 'pick'}
			<div class="pick-list">
				<p class="pick-hint">Multiple replicas found — select a container:</p>
				{#each replicas as r}
					<button class="pick-item" onclick={() => startExec(r.container_id!)}>
						<span class="pick-slot">Replica {r.slot ?? '?'}</span>
						<Badge tone={r.status === 'running' ? 'green' : 'neutral'}>{r.status}</Badge>
						<code class="pick-id">{r.container_id!.slice(0, 12)}</code>
					</button>
				{/each}
			</div>

		{:else}
			<!-- connecting or connected — terminal renders here -->
			{#if state === 'connecting'}
				<div class="term-overlay">
					<Spinner size={16} />
					<span>Connecting…</span>
				</div>
			{/if}
			<div class="term-wrap" bind:this={termEl}></div>
		{/if}
	</div>
</div>

<ConfirmDialog
	bind:open={showCloseConfirm}
	title="Close terminal?"
	message="The active terminal session will be terminated."
	confirmLabel="Close"
	onConfirm={() => { onClose(); }}
/>

<style>
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
		background: var(--bg-base);
		border-left: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		z-index: 71;
		box-shadow: var(--shadow-lg);
		isolation: isolate;
	}

	.exec-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 0 12px 0 16px;
		height: 44px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-surface);
		flex-shrink: 0;
	}

	.exec-title {
		display: flex;
		align-items: center;
		gap: 8px;
		font-size: 13px;
		color: var(--text-muted);
	}
	.exec-title strong { color: var(--text-primary); }

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
		color: var(--text-muted);
		font-size: 13px;
	}
	.exec-center.error { color: var(--accent-red); }

	.pick-list {
		display: flex;
		flex-direction: column;
		gap: 8px;
		padding: 24px;
	}
	.pick-hint {
		font-size: 13px;
		color: var(--text-muted);
		margin: 0 0 8px;
	}
	.pick-item {
		display: flex;
		align-items: center;
		gap: 12px;
		padding: 12px 16px;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		cursor: pointer;
		text-align: left;
		transition: border-color var(--transition-fast);
		font-family: inherit;
	}
	.pick-item:hover { border-color: var(--accent); }
	.pick-slot { font-size: 13px; color: var(--text-primary); font-weight: 600; flex: 1; }
	.pick-id { font-size: 11px; color: var(--text-muted); font-family: var(--font-mono); }

	/* Terminal surface keeps the xterm theme's own dark colors in both app themes */
	.term-wrap {
		flex: 1;
		padding: 8px;
		overflow: hidden;
		background: #0d1117;
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
</style>
