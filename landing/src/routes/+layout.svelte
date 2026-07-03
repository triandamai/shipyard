<script lang="ts">
	import favicon from '$lib/assets/favicon.svg';

	let { children } = $props();

	const SITE_URL = 'https://shipyard.trian.space';
	const OG_TITLE = 'Shipyard — Deploy & Orchestrate Containers on Your VPS';
	const OG_DESC  = 'Open-source platform to deploy, manage, and scale Docker services on your own VPS. One-command install, automatic HTTPS, and a live topology canvas.';
	const OG_IMAGE = `${SITE_URL}/og-image.svg`;
</script>

<svelte:head>
	<link rel="icon" href={favicon} />

	<!-- Primary -->
	<title>{OG_TITLE}</title>
	<meta name="description" content={OG_DESC} />
	<link rel="canonical" href={SITE_URL} />

	<!-- Open Graph (Facebook, WhatsApp, Telegram, LinkedIn, etc.) -->
	<meta property="og:type"        content="website" />
	<meta property="og:url"         content={SITE_URL} />
	<meta property="og:site_name"   content="Shipyard" />
	<meta property="og:title"       content={OG_TITLE} />
	<meta property="og:description" content={OG_DESC} />
	<meta property="og:image"       content={OG_IMAGE} />
	<meta property="og:image:type"  content="image/svg+xml" />
	<meta property="og:image:width" content="1200" />
	<meta property="og:image:height" content="630" />
	<meta property="og:image:alt"   content="Shipyard — self-hosted container orchestration platform" />
	<meta property="og:locale"      content="en_US" />

	<!-- Twitter / X Card -->
	<meta name="twitter:card"        content="summary_large_image" />
	<meta name="twitter:title"       content={OG_TITLE} />
	<meta name="twitter:description" content={OG_DESC} />
	<meta name="twitter:image"       content={OG_IMAGE} />
	<meta name="twitter:image:alt"   content="Shipyard — self-hosted container orchestration platform" />

	<!-- Extra signals for crawlers -->
	<meta name="robots"   content="index, follow" />
	<meta name="theme-color" content="#0F1827" />
</svelte:head>

{@render children()}
