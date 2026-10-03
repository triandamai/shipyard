<script lang="ts">
	import { onMount } from 'svelte';
	import { api } from '$lib/api/client';
	import { PageHeader, Card, Badge, Textarea, Toggle, Button, InlineAlert, EmptyState, Skeleton } from '$lib/components/ui';

	let config = $state<Record<string, unknown>>({});
	let edits = $state<Record<string, string>>({});
	let saving = $state<string | null>(null);
	let saved = $state<string | null>(null);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let saveErrors = $state<Record<string, string>>({});

	let maintMode = $state(false);
	let savingMaint = $state(false);
	let savedMaint = $state(false);

	onMount(async () => {
		const res = await api.getSystemConfig();
		if (res.data) {
			config = res.data;
			for (const [k, v] of Object.entries(res.data)) {
				edits[k] = JSON.stringify(v, null, 2);
			}
			if (typeof res.data['maintenance_mode'] === 'boolean') {
				maintMode = res.data['maintenance_mode'] as boolean;
			}
		} else {
			error = res.error?.message ?? 'Failed to load system config';
		}
		loading = false;
	});

	// Maintenance mode now has its own explicit save step (Toggle just flips
	// local state; this persists it) rather than saving on every toggle click —
	// matches the Card+Toggle+save-button layout used by smtp's Enable row.
	async function saveMaintenance() {
		savingMaint = true;
		savedMaint = false;
		await api.patchSystemConfig('maintenance_mode', maintMode);
		savingMaint = false;
		savedMaint = true;
		setTimeout(() => (savedMaint = false), 2500);
	}

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

	// Badge tone per value type — same semantics as the old page's hand-rolled
	// typeColor map (string=ok/green, number=accent/blue, boolean=warn/yellow,
	// object/array/null=neutral), just expressed as Badge's tone enum.
	const typeTone: Record<string, 'green' | 'blue' | 'yellow' | 'neutral'> = {
		string: 'green',
		number: 'blue',
		boolean: 'yellow',
		object: 'neutral',
		array: 'neutral',
		null: 'neutral',
	};

	// Existing auto-size calculation, unchanged — now drives Textarea's `rows`
	// prop instead of a fixed number / ad hoc sizing.
	function rowsFor(key: string): number {
		return Math.min(Math.max(edits[key]?.split('\n').length ?? 1, 1), 8);
	}

	// Existing ⌘/Ctrl+Enter save keybinding, unchanged. Attached to the editor
	// wrapper rather than the Textarea directly (the shared Textarea doesn't
	// forward arbitrary event props) — keydown bubbles up from the textarea.
	function onEditorKeydown(e: KeyboardEvent, key: string) {
		if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) save(key);
	}
</script>

<div class="cfg-page">
	<Card padding="18px">
		<div class="maint-row">
			<div class="maint-text">
				<div class="maint-title">Maintenance Mode</div>
				<div class="maint-desc">
					{maintMode
						? 'Platform is currently in maintenance mode — users see a maintenance page.'
						: 'Platform is operating normally.'}
				</div>
			</div>
			<Toggle bind:checked={maintMode} label="Maintenance Mode" disabled={savingMaint} />
		</div>
		<div class="maint-foot">
			<Button variant="secondary" onclick={saveMaintenance} disabled={savingMaint}>
				{#if savingMaint}Saving…{:else if savedMaint}Saved{:else}Save Changes{/if}
			</Button>
		</div>
	</Card>

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
	.cfg-page { max-width: 720px; }

	.maint-row { display: flex; align-items: center; justify-content: space-between; gap: 16px; }
	.maint-title { font-size: 13px; font-weight: 700; color: var(--text-primary); margin-bottom: 2px; }
	.maint-desc { font-size: 11.5px; color: var(--text-muted); }
	.maint-foot { display: flex; justify-content: flex-end; margin-top: 16px; padding-top: 14px; border-top: 1px solid var(--border); }

	.cfg-list { display: flex; flex-direction: column; gap: 10px; }
	.cfg-sk-gap { height: 10px; }

	.cfg-head { display: flex; align-items: center; justify-content: space-between; gap: 10px; margin-bottom: 10px; }
	.cfg-key { font-size: 12.5px; font-weight: 600; color: var(--text-primary); font-family: var(--font-mono); word-break: break-all; }

	.cfg-editor :global(textarea) { font-family: var(--font-mono); }

	.cfg-foot { display: flex; align-items: center; justify-content: flex-end; gap: 10px; margin-top: 10px; }
	.cfg-hint { font-size: 10.5px; color: var(--text-dim); font-family: var(--font-mono); }
</style>
