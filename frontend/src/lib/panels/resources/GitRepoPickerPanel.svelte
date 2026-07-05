<script lang="ts">
	import { onMount } from 'svelte';
	import { Search, Lock, Globe, Loader2 } from '@lucide/svelte';

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

<div class="picker-wrap">
	<!-- Search bar -->
	<div class="search-bar">
		{#if searching}
			<Loader2 size={14} class="search-spinner" />
		{:else}
			<Search size={14} class="search-icon" />
		{/if}
		<input
			class="search-input"
			type="text"
			placeholder="Search {providerLabel} repositories…"
			bind:value={search}
			autofocus
		/>
		{#if search.trim()}
			<button class="clear-btn" onclick={() => search = ''}>✕</button>
		{/if}
	</div>

	<!-- Body -->
	{#if initialLoading}
		<div class="state-msg">
			<div class="spinner"></div>
			<span>Fetching repositories…</span>
		</div>

	{:else if initialError}
		<div class="state-msg error">{initialError}</div>

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
			<div class="state-msg">
				{search.trim() ? `No repositories found on ${providerLabel} matching "${search.trim()}".` : 'No repositories found.'}
			</div>
		{:else}
			<div class="repo-list">
				{#each displayed as repo (repo.fullName)}
					<button type="button" class="repo-row" onclick={() => onSelect(repo)}>
						<span class="repo-vis">
							{#if repo.isPrivate}
								<Lock size={12} />
							{:else}
								<Globe size={12} />
							{/if}
						</span>
						<span class="repo-name">{repo.fullName}</span>
						<span class="chevron">›</span>
					</button>
				{/each}
			</div>
		{/if}
	{/if}
</div>

<style>
	.picker-wrap { display: flex; flex-direction: column; height: 100%; overflow: hidden; }

	/* ── Search bar ── */
	.search-bar {
		display: flex;
		align-items: center;
		gap: 8px;
		padding: 12px 16px;
		border-bottom: 1px solid var(--border);
		flex-shrink: 0;
	}

	:global(.search-icon)    { color: var(--text-dim); flex-shrink: 0; }
	:global(.search-spinner) { color: var(--accent); flex-shrink: 0; animation: spin 0.7s linear infinite; }

	.search-input {
		flex: 1;
		background: transparent;
		border: none;
		outline: none;
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
	}
	.search-input::placeholder { color: var(--text-dim); }

	.clear-btn {
		background: none;
		border: none;
		cursor: pointer;
		color: var(--text-dim);
		font-size: 11px;
		padding: 2px 4px;
		border-radius: 3px;
		line-height: 1;
		flex-shrink: 0;
	}
	.clear-btn:hover { color: var(--text-primary); background: var(--bg-surface); }

	/* ── Context label ── */
	.list-context {
		padding: 6px 16px 4px;
		flex-shrink: 0;
	}

	.ctx-muted {
		font-size: 11px;
		color: var(--text-dim);
		line-height: 1.5;
	}
	.ctx-muted strong { color: var(--text-muted); font-weight: 600; }
	.ctx-error { font-size: 11px; color: var(--accent-red, #ef4444); }

	/* ── States ── */
	.state-msg {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 32px 16px;
		font-size: 13px;
		color: var(--text-muted);
		justify-content: center;
		text-align: center;
	}
	.state-msg.error { color: var(--accent-red, #ef4444); }

	.spinner {
		width: 16px;
		height: 16px;
		border: 2px solid var(--border);
		border-top-color: var(--accent);
		border-radius: 50%;
		animation: spin 0.7s linear infinite;
		flex-shrink: 0;
	}

	/* ── Repo list ── */
	.repo-list { flex: 1; overflow-y: auto; display: flex; flex-direction: column; }

	.repo-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 10px 16px;
		background: transparent;
		border: none;
		border-bottom: 1px solid var(--border);
		color: var(--text-primary);
		font-size: 13px;
		cursor: pointer;
		text-align: left;
		width: 100%;
		transition: background var(--transition-fast);
	}
	.repo-row:hover { background: var(--bg-elevated); }
	.repo-row:last-child { border-bottom: none; }

	.repo-vis {
		color: var(--text-dim);
		flex-shrink: 0;
		display: flex;
		align-items: center;
	}

	.repo-name {
		flex: 1;
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		font-family: var(--font-mono);
		font-size: 12px;
	}

	.chevron { font-size: 18px; color: var(--text-dim); flex-shrink: 0; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
