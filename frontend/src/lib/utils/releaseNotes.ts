/**
 * Minimal markdown for GitHub release notes: headings, bullets, bold, inline
 * code, links and fenced code. Input is escaped first, so the result is safe
 * to use with {@html}.
 */
const esc = (s: string) =>
	s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

function inline(s: string): string {
	return esc(s)
		.replace(/`([^`]+)`/g, '<code>$1</code>')
		.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
		.replace(
			/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
			'<a href="$2" target="_blank" rel="noreferrer">$1</a>'
		)
		.replace(
			/(^|[\s(])(https?:\/\/[^\s<)]+)/g,
			'$1<a href="$2" target="_blank" rel="noreferrer">$2</a>'
		);
}

export function renderReleaseNotes(md: string): string {
	const out: string[] = [];
	let list = false;
	let fence = false;
	const closeList = () => {
		if (list) out.push('</ul>');
		list = false;
	};
	for (const raw of md.replace(/\r/g, '').split('\n')) {
		if (raw.trim().startsWith('```')) {
			closeList();
			out.push(fence ? '</pre>' : '<pre>');
			fence = !fence;
			continue;
		}
		if (fence) {
			out.push(esc(raw));
			continue;
		}
		const line = raw.trim();
		const h = /^(#{1,6})\s+(.*)$/.exec(line);
		const li = /^[-*]\s+(.*)$/.exec(line);
		if (h) {
			closeList();
			out.push(`<h4>${inline(h[2])}</h4>`);
		} else if (li) {
			if (!list) out.push('<ul>');
			list = true;
			out.push(`<li>${inline(li[1])}</li>`);
		} else if (line === '') {
			closeList();
		} else {
			closeList();
			out.push(`<p>${inline(line)}</p>`);
		}
	}
	closeList();
	if (fence) out.push('</pre>');
	return out.join('\n');
}
