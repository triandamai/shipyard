<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import FileTree from '$lib/components/FileTree.svelte';
	import CodeEditor from '$lib/components/CodeEditor.svelte';
	import SandboxTerminal from '$lib/components/SandboxTerminal.svelte';
	import { ArrowLeft, ArrowRight, RotateCw, Code2, Globe } from '@lucide/svelte';
	import type { SandboxFileEntry, SandboxInstance } from '$lib/api/types';

	let serviceId = $derived(page.params.serviceId ?? '');

	let instance = $state<SandboxInstance | null>(null);
	let bootState = $state<'starting' | 'ready' | 'error'>('starting');
	let bootError = $state('');

	// Editor and preview show one at a time (not split) — pick which is active.
	let activeTab = $state<'editor' | 'preview'>('editor');

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
	});
</script>

<div class="editor-layout">
	{#if bootState === 'starting'}
		<div class="boot-overlay">Waking up your sandbox…</div>
	{:else if bootState === 'error'}
		<div class="boot-overlay error">{bootError}</div>
	{:else}
		<aside class="sidebar">
			<FileTree {entries} selectedPath={openPath ?? undefined} onSelect={openFile} />
		</aside>

		<div class="main-panel">
			<div class="panel-tabs">
				<button class="panel-tab" class:active={activeTab === 'editor'} onclick={() => (activeTab = 'editor')}>
					<Code2 size={13} /> Editor
				</button>
				<button class="panel-tab" class:active={activeTab === 'preview'} onclick={() => (activeTab = 'preview')}>
					<Globe size={13} /> Preview
				</button>
			</div>

			<main class="editor-main" class:hidden={activeTab !== 'editor'}>
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

			<section class="preview-pane" class:hidden={activeTab !== 'preview'}>
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
					<div class="preview-surface">
						<iframe bind:this={previewIframe} title="Live preview" src={instance.preview_url}></iframe>
					</div>
				{:else}
					<div class="empty-state">No preview available</div>
				{/if}
			</section>
		</div>

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
		grid-template-columns: 220px 1fr;
		grid-template-rows: 1fr auto;
		height: 100vh;
		width: 100vw;
	}
	.sidebar {
		border-right: 1px solid var(--border);
		overflow-y: auto;
		grid-row: 1;
	}
	.main-panel {
		display: flex;
		flex-direction: column;
		min-width: 0;
		grid-row: 1;
	}
	.hidden {
		display: none !important;
	}

	.panel-tabs {
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

	.editor-main {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
		min-height: 0;
	}
	.preview-pane {
		display: flex;
		flex-direction: column;
		min-width: 0;
		flex: 1;
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
	}
	.preview-surface iframe {
		width: 100%;
		height: 100%;
		border: none;
		background: #fff;
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
