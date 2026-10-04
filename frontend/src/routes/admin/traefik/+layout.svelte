<script lang="ts">
	import { page } from '$app/state';

	let { children } = $props();

	const tabs = [
		{ label: 'Settings', href: '/admin/traefik/settings' },
		{ label: 'Static File', href: '/admin/traefik/static' },
		{ label: 'Dynamic Dir', href: '/admin/traefik/dynamic' }
	];
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Traefik</h1>
			<p class="sub">Proxy settings, static config, and dynamic files.</p>
		</div>
	</header>

	<div class="tabs">
		{#each tabs as t}
			<a class="tab" class:active={page.url.pathname === t.href} href={t.href}>
				{t.label}
			</a>
		{/each}
	</div>

	{@render children()}
</div>

<style>
	.p { max-width:860px; margin:0 auto; padding:40px 36px; }
	.hdr { margin-bottom:20px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text-primary); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-muted); margin:0; }

	.tabs { display:flex; gap:2px; margin-bottom:16px; background:var(--bg-elevated); border:1px solid var(--border); border-radius:var(--radius-md); padding:3px; width:fit-content; }
	.tab { padding:5px 14px; border-radius:5px; font-size:12.5px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-secondary); transition:background .15s, color .15s; font-family:var(--font-sans); text-decoration:none; display:inline-block; }
	.tab.active { background:var(--bg-surface); color:var(--text-primary); box-shadow:var(--shadow-sm); }
	.tab:hover:not(.active) { color:var(--text-primary); }

	@media (max-width: 640px) {
		.p { padding:20px 12px; }
		.tabs { width:100%; overflow-x:auto; white-space:nowrap; }
	}
</style>
