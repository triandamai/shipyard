<script lang="ts">
	import { onMount } from 'svelte';
	import { PAGES, SITE_URL, type Doc } from './index';
	import '$lib/styles/docs.css';

	interface Props {
		doc: Doc;
		/** The page as Markdown with absolute links, for "Copy page". */
		markdown: string;
	}
	let { doc, markdown }: Props = $props();

	let active = $state('');
	let copied = $state(false);
	let dark = $state(false);

	const index = $derived(PAGES.findIndex((p) => p.file === doc.file));
	const prev = $derived(index > 0 ? PAGES[index - 1] : null);
	const next = $derived(index < PAGES.length - 1 ? PAGES[index + 1] : null);
	const href = (slug: string) => (slug ? `/docs/${slug}` : '/docs');

	onMount(() => {
		dark =
			document.documentElement.dataset.theme === 'dark' ||
			(!document.documentElement.dataset.theme && matchMedia('(prefers-color-scheme: dark)').matches);

		const heads = [...document.querySelectorAll<HTMLElement>('.prose h2[id]')];
		const io = new IntersectionObserver(
			(entries) => {
				for (const e of entries) if (e.isIntersecting) active = e.target.id;
			},
			{ rootMargin: '-72px 0px -70% 0px' }
		);
		heads.forEach((h) => io.observe(h));
		return () => io.disconnect();
	});

	function toggleTheme() {
		dark = !dark;
		const theme = dark ? 'dark' : 'light';
		document.documentElement.dataset.theme = theme;
		try {
			localStorage.setItem('shipyard_docs_theme', theme);
		} catch {
			/* storage unavailable */
		}
	}

	async function copyText(text: string): Promise<boolean> {
		try {
			await navigator.clipboard.writeText(text);
			return true;
		} catch {
			return false;
		}
	}

	async function copyPage() {
		copied = await copyText(markdown);
		setTimeout(() => (copied = false), 1600);
	}

	// Copy buttons inside rendered Markdown (code blocks).
	async function onArticleClick(e: MouseEvent) {
		const btn = (e.target as HTMLElement).closest<HTMLButtonElement>('[data-copy]');
		if (!btn) return;
		const code = btn.closest('figure')?.querySelector('pre')?.textContent ?? '';
		if (await copyText(code)) {
			btn.textContent = 'Copied';
			setTimeout(() => (btn.textContent = 'Copy'), 1400);
		}
	}
</script>

<svelte:head>
	<title>{doc.title} · Shipyard docs</title>
	<meta name="description" content={doc.description} />
	<link rel="canonical" href="{SITE_URL}{doc.path}" />
	<link rel="alternate" type="text/markdown" href={doc.mdPath} title="{doc.title} (Markdown)" />
	<link rel="alternate" type="text/plain" href="/llms.txt" title="llms.txt" />
	<meta property="og:title" content="{doc.title} · Shipyard docs" />
	<meta property="og:description" content={doc.description} />
	<meta property="og:url" content="{SITE_URL}{doc.path}" />
</svelte:head>

{#snippet pageNav(withSections: boolean)}
	<nav class="side" aria-label="Documentation">
		<p class="side-title">Documentation</p>
		<ul>
			{#each PAGES as p (p.file)}
				<li>
					<a href={href(p.slug)} aria-current={p.file === doc.file ? 'page' : undefined}>{p.label}</a>
					{#if withSections && p.file === doc.file && doc.toc.length > 1}
						<ul class="sections">
							{#each doc.toc as t (t.id)}
								<li><a href="#{t.id}" class:on={active === t.id}>{t.text}</a></li>
							{/each}
						</ul>
					{/if}
				</li>
			{/each}
		</ul>
	</nav>
{/snippet}

<div class="docs">
	<a class="skip-link" href="#content">Skip to content</a>

	<header class="bar">
		<div class="bar-inner">
			<a class="brand" href="/">
				<span class="brand-mark" aria-hidden="true">
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="5" r="3"></circle><line x1="12" y1="22" x2="12" y2="8"></line><path d="M5 12H2a10 10 0 0 0 20 0h-3"></path></svg>
				</span>
				Shipyard <span class="brand-sub">docs</span>
			</a>
			<ul class="bar-links">
				{#each PAGES as p (p.file)}
					<li><a href={href(p.slug)} aria-current={p.file === doc.file ? 'page' : undefined}>{p.label}</a></li>
				{/each}
				<li><a href="https://github.com/triandamai/shipyard" rel="noopener noreferrer">GitHub</a></li>
			</ul>
			<button type="button" class="theme-btn" onclick={toggleTheme} aria-label={dark ? 'Use light theme' : 'Use dark theme'}>
				{#if dark}
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><circle cx="12" cy="12" r="4"></circle><path d="M12 2v2M12 20v2M2 12h2M20 12h2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"></path></svg>
				{:else}
					<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"></path></svg>
				{/if}
			</button>
		</div>
	</header>

	<div class="body">
		{@render pageNav(true)}

		<main class="article" id="content">
			<details class="mobile-nav">
				<summary>Pages and sections</summary>
				{@render pageNav(true)}
			</details>

			<header class="page-head">
				<p class="page-path">shipyard.trian.space{doc.path}</p>
				<h1>{doc.title}</h1>
				{#if doc.description}<p class="page-lead">{doc.description}</p>{/if}
				<div class="machine-inline">
					<a class="chip-btn" href={doc.mdPath}>View as <code>.md</code></a>
					<button type="button" class="chip-btn" onclick={copyPage}>{copied ? 'Copied' : 'Copy page as Markdown'}</button>
				</div>
			</header>

			<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
			<div class="prose" onclick={onArticleClick}>
				{@html doc.html}
			</div>

			<nav class="page-foot" aria-label="Previous and next page">
				{#if prev}
					<a href={href(prev.slug)}><span>Previous</span><strong>{prev.label}</strong></a>
				{/if}
				{#if next}
					<a href={href(next.slug)}><span>Next</span><strong>{next.label}</strong></a>
				{/if}
			</nav>
		</main>

		<aside class="outline" aria-label="On this page">
			{#if doc.toc.length > 1}
				<div>
					<p class="side-title">On this page</p>
					<ul>
						{#each doc.toc as t (t.id)}
							<li><a href="#{t.id}" class:on={active === t.id}>{t.text}</a></li>
						{/each}
					</ul>
				</div>
			{/if}
			<div class="machine">
				<p class="side-title" style="margin:0">For LLMs and agents</p>
				<p>This page is plain Markdown underneath. Point a model at the file, or paste it into a chat.</p>
				<div class="row">
					<a class="chip-btn" href={doc.mdPath}><code>{doc.file}.md</code></a>
					<button type="button" class="chip-btn" onclick={copyPage}>{copied ? 'Copied' : 'Copy page'}</button>
				</div>
				<p>All docs: <a href="/llms.txt">llms.txt</a> (index) or <a href="/llms-full.txt">llms-full.txt</a> (everything in one file).</p>
			</div>
		</aside>
	</div>
</div>
