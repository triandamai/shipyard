<script lang="ts">
	import { onMount } from 'svelte';
	import { GitBranch, ChevronRight } from '@lucide/svelte';
	import { SearchInput, ActivityList, ListRow, Spinner, EmptyState, InlineAlert } from '$lib/components/ui';

	interface Props {
		provider: string;
		token: string;
		repoFullName: string;
		onSelect: (branch: string) => void;
	}

	let { provider, token, repoFullName, onSelect }: Props = $props();

	let branches = $state<string[]>([]);
	let loading = $state(true);
	let error = $state('');
	let search = $state('');

	let filtered = $derived(
		search.trim()
			? branches.filter(b => b.toLowerCase().includes(search.toLowerCase()))
			: branches
	);

	onMount(async () => {
		loading = true;
		error = '';
		try {
			if (provider === 'github') {
				const res = await fetch(
					`https://api.github.com/repos/${repoFullName}/branches?per_page=100`,
					{ headers: { Authorization: `token ${token}`, Accept: 'application/vnd.github+json' } }
				);
				if (!res.ok) throw new Error(`GitHub API returned ${res.status}`);
				const data = await res.json();
				branches = data.map((b: { name: string }) => b.name);
			} else if (provider === 'gitlab') {
				const encoded = encodeURIComponent(repoFullName);
				const res = await fetch(
					`https://gitlab.com/api/v4/projects/${encoded}/repository/branches?per_page=100`,
					{ headers: { Authorization: `Bearer ${token}` } }
				);
				if (!res.ok) throw new Error(`GitLab API returned ${res.status}`);
				const data = await res.json();
				branches = data.map((b: { name: string }) => b.name);
			} else if (provider === 'bitbucket') {
				const res = await fetch(
					`https://api.bitbucket.org/2.0/repositories/${repoFullName}/refs/branches?pagelen=100`,
					{ headers: { Authorization: `Basic ${btoa(token)}` } }
				);
				if (!res.ok) throw new Error(`Bitbucket API returned ${res.status}`);
				const data = await res.json();
				branches = (data.values ?? []).map((b: { name: string }) => b.name);
			}
		} catch (e) {
			error = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	});
</script>

<div class="picker-wrap">
	<div class="search-bar">
		<SearchInput bind:value={search} placeholder="Search branches…" />
	</div>

	{#if loading}
		<div class="state-msg">
			<Spinner size={16} />
			<span>Fetching branches…</span>
		</div>
	{:else if error}
		<div class="state-pad" role="alert"><InlineAlert tone="error">{error}</InlineAlert></div>
	{:else if filtered.length === 0}
		<EmptyState message={search.trim() ? 'No branches match your search.' : 'No branches found.'} />
	{:else}
		<div class="branch-count">{filtered.length} {filtered.length === 1 ? 'branch' : 'branches'}</div>
		<div class="branch-list">
			<ActivityList>
				{#each filtered as branch (branch)}
					<ListRow title={branch} onclick={() => onSelect(branch)}>
							{#snippet icon()}<GitBranch size={14} />{/snippet}
							{#snippet trailing()}<ChevronRight size={16} />{/snippet}
						</ListRow>
				{/each}
			</ActivityList>
		</div>
	{/if}
</div>

<style>
	.picker-wrap { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

	.search-bar { padding: 12px 16px; border-bottom: 1px solid var(--border); flex-shrink: 0; }

	.state-msg {
		display: flex; align-items: center; gap: 10px;
		padding: 32px 16px; font-size: 13px; color: var(--text-muted);
		justify-content: center; text-align: center;
	}
	.state-pad { padding: 16px; }

	.branch-count { font-size: 11px; color: var(--text-dim); padding: 8px 16px 4px; flex-shrink: 0; }

	.branch-list { flex: 1; overflow-y: auto; padding: 0 16px 16px; }

</style>
