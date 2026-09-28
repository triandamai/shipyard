<script lang="ts">
	import { Search } from '@lucide/svelte';

	interface Props {
		value: string;
		placeholder?: string;
	}

	let { value = $bindable(), placeholder = 'Search…' }: Props = $props();
</script>

<div class="ui-search">
	<Search size={13} class="ui-search-icon" />
	<input type="text" {placeholder} bind:value class="ui-search-input" />
</div>

<style>
	.ui-search {
		position: relative;
		display: flex;
		align-items: center;
	}
	:global(.ui-search-icon) {
		position: absolute;
		left: 11px;
		color: var(--text-dim);
		pointer-events: none;
	}
	.ui-search-input {
		height: 34px;
		padding: 0 12px 0 30px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: 999px;
		font-size: 12.5px;
		color: var(--text-primary);
		outline: none;
		width: 100%;
		box-sizing: border-box;
		font-family: var(--font-sans);
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-search-input::placeholder { color: var(--text-dim); }
	.ui-search-input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
</style>
