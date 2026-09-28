<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		value: string;
		type?: string;
		placeholder?: string;
		disabled?: boolean;
		id?: string;
		icon?: Snippet;
	}

	let {
		value = $bindable(),
		type = 'text',
		placeholder,
		disabled = false,
		id,
		icon
	}: Props = $props();
</script>

{#if icon}
	<div class="ui-textfield-icon-wrap">
		<span class="ui-textfield-icon">{@render icon()}</span>
		<input
			{id}
			{type}
			{placeholder}
			{disabled}
			bind:value
			class="ui-textfield ui-textfield--with-icon"
		/>
	</div>
{:else}
	<input
		{id}
		{type}
		{placeholder}
		{disabled}
		bind:value
		class="ui-textfield"
	/>
{/if}

<style>
	.ui-textfield {
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
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-textfield::placeholder { color: var(--text-dim); }
	.ui-textfield:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
	.ui-textfield:disabled { opacity: 0.5; cursor: not-allowed; }

	.ui-textfield-icon-wrap { position: relative; display: flex; align-items: center; }
	.ui-textfield-icon {
		position: absolute;
		left: 11px;
		display: flex;
		color: var(--text-dim);
		pointer-events: none;
	}
	.ui-textfield--with-icon { padding-left: 32px; }
</style>
