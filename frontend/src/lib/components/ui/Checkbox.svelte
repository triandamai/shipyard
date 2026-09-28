<script lang="ts">
	interface Props {
		checked: boolean;
		label?: string;
	}

	let { checked = $bindable(), label }: Props = $props();
</script>

<label class="ui-checkbox">
	<input type="checkbox" bind:checked />
	<span class="ui-checkbox-box"></span>
	{#if label}<span class="ui-checkbox-label">{label}</span>{/if}
</label>

<style>
	.ui-checkbox {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		cursor: pointer;
		font-size: 13px;
		color: var(--text-secondary);
	}
	.ui-checkbox input {
		position: absolute;
		opacity: 0;
		width: 0;
		height: 0;
	}
	.ui-checkbox-box {
		width: 16px;
		height: 16px;
		border-radius: 4px;
		border: 1.5px solid var(--border);
		background: var(--bg-elevated);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition: background var(--transition-fast), border-color var(--transition-fast);
	}
	.ui-checkbox input:checked + .ui-checkbox-box {
		background: var(--accent);
		border-color: var(--accent);
	}
	.ui-checkbox input:checked + .ui-checkbox-box::after {
		content: '';
		width: 4px;
		height: 8px;
		border: solid #fff;
		border-width: 0 1.5px 1.5px 0;
		transform: rotate(45deg) translate(-1px, -1px);
	}
	.ui-checkbox-label { color: var(--text-secondary); }
</style>
