<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import FileTree from '$lib/components/FileTree.svelte';
	import CodeEditor from '$lib/components/CodeEditor.svelte';
	import SandboxTerminal from '$lib/components/SandboxTerminal.svelte';
	import { ArrowLeft, ArrowRight, RotateCw, Code2, Globe, Monitor, Smartphone } from '@lucide/svelte';
	import type { SandboxFileEntry, SandboxInstance } from '$lib/api/types';

	let serviceId = $derived(page.params.serviceId ?? '');

	let instance = $state<SandboxInstance | null>(null);
	let bootState = $state<'starting' | 'ready' | 'error'>('starting');
	let bootError = $state('');

	// Editor (sidebar + code editor, like VS Code) and preview show one at a
	// time, full width — pick which is active.
	let activeTab = $state<'editor' | 'preview'>('editor');

	// ── Preview browser chrome ──────────────────────────────────────────────
	let previewIframe: HTMLIFrameElement | undefined = $state();
	let urlBarValue = $state('');

	// A cross-origin iframe's `contentWindow.history` throws a SecurityError
	// just from being READ (not only from calling back()/forward() on it) —
	// sandbox previews are always cross-origin from this app, so that path
	// can never work. Back/forward instead replay our own record of the URLs
	// we've explicitly navigated the iframe to (initial load + anything typed
	// into the URL bar) — it can't know about in-page link clicks the preview
	// makes on its own, since reading its live location is equally blocked,
	// but it's real navigation over everything driven from this toolbar.
	let navHistory = $state<string[]>([]);
	let navIndex = $state(-1);
	let canGoBack = $derived(navIndex > 0);
	let canGoForward = $derived(navIndex >= 0 && navIndex < navHistory.length - 1);

	function navigateIframeTo(url: string, pushHistory: boolean) {
		if (!previewIframe) return;
		if (pushHistory) {
			navHistory = [...navHistory.slice(0, navIndex + 1), url];
			navIndex = navHistory.length - 1;
		}
		urlBarValue = url;
		previewIframe.src = url;
	}

	function previewGoBack() {
		if (!canGoBack) return;
		navIndex -= 1;
		navigateIframeTo(navHistory[navIndex], false);
	}

	function previewGoForward() {
		if (!canGoForward) return;
		navIndex += 1;
		navigateIframeTo(navHistory[navIndex], false);
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
		navigateIframeTo(target, true);
	}

	// ── Preview device-width sizing (desktop/mobile presets + free drag) ────
	// The preview PANE itself never resizes — only the simulated device width
	// of the content inside it, like a browser's responsive design mode.
	const PREVIEW_CONTENT_MIN_WIDTH = 280;
	let previewContentWidth = $state<number | null>(null); // null = fill the pane (Desktop)
	let previewSurfaceEl: HTMLDivElement | undefined = $state();
	let resizingPreviewContent = $state(false);
	let previewResizeCenterX = 0;
	let previewResizeMaxWidth = 0;

	function setPreviewDesktop() {
		previewContentWidth = null;
	}

	function setPreviewMobile() {
		previewContentWidth = 375;
	}

	function startPreviewContentResize(e: PointerEvent) {
		if (!previewSurfaceEl) return;
		const rect = previewSurfaceEl.getBoundingClientRect();
		previewResizeCenterX = rect.left + rect.width / 2;
		previewResizeMaxWidth = rect.width;
		resizingPreviewContent = true;
		window.addEventListener('pointermove', onPreviewContentResizeMove);
		window.addEventListener('pointerup', stopPreviewContentResize, { once: true });
	}

	function onPreviewContentResizeMove(e: PointerEvent) {
		if (!resizingPreviewContent) return;
		// The frame box is centered in the pane, so growing it by `delta` on the
		// dragged edge grows it by `delta` on the mirrored edge too.
		const raw = 2 * Math.abs(e.clientX - previewResizeCenterX);
		previewContentWidth = Math.round(
			Math.min(previewResizeMaxWidth, Math.max(PREVIEW_CONTENT_MIN_WIDTH, raw))
		);
	}

	function stopPreviewContentResize() {
		resizingPreviewContent = false;
		window.removeEventListener('pointermove', onPreviewContentResizeMove);
	}

	let entries = $state<SandboxFileEntry[]>([]);
	let openPath = $state<string | null>(null);
	let fileContent = $state('');
	let editorRef: CodeEditor | undefined = $state();
	let isSaving = $state(false);
	let justSaved = $state(false);
	let showTerminal = $state(false);
	let newFileError = $state('');

	let heartbeatTimer: ReturnType<typeof setInterval> | undefined;
	let saveTimer: ReturnType<typeof setTimeout> | undefined;
	let savedFlashTimer: ReturnType<typeof setTimeout> | undefined;
	let pendingSave: { path: string; content: string } | null = null;
	let saveInFlight = false;

	// ── Resizable sidebar ────────────────────────────────────────────────────
	const SIDEBAR_MIN_WIDTH = 140;
	const SIDEBAR_MAX_WIDTH = 480;
	let sidebarWidth = $state(220);
	let resizingSidebar = $state(false);
	let sidebarResizeStartX = 0;
	let sidebarResizeStartWidth = 0;

	function startSidebarResize(e: PointerEvent) {
		resizingSidebar = true;
		sidebarResizeStartX = e.clientX;
		sidebarResizeStartWidth = sidebarWidth;
		window.addEventListener('pointermove', onSidebarResizeMove);
		window.addEventListener('pointerup', stopSidebarResize, { once: true });
	}

	function onSidebarResizeMove(e: PointerEvent) {
		if (!resizingSidebar) return;
		const delta = e.clientX - sidebarResizeStartX;
		sidebarWidth = Math.min(SIDEBAR_MAX_WIDTH, Math.max(SIDEBAR_MIN_WIDTH, sidebarResizeStartWidth + delta));
	}

	function stopSidebarResize() {
		resizingSidebar = false;
		window.removeEventListener('pointermove', onSidebarResizeMove);
	}

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
		instance = { ...instance, status: startRes.data.status, preview_url: startRes.data.preview_url, pending: startRes.data.pending } as SandboxInstance;
		urlBarValue = startRes.data.preview_url ?? '';
		navHistory = startRes.data.preview_url ? [startRes.data.preview_url] : [];
		navIndex = navHistory.length - 1;

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
		const res = await api.writeSandboxFile(serviceId, path, content);
		saveInFlight = false;
		if (pendingSave) {
			flushSave();
			return;
		}
		isSaving = false;
		if (!res.error) {
			justSaved = true;
			if (savedFlashTimer) clearTimeout(savedFlashTimer);
			savedFlashTimer = setTimeout(() => (justSaved = false), 1500);
			// "Hot reload": most dev servers already live-update the page
			// themselves via their own HMR client script, but not every stack
			// has one (e.g. plain static files) — refreshing the preview after
			// every save guarantees it reflects the latest saved content
			// either way, on top of whatever the framework already does.
			if (instance?.preview_url) previewRefresh();
		}
	}

	async function createFile(rawPath: string) {
		const path = rawPath.trim().replace(/^\/+/, '');
		if (!path) return;
		newFileError = '';
		if (entries.some((e) => e.path === path)) {
			newFileError = `'${path}' already exists`;
			return;
		}
		const res = await api.writeSandboxFile(serviceId, path, '');
		if (res.error) {
			newFileError = res.error.message;
			return;
		}
		const treeRes = await api.getSandboxFileTree(serviceId);
		if (treeRes.data) entries = treeRes.data.entries;
		await openFile(path);
	}

	// No stop-on-unload here, deliberately: closing or reloading this tab fires
	// the same 'beforeunload' event as actually leaving for good, and there's
	// no reliable way to tell those apart client-side. Stopping immediately on
	// every reload was killing sandboxes the user only meant to refresh,
	// forcing a full re-provision. The idle reaper (sandbox_runtime/reaper.rs)
	// already stops a sandbox once its heartbeat goes stale for
	// idle_timeout_secs (default 20 min) — that's the real "user is actually
	// gone" signal, with the grace period this needs.
	onMount(() => {
		boot();
	});

	onDestroy(() => {
		if (heartbeatTimer) clearInterval(heartbeatTimer);
		if (saveTimer) clearTimeout(saveTimer);
		if (savedFlashTimer) clearTimeout(savedFlashTimer);
		window.removeEventListener('pointermove', onPreviewContentResizeMove);
		window.removeEventListener('pointermove', onSidebarResizeMove);
	});
