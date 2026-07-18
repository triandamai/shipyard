import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vite';

export default defineConfig({
	plugins: [
		tailwindcss(),
		sveltekit()
	],
	server: {
		proxy: {
			// Proxy only the exec WebSocket path directly to the backend.
			// SvelteKit's Node adapter cannot handle HTTP upgrade events, so WS
			// connections to /api/.../exec are silently closed before the handshake.
			// The `bypass` function lets HTTP requests (e.g. /exec/token POST) fall
			// through to SvelteKit; only WS upgrade events are forwarded here.
			'^/api/projects/[^/]+/services/[^/]+/exec': {
				target: process.env.PRIVATE_API_URL ?? 'http://localhost:3001',
				changeOrigin: true,
				ws: true,
				bypass(req) {
					if (!req.headers.upgrade) return req.url;
				}
			}
		}
	}
});
