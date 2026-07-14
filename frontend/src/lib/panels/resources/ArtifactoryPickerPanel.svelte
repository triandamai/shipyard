<script lang="ts">
	import { orgStore } from '$lib/stores/org.store';
	import { api } from '$lib/api/client';
	import { Package, Search, X } from '@lucide/svelte';

	interface Artifact {
		id:             string;
		namespace_id:   string;
		namespace_slug: string;
		repo:           string;
		tag:            string;
		kind:           string;
		size_bytes:     number;
		pushed_at:      string;
	}

	interface Props {
		/** Restrict to a single kind. Omit to show all. */
		kind?:     'docker_image' | 'static_bundle' | 'edge_function' | null;
		onSelect:  (artifact: Artifact) => void;
	}

	let { kind = null, onSelect }: Props = $props();

	let orgId   = $derived($orgStore.activeOrg?.id ?? '');
	let query   = $state('');
	let results = $state<Artifact[]>([]);
	let loading = $state(false);
	let timer:  ReturnType<typeof setTimeout> | null = null;

	const KIND_META: Record<string, { label: string; color: string }> = {
		docker_image:  { label: 'Image',   color: '#3b82f6' },
		static_bundle: { label: 'Static',  color: '#8b5cf6' },
		edge_function: { label: 'Edge Fn', color: '#f59e0b' },
		build_cache:   { label: 'Cache',   color: '#6b7280' },
	};

	function fmtBytes(n: number) {
		if (!n) return '0 B';
		if (n < 1024) return `${n} B`;
		if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
		if (n < 1024 ** 3) return `${(n / 1024 / 1024).toFixed(1)} MB`;
		return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
	}

	function timeAgo(d: string) {
		const s = Math.floor((Date.now() - new Date(d).getTime()) / 1000);
		if (s < 60)    return 'just now';
		if (s < 3600)  return `${Math.floor(s / 60)}m ago`;
		if (s < 86400) return `${Math.floor(s / 3600)}h ago`;
		return `${Math.floor(s / 86400)}d ago`;
	}

	async function search() {
		if (!orgId) return;
		loading = true;
		const params = new URLSearchParams({ q: query });
		if (kind) params.set('kind', kind);
		const res = await api.get(`/orgs/${orgId}/registry/artifacts/search?${params}`);
		results = res.data ?? [];
		loading = false;
	}

	function onInput() {
		if (timer) clearTimeout(timer);
		timer = setTimeout(search, 280);
	}

	function clearQuery() {
		query = '';
		results = [];
	}

	// Load all on mount (empty query = list recent)
	$effect(() => {
		if (orgId) search();
	});
</script>

