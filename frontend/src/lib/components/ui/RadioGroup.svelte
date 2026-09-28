<script lang="ts">
	interface Option {
		value: string;
		label: string;
	}

	interface Props {
		value: string;
		options: Option[];
		name: string;
	}

	let { value = $bindable(), options, name }: Props = $props();
</script>

<div class="ui-radio-group">
	{#each options as opt (opt.value)}
		<label class="ui-radio">
			<input type="radio" {name} value={opt.value} bind:group={value} />
			<span class="ui-radio-dot"></span>
			<span class="ui-radio-label">{opt.label}</span>
		</label>
	{/each}
</div>

<style>
	.ui-radio-group { display: flex; flex-direction: column; gap: 8px; }
	.ui-radio {
		display: inline-flex;
		align-items: center;
		gap: 8px;
		cursor: pointer;
		font-size: 13px;
	}
	.ui-radio input {
		position: absolute;
		opacity: 0;
		width: 0;
		height: 0;
	}
	.ui-radio-dot {
		width: 16px;
		height: 16px;
		border-radius: 50%;
		border: 1.5px solid var(--border);
		background: var(--bg-elevated);
		display: inline-flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
		transition: border-color var(--transition-fast);
	}
	.ui-radio input:checked + .ui-radio-dot { border-color: var(--accent); border-width: 5px; }
	.ui-radio-label { color: var(--text-secondary); }
</style>
