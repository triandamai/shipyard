// Docs content layer. Markdown files under src/content/docs are the single
// source of truth: the same text renders the HTML pages, the raw /docs/*.md
// files and /llms.txt + /llms-full.txt for language models.
import { Marked, type Tokens } from 'marked';

export const SITE_URL = 'https://shipyard.trian.space';

/** Page order in the navigation. `slug` is the URL segment under /docs ('' = /docs). */
export const PAGES = [
	{ file: 'guide', slug: '', label: 'Guide' },
	{ file: 'api', slug: 'api', label: 'API reference' },
	{ file: 'edge-functions', slug: 'edge-functions', label: 'Edge functions' },
	{ file: 'sandbox', slug: 'sandbox', label: 'Sandbox apps' },
	{ file: 'registry', slug: 'registry', label: 'Container registry' }
] as const;

export type PageMeta = (typeof PAGES)[number];

export interface TocEntry {
	id: string;
	text: string;
}

export interface Doc {
	file: string;
	slug: string;
	label: string;
	title: string;
	description: string;
	/** Markdown body without frontmatter. */
	markdown: string;
	html: string;
	toc: TocEntry[];
	/** Path of the page, e.g. /docs/api */
	path: string;
	/** Path of the raw Markdown, e.g. /docs/api.md */
	mdPath: string;
}

const RAW = import.meta.glob('/src/content/docs/*.md', {
	query: '?raw',
	import: 'default',
	eager: true
}) as Record<string, string>;

function parseFrontmatter(src: string): { data: Record<string, string>; body: string } {
	const m = /^---\n([\s\S]*?)\n---\n?/.exec(src);
	if (!m) return { data: {}, body: src };
	const data: Record<string, string> = {};
	for (const line of m[1].split('\n')) {
		const i = line.indexOf(':');
		if (i > 0) data[line.slice(0, i).trim()] = line.slice(i + 1).trim();
	}
	return { data, body: src.slice(m[0].length) };
}

export function slugify(s: string): string {
	return s
		.toLowerCase()
		.replace(/<[^>]+>/g, '')
		.replace(/&[a-z]+;/g, '')
		.replace(/[^a-z0-9]+/g, '-')
		.replace(/^-|-$/g, '');
}