<div class="picker">
	<div class="search-bar">
		<Search size={14} class="search-icon" />
		<input
			class="search-input"
			type="text"
			placeholder="Search by repo, namespace slug…"
			bind:value={query}
			oninput={onInput}
			autofocus
		/>
		{#if query}
			<button class="clear-btn" type="button" onclick={clearQuery} aria-label="Clear">
				<X size={13} />
			</button>
		{/if}
	</div>

	{#if kind}
		<div class="kind-badge" style="--kc:{KIND_META[kind]?.color ?? '#6b7280'}">
			Filtered: {KIND_META[kind]?.label ?? kind}
		</div>
	{/if}

	<div class="results">
		{#if loading}
			<div class="state-msg">
				<div class="spinner"></div>
				<span>Searching…</span>
			</div>
		{:else if results.length === 0}
			<div class="state-msg empty">
				<Package size={28} />
				<span>{query ? 'No artifacts matched your search.' : 'No artifacts found in this org.'}</span>
			</div>
		{:else}
			{#each results as art (art.id)}
				{@const meta = KIND_META[art.kind] ?? { label: art.kind, color: '#6b7280' }}
				<button class="result-row" type="button" onclick={() => onSelect(art)}>
					<div class="row-left">
						<span class="kind-dot" style="background:{meta.color}"></span>
						<div class="row-name">
							<span class="ns-slug">{art.namespace_slug}</span>
							<span class="sep">/</span>
							<span class="repo">{art.repo}</span>
							<span class="tag">:{art.tag}</span>
						</div>
					</div>
					<div class="row-right">
						<span class="kind-pill" style="--kc:{meta.color}">{meta.label}</span>
						<span class="meta-text">{fmtBytes(art.size_bytes)}</span>
						<span class="meta-text muted">{timeAgo(art.pushed_at)}</span>
					</div>
				</button>
			{/each}
		{/if}
	</div>
</div>

<style>
.picker {
	display: flex;
	flex-direction: column;
	height: 100%;
	overflow: hidden;
}

.search-bar {
	display: flex;
	align-items: center;
	gap: 8px;
	padding: 10px 14px;
	border-bottom: 1px solid var(--border);
	background: var(--surface);
	flex-shrink: 0;
}
:global(.search-icon) { color: var(--text-muted); flex-shrink: 0; }

.search-input {
	flex: 1;
	background: none;
	border: none;
	outline: none;
	font-size: 13px;
	color: var(--text-primary);
	font-family: var(--font-sans);
}
.search-input::placeholder { color: var(--text-muted); }

.clear-btn {
	display: flex; align-items: center; justify-content: center;
	width: 20px; height: 20px; border-radius: 50%;
	border: none; background: var(--surface-2); color: var(--text-muted);
	cursor: pointer; flex-shrink: 0;
}
.clear-btn:hover { color: var(--text-primary); }

.kind-badge {
	display: inline-flex; align-items: center;
	margin: 8px 14px 0;
	font-size: 11px; font-weight: 600; padding: 3px 10px; border-radius: 999px;
	background: color-mix(in srgb, var(--kc) 12%, transparent);
	color: var(--kc);
	border: 1px solid color-mix(in srgb, var(--kc) 25%, transparent);
	width: fit-content;
	flex-shrink: 0;
}

.results {
	flex: 1;
	overflow-y: auto;
	display: flex;
	flex-direction: column;
}

.state-msg {
	display: flex; flex-direction: column; align-items: center; gap: 10px;
	padding: 48px 20px; color: var(--text-muted); font-size: 13px;
	text-align: center;
}
.state-msg.empty { color: var(--text-muted); }
.spinner {
	width: 20px; height: 20px; border: 2px solid var(--border);
	border-top-color: var(--accent); border-radius: 50%;
	animation: spin 0.7s linear infinite;
}

.result-row {
	display: flex; align-items: center; justify-content: space-between;
	gap: 12px; padding: 10px 14px;
	background: none; border: none; border-bottom: 1px solid var(--border);
	cursor: pointer; text-align: left; width: 100%;
	transition: background var(--transition-fast);
}
.result-row:hover { background: var(--surface-2); }
.result-row:last-child { border-bottom: none; }

.row-left { display: flex; align-items: center; gap: 10px; min-width: 0; }
.kind-dot { width: 8px; height: 8px; border-radius: 50%; flex-shrink: 0; }

.row-name {
	display: flex; align-items: baseline; gap: 2px;
	font-family: var(--font-mono); font-size: 12px;
	overflow: hidden; white-space: nowrap; text-overflow: ellipsis;
}
.ns-slug { color: var(--text-muted); }
.sep { color: var(--border); }
.repo { color: var(--text-primary); font-weight: 600; }
.tag { color: var(--text-muted); }

.row-right {
	display: flex; align-items: center; gap: 8px; flex-shrink: 0;
}
.kind-pill {
	font-size: 10px; font-weight: 600; padding: 2px 7px; border-radius: 999px;
	background: color-mix(in srgb, var(--kc) 12%, transparent);
	color: var(--kc);
	border: 1px solid color-mix(in srgb, var(--kc) 25%, transparent);
	white-space: nowrap;
}
.meta-text { font-size: 11px; color: var(--text-muted); white-space: nowrap; }
.meta-text.muted { color: color-mix(in srgb, var(--text-muted) 60%, transparent); }

@keyframes spin { to { transform: rotate(360deg); } }
</style>
