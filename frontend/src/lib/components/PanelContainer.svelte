<script lang="ts">
	import { uiStore } from '$lib/stores/ui.store';
	import SlidePanel from './SlidePanel.svelte';

	let panelStack = $derived($uiStore.panelStack);

	// Panels spaced by 2 so the scrim can sit in the gap just below the top panel.
	// i=0 → zIndex 60, i=1 → 62, i=2 → 64 …
	let topPanelZIndex = $derived(60 + (panelStack.length - 1) * 2);
	let scrimZIndex    = $derived(topPanelZIndex - 1);

	function handleScrimKey(e: KeyboardEvent) {
		if (e.key === 'Enter' || e.key === ' ') uiStore.popPanel();
	}
</script>

{#if panelStack.length > 0}
	<div
		class="panel-scrim"
		style="z-index: {scrimZIndex};"
		onclick={() => uiStore.popPanel()}
		onkeydown={handleScrimKey}
		role="button"
		tabindex="0"
		aria-label="Close panel"
	></div>

	{#each panelStack as entry, i (entry.id)}
		<SlidePanel
			title={entry.title}
			onClose={() => uiStore.removePanel(entry.id)}
			zIndex={60 + i * 2}
		>
			{#snippet children()}
				{@const Comp = entry.component}
				<Comp {...entry.props} />
			{/snippet}
		</SlidePanel>
	{/each}
{/if}

<style>
	.panel-scrim {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.5);
		cursor: pointer;
	}
</style>
