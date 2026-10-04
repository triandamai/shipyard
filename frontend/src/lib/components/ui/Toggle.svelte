<script lang="ts">
	interface Props {
		checked: boolean;
		disabled?: boolean;
		label?: string;
		onchange?: (checked: boolean) => void;
	}

	let { checked = $bindable(), disabled = false, label, onchange }: Props = $props();
</script>

<button
	type="button"
	role="switch"
	aria-checked={checked}
	aria-label={label}
	{disabled}
	class="ui-toggle"
	class:ui-toggle--on={checked}
	onclick={() => { checked = !checked; onchange?.(checked); }}
>
	<span class="ui-toggle-thumb"></span>
</button>

<style>
	.ui-toggle {
		width: 38px;
		height: 22px;
		border-radius: 999px;
		border: none;
		padding: 0;
		background: var(--border);
		position: relative;
		cursor: pointer;
		transition: background var(--transition-fast);
		flex-shrink: 0;
	}
	.ui-toggle:disabled { opacity: 0.5; cursor: not-allowed; }
	.ui-toggle--on { background: var(--accent); }

	.ui-toggle-thumb {
		position: absolute;
		top: 3px;
		left: 3px;
		width: 16px;
		height: 16px;
		border-radius: 50%;
		background: #fff;
		transition: transform var(--transition-fast);
		box-shadow: var(--shadow-sm);
	}
	.ui-toggle--on .ui-toggle-thumb { transform: translateX(16px); }
</style>
