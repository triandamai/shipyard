import { error } from '@sveltejs/kit';
import { PAGES, getDocByFile, docAsMarkdown } from '$lib/docs';
import type { EntryGenerator } from './$types';

export const prerender = true;

export const entries: EntryGenerator = () => PAGES.map((p) => ({ file: p.file }));

export function GET({ params }) {
	const doc = getDocByFile(params.file);
	if (!doc) error(404, 'No docs page with this name');
	return new Response(docAsMarkdown(doc), {
		headers: { 'Content-Type': 'text/markdown; charset=utf-8' }
	});
}
