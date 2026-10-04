<script lang="ts">
	import { onMount } from 'svelte';
	import { Lock, Globe, X, ChevronRight } from '@lucide/svelte';
	import { SearchInput, ActivityList, ListRow, Button, Spinner, EmptyState, InlineAlert } from '$lib/components/ui';

	interface RepoItem {
		name: string;
		fullName: string;
		cloneUrl: string;
		isPrivate: boolean;
	}

	interface Props {
		provider: string;
		token: string;
		onSelect: (repo: RepoItem) => void;
	}

	let { provider, token, onSelect }: Props = $props();

	let wrapEl: HTMLDivElement | undefined = $state();

	// ── State ───────────────────────────────────────────────────────────────────
	let initialRepos   = $state<RepoItem[]>([]);  // first 20 on mount
	let searchResults  = $state<RepoItem[]>([]);  // results of the live API search

	let initialLoading = $state(true);
	let searching      = $state(false);
	let initialError   = $state('');
	let searchError    = $state('');
	let search         = $state('');

	// GitHub only — needed to scope the search to the authenticated user's repos
	let githubUsername = $state('');

	// ── Derived ─────────────────────────────────────────────────────────────────
	let displayed = $derived(search.trim() ? searchResults : initialRepos);
	let providerLabel = $derived(
		provider === 'github' ? 'GitHub' :
		provider === 'gitlab' ? 'GitLab' :
		provider === 'bitbucket' ? 'Bitbucket' : provider
	);

	// ── Search debounce ─────────────────────────────────────────────────────────
	let searchTimer: ReturnType<typeof setTimeout> | null = null;

	$effect(() => {
		const q = search.trim();

		if (searchTimer) { clearTimeout(searchTimer); searchTimer = null; }

		if (!q) {
			searchResults = [];
			searching = false;
			searchError = '';
			return;
		}

		searching = true;
		searchError = '';
		searchTimer = setTimeout(() => doSearch(q), 350);
	});

	async function doSearch(q: string) {
		try {
			if (provider === 'github') {
				// Scope to the authenticated user's repos if we have their username.
				// The GitHub Search API returns all matching repos; adding user:{login} filters
				// to repos they own or collaborate on.
				const scope = githubUsername ? `+user:${githubUsername}` : '';
				const res = await fetch(
					`https://api.github.com/search/repositories?q=${encodeURIComponent(q)}${scope}&per_page=20&sort=updated`,
					{ headers: { Authorization: `token ${token}`, Accept: 'application/vnd.github+json' } }
				);
				if (!res.ok) throw new Error(`GitHub search returned ${res.status}`);
				const data = await res.json();
				searchResults = (data.items ?? []).map((r: any) => ({
					name: r.name,
					fullName: r.full_name,
					cloneUrl: r.clone_url,
					isPrivate: r.private,
				}));

			} else if (provider === 'gitlab') {
				// GitLab's projects endpoint has native `search` support
				const res = await fetch(
					`https://gitlab.com/api/v4/projects?membership=true&search=${encodeURIComponent(q)}&per_page=20&order_by=last_activity_at`,
					{ headers: { Authorization: `Bearer ${token}` } }
				);
				if (!res.ok) throw new Error(`GitLab search returned ${res.status}`);
				const data = await res.json();
				searchResults = data.map((r: any) => ({
					name: r.name,
					fullName: r.path_with_namespace,
					cloneUrl: r.http_url_to_repo,
					isPrivate: r.visibility !== 'public',
				}));

			} else if (provider === 'bitbucket') {
				// Bitbucket's repositories endpoint supports CQL-style `q` param
				const res = await fetch(
					`https://api.bitbucket.org/2.0/repositories?role=member&pagelen=20&q=name~"${encodeURIComponent(q)}"`,
					{ headers: { Authorization: `Basic ${btoa(token)}` } }
				);
				if (!res.ok) throw new Error(`Bitbucket search returned ${res.status}`);
				const data = await res.json();
				searchResults = (data.values ?? []).map((r: any) => ({
					name: r.name,
					fullName: r.full_name,
					cloneUrl: r.links?.clone?.find((c: any) => c.name === 'https')?.href ?? '',
					isPrivate: r.is_private,
				}));
			}
		} catch (e) {
			searchError = e instanceof Error ? e.message : String(e);
			searchResults = [];
		} finally {
			searching = false;
		}
	}

	// Focus the search box on open (the old input used the autofocus attribute)
	onMount(() => { wrapEl?.querySelector('input')?.focus(); });

	// ── Initial load (20 most-recently-updated repos) ──────────────────────────
	onMount(async () => {
		initialLoading = true;
		initialError = '';
		try {
			if (provider === 'github') {
				// Fetch username and first-page repos in parallel
				const headers = { Authorization: `token ${token}`, Accept: 'application/vnd.github+json' };
				const [userRes, reposRes] = await Promise.all([
					fetch('https://api.github.com/user', { headers }),
					fetch(
						'https://api.github.com/user/repos?per_page=20&sort=updated&affiliation=owner,collaborator,organization_member',
						{ headers }
					),
				]);
				if (!userRes.ok)  throw new Error(`GitHub API: ${userRes.status} — check token has repo scope`);
				if (!reposRes.ok) throw new Error(`GitHub API: ${reposRes.status} — check token has repo scope`);
				const userData  = await userRes.json();
				const reposData = await reposRes.json();
				githubUsername = userData.login ?? '';
				initialRepos = reposData.map((r: any) => ({
					name: r.name,
					fullName: r.full_name,
					cloneUrl: r.clone_url,
					isPrivate: r.private,
				}));

			} else if (provider === 'gitlab') {
				const res = await fetch(
					'https://gitlab.com/api/v4/projects?membership=true&per_page=20&order_by=last_activity_at',
					{ headers: { Authorization: `Bearer ${token}` } }
				);
				if (!res.ok) throw new Error(`GitLab API: ${res.status} — check token has read_repository scope`);
				const data = await res.json();
				initialRepos = data.map((r: any) => ({
					name: r.name,
					fullName: r.path_with_namespace,
					cloneUrl: r.http_url_to_repo,
					isPrivate: r.visibility !== 'public',
				}));

			} else if (provider === 'bitbucket') {
				const res = await fetch(
					'https://api.bitbucket.org/2.0/repositories?role=member&pagelen=20',
					{ headers: { Authorization: `Basic ${btoa(token)}` } }
				);
				if (!res.ok) throw new Error(`Bitbucket API: ${res.status} — token should be username:apppassword`);
				const data = await res.json();
				initialRepos = (data.values ?? []).map((r: any) => ({
					name: r.name,
					fullName: r.full_name,
					cloneUrl: r.links?.clone?.find((c: any) => c.name === 'https')?.href ?? '',
					isPrivate: r.is_private,
				}));
			}
		} catch (e) {
			initialError = e instanceof Error ? e.message : String(e);
		} finally {
			initialLoading = false;
		}
	});
