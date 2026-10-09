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
		font-size: 12.5px;
		font-weight: 500;
		color: var(--text-secondary);
	}
	.ui-form-hint { font-size: 12px; color: var(--text-muted); line-height: 1.5; }
	.ui-form-error { font-size: 12px; color: var(--accent-red-text); line-height: 1.5; }
</style>
