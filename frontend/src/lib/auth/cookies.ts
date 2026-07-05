const TOKEN_KEY = 'shipyard_token';
const ACCESS_MAX_AGE = 60 * 60; // 1 hour

// Safari on HTTP refuses to store Secure cookies even on localhost.
// Only add Secure when we're actually on HTTPS.
function isSecureContext(): boolean {
	return typeof window !== 'undefined' && window.location.protocol === 'https:';
}

function cookieAttrs(maxAge: number): string {
	const base = `path=/; max-age=${maxAge}; SameSite=Strict`;
	return isSecureContext() ? `${base}; Secure` : base;
}

export function setAuthCookies(token: string) {
	document.cookie = `${TOKEN_KEY}=${encodeURIComponent(token)}; ${cookieAttrs(ACCESS_MAX_AGE)}`;
}

export function getAuthToken(): string | null {
	return parseCookie(TOKEN_KEY);
}

/** No-op — refresh token is HttpOnly and managed by the server. */
export function getRefreshToken(): null {
	return null;
}

export function clearAuthCookies() {
	document.cookie = `${TOKEN_KEY}=; ${cookieAttrs(0)}`;
}

function parseCookie(key: string): string | null {
	const match = document.cookie.match(new RegExp(`(?:^|;\\s*)${key}=([^;]*)`));
	return match ? decodeURIComponent(match[1]) : null;
}
