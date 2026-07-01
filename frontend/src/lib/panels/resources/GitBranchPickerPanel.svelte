<script lang="ts">
	import { onMount } from 'svelte';
	import { Search, GitBranch } from '@lucide/svelte';

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
		<Search size={14} class="search-icon" />
		<input
			class="search-input"
			type="text"
			placeholder="Search branches…"
			bind:value={search}
		/>
	</div>

	{#if loading}
		<div class="state-msg">
			<div class="spinner"></div>
			<span>Fetching branches…</span>
		</div>
	{:else if error}
		<div class="state-msg error">{error}</div>
	{:else if filtered.length === 0}
		<div class="state-msg">
			{search.trim() ? 'No branches match your search.' : 'No branches found.'}
		</div>
	{:else}
		<div class="branch-count">{filtered.length} {filtered.length === 1 ? 'branch' : 'branches'}</div>
		<div class="branch-list">
			{#each filtered as branch (branch)}
				<button type="button" class="branch-row" onclick={() => onSelect(branch)}>
					<GitBranch size={13} class="branch-icon" />
					<span class="branch-name">{branch}</span>
					<span class="chevron">›</span>
				</button>
			{/each}
		</div>
	{/if}
</div>

<style>
	.picker-wrap { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

	.search-bar {
		display: flex; align-items: center; gap: 8px;
		padding: 12px 16px; border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	:global(.search-icon) { color: var(--text-dim); flex-shrink: 0; }

	.search-input {
		flex: 1; background: transparent; border: none; outline: none;
		color: var(--text-primary); font-size: 13px; font-family: var(--font-sans);
	}
	.search-input::placeholder { color: var(--text-dim); }

	.state-msg {
		display: flex; align-items: center; gap: 10px;
		padding: 32px 16px; font-size: 13px; color: var(--text-muted);
		justify-content: center; text-align: center;
	}
	.state-msg.error { color: var(--accent-red); }

	.spinner {
		width: 16px; height: 16px; border: 2px solid var(--border);
		border-top-color: var(--accent); border-radius: 50%;
		animation: spin 0.7s linear infinite; flex-shrink: 0;
	}

	.branch-count {
		font-size: 11px; color: var(--text-dim); padding: 8px 16px 4px;
		flex-shrink: 0;
	}

	.branch-list { flex: 1; overflow-y: auto; display: flex; flex-direction: column; }

	.branch-row {
		display: flex; align-items: center; gap: 10px;
		padding: 10px 16px; background: transparent; border: none;
		border-bottom: 1px solid var(--border); color: var(--text-primary);
		font-size: 13px; cursor: pointer; text-align: left; width: 100%;
		transition: background var(--transition-fast);
	}
	.branch-row:hover { background: var(--bg-elevated); }
	.branch-row:last-child { border-bottom: none; }

	:global(.branch-icon) { color: var(--text-dim); flex-shrink: 0; }

	.branch-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); font-size: 12px; }

	.chevron { font-size: 18px; color: var(--text-dim); flex-shrink: 0; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
