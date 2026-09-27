<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import FileTree from '$lib/components/FileTree.svelte';
	import CodeEditor from '$lib/components/CodeEditor.svelte';
	import SandboxTerminal from '$lib/components/SandboxTerminal.svelte';
	import { ArrowLeft, ArrowRight, RotateCw } from '@lucide/svelte';
	import type { SandboxFileEntry, SandboxInstance } from '$lib/api/types';

	let serviceId = $derived(page.params.serviceId ?? '');

	let instance = $state<SandboxInstance | null>(null);
	let bootState = $state<'starting' | 'ready' | 'error'>('starting');
	let bootError = $state('');

	// ── Preview browser chrome ──────────────────────────────────────────────
	let previewIframe: HTMLIFrameElement | undefined = $state();
	let urlBarValue = $state('');

	function previewGoBack() {
		try { previewIframe?.contentWindow?.history.back(); } catch { /* cross-origin restrictions vary by browser */ }
	}

	function previewGoForward() {
		try { previewIframe?.contentWindow?.history.forward(); } catch { /* cross-origin restrictions vary by browser */ }
	}

	function previewRefresh() {
		if (!previewIframe) return;
		const src = previewIframe.src;
		// Cross-origin content can't be reloaded via contentWindow.location.reload()
		// (throws a SecurityError) — bouncing through about:blank forces a real
		// reload instead of a same-value-assignment no-op.
		previewIframe.src = 'about:blank';
		requestAnimationFrame(() => {
			if (previewIframe) previewIframe.src = src;
		});
	}

	function previewNavigate(e: Event) {
		e.preventDefault();
		if (!previewIframe || !urlBarValue.trim()) return;
		let target = urlBarValue.trim();
		// A bare path (e.g. "/about") is resolved against the current preview's
		// own origin — we can't read the iframe's live location cross-origin, so
		// this resolves against the preview's original origin, not wherever an
		// in-app navigation may have since taken it.
		if (target.startsWith('/') && instance?.preview_url) {
			try {
				target = new URL(target, instance.preview_url).toString();
			} catch { /* fall through with the raw input */ }
		} else if (!/^https?:\/\//i.test(target)) {
			target = `https://${target}`;
		}
		urlBarValue = target;
		previewIframe.src = target;
	}

	// ── Resizable preview pane ──────────────────────────────────────────────
	const PREVIEW_MIN_WIDTH = 240;
	const PREVIEW_MAX_WIDTH = 1400;
	let previewWidth = $state(480);
	let resizing = $state(false);
	let resizeStartX = 0;
	let resizeStartWidth = 0;

	function startResize(e: PointerEvent) {
		resizing = true;
		resizeStartX = e.clientX;
		resizeStartWidth = previewWidth;
		window.addEventListener('pointermove', onResizeMove);
		window.addEventListener('pointerup', stopResize, { once: true });
	}

	function onResizeMove(e: PointerEvent) {
		if (!resizing) return;
		const delta = e.clientX - resizeStartX;
		const maxAllowed = Math.min(PREVIEW_MAX_WIDTH, window.innerWidth - 400);
		previewWidth = Math.min(maxAllowed, Math.max(PREVIEW_MIN_WIDTH, resizeStartWidth - delta));
	}

	function stopResize() {
		resizing = false;
		window.removeEventListener('pointermove', onResizeMove);
	}

	let entries = $state<SandboxFileEntry[]>([]);
	let openPath = $state<string | null>(null);
	let fileContent = $state('');
	let editorRef: CodeEditor | undefined = $state();
	let isSaving = $state(false);
	let showTerminal = $state(false);

	let heartbeatTimer: ReturnType<typeof setInterval> | undefined;
	let saveTimer: ReturnType<typeof setTimeout> | undefined;
	let pendingSave: { path: string; content: string } | null = null;
	let saveInFlight = false;

	function languageForPath(path: string): 'javascript' | 'json' | 'plain' {
		if (path.endsWith('.json')) return 'json';
		if (path.endsWith('.js') || path.endsWith('.ts') || path.endsWith('.jsx') || path.endsWith('.tsx')) return 'javascript';
		return 'plain';
	}

	async function boot() {
		const startRes = await api.startSandbox(serviceId);
		if (!startRes.data) {
			bootError = startRes.error?.message ?? 'Failed to start sandbox';
			bootState = 'error';
			return;
		}
		instance = { ...instance, status: startRes.data.status, preview_url: startRes.data.preview_url } as SandboxInstance;
		urlBarValue = startRes.data.preview_url ?? '';

		const treeRes = await api.getSandboxFileTree(serviceId);
		if (treeRes.data) entries = treeRes.data.entries;

		bootState = 'ready';

		heartbeatTimer = setInterval(() => {
			api.heartbeatSandbox(serviceId);
		}, 60_000);
	}

	async function openFile(path: string) {
		const res = await api.readSandboxFile(serviceId, path);
		if (res.data !== null && res.data !== undefined) {
			openPath = path;
			fileContent = res.data;
			editorRef?.setValue(res.data);
			editorRef?.setLanguage(languageForPath(path));
		}
	}

	function saveFile(content: string) {
		if (!openPath) return;
		pendingSave = { path: openPath, content };
		if (saveTimer) clearTimeout(saveTimer);
		saveTimer = setTimeout(flushSave, 500);
	}

	async function flushSave() {
		if (!pendingSave || saveInFlight) return;
		const { path, content } = pendingSave;
		pendingSave = null;
		saveInFlight = true;
		isSaving = true;
		await api.writeSandboxFile(serviceId, path, content);
		saveInFlight = false;
		if (pendingSave) {
			flushSave();
		} else {
			isSaving = false;
		}
	}

	function stopBeacon() {
		navigator.sendBeacon(`/api/apps/${serviceId}/sandbox/stop`, new Blob());
	}

	onMount(() => {
		boot();
		window.addEventListener('beforeunload', stopBeacon);
	});

	onDestroy(() => {
		if (heartbeatTimer) clearInterval(heartbeatTimer);
		if (saveTimer) clearTimeout(saveTimer);
		window.removeEventListener('beforeunload', stopBeacon);
		window.removeEventListener('pointermove', onResizeMove);
	});
