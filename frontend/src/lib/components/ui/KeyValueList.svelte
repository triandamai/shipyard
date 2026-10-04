<script lang="ts" module>
	export interface KeyValueItem {
		key: string;
		value?: string | number | null;
		mono?: boolean;
	}
</script>

<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		items: KeyValueItem[];
		keyWidth?: string;
		value?: Snippet<[KeyValueItem]>;
		action?: Snippet<[KeyValueItem]>;
	}

	let { items, keyWidth = '110px', value, action }: Props = $props();

	function display(v: KeyValueItem['value']): string {
		return v === null || v === undefined || v === '' ? '—' : String(v);
	}
</script>

<div class="ui-kv">
	{#each items as item}
		<div class="ui-kv-row">
			<span class="ui-kv-key" style="width:{keyWidth}">{item.key}</span>
			<span
				class="ui-kv-value"
				class:ui-kv-value--mono={item.mono}
				title={item.value != null && item.value !== '' ? display(item.value) : undefined}
			>
				{#if value}{@render value(item)}{:else}<span class="ui-kv-text">{display(item.value)}</span>{/if}
			</span>
			{#if action}<span class="ui-kv-action">{@render action(item)}</span>{/if}
		</div>
	{/each}
</div>

<style>
	.ui-kv {
		display: flex;
		flex-direction: column;
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		overflow: hidden;
		background: var(--bg-surface);
	}
	.ui-kv-row {
		display: flex;
		align-items: center;
		gap: 10px;
		padding: 8px 12px;
		font-size: 12.5px;
		border-bottom: 1px solid var(--border);
		min-width: 0;
	}
	.ui-kv-row:last-child { border-bottom: none; }
	.ui-kv-key { color: var(--text-muted); flex-shrink: 0; }
	.ui-kv-value {
		flex: 1;
		min-width: 0;
		color: var(--text-primary);
		font-weight: 500;
		overflow: hidden;
		display: flex;
		align-items: center;
		gap: 6px;
	}
	.ui-kv-text {
		min-width: 0;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ui-kv-value--mono { font-family: var(--font-mono); font-size: 12px; }
	.ui-kv-action { flex-shrink: 0; display: flex; align-items: center; }
</style>