</script>

<div class="picker-wrap" bind:this={wrapEl}>
	<!-- Search bar -->
	<div class="search-bar">
		<div class="search-grow">
			<SearchInput bind:value={search} placeholder="Search {providerLabel} repositories…" />
		</div>
		{#if searching}
			<Spinner size={14} />
		{/if}
		{#if search.trim()}
			<Button variant="ghost" size="icon" aria-label="Clear search" onclick={() => search = ''}><X size={12} /></Button>
		{/if}
	</div>

	<!-- Body -->
	{#if initialLoading}
		<div class="state-msg">
			<Spinner size={16} />
			<span>Fetching repositories…</span>
		</div>

	{:else if initialError}
		<div class="state-pad" role="alert"><InlineAlert tone="error">{initialError}</InlineAlert></div>

	{:else}
		<!-- Context label -->
		<div class="list-context">
			{#if search.trim()}
				{#if searching}
					<span class="ctx-muted">Searching {providerLabel} for "<strong>{search.trim()}</strong>"…</span>
				{:else if searchError}
					<span class="ctx-error">{searchError}</span>
				{:else}
					<span class="ctx-muted">{searchResults.length} result{searchResults.length === 1 ? '' : 's'} on {providerLabel} for "<strong>{search.trim()}</strong>"</span>
				{/if}
			{:else}
				<span class="ctx-muted">20 most recently updated — type to search all</span>
			{/if}
		</div>

		{#if displayed.length === 0 && !searching}
			<EmptyState message={search.trim() ? `No repositories found on ${providerLabel} matching "${search.trim()}".` : 'No repositories found.'} />
		{:else}
			<div class="repo-list">
				<ActivityList>
					{#each displayed as repo (repo.fullName)}
						<button type="button" class="pick-row" onclick={() => onSelect(repo)}>
							<ListRow title={repo.fullName} meta={repo.isPrivate ? 'Private' : 'Public'} iconTone={repo.isPrivate ? 'yellow' : 'blue'}>
								{#snippet icon()}
									{#if repo.isPrivate}<Lock size={14} />{:else}<Globe size={14} />{/if}
								{/snippet}
								{#snippet trailing()}<ChevronRight size={16} />{/snippet}
							</ListRow>
						</button>
					{/each}
				</ActivityList>
			</div>
		{/if}
	{/if}
</div>

<style>
	.picker-wrap { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

	.search-bar {
		display: flex; align-items: center; gap: 8px;
		padding: 12px 16px; border-bottom: 1px solid var(--border); flex-shrink: 0;
	}
	.search-grow { flex: 1; min-width: 0; }

	.list-context { padding: 6px 16px 4px; flex-shrink: 0; }
	.ctx-muted { font-size: 11px; color: var(--text-dim); line-height: 1.5; }
	.ctx-muted strong { color: var(--text-muted); font-weight: 600; }
	.ctx-error { font-size: 11px; color: var(--accent-red); }

	.state-msg {
		display: flex; align-items: center; gap: 10px;
		padding: 32px 16px; font-size: 13px; color: var(--text-muted);
		justify-content: center; text-align: center;
	}
	.state-pad { padding: 16px; }

	.repo-list { flex: 1; overflow-y: auto; padding: 0 16px 16px; }

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
