import { allDocs, docAsMarkdown } from '$lib/docs';

export const prerender = true;

export function GET() {
	const body = allDocs()
		.map((d) => docAsMarkdown(d))
		.join('\n\n');
	return new Response(body, { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
}
