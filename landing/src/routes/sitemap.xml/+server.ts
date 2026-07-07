export const prerender = true;

export async function GET() {
	const pages = [
		{ path: '', changefreq: 'daily', priority: '1.0' },
		{ path: 'docs', changefreq: 'weekly', priority: '0.8' },
		{ path: 'docs/api', changefreq: 'weekly', priority: '0.7' }
	];

	const sitemap = `<?xml version="1.0" encoding="UTF-8"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
	${pages
		.map(
			(page) => `
	<url>
		<loc>https://shipyard.trian.space/${page.path}</loc>
		<changefreq>${page.changefreq}</changefreq>
		<priority>${page.priority}</priority>
	</url>`
		)
		.join('')}
</urlset>`;

	return new Response(sitemap, {
		headers: {
			'Content-Type': 'application/xml',
			'Cache-Control': 'max-age=0, s-maxage=3600'
		}
	});
}
