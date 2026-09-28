<script lang="ts">
	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		value: string;
		options: Option[];
		id?: string;
	}

	let { value = $bindable(), options, id }: Props = $props();
</script>

<select {id} bind:value class="ui-select">
	{#each options as opt (opt.value)}
		<option value={opt.value}>{opt.label}</option>
	{/each}
</select>

<style>
	.ui-select {
		width: 100%;
		box-sizing: border-box;
		height: 36px;
		padding: 0 11px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		outline: none;
		cursor: pointer;
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-select:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
</style>
