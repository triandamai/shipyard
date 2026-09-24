<script lang="ts">
	import { File, Folder, FolderOpen } from '@lucide/svelte';
	import type { SandboxFileEntry } from '$lib/api/types';

	interface Props {
		entries: SandboxFileEntry[];
		selectedPath?: string;
		onSelect: (path: string) => void;
	}

	let { entries, selectedPath, onSelect }: Props = $props();

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
	{#each tree as n (n.path)}
		{@render node(n, 0)}
	{/each}
</div>

<style>
	.file-tree {
		display: flex;
		flex-direction: column;
		font-size: 12px;
		overflow-y: auto;
		height: 100%;
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
