<script lang="ts">
	import { File, Folder, FolderOpen, Plus } from '@lucide/svelte';
	import Button from '$lib/components/ui/Button.svelte';
	import type { SandboxFileEntry } from '$lib/api/types';

	interface Props {
		entries: SandboxFileEntry[];
		selectedPath?: string;
		onSelect: (path: string) => void;
		onCreateFile?: (path: string) => void;
		createError?: string;
	}

	let { entries, selectedPath, onSelect, onCreateFile, createError = '' }: Props = $props();

	let creatingFile = $state(false);
	let newFileName = $state('');
	let newFileInputEl: HTMLInputElement | undefined = $state();

	function startCreateFile() {
		creatingFile = true;
		newFileName = '';
		requestAnimationFrame(() => newFileInputEl?.focus());
	}

	function submitCreateFile() {
		if (!newFileName.trim()) {
			creatingFile = false;
			return;
		}
		onCreateFile?.(newFileName);
		creatingFile = false;
		newFileName = '';
	}

	function handleNewFileKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			e.preventDefault();
			submitCreateFile();
		} else if (e.key === 'Escape') {
			creatingFile = false;
			newFileName = '';
		}
	}

	interface TreeNode {
		name: string;
		path: string;
		isDir: boolean;
		children: TreeNode[];
	}

	function buildTree(flat: SandboxFileEntry[]): TreeNode[] {
		const root: TreeNode[] = [];
		const byPath = new Map<string, TreeNode>();

		const sorted = [...flat].sort((a, b) => a.path.localeCompare(b.path));
		for (const entry of sorted) {
			const parts = entry.path.split('/');
			let parentChildren = root;
			let currentPath = '';
			for (let i = 0; i < parts.length; i++) {
				currentPath = currentPath ? `${currentPath}/${parts[i]}` : parts[i];
				let node = byPath.get(currentPath);
				if (!node) {
					node = {
						name: parts[i],
						path: currentPath,
						isDir: i < parts.length - 1 ? true : entry.is_dir,
						children: []
					};
					byPath.set(currentPath, node);
					parentChildren.push(node);
				}
				parentChildren = node.children;
			}
		}
		return root;
	}

	let tree = $derived(buildTree(entries));
	let expanded = $state(new Set<string>());

	function toggle(path: string) {
		if (expanded.has(path)) expanded.delete(path);
		else expanded.add(path);
		expanded = new Set(expanded);
	}
</script>

{#snippet node(n: TreeNode, depth: number)}
	<div class="tree-row" style="padding-left: {depth * 14}px">
		{#if n.isDir}
			<button class="tree-item dir" onclick={() => toggle(n.path)}>
				{#if expanded.has(n.path)}
					<FolderOpen size={13} />
				{:else}
					<Folder size={13} />
				{/if}
				{n.name}
			</button>
			{#if expanded.has(n.path)}
				{#each n.children as child (child.path)}
					{@render node(child, depth + 1)}
				{/each}
			{/if}
		{:else}
			<button class="tree-item file" class:selected={selectedPath === n.path} onclick={() => onSelect(n.path)}>
				<File size={13} />
				{n.name}
			</button>
		{/if}
	</div>
{/snippet}

<div class="file-tree">
	<div class="file-tree-header">
		<span class="file-tree-title">Files</span>
		<Button variant="ghost" size="icon" onclick={startCreateFile} title="New file" aria-label="New file">
			<Plus size={13} />
		</Button>
	</div>
	{#if creatingFile}
		<div class="new-file-row">
			<File size={13} />
			<input
				bind:this={newFileInputEl}
				bind:value={newFileName}
				class="new-file-input"
				type="text"
				placeholder="path/to/file.ts"
				spellcheck="false"
				autocomplete="off"
				onkeydown={handleNewFileKeydown}
				onblur={submitCreateFile}
			/>
		</div>
	{/if}
	{#if createError}
		<div class="new-file-error">{createError}</div>
	{/if}
	<div class="file-tree-scroll">
		{#each tree as n (n.path)}
			{@render node(n, 0)}
		{/each}
	</div>
</div>

<style>
	.file-tree {
		display: flex;
		flex-direction: column;
		font-size: 12px;
		height: 100%;
	}
	.file-tree-header {
		display: flex;
		align-items: center;
		justify-content: space-between;
		padding: 2px 8px;
		flex-shrink: 0;
	}
	.file-tree-title {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
		text-transform: uppercase;
		letter-spacing: 0.05em;
	}
	.new-file-row {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 3px 8px;
		color: var(--text-secondary);
	}
	.new-file-input {
		flex: 1;
		min-width: 0;
		background: var(--bg-base);
		border: 1px solid var(--accent);
		border-radius: var(--radius-sm, 4px);
		color: var(--text-primary);
		font-size: 12px;
		font-family: var(--font-mono);
		padding: 2px 6px;
		outline: none;
	}
	.new-file-error {
		font-size: 11px;
		color: var(--accent-red);
		padding: 2px 8px 6px;
	}
	.file-tree-scroll {
		flex: 1;
		min-height: 0;
		overflow-y: auto;
	}
	.tree-item {
		display: flex;
		align-items: center;
		gap: 6px;
		width: 100%;
		padding: 3px 8px;
		background: none;
		border: none;
		color: var(--text-secondary);
		cursor: pointer;
		text-align: left;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.tree-item:hover {
		background: var(--bg-hover);
	}
	.tree-item.selected {
		background: var(--accent-muted);
		color: var(--accent);
	}
</style>
