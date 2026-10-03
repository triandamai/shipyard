<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import {
		PageHeader,
		SectionLabel,
		DataTable,
		StatusDot,
		Badge,
		Card,
		Textarea,
		Button,
		InlineAlert,
		EmptyState,
		Skeleton
	} from '$lib/components/ui';

	type SwarmNode = { id: string; hostname: string; role: string; status: string; availability: string; engine_version: string | null; addr: string | null };

	let config = $state<Record<string, unknown>>({});
	let edits = $state<Record<string, string>>({});
	let saving = $state<string | null>(null);
	let saved = $state<string | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let saveErrors = $state<Record<string, string>>({});

	let swarmNodes = $state<SwarmNode[]>([]);
	let swarmLoading = $state(true);
	let swarmError = $state<string | null>(null);

	onMount(async () => {
		const [configRes, swarmRes] = await Promise.all([
			api.getSystemConfig(),
			api.getAdminSwarmNodes(),
		]);

		if (configRes.data) {
			config = configRes.data;
			for (const [k, v] of Object.entries(configRes.data)) {
				edits[k] = JSON.stringify(v, null, 2);
			}
		} else {
			error = configRes.error?.message ?? 'Failed to load system config';
		}
		loading = false;

		if (swarmRes.data) {
			swarmNodes = swarmRes.data;
		} else {
			swarmError = swarmRes.error?.message ?? 'Failed to load swarm nodes';
		}
		swarmLoading = false;
	});

	async function save(key: string) {
		saving = key;
		saveErrors = { ...saveErrors, [key]: '' };
		let parsed: unknown;
		try {
			parsed = JSON.parse(edits[key]);
		} catch {
			saveErrors = { ...saveErrors, [key]: 'Invalid JSON' };
			saving = null;
			return;
		}
		const res = await api.patchSystemConfig(key, parsed);
		if (res.error) {
			saveErrors = { ...saveErrors, [key]: res.error.message };
		} else {
			saved = key;
			setTimeout(() => { if (saved === key) saved = null; }, 2500);
		}
		saving = null;
	}

	function typeOf(v: unknown): string {
		if (v === null) return 'null';
		if (Array.isArray(v)) return 'array';
		return typeof v;
	}

	// Badge tone per value type — Task 53's recipe (config page), reused
	// verbatim: same semantics as the old hand-rolled typeColor map
	// (string=ok/green, number=accent/blue, boolean=warn/yellow,
	// object/array/null=neutral), expressed as Badge's tone enum.
	const typeTone: Record<string, 'green' | 'blue' | 'yellow' | 'neutral'> = {
		string: 'green',
		number: 'blue',
		boolean: 'yellow',
		object: 'neutral',
		array: 'neutral',
		null: 'neutral',
	};

	// Existing auto-size calculation, unchanged — now drives Textarea's `rows`
	// prop instead of a fixed number / ad hoc sizing. (Task 53 recipe.)
	function rowsFor(key: string): number {
		return Math.min(Math.max(edits[key]?.split('\n').length ?? 1, 1), 8);
	}

	// Existing ⌘/Ctrl+Enter save keybinding, unchanged. Attached to the editor
	// wrapper rather than the Textarea directly (the shared Textarea doesn't
	// forward arbitrary event props) — keydown bubbles up from the textarea.
	// (Task 53 recipe.)
	function onEditorKeydown(e: KeyboardEvent, key: string) {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) save(key);
	}

	// Same binary status semantics as the old badge-ok/badge-err mapping
	// (status === 'ready' is the only "good" state; anything else is shown as
	// failed) — now expressed as StatusDot's status enum. Note this page's
	// original logic was two-way (ok/err), unlike infra's three-way
	// ready/down-disconnected/other mapping over a different, nested data
	// shape — each page's own semantics are preserved unchanged.
	function nodeStatusDot(status: string): 'running' | 'failed' {
		return status === 'ready' ? 'running' : 'failed';
	}
</script>

