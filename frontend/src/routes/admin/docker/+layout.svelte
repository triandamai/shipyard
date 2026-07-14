<script lang="ts">
	import { page } from '$app/state';

	let { children } = $props();

	const tabs = [
		{ label: 'Containers', href: '/admin/docker/containers' },
		{ label: 'Services', href: '/admin/docker/services' },
		{ label: 'Volumes', href: '/admin/docker/volumes' },
		{ label: 'Networks', href: '/admin/docker/networks' },
		{ label: 'Images', href: '/admin/docker/images' }
	];
</script>

<div class="p">
	<header class="hdr">
		<div>
			<h1 class="ttl">Docker</h1>
			<p class="sub">Platform Docker daemon — containers, services, volumes, networks, images.</p>
		</div>
	</header>

	<div class="toolbar">
		<div class="tabs">
			{#each tabs as t}
				<a class="tab" class:active={page.url.pathname === t.href} href={t.href}>
					{t.label}
				</a>
			{/each}
		</div>
	</div>

	{@render children()}
</div>

<style>
	.p { max-width:1080px; margin:0 auto; padding:40px 36px; }
	.hdr { display:flex; align-items:flex-start; justify-content:space-between; gap:12px; margin-bottom:20px; }
	.ttl { font-size:18px; font-weight:700; color:var(--text); margin:0 0 4px; letter-spacing:-0.02em; }
	.sub { font-size:12.5px; color:var(--text-3); margin:0; }

	.toolbar { display:flex; align-items:center; justify-content:space-between; margin-bottom:14px; gap:12px; }
	.tabs { display:flex; gap:2px; background:var(--surface-2); border:1px solid var(--border); border-radius:var(--radius-sm); padding:3px; }
	.tab { padding:5px 12px; border-radius:5px; font-size:12px; font-weight:500; cursor:pointer; border:none; background:transparent; color:var(--text-2); transition:background .15s, color .15s; font-family:var(--font); text-decoration:none; display:inline-block; }
	.tab.active { background:var(--surface); color:var(--text); box-shadow:0 1px 2px rgba(0,0,0,.07); }
	.tab:hover:not(.active) { color:var(--text); }

	@media (max-width: 680px) {
		.p { padding:20px 12px; }
		.toolbar { flex-direction:column; align-items:flex-start; gap:8px; }
		.tabs { width:100%; overflow-x:auto; white-space:nowrap; }
		.hdr { flex-direction:column; align-items:flex-start; gap:8px; }
	}
</style>
