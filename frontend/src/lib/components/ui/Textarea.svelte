<script lang="ts">
	interface Props {
		value: string;
		rows?: number;
		placeholder?: string;
		id?: string;
	}

	let { value = $bindable(), rows = 3, placeholder, id }: Props = $props();
</script>

<textarea {id} {rows} {placeholder} bind:value class="ui-textarea"></textarea>

<style>
	.ui-textarea {
		width: 100%;
		box-sizing: border-box;
		padding: 9px 11px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		line-height: 1.5;
		outline: none;
		resize: vertical;
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-textarea::placeholder { color: var(--text-dim); }
	.ui-textarea:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
</style>
