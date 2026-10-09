<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		label: string;
		hint?: string;
		error?: string;
		for?: string;
		children: Snippet;
	}

	let { label, hint, error, for: htmlFor, children }: Props = $props();
</script>

<div class="ui-form-field">
	<label class="ui-form-label" for={htmlFor}>{label}</label>
	{@render children()}
	{#if error}
		<span class="ui-form-error">{error}</span>
	{:else if hint}
		<span class="ui-form-hint">{hint}</span>
	{/if}
</div>

<style>
	.ui-form-field { display: flex; flex-direction: column; gap: 6px; }
	.ui-form-label {
		font-size: 11px;
		font-weight: 600;
		color: var(--text-dim);
	}
	.ui-form-hint { font-size: 11px; color: var(--text-dim); line-height: 1.5; }
	.ui-form-error { font-size: 11px; color: var(--accent-red); line-height: 1.5; }
</style>