<div class="page-wrap">
	<!-- Swarm Nodes -->
	<div class="sec-head">
		<SectionLabel>Swarm Nodes</SectionLabel>
		{#if !swarmLoading}<Badge tone="neutral">{swarmNodes.length}</Badge>{/if}
	</div>
	{#if swarmLoading}
		<div class="skel-stack"><Skeleton variant="row" /><Skeleton variant="row" /></div>
	{:else if swarmError}
		<InlineAlert tone="error">{swarmError}</InlineAlert>
	{:else}
		<DataTable
			items={swarmNodes}
			rowKey={(n) => n.id}
			searchFields={['hostname', 'id']}
			columns={[
				{ key: 'id', label: 'Node ID', width: '14%' },
				{ key: 'hostname', label: 'Hostname', width: '20%' },
				{ key: 'ip', label: 'IP', width: '14%' },
				{ key: 'role', label: 'Role', width: '10%' },
				{ key: 'state', label: 'State', width: '14%' },
				{ key: 'availability', label: 'Availability', width: '14%' },
				{ key: 'engine', label: 'Engine', width: '14%' }
			]}
			emptyMessage="No swarm nodes found."
		>
			{#snippet row(node)}
				<tr>
					<td class="mono muted">{node.id.slice(0, 12)}</td>
					<td class="mono">{node.hostname}</td>
					<td class="mono cell">{node.addr ?? '—'}</td>
					<td><Badge tone={node.role === 'manager' ? 'blue' : 'neutral'}>{node.role}</Badge></td>
					<td><StatusDot status={nodeStatusDot(node.status)} /> {node.status}</td>
					<td class="cell">{node.availability}</td>
					<td class="mono muted">{node.engine_version ?? '—'}</td>
				</tr>
			{/snippet}
		</DataTable>
	{/if}

	<PageHeader title="System Config" subtitle="Platform-wide JSONB settings. Changes apply immediately.">
		{#snippet actions()}
			<Badge tone="yellow">Danger zone</Badge>
		{/snippet}
	</PageHeader>

	{#if loading}
		<div class="cfg-list">
			{#each [0, 1, 2] as _}
				<Card padding="14px">
					<Skeleton variant="text" width="140px" />
					<div class="cfg-sk-gap"></div>
					<Skeleton variant="row" height="60px" />
				</Card>
			{/each}
		</div>
	{:else if error}
		<InlineAlert tone="error">{error}</InlineAlert>
	{:else if Object.keys(config).length === 0}
		<EmptyState message="No configuration entries." />
	{:else}
		<div class="cfg-list">
			{#each Object.entries(config) as [key, rawVal]}
				{@const t = typeOf(rawVal)}
				<Card padding="14px">
					<div class="cfg-head">
						<code class="cfg-key">{key}</code>
						<Badge tone={typeTone[t] ?? 'neutral'}>{t}</Badge>
					</div>
					<!-- svelte-ignore a11y_no_static_element_interactions -->
					<div class="cfg-editor" onkeydown={(e) => onEditorKeydown(e, key)}>
						<Textarea bind:value={edits[key]} rows={rowsFor(key)} />
					</div>
					{#if saveErrors[key]}
						<InlineAlert tone="error">{saveErrors[key]}</InlineAlert>
					{/if}
					<div class="cfg-foot">
						<span class="cfg-hint">⌘+Enter</span>
						<Button variant="secondary" onclick={() => save(key)} disabled={saving === key}>
							{#if saved === key}Saved{:else if saving === key}Saving…{:else}Save{/if}
						</Button>
					</div>
				</Card>
			{/each}
		</div>
	{/if}
</div>

<style>
	.page-wrap { max-width: 900px; margin: 0 auto; padding: 40px 36px; }

	.sec-head { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
	.sec-head :global(.ui-section-label) { margin-bottom: 0; }

	.skel-stack { display: flex; flex-direction: column; gap: 8px; margin-bottom: 36px; }

	.mono { font-family: var(--font-mono); color: var(--text-primary); }
	.muted { color: var(--text-muted); }
	.cell { color: var(--text-secondary); }

	/* Swarm Nodes table sits above the System Config header; give it the same
	   bottom spacing the old `.section { margin-bottom: 36px }` provided. */
	.page-wrap > :global(.ui-data-table) { margin-bottom: 36px; }

	.cfg-list { display: flex; flex-direction: column; gap: 10px; }
	.cfg-sk-gap { height: 10px; }

	.cfg-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 10px; }
	.cfg-key { font-size: 12.5px; font-weight: 600; color: var(--text-primary); font-family: var(--font-mono); word-break: break-all; }

	.cfg-editor :global(textarea) { font-family: var(--font-mono); }

	.cfg-foot { display: flex; align-items: center; justify-content: flex-end; gap: 10px; margin-top: 10px; }
	.cfg-hint { font-size: 10.5px; color: var(--text-dim); font-family: var(--font-mono); }
</style>
