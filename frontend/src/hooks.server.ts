import type { Handle } from '@sveltejs/kit';
import { env } from '$env/dynamic/private';

const BACKEND_URL = env.PRIVATE_API_URL ?? 'http://localhost:3001';

// Headers that must not be forwarded to the backend
const STRIP_REQUEST_HEADERS = new Set([
    'host', 'connection', 'keep-alive', 'te', 'trailer', 'transfer-encoding', 'upgrade',
]);

// Headers that must not be forwarded back to the client
const STRIP_RESPONSE_HEADERS = new Set([
    'connection', 'keep-alive', 'te', 'trailer', 'transfer-encoding', 'upgrade',
]);

// Is the app running in a secure context (HTTPS)?
const IS_SECURE = (env.PUBLIC_APP_URL ?? '').startsWith('https');

// ── Helpers ───────────────────────────────────────────────────────────────────

function buildProxiedRequest(
    targetUrl: string,
    method: string,
    headers: Headers,
    body: ArrayBuffer | undefined,
): ReturnType<typeof fetch> {
    return fetch(targetUrl, {
        method,
        headers,
        body,
        redirect: 'manual',
        // @ts-expect-error — Node 18+ fetch duplex option
        duplex: 'half',
    });
}

function buildResponseHeaders(backendRes: Response): Headers {
    const h = new Headers();
    backendRes.headers.forEach((value, key) => {
        if (STRIP_RESPONSE_HEADERS.has(key.toLowerCase())) return;
        if (key.toLowerCase() === 'set-cookie' && !IS_SECURE) {
            // Safari (and spec-compliant browsers) refuse to store Secure cookies
            // over HTTP, so strip the attribute in non-HTTPS environments.
            h.append(key, value.replace(/;\s*Secure/gi, ''));
        } else {
            h.append(key, value);
        }
    });
    return h;
}

/**
 * Call the backend refresh endpoint using the shipyard_refresh value we
 * already have on the SvelteKit server (avoids all browser Secure/HttpOnly
 * and cookie-forwarding edge cases).
 * Returns the new access token or null if refresh fails.
 */
async function serverRefresh(refreshToken: string): Promise<string | null> {
    try {
        const res = await fetch(`${BACKEND_URL}/api/auth/refresh`, {
            method: 'POST',
            headers: {
                'Content-Type': 'application/json',
                Cookie: `shipyard_refresh=${refreshToken}`,
            },
        });
        if (!res.ok) return null;
        const json = await res.json();
        return (json?.data?.access_token as string) ?? null;
    } catch {
        return null;
    }
}

// ── Handle ────────────────────────────────────────────────────────────────────

export const handle: Handle = async ({ event, resolve }) => {
    const isFnInvoke = event.url.pathname.startsWith('/fn/');

    if (!event.url.pathname.startsWith('/api') && !isFnInvoke) {
        return resolve(event);
    }

    // /fn/{org_slug}/... → backend /fn/{org_slug}/... (no auth injection)
    if (isFnInvoke) {
        const method = event.request.method;
        const targetUrl = `${BACKEND_URL}${event.url.pathname}${event.url.search}`;
        const forwardHeaders = new Headers();
        event.request.headers.forEach((value, key) => {
            if (!STRIP_REQUEST_HEADERS.has(key.toLowerCase())) {
                forwardHeaders.set(key, value);
            }
        });
        let body: ArrayBuffer | undefined;
        if (!['GET', 'HEAD'].includes(method)) {
            body = await event.request.arrayBuffer();
            if (body.byteLength === 0) body = undefined;
        }
        try {
            const res = await buildProxiedRequest(targetUrl, method, forwardHeaders, body);
            return new Response(res.body, {
                status: res.status,
                statusText: res.statusText,
                headers: buildResponseHeaders(res),
            });
        } catch (err) {
            console.error('[fn-proxy] runtime unreachable at', targetUrl, err);
            return new Response('edge runtime unavailable', { status: 502 });
        }
    }

    const method   = event.request.method;
    const apiPath  = event.url.pathname.replace(/^\/api/, '') || '/';
    const targetUrl = `${BACKEND_URL}/api${apiPath}${event.url.search}`;
    console.log(`[proxy] ${method} ${event.url.pathname} → ${targetUrl}`);

    // ── Build forwarded request headers ─────────────────────────────────────

    const forwardHeaders = new Headers();
    event.request.headers.forEach((value, key) => {
        if (!STRIP_REQUEST_HEADERS.has(key.toLowerCase())) {
            forwardHeaders.set(key, value);
        }
    });

    // Inject Authorization from access-token cookie if the client didn't send one.
    if (!forwardHeaders.has('authorization')) {
        const cookieToken = event.cookies.get('shipyard_token');
        if (cookieToken) {
            forwardHeaders.set('authorization', `Bearer ${cookieToken}`);
        }
    }

    // For the refresh endpoint, explicitly ensure shipyard_refresh is in the
    // Cookie header. SvelteKit server can always read HttpOnly cookies via
    // event.cookies even when the browser Secure / forwarding chain is broken.
    if (apiPath === '/auth/refresh') {
        const rt = event.cookies.get('shipyard_refresh');
        if (rt) {
            const existing = forwardHeaders.get('cookie') ?? '';
            if (!existing.includes('shipyard_refresh=')) {
                forwardHeaders.set(
                    'cookie',
                    existing ? `${existing}; shipyard_refresh=${rt}` : `shipyard_refresh=${rt}`,
                );
            }
        }
    }

    // ── Read body once (reusable for retry) ──────────────────────────────────

    let body: ArrayBuffer | undefined;
    if (!['GET', 'HEAD'].includes(method)) {
        body = await event.request.arrayBuffer();
        if (body.byteLength === 0) body = undefined;
    }

    // ── Forward request to backend ───────────────────────────────────────────

    try {
        let backendRes = await buildProxiedRequest(targetUrl, method, forwardHeaders, body);
        console.log(`[proxy] ← ${backendRes.status} ${backendRes.statusText} (${targetUrl})`);

        // ── Transparent server-side token refresh on 401 ─────────────────────
        // Only attempt once, and only for non-auth endpoints (avoids loops).
        if (backendRes.status === 401 && !apiPath.startsWith('/auth/')) {
            const refreshToken = event.cookies.get('shipyard_refresh');
            if (refreshToken) {
                const newToken = await serverRefresh(refreshToken);
                if (newToken) {
                    // Persist the new access token in the browser cookie.
                    // We build the Set-Cookie header manually so it lands in
                    // the final Response even though we're returning a raw Response.
                    const cookieAttrs = [
                        `shipyard_token=${newToken}`,
                        'path=/',
                        'max-age=3600',
                        'SameSite=Strict',
                        ...(IS_SECURE ? ['Secure'] : []),
                    ].join('; ');

                    // Retry the original request with the fresh token.
                    forwardHeaders.set('authorization', `Bearer ${newToken}`);
                    backendRes = await buildProxiedRequest(targetUrl, method, forwardHeaders, body);
                    console.log(`[proxy] ← retry ${backendRes.status} (${targetUrl})`);

                    const resHeaders = buildResponseHeaders(backendRes);
                    resHeaders.append('set-cookie', cookieAttrs);
                    return new Response(backendRes.body, {
                        status: backendRes.status,
                        statusText: backendRes.statusText,
                        headers: resHeaders,
                    });
                }
            }
        }

        // ── Normal response ───────────────────────────────────────────────────

        const resHeaders = buildResponseHeaders(backendRes);
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
            { status: 503, headers: { 'content-type': 'application/json' } },
        );
    }
};
