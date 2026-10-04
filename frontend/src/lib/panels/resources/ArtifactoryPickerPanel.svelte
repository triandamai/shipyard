<script lang="ts">
	import { orgStore } from '$lib/stores/org.store';
	import { api } from '$lib/api/client';
	import { onMount } from 'svelte';
	import { SearchInput, ActivityList, ListRow, Badge, Button, Spinner, EmptyState } from '$lib/components/ui';
	import { Package, X } from '@lucide/svelte';

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

	type KindTone = 'blue' | 'green' | 'yellow' | 'red';
	type BadgeTone = 'blue' | 'green' | 'yellow' | 'red' | 'neutral';
	const KIND_META: Record<string, { label: string; tone: KindTone; badge: BadgeTone }> = {
		docker_image:  { label: 'Image',   tone: 'blue',   badge: 'blue' },
		static_bundle: { label: 'Static',  tone: 'green',  badge: 'green' },
		edge_function: { label: 'Edge Fn', tone: 'yellow', badge: 'yellow' },
		build_cache:   { label: 'Cache',   tone: 'blue',   badge: 'neutral' },
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

	// SearchInput has no input event: debounce on typed changes only
	// (a programmatic clear never triggered a search in the old input either).
	let lastQuery = '';
	$effect(() => {
		const q = query;
		if (q === lastQuery) return;
		lastQuery = q;
		onInput();
	});

	function clearQuery() {
		lastQuery = '';
		query = '';
		results = [];
	}

	let pickerEl: HTMLDivElement | undefined = $state();
	// Focus the search box on open (the old input used the autofocus attribute)
	onMount(() => { pickerEl?.querySelector('input')?.focus(); });

	// Load all on mount (empty query = list recent)
	$effect(() => {
		if (orgId) search();
	});
</script>

<div class="picker" bind:this={pickerEl}>
	<div class="search-bar">
		<div class="search-grow">
			<SearchInput bind:value={query} placeholder="Search by repo, namespace slug…" />
		</div>
		{#if query}
			<Button variant="ghost" size="icon" aria-label="Clear" onclick={clearQuery}><X size={13} /></Button>
		{/if}
	</div>

	{#if kind}
		<div class="kind-badge">
			<Badge tone={KIND_META[kind]?.badge ?? 'neutral'}>Filtered: {KIND_META[kind]?.label ?? kind}</Badge>
		</div>
	{/if}

	<div class="results">
		{#if loading}
			<div class="state-msg">
				<Spinner size={20} />
				<span>Searching…</span>
			</div>
		{:else if results.length === 0}
			<EmptyState message={query ? 'No artifacts matched your search.' : 'No artifacts found in this org.'}>
				{#snippet icon()}<Package size={28} />{/snippet}
			</EmptyState>
		{:else}
			<div class="list">
				<ActivityList>
					{#each results as art (art.id)}
						{@const meta = KIND_META[art.kind] ?? { label: art.kind, tone: 'blue' as KindTone, badge: 'neutral' as BadgeTone }}
						<ListRow
							onclick={() => onSelect(art)}
							title="{art.namespace_slug}/{art.repo}:{art.tag}"
							meta="{fmtBytes(art.size_bytes)} · {timeAgo(art.pushed_at)}"
							iconTone={meta.tone}
						>
							{#snippet icon()}<Package size={14} />{/snippet}
							{#snippet trailing()}<Badge tone={meta.badge}>{meta.label}</Badge>{/snippet}
						</ListRow>
					{/each}
				</ActivityList>
			</div>
		{/if}
	</div>
</div>

<style>
	.picker { display: flex; flex-direction: column; height: 100%; overflow: hidden; }
	.search-bar {
		display: flex; align-items: center; gap: 8px;
		padding: 10px 14px; border-bottom: 1px solid var(--border); flex-shrink: 0;
	}
	.search-grow { flex: 1; min-width: 0; }
	.kind-badge { margin: 8px 14px 0; flex-shrink: 0; }
	.results { flex: 1; overflow-y: auto; display: flex; flex-direction: column; }
	.state-msg {
		display: flex; flex-direction: column; align-items: center; gap: 10px;
		padding: 48px 20px; color: var(--text-muted); font-size: 13px; text-align: center;
	}
	.list { padding: 0 14px 14px; }
</style>
