<script lang="ts">
	import { onMount } from 'svelte';
	import { Search, Lock, Globe } from '@lucide/svelte';

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

	let repos = $state<RepoItem[]>([]);
	let loading = $state(true);
	let error = $state('');
	let search = $state('');

	let filtered = $derived(
		search.trim()
			? repos.filter(r => r.fullName.toLowerCase().includes(search.toLowerCase()))
			: repos
	);

	onMount(async () => {
		loading = true;
		error = '';
		try {
			if (provider === 'github') {
				const res = await fetch(
					'https://api.github.com/user/repos?per_page=100&sort=updated&affiliation=owner,collaborator,organization_member',
					{ headers: { Authorization: `token ${token}`, Accept: 'application/vnd.github+json' } }
				);
				if (!res.ok) throw new Error(`GitHub API returned ${res.status} — check your token has repo scope`);
				const data = await res.json();
				repos = data.map((r: { name: string; full_name: string; clone_url: string; private: boolean }) => ({
					name: r.name,
					fullName: r.full_name,
					cloneUrl: r.clone_url,
					isPrivate: r.private,
				}));
			} else if (provider === 'gitlab') {
				const res = await fetch(
					'https://gitlab.com/api/v4/projects?membership=true&per_page=100&order_by=last_activity_at',
					{ headers: { Authorization: `Bearer ${token}` } }
				);
				if (!res.ok) throw new Error(`GitLab API returned ${res.status} — check your token has read_repository scope`);
				const data = await res.json();
				repos = data.map((r: { name: string; path_with_namespace: string; http_url_to_repo: string; visibility: string }) => ({
					name: r.name,
					fullName: r.path_with_namespace,
					cloneUrl: r.http_url_to_repo,
					isPrivate: r.visibility !== 'public',
				}));
			} else if (provider === 'bitbucket') {
				const res = await fetch(
					'https://api.bitbucket.org/2.0/repositories?role=member&pagelen=100',
					{ headers: { Authorization: `Basic ${btoa(token)}` } }
				);
				if (!res.ok) throw new Error(`Bitbucket API returned ${res.status} — token should be username:apppassword`);
				const data = await res.json();
				repos = (data.values ?? []).map((r: { name: string; full_name: string; links: { clone: Array<{ name: string; href: string }> }; is_private: boolean }) => ({
					name: r.name,
					fullName: r.full_name,
					cloneUrl: r.links?.clone?.find(c => c.name === 'https')?.href ?? '',
					isPrivate: r.is_private,
				}));
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
			placeholder="Search repositories…"
			bind:value={search}
		/>
	</div>

	{#if loading}
		<div class="state-msg">
			<div class="spinner"></div>
			<span>Fetching repositories…</span>
		</div>
	{:else if error}
		<div class="state-msg error">{error}</div>
	{:else if filtered.length === 0}
		<div class="state-msg">
			{search.trim() ? 'No repositories match your search.' : 'No repositories found.'}
		</div>
	{:else}
		<div class="repo-count">{filtered.length} {filtered.length === 1 ? 'repository' : 'repositories'}</div>
		<div class="repo-list">
			{#each filtered as repo (repo.fullName)}
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

	.repo-count {
		font-size: 11px; color: var(--text-dim); padding: 8px 16px 4px;
		flex-shrink: 0;
	}

	.repo-list { flex: 1; overflow-y: auto; display: flex; flex-direction: column; }

	.repo-row {
		display: flex; align-items: center; gap: 10px;
		padding: 10px 16px; background: transparent; border: none;
		border-bottom: 1px solid var(--border); color: var(--text-primary);
		font-size: 13px; cursor: pointer; text-align: left; width: 100%;
		transition: background var(--transition-fast);
	}
	.repo-row:hover { background: var(--bg-elevated); }
	.repo-row:last-child { border-bottom: none; }

	.repo-vis { color: var(--text-dim); flex-shrink: 0; display: flex; align-items: center; }

	.repo-name { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-family: var(--font-mono); font-size: 12px; }

	.chevron { font-size: 18px; color: var(--text-dim); flex-shrink: 0; }

	@keyframes spin { to { transform: rotate(360deg); } }
</style>