</script>

<div
	class="editor-layout"
	class:resizing
	style="grid-template-columns: 220px 1fr 6px {previewWidth}px;"
>
	{#if bootState === 'starting'}
		<div class="boot-overlay">Waking up your sandbox…</div>
	{:else if bootState === 'error'}
		<div class="boot-overlay error">{bootError}</div>
	{:else}
		<aside class="sidebar">
			<FileTree {entries} selectedPath={openPath ?? undefined} onSelect={openFile} />
		</aside>

		<main class="editor-main">
			{#if openPath}
				<div class="tab-bar">
					<span class="tab">{openPath}</span>
					{#if isSaving}<span class="saving">Saving…</span>{/if}
				</div>
				<CodeEditor
					bind:this={editorRef}
					value={fileContent}
					language={languageForPath(openPath)}
					onChange={saveFile}
					height="100%"
				/>
			{:else}
				<div class="empty-state">Select a file to start editing</div>
			{/if}
		</main>

		<div
			class="resize-handle"
			role="separator"
			aria-orientation="vertical"
			aria-label="Resize preview pane"
			tabindex="-1"
			onpointerdown={startResize}
		></div>

		<section class="preview-pane">
			{#if instance?.preview_url}
				<div class="preview-toolbar">
					<button class="preview-nav-btn" onclick={previewGoBack} title="Back" aria-label="Back">
						<ArrowLeft size={13} />
					</button>
					<button class="preview-nav-btn" onclick={previewGoForward} title="Forward" aria-label="Forward">
						<ArrowRight size={13} />
					</button>
					<button class="preview-nav-btn" onclick={previewRefresh} title="Refresh" aria-label="Refresh">
						<RotateCw size={12} />
					</button>
					<form class="preview-url-form" onsubmit={previewNavigate}>
						<input
							class="preview-url-input"
							type="text"
							bind:value={urlBarValue}
							spellcheck="false"
							autocomplete="off"
						/>
					</form>
				</div>
				<iframe bind:this={previewIframe} title="Live preview" src={instance.preview_url}></iframe>
			{:else}
				<div class="empty-state">No preview available</div>
			{/if}
		</section>

		<button class="terminal-toggle" onclick={() => (showTerminal = !showTerminal)}>
			{showTerminal ? 'Hide' : 'Show'} Terminal
		</button>
		{#if showTerminal}
			<div class="terminal-pane">
				<SandboxTerminal {serviceId} />
			</div>
		{/if}
	{/if}
</div>

<style>
	.editor-layout {
		display: grid;
		grid-template-rows: 1fr auto;
		height: 100vh;
		width: 100vw;
	}
	.editor-layout.resizing {
		cursor: col-resize;
		user-select: none;
	}
	.sidebar {
		border-right: 1px solid var(--border);
		overflow-y: auto;
		grid-row: 1;
	}
	.editor-main {
		display: flex;
		flex-direction: column;
		min-width: 0;
		border-right: 1px solid var(--border);
		grid-row: 1;
	}
	.resize-handle {
		grid-row: 1;
		cursor: col-resize;
		background: transparent;
		position: relative;
	}
	.resize-handle::after {
		content: '';
		position: absolute;
		top: 0;
		bottom: 0;
		left: 2px;
		width: 2px;
		background: var(--border);
	}
	.resize-handle:hover::after,
	.editor-layout.resizing .resize-handle::after {
		background: var(--accent);
	}
	.preview-pane {
		grid-row: 1;
		display: flex;
		flex-direction: column;
		min-width: 0;
	}
	.preview-pane iframe {
		flex: 1;
		width: 100%;
		border: none;
	}
	.editor-layout.resizing .preview-pane iframe {
		pointer-events: none;
	}
	.preview-toolbar {
		display: flex;
		align-items: center;
		gap: 4px;
		padding: 5px 8px;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elevated);
		flex-shrink: 0;
	}
	.preview-nav-btn {
		display: flex;
		align-items: center;
		justify-content: center;
		width: 22px;
		height: 22px;
		border: none;
		background: transparent;
		color: var(--text-secondary);
		border-radius: var(--radius-sm, 4px);
		cursor: pointer;
		flex-shrink: 0;
	}
	.preview-nav-btn:hover {
		background: var(--bg-surface, rgba(127,127,127,0.12));
		color: var(--text-primary);
	}
	.preview-url-form {
		flex: 1;
		min-width: 0;
	}
	.preview-url-input {
		width: 100%;
		box-sizing: border-box;
		font-size: 11px;
		font-family: var(--font-mono);
		color: var(--text-secondary);
		background: var(--bg-base, var(--bg-elevated));
		border: 1px solid var(--border);
		border-radius: 99px;
		padding: 3px 10px;
		outline: none;
	}
	.preview-url-input:focus {
		border-color: var(--accent);
		color: var(--text-primary);
	}
	.tab-bar {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 6px 10px;
		border-bottom: 1px solid var(--border);
		font-size: 12px;
		font-family: var(--font-mono);
	}
	.saving {
		color: var(--text-dim);
		font-size: 11px;
	}
	.empty-state {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100%;
		color: var(--text-dim);
		font-size: 13px;
	}
	.boot-overlay {
		grid-column: 1 / -1;
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100vh;
		font-size: 14px;
		color: var(--text-secondary);
	}
	.boot-overlay.error {
		color: var(--accent-red);
	}
	.terminal-toggle {
		grid-column: 1 / -1;
		grid-row: 2;
		padding: 6px 12px;
		background: var(--bg-elevated);
		border: none;
		border-top: 1px solid var(--border);
		color: var(--text-secondary);
		font-size: 11px;
		cursor: pointer;
	}
	.terminal-pane {
		position: fixed;
		bottom: 32px;
		left: 0;
		right: 0;
		height: 240px;
		border-top: 1px solid var(--border);
	}
</style>
