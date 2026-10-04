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
				<ListRow onclick={() => onSelect(account)} title={account.label} meta={account.host}>
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
			{/each}
		</ActivityList>
	{/if}
</div>

<style>
	.picker-wrap { padding: 16px; height: 100%; overflow-y: auto; }
	.hint { font-size: 12px; color: var(--text-muted); margin: 0 0 14px; }

</style>
