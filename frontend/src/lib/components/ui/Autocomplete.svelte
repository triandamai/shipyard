<script lang="ts" module>
	let acCounter = 0;

	export interface AutocompleteOption {
		value: string;
		label?: string;
		meta?: string;
		disabled?: boolean;
	}
</script>

<script lang="ts">
	import Spinner from './Spinner.svelte';

	interface Props {
		value: string;
		options: AutocompleteOption[];
		loading?: boolean;
		type?: string;
		placeholder?: string;
		disabled?: boolean;
		id?: string;
		emptyText?: string;
		onSelect?: (option: AutocompleteOption) => void;
	}

	let {
		value = $bindable(),
		options,
		loading = false,
		type = 'text',
		placeholder,
		disabled = false,
		id,
		emptyText,
		onSelect
	}: Props = $props();

	const listId = `ui-ac-list-${++acCounter}`;
	let focused = $state(false);
	let dismissed = $state(false);
	let activeIndex = $state(-1);

	let showEmpty = $derived(!!emptyText && !loading && options.length === 0 && value.trim() !== '');
	let open = $derived(focused && !dismissed && (options.length > 0 || loading || showEmpty));

	$effect(() => {
		options;
		activeIndex = -1;
	});

	function select(opt: AutocompleteOption) {
		if (opt.disabled) return;
		value = opt.value;
		dismissed = true;
		onSelect?.(opt);
	}

	function move(delta: number) {
		const enabled = options.map((o, i) => (o.disabled ? -1 : i)).filter((i) => i >= 0);
		if (enabled.length === 0) return;
		const pos = enabled.indexOf(activeIndex);
		const next = pos === -1 ? (delta > 0 ? 0 : enabled.length - 1) : (pos + delta + enabled.length) % enabled.length;
		activeIndex = enabled[next];
	}

	function onKeydown(e: KeyboardEvent) {
		if (e.key === 'ArrowDown') {
			e.preventDefault();
			dismissed = false;
			move(1);
		} else if (e.key === 'ArrowUp') {
			e.preventDefault();
			move(-1);
		} else if (e.key === 'Enter' && open && activeIndex >= 0) {
			e.preventDefault();
			select(options[activeIndex]);
		} else if (e.key === 'Escape' && open) {
			e.stopPropagation();
			dismissed = true;
		}
	}
</script>

<div class="ui-ac">
	<input
		{id}
		{type}
		{placeholder}
		{disabled}
		bind:value
		class="ui-ac-input"
		role="combobox"
		autocomplete="off"
		aria-autocomplete="list"
		aria-expanded={open}
		aria-controls={listId}
		aria-activedescendant={activeIndex >= 0 ? `${listId}-${activeIndex}` : undefined}
		onfocus={() => (focused = true)}
		onblur={() => (focused = false)}
		oninput={() => (dismissed = false)}
		onkeydown={onKeydown}
	/>
	{#if loading}
		<span class="ui-ac-spinner"><Spinner size={13} /></span>
	{/if}
	{#if open}
		<ul class="ui-ac-list" id={listId} role="listbox">
			{#each options as opt, i (opt.value)}
				<li
					id="{listId}-{i}"
					role="option"
					aria-selected={i === activeIndex}
					aria-disabled={opt.disabled || undefined}
					class="ui-ac-option"
					class:ui-ac-option--active={i === activeIndex}
					class:ui-ac-option--disabled={opt.disabled}
					onmousedown={(e) => { e.preventDefault(); select(opt); }}
					onmouseenter={() => { if (!opt.disabled) activeIndex = i; }}
				>
					<span class="ui-ac-label">{opt.label ?? opt.value}</span>
					{#if opt.meta}<span class="ui-ac-meta">{opt.meta}</span>{/if}
				</li>
			{/each}
			{#if showEmpty}
				<li class="ui-ac-empty" role="presentation">{emptyText}</li>
			{/if}
		</ul>
	{/if}
</div>

<style>
	.ui-ac { position: relative; }
	.ui-ac-input {
		width: 100%;
		box-sizing: border-box;
		height: 36px;
		padding: 0 32px 0 11px;
		background: var(--bg-elevated);
		border: 1px solid var(--border);
		border-radius: var(--radius-sm);
		color: var(--text-primary);
		font-size: 13px;
		font-family: var(--font-sans);
		outline: none;
		transition: border-color var(--transition-fast), box-shadow var(--transition-fast);
	}
	.ui-ac-input::placeholder { color: var(--text-dim); }
	.ui-ac-input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-muted); }
	.ui-ac-input:disabled { opacity: 0.5; cursor: not-allowed; }
	.ui-ac-spinner { position: absolute; right: 10px; top: 50%; transform: translateY(-50%); display: flex; }
	.ui-ac-list {
		position: absolute;
		top: calc(100% + 4px);
		left: 0;
		right: 0;
		margin: 0;
		padding: 4px;
		list-style: none;
		background: var(--bg-surface);
		border: 1px solid var(--border);
		border-radius: var(--radius-md);
		box-shadow: var(--shadow-md);
		max-height: 220px;
		overflow-y: auto;
		z-index: 10;
	}
	.ui-ac-option {
		display: flex;
		align-items: center;
		justify-content: space-between;
		gap: 10px;
		padding: 7px 10px;
		border-radius: var(--radius-sm);
		font-size: 12.5px;
		color: var(--text-primary);
		cursor: pointer;
	}
	.ui-ac-option--active { background: var(--bg-hover); }
	.ui-ac-option--disabled { cursor: not-allowed; color: var(--text-dim); }
	.ui-ac-label { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; min-width: 0; }
	.ui-ac-meta { font-size: 10.5px; color: var(--text-dim); flex-shrink: 0; }
	.ui-ac-empty { padding: 8px 10px; font-size: 12px; color: var(--text-dim); }
</style>