function decodeEntities(s: string): string {
	return s
		.replace(/&lt;/g, '<')
		.replace(/&gt;/g, '>')
		.replace(/&quot;/g, '"')
		.replace(/&#39;/g, "'")
		.replace(/&amp;/g, '&');
}

function escapeHtml(s: string): string {
	return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

const ALERTS: Record<string, { label: string; kind: string }> = {
	NOTE: { label: 'Note', kind: 'note' },
	TIP: { label: 'Tip', kind: 'tip' },
	IMPORTANT: { label: 'Important', kind: 'note' },
	WARNING: { label: 'Warning', kind: 'warning' },
	CAUTION: { label: 'Caution', kind: 'warning' }
};

const METHOD = /^(GET|POST|PUT|PATCH|DELETE) (\S+)$/;

function render(markdown: string): { html: string; toc: TocEntry[] } {
	const toc: TocEntry[] = [];
	const used = new Set<string>();
	const marked = new Marked({ gfm: true });
	const inlineMd = new Marked({ gfm: true });

	marked.use({
		renderer: {
			heading(token: Tokens.Heading) {
				let raw = token.text;
				let id = '';
				const explicit = /\s*\{#([\w-]+)\}\s*$/.exec(raw);
				if (explicit) {
					id = explicit[1];
					raw = raw.slice(0, explicit.index);
				}
				const method = token.depth === 3 ? METHOD.exec(raw) : null;
				let inner: string;
				if (method) {
					const path = escapeHtml(method[2]).replace(/\{([\w]+)\}/g, '<span class="ep-param">{$1}</span>');
					inner = `<span class="ep-method">${method[1]}</span> <code class="ep-path">${path}</code>`;
					id ||= slugify(`${method[1]} ${method[2]}`);
				} else {
					inner = inlineMd.parseInline(raw) as string;
					id ||= slugify(inner);
				}
				let unique = id;
				for (let n = 2; used.has(unique); n++) unique = `${id}-${n}`;
				used.add(unique);
				if (token.depth === 2) toc.push({ id: unique, text: decodeEntities(inner.replace(/<[^>]+>/g, '')) });
				if (token.depth === 1) return ''; // the page title is rendered by the layout
				const cls = method ? ' class="endpoint"' : '';
				return `<h${token.depth} id="${unique}"${cls}><a class="anchor" href="#${unique}" aria-label="Link to this section">#</a>${inner}</h${token.depth}>\n`;
			},
			code(token: Tokens.Code) {
				const info = token.lang ?? '';
				const lang = info.split(/\s+/)[0] || 'text';
				const title = /title="([^"]*)"/.exec(info)?.[1] ?? '';
				const label = title || (lang === 'text' ? '' : lang);
				return (
					`<figure class="code">` +
					`<figcaption><span class="code-title">${escapeHtml(label)}</span>` +
					`<button type="button" class="code-copy" data-copy>Copy</button></figcaption>` +
					`<pre><code class="language-${escapeHtml(lang)}">${escapeHtml(token.text)}</code></pre></figure>\n`
				);
			},
			blockquote(this: { parser: { parse(t: Tokens.Generic[]): string } }, token: Tokens.Blockquote) {
				const m = /^\[!(\w+)\]\s*\n?/.exec(token.text);
				const alert = m ? ALERTS[m[1].toUpperCase()] : undefined;
				if (!alert) return `<blockquote>${this.parser.parse(token.tokens as Tokens.Generic[])}</blockquote>\n`;
				const body = inlineMd.parse(token.text.slice(m![0].length)) as string;
				return `<aside class="alert alert-${alert.kind}" role="note"><p class="alert-label">${alert.label}</p>${body}</aside>\n`;
			},
			table(this: { parser: { parseInline(t: Tokens.Generic[]): string } }, token: Tokens.Table) {
				const cell = (c: Tokens.TableCell, tag: 'th' | 'td') =>
					`<${tag}${c.align ? ` style="text-align:${c.align}"` : ''}>${this.parser.parseInline(c.tokens as Tokens.Generic[])}</${tag}>`;
				const head = `<tr>${token.header.map((c) => cell(c, 'th')).join('')}</tr>`;
				const rows = token.rows.map((r) => `<tr>${r.map((c) => cell(c, 'td')).join('')}</tr>`).join('');
				return `<div class="table-wrap"><table><thead>${head}</thead><tbody>${rows}</tbody></table></div>\n`;
			}
		}
	});

	const html = marked.parse(markdown) as string;
	return { html, toc };
}

const cache = new Map<string, Doc>();

export function getDoc(slug: string): Doc | undefined {
	const meta = PAGES.find((p) => p.slug === slug);
	if (!meta) return undefined;
	const hit = cache.get(meta.file);
	if (hit) return hit;
	const src = RAW[`/src/content/docs/${meta.file}.md`];
	if (src === undefined) return undefined;
	const { data, body } = parseFrontmatter(src);
	const { html, toc } = render(body);
	const doc: Doc = {
		file: meta.file,
		slug: meta.slug,
		label: meta.label,
		title: data.title ?? meta.label,
		description: data.description ?? '',
		markdown: body.trim() + '\n',
		html,
		toc,
		path: meta.slug ? `/docs/${meta.slug}` : '/docs',
		mdPath: `/docs/${meta.file}.md`
	};
	cache.set(meta.file, doc);
	return doc;
}

export function getDocByFile(file: string): Doc | undefined {
	const meta = PAGES.find((p) => p.file === file);
	return meta ? getDoc(meta.slug) : undefined;
}

export function allDocs(): Doc[] {
	return PAGES.map((p) => getDoc(p.slug)!).filter(Boolean);
}

/** The Markdown a model should read for one page: source text with absolute links. */
export function docAsMarkdown(doc: Doc): string {
	const body = doc.markdown
		.replace(/ \{#[\w-]+\}$/gm, '')
		.replace(/\]\(\/(?!\/)/g, `](${SITE_URL}/`);
	return `${body.trimEnd()}\n\n---\nSource: ${SITE_URL}${doc.path}\n`;
}
