import { error } from '@sveltejs/kit';
import { PAGES, getDoc, docAsMarkdown } from '$lib/docs';
import type { EntryGenerator } from './$types';

export const prerender = true;

export const entries: EntryGenerator = () => PAGES.filter((p) => p.slug).map((p) => ({ slug: p.slug }));

export function load({ params }) {
	const doc = getDoc(params.slug);
	if (!doc || !doc.slug) error(404, 'No docs page at this address');
	return { doc, markdown: docAsMarkdown(doc) };
}
