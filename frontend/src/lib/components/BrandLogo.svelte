<script lang="ts">
	import { Database, Globe, Container } from '@lucide/svelte';

	interface Props {
		icon: string | null | undefined;
		type?: string;
		size?: number;
		iconSize?: number;
		class?: string;
	}

	let {
		icon,
		type = '',
		size = 24,
		iconSize = 13,
		class: className = ''
	}: Props = $props();

	const BRAND_COLORS: Record<string, string> = {
		postgresql:  '#336791',
		mysql:       '#00758f',
		redis:       '#dc382d',
		mongodb:     '#47a248',
		nextjs:      '#000000',
		nuxtjs:      '#00dc82',
		sveltekit:   '#ff3e00',
		astro:       '#bc52ee',
		vite:        '#646cff',
		vue:         '#4fc08d',
		react:       '#61dafb',
		angular:     '#dd0031',
		solid:       '#2c4f7c',
		tanstack:    '#ef4444',
		docker:      '#2496ed',
		html5:       '#e34f26',
		rust:        '#ce422b',
		java:        '#f89820',
		kotlin:      '#7f52ff',
		php:         '#8892bf',
		typescript:  '#3178c6',
		javascript:  '#f7df1e',
	};

	let hasError = $state(false);

	// Reset error state if icon changes
	$effect(() => {
		icon;
		hasError = false;
	});

	let brandColor = $derived(icon && !hasError ? BRAND_COLORS[icon] : null);
	let style = $derived(brandColor
		? `background: ${brandColor}12; color: ${brandColor}; border: 1px solid ${brandColor}24; width: ${size}px; height: ${size}px;`
		: `background: var(--accent-muted); color: var(--accent); width: ${size}px; height: ${size}px;`
	);
</script>

<div class="brand-logo-container {className}" {style}>
	{#if icon && !hasError}
		<img
			src="/brands/{icon}/logo.svg"
			alt={icon}
			style="width: {iconSize}px; height: {iconSize}px; object-fit: contain;"
			onerror={() => (hasError = true)}
		/>
	{:else if type === 'database'}
		<Database size={iconSize} />
	{:else if type === 'static' || type === 'static_site'}
		<Globe size={iconSize} />
	{:else}
		<Container size={iconSize} />
	{/if}
</div>

<style>
	.brand-logo-container {
		border-radius: var(--radius-sm);
		display: flex;
		align-items: center;
		justify-content: center;
		flex-shrink: 0;
	}
</style>
