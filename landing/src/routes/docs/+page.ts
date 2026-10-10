import { getDoc, docAsMarkdown } from '$lib/docs';

export const prerender = true;

export function load() {
	const doc = getDoc('')!;
	return { doc, markdown: docAsMarkdown(doc) };
}
