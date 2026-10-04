<script lang="ts">
	import { GitBranch, GitMerge, GitFork, ChevronRight } from '@lucide/svelte';
	import { ActivityList, ListRow, EmptyState } from '$lib/components/ui';

	interface Account {
		id: string;
		label: string;
		host: string;
		token: string;
	}

	interface Props {
		accounts: Account[];
		onSelect: (account: Account) => void;
	}

	let { accounts, onSelect }: Props = $props();
</script>

<div class="picker-wrap">
	{#if accounts.length === 0}
		<EmptyState message="No Git accounts connected. Go to Settings → Git Providers to add one." />
	{:else}
		<p class="hint">Select the account that has access to the repository.</p>
		<ActivityList>
			{#each accounts as account (account.id)}
				<button type="button" class="pick-row" onclick={() => onSelect(account)}>
					<ListRow title={account.label} meta={account.host}>
						{#snippet icon()}
							<!-- lucide has no brand marks: GitBranch = github, GitMerge = gitlab, GitFork = other -->
							{#if account.id === 'github'}
								<GitBranch size={16} />
							{:else if account.id === 'gitlab'}
								<GitMerge size={16} />
							{:else}
								<GitFork size={16} />
							{/if}
						{/snippet}
						{#snippet trailing()}<ChevronRight size={16} />{/snippet}
					</ListRow>
				</button>
			{/each}
		</ActivityList>
	{/if}
</div>

<style>
	.picker-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.hint { font-size: 12px; color: var(--text-muted); margin: 0 0 14px; }

	.pick-row {
		display: block; width: 100%; padding: 0 8px; margin: 0;
		background: transparent; border: none; border-bottom: 1px solid var(--border);
		color: inherit; font-family: var(--font-sans); text-align: left; cursor: pointer;
		transition: background var(--transition-fast);
	}
	.pick-row:last-child { border-bottom: none; }
	.pick-row:hover { background: var(--bg-hover); }
	.pick-row:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
	.pick-row :global(.ui-list-row) { border-bottom: none; }
	.pick-row :global(.ui-list-row-trailing) { color: var(--text-dim); display: flex; }
</style>
