<script lang="ts">
	import { page } from '$app/state';
	import { Package } from '@lucide/svelte';

	let { children } = $props();

	let orgSlug     = $derived(page.params.orgSlug ?? '');
	let currentPath = $derived(page.url.pathname);

	const tabs = [
		{ label: 'Browse',   href: (slug: string) => `/orgs/${slug}/registry`          },
		{ label: 'Settings', href: (slug: string) => `/orgs/${slug}/registry/settings` },
	];

	function isActive(href: string) {
		if (href === `/orgs/${orgSlug}/registry`) {
			return currentPath === href || (
				currentPath.startsWith(href + '/') && !currentPath.startsWith(href + '/settings')
			);
		}
		return currentPath === href || currentPath.startsWith(href + '/');
	}
</script>

<div class="registry-shell">
	<div class="registry-header">
		<div class="title-row">
			<Package size={20} />
			<div>
				<h1>Registry</h1>
				<p>Artifact storage for images, static bundles, and edge functions</p>
			</div>
		</div>
		<nav class="tab-bar">
			{#each tabs as tab}
				{@const href = tab.href(orgSlug)}
				<a class="tab-btn" class:active={isActive(href)} {href}>
					{tab.label}
				</a>
			{/each}
		</nav>
	</div>

	<div class="registry-content">
		{@render children()}
	</div>
</div>

<style>
	.registry-shell {
		display: flex;
		flex-direction: column;
		height: 100%;
		overflow: hidden;
	}

	.registry-header {
		flex-shrink: 0;
		padding: 24px 32px 0;
		display: flex;
		flex-direction: column;
		gap: 16px;
		border-bottom: 1px solid var(--border);
	}

	.title-row {
		display: flex;
		align-items: flex-start;
		gap: 10px;
		color: var(--text-primary);
	}
	.title-row h1 {
		font-size: 18px;
		font-weight: 700;
		letter-spacing: -0.02em;
		margin: 0;
		line-height: 1.2;
	}
	.title-row p {
		font-size: 12px;
		color: var(--text-muted);
		margin: 2px 0 0;
	}

	.tab-bar {
		display: flex;
		gap: 2px;
		overflow-x: auto;
		overflow-y: hidden;
		scrollbar-width: none;
	}
	.tab-bar::-webkit-scrollbar { display: none; }

	.tab-btn {
		display: flex;
		align-items: center;
		gap: 6px;
		padding: 9px 14px;
		font-size: 13px;
		font-weight: 500;
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		color: var(--text-muted);
		cursor: pointer;
		margin-bottom: -1px;
		text-decoration: none;
		white-space: nowrap;
		transition: color var(--transition-fast), border-color var(--transition-fast);
	}
	.tab-btn:hover { color: var(--text-primary); }
	.tab-btn.active { color: var(--accent); border-bottom-color: var(--accent); }

	.registry-content {
		flex: 1;
		overflow-y: auto;
	}
</style>
