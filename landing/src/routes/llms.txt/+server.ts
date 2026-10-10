import { SITE_URL, allDocs } from '$lib/docs';

export const prerender = true;

// https://llmstxt.org — an index a model can read to find the right page.
export function GET() {
	const lines = [
		'# Shipyard',
		'',
		'> Shipyard is an open-source, self-hosted platform for deploying containers to your own servers on Docker Swarm: git and image deploys, automatic HTTPS through Traefik, persistent volumes, a live topology canvas, edge functions, sandbox apps and a built-in OCI registry.',
		'',
		'Every page below is plain Markdown. `/llms-full.txt` holds all of them in one file.',
		'',
		'## Docs',
		'',
		...allDocs().map((d) => `- [${d.title}](${SITE_URL}${d.mdPath}): ${d.description}`),
		'',
		'## Optional',
		'',
		`- [Full documentation](${SITE_URL}/llms-full.txt): every page concatenated`,
		'- [Source code](https://github.com/triandamai/shipyard): Rust backend, SvelteKit dashboard',
		''
	];
	return new Response(lines.join('\n'), { headers: { 'Content-Type': 'text/plain; charset=utf-8' } });
}
