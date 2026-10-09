<script lang="ts">
	import type { HTMLSelectAttributes } from 'svelte/elements';

	interface Option {
		value: string;
		label: string;
		disabled?: boolean;
	}

	interface Props extends Omit<HTMLSelectAttributes, 'value' | 'class'> {
		value: string;
		options: Option[];
	}

	let { value = $bindable(), options, ...rest }: Props = $props();
</script>

<select {...rest} bind:value class="ui-select">
	{#each options as opt (opt.value)}
		<option value={opt.value} disabled={opt.disabled}>{opt.label}</option>
	{/each}
</select>

<style>
	.ui-select {
		width: 100%;
		box-sizing: border-box;
		height: 36px;
		padding: 0 11px;
		background: var(--bg-elevated);
		border: 1px solid var(--border-control);
		border-radius: var(--radius-md);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		outline: none;
		cursor: pointer;
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-select:disabled { opacity: 0.5; cursor: not-allowed; }
	.ui-select:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
</style>
