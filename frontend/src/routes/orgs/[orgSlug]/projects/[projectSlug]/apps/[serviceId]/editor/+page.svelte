<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import { page } from '$app/state';
	import { api } from '$lib/api/client';
	import FileTree from '$lib/components/FileTree.svelte';
	import CodeEditor from '$lib/components/CodeEditor.svelte';
	import SandboxTerminal from '$lib/components/SandboxTerminal.svelte';
	import type { SandboxFileEntry, SandboxInstance } from '$lib/api/types';

	let serviceId = $derived(page.params.serviceId ?? '');

	let instance = $state<SandboxInstance | null>(null);
	let bootState = $state<'starting' | 'ready' | 'error'>('starting');
	let bootError = $state('');

	let entries = $state<SandboxFileEntry[]>([]);
	let openPath = $state<string | null>(null);
	let fileContent = $state('');
	let isSaving = $state(false);
	let showTerminal = $state(false);

	let heartbeatTimer: ReturnType<typeof setInterval> | undefined;

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
		}
	}

	async function saveFile(content: string) {
		if (!openPath) return;
		isSaving = true;
		await api.writeSandboxFile(serviceId, openPath, content);
		isSaving = false;
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
		window.removeEventListener('beforeunload', stopBeacon);
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

		<main class="editor-main">
			{#if openPath}
				<div class="tab-bar">
					<span class="tab">{openPath}</span>
					{#if isSaving}<span class="saving">Saving…</span>{/if}
				</div>
				<CodeEditor
					value={fileContent}
					language={languageForPath(openPath)}
					onChange={saveFile}
					height="100%"
				/>
			{:else}
				<div class="empty-state">Select a file to start editing</div>
			{/if}
		</main>

		<section class="preview-pane">
			{#if instance?.preview_url}
				<iframe title="Live preview" src={instance.preview_url}></iframe>
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
		grid-template-columns: 220px 1fr 1fr;
		grid-template-rows: 1fr auto;
		height: 100vh;
		width: 100vw;
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
	.preview-pane {
		grid-row: 1;
	}
	.preview-pane iframe {
		width: 100%;
		height: 100%;
		border: none;
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
