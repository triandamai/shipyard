import type { Handle } from '@sveltejs/kit';
import { env } from '$env/dynamic/private';

/**
 * Server-side API proxy — forwards /api/* to the private backend.
 *
 * PRIVATE_API_URL is read from .env via SvelteKit's $env/dynamic/private
 * so it is never bundled into client code.
 */
const BACKEND_URL = env.PRIVATE_API_URL ?? 'http://localhost:3001';

// Headers that should not be forwarded to the backend
const STRIP_REQUEST_HEADERS = new Set([
    'host',
    'connection',
    'keep-alive',
    'te',
    'trailer',
    'transfer-encoding',
    'upgrade',
]);

// Headers that should not be forwarded back to the client
const STRIP_RESPONSE_HEADERS = new Set([
    'connection',
    'keep-alive',
    'te',
    'trailer',
    'transfer-encoding',
    'upgrade',
]);

export const handle: Handle = async ({ event, resolve }) => {
    if (!event.url.pathname.startsWith('/api')) {
        return resolve(event);
    }

    const method = event.request.method;
    const backendPath = event.url.pathname.replace(/^\/api/, '') || '/';
    const targetUrl = `${BACKEND_URL}/api${backendPath}${event.url.search}`;
    console.log(`[proxy] ${method} ${event.url.pathname} → ${targetUrl}`);

    // Build forwarded headers — keep auth and content-type, strip hop-by-hop
    const forwardHeaders = new Headers();
    event.request.headers.forEach((value, key) => {
        if (!STRIP_REQUEST_HEADERS.has(key.toLowerCase())) {
            forwardHeaders.set(key, value);
        }
    });

    // Auto-inject Authorization from cookie if the client didn't send one.
    // This means every /api/* request is authenticated as long as the cookie exists,
    // without relying on the JS client to manually set the header.
    if (!forwardHeaders.has('authorization')) {
        const cookieToken = event.cookies.get('shipyard_token');
        if (cookieToken) {
            forwardHeaders.set('authorization', `Bearer ${cookieToken}`);
        }
    }

    // Always forward the body for non-GET/HEAD — don't rely on content-length
    // (SvelteKit may not propagate that header to the Request object)
    let body: ArrayBuffer | undefined;
    if (!['GET', 'HEAD'].includes(method)) {
        body = await event.request.arrayBuffer();
        if (body.byteLength === 0) body = undefined;
    }

    try {
        const backendRes = await fetch(targetUrl, {
            method,
            headers: forwardHeaders,
            body,
            // Preserve redirect responses so the client can follow them
            redirect: 'manual',
            // Allow streaming responses (logs, SSE)
            // @ts-expect-error -- Node 18+ fetch supports this
            duplex: 'half',
        });

        // Build clean response headers
        const resHeaders = new Headers();
        backendRes.headers.forEach((value, key) => {
            if (!STRIP_RESPONSE_HEADERS.has(key.toLowerCase())) {
                resHeaders.set(key, value);
            }
        });

        console.log(`[proxy] ← ${backendRes.status} ${backendRes.statusText} (${targetUrl})`);
        return new Response(backendRes.body, {
            status: backendRes.status,
            statusText: backendRes.statusText,
            headers: resHeaders,
        });
    } catch (err) {
        console.error('[api-proxy] Backend unreachable at', targetUrl, err);
        return new Response(
            JSON.stringify({
                data: null,
                error: { code: 'PROXY_ERROR', message: 'Backend service is currently unavailable' },
            }),
            {
                status: 503,
                headers: { 'content-type': 'application/json' },
            }
        );
    }
};
