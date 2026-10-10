<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';

	let { children } = $props();

	const OG_IMAGE = 'https://shipyard.trian.space/og-image.svg';
</script>

<!-- Site-wide tags only. Each page sets its own <title>, description,
     canonical URL and og:title / og:description / og:url. -->
<svelte:head>
	<link rel="icon" href={favicon} />
	<meta property="og:type" content="website" />
	<meta property="og:site_name" content="Shipyard" />
	<meta property="og:image" content={OG_IMAGE} />
	<meta property="og:image:type" content="image/svg+xml" />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:alt" content="Shipyard: deploy apps to servers you own" />
	<meta property="og:locale" content="en_US" />
	<meta name="twitter:card" content="summary_large_image" />
	<meta name="twitter:image" content={OG_IMAGE} />
	<meta name="robots" content="index, follow" />
	<meta name="theme-color" content="#141619" />
</svelte:head>

{@render children()}
