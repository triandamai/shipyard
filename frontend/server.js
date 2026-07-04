// Custom HTTP + WebSocket server entry for the SvelteKit frontend.
// Replaces build/index.js as the Docker entrypoint so that WebSocket upgrade
// requests to /api/* are tunneled directly to the backend over the internal
// Docker network — the same trust boundary as the existing HTTP proxy in
// hooks.server.ts — without exposing the backend publicly.
//
// HTTP requests are handled by the SvelteKit handler unchanged.

import { handler } from './build/handler.js';
import http from 'node:http';
import net from 'node:net';

const BACKEND = process.env.PRIVATE_API_URL ?? 'http://localhost:3001';
const { hostname: backendHost, port: backendPortStr } = new URL(BACKEND);
const backendPort = parseInt(backendPortStr || '3001');

const HOST = process.env.HOST ?? '0.0.0.0';
const PORT = parseInt(process.env.PORT ?? '3000');

const server = http.createServer(handler);

// WebSocket upgrade proxy — tunnels /api/* WebSocket connections to the backend.
// Uses a raw TCP tunnel so the WebSocket frames are forwarded verbatim without
// re-parsing, just like a transparent proxy.
server.on('upgrade', (req, socket, headBuf) => {
    if (!req.url?.startsWith('/api/')) {
        socket.end('HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\n\r\n');
        return;
    }

    const backend = net.connect({ host: backendHost, port: backendPort }, () => {
        // Reconstruct the HTTP upgrade request for the backend.
        let requestHead = `${req.method ?? 'GET'} ${req.url} HTTP/1.1\r\n`;
        requestHead += `host: ${backendHost}:${backendPort}\r\n`;

        for (const [key, value] of Object.entries(req.headers)) {
            if (key === 'host') continue;
            requestHead += `${key}: ${Array.isArray(value) ? value.join(', ') : value}\r\n`;
        }

        // Inject auth from the httponly session cookie when no Authorization
        // header is present — mirrors what hooks.server.ts does for HTTP requests.
        if (!req.headers['authorization']) {
            const cookie = req.headers['cookie'] ?? '';
            const match = cookie.match(/(?:^|;\s*)shipyard_token=([^;]+)/);
            if (match) requestHead += `authorization: Bearer ${match[1]}\r\n`;
        }

        requestHead += '\r\n';
        backend.write(requestHead);
        if (headBuf.length > 0) backend.write(headBuf);

        // Bidirectional pipe: browser ↔ backend
        socket.pipe(backend);
        backend.pipe(socket);
    });

    backend.on('error', (err) => {
        console.error('[ws-proxy] backend error:', err.message);
        socket.destroy();
    });
    socket.on('error', () => backend.destroy());
});

server.listen(PORT, HOST, () => {
    console.log(`Shipyard frontend listening on ${HOST}:${PORT}`);
});