</script>

<div class="editor-layout" class:resizing-preview={resizingPreviewContent} class:resizing-sidebar={resizingSidebar}>
	{#if bootState === 'starting'}
		<div class="boot-overlay">Waking up your sandbox…</div>
	{:else if bootState === 'error'}
		<div class="boot-overlay error">{bootError}</div>
	{:else}
		<div class="panel-tabs">
			<button class="panel-tab" class:active={activeTab === 'editor'} onclick={() => (activeTab = 'editor')}>
				<Code2 size={13} /> Editor
			</button>
			<button class="panel-tab" class:active={activeTab === 'preview'} onclick={() => (activeTab = 'preview')}>
				<Globe size={13} /> Preview
			</button>
		</div>

		<div class="editor-view-wrap" class:hidden={activeTab !== 'editor'}>
			{#if instance?.pending}
				<div class="pending-banner">
					No project detected yet — scaffold one in the Terminal below, then Stop and Start to apply it.
				</div>
			{/if}
			<div class="editor-view">
				<aside class="sidebar" style="width: {sidebarWidth}px">
					<FileTree {entries} selectedPath={openPath ?? undefined} onSelect={openFile} onCreateFile={createFile} createError={newFileError} />
				</aside>
				<div
					class="sidebar-resize-handle"
					role="separator"
					aria-orientation="vertical"
					aria-label="Resize sidebar"
					tabindex="-1"
					onpointerdown={startSidebarResize}
				></div>
				<main class="editor-main">
					{#if openPath}
						<div class="tab-bar">
							<span class="tab">{openPath}</span>
							{#if isSaving}<span class="saving">Saving…</span>{:else if justSaved}<span class="saved">Saved</span>{/if}
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
			</div>
		</div>

		<section class="preview-pane" class:hidden={activeTab !== 'preview'}>
			{#if instance?.preview_url}
				<div class="preview-toolbar">
					<button class="preview-nav-btn" onclick={previewGoBack} disabled={!canGoBack} title="Back" aria-label="Back">
						<ArrowLeft size={13} />
					</button>
					<button class="preview-nav-btn" onclick={previewGoForward} disabled={!canGoForward} title="Forward" aria-label="Forward">
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
					<div class="preview-device-btns">
						<button
							class="preview-nav-btn"
							class:active={previewContentWidth === null}
							onclick={setPreviewDesktop}
							title="Desktop width"
							aria-label="Desktop width"
						>
							<Monitor size={13} />
						</button>
						<button
							class="preview-nav-btn"
							class:active={previewContentWidth === 375}
							onclick={setPreviewMobile}
							title="Mobile width"
							aria-label="Mobile width"
						>
							<Smartphone size={13} />
						</button>
					</div>
				</div>
				<div class="preview-surface" bind:this={previewSurfaceEl}>
					<div
						class="preview-frame-box"
						style={previewContentWidth ? `width:${previewContentWidth}px` : 'width:100%'}
					>
						<iframe bind:this={previewIframe} title="Live preview" src={instance.preview_url}></iframe>
						<div
							class="preview-resize-handle"
							role="separator"
							aria-orientation="vertical"
							aria-label="Resize preview width"
							tabindex="-1"
							onpointerdown={startPreviewContentResize}
						></div>
					</div>
				</div>
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
		grid-template-rows: auto 1fr auto;
		height: 100vh;
		width: 100%;
		max-width: 100%;
		overflow: hidden;
	}
	.editor-layout.resizing-preview,
	.editor-layout.resizing-sidebar {
		cursor: ew-resize;
		user-select: none;
	}
	.hidden {
		display: none !important;
	}

	.panel-tabs {
		grid-row: 1;
		display: flex;
		gap: 2px;
		padding: 6px 8px 0;
		background: var(--bg-elevated);
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}
	.panel-tab {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 7px 14px;
		border: none;
		border-bottom: 2px solid transparent;
		background: transparent;
		color: var(--text-dim);
		font-size: 12px;
		font-weight: 500;
		cursor: pointer;
		border-radius: var(--radius-sm, 4px) var(--radius-sm, 4px) 0 0;
	}
	.panel-tab:hover {
		color: var(--text-secondary);
		background: var(--bg-surface, rgba(127,127,127,0.08));
	}
	.panel-tab.active {
		color: var(--text-primary);
		border-bottom-color: var(--accent);
	}

	/* Sidebar + editor travel together, like VS Code — only shown as a pair
	   on the Editor tab; the Preview tab gets the full width to itself. */
	.editor-view-wrap {
		grid-row: 2;
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	.editor-view {
		display: flex;
		flex: 1;
		min-height: 0;
	}
	.pending-banner {
		flex-shrink: 0;
		padding: 8px 14px;
		background: color-mix(in srgb, var(--accent) 10%, var(--bg-elevated));
		border-bottom: 1px solid var(--border);
		color: var(--text-secondary);
		font-size: 12px;
	}
	.sidebar {
		flex-shrink: 0;
		overflow-y: auto;
	}
	.sidebar-resize-handle {
		flex-shrink: 0;
		width: 6px;
		cursor: ew-resize;
		background: transparent;
		position: relative;
	}
	.sidebar-resize-handle::after {
		content: '';
		position: absolute;
		top: 0;
		bottom: 0;
		left: 2px;
		width: 2px;
		background: var(--border);
	}
	.sidebar-resize-handle:hover::after,
	.editor-layout.resizing-sidebar .sidebar-resize-handle::after {
		background: var(--accent);
	}
	.editor-main {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
		min-height: 0;
	}

	.preview-pane {
		grid-row: 2;
		display: flex;
		flex-direction: column;
		min-width: 0;
		min-height: 0;
	}
	/* The preview shows an arbitrary website that may be any color scheme —
	   giving it its own neutral, always-light "browser content" surface
	   (rather than inheriting Shipyard's own dark app background) is what
	   makes it read as a separate embedded page instead of blending into
	   the surrounding UI, especially before the iframe has painted anything. */
	.preview-surface {
		flex: 1;
		min-height: 0;
		background: #fff;
		display: flex;
		justify-content: center;
		overflow: hidden;
	}
	/* Only this box's width changes on resize/device-preset — the pane
	   around it stays put, so resizing simulates a narrower device viewport
	   rather than shrinking the preview panel itself. */
	.preview-frame-box {
		position: relative;
		height: 100%;
		max-width: 100%;
		padding-right: 8px;
		box-sizing: border-box;
	}
	.preview-frame-box iframe {
		width: 100%;
		height: 100%;
		border: none;
		display: block;
		background: #fff;
	}
	.editor-layout.resizing-preview .preview-frame-box iframe {
		pointer-events: none;
	}
	.preview-resize-handle {
		position: absolute;
		top: 0;
		right: 0;
		width: 8px;
		height: 100%;
		cursor: ew-resize;
	}
	.preview-resize-handle::after {
		content: '';
		position: absolute;
		top: 0;
		bottom: 0;
		left: 3px;
		width: 2px;
		background: var(--border);
	}
	.preview-resize-handle:hover::after,
	.editor-layout.resizing-preview .preview-resize-handle::after {
		background: var(--accent);
	}
	.preview-device-btns {
		display: flex;
		align-items: center;
		gap: 2px;
		padding-left: 6px;
		margin-left: 4px;
		border-left: 1px solid var(--border);
		flex-shrink: 0;
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
	.preview-nav-btn:hover:not(:disabled) {
		background: var(--bg-surface, rgba(127,127,127,0.12));
		color: var(--text-primary);
	}
	.preview-nav-btn:disabled {
		opacity: 0.35;
		cursor: default;
	}
	.preview-nav-btn.active {
		color: var(--accent);
		background: var(--bg-surface, rgba(127,127,127,0.12));
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
	.saved {
		color: var(--accent-green, #22c55e);
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
		grid-row: 3;
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
