const TOKEN_KEY = 'shipyard_token';
// Access token lives 1 hour — matches server-side access_token_expiry default.
// The refresh token is set server-side as an HttpOnly cookie and is never
// readable or writable by client-side JavaScript.
const ACCESS_MAX_AGE = 60 * 60; // 1 hour
const ATTRS = `path=/; max-age=${ACCESS_MAX_AGE}; SameSite=Strict; Secure`;

export function setAuthCookies(token: string) {
	document.cookie = `${TOKEN_KEY}=${encodeURIComponent(token)}; ${ATTRS}`;
}

export function getAuthToken(): string | null {
	return parseCookie(TOKEN_KEY);
}

/** No-op — refresh token is HttpOnly and managed by the server. */
export function getRefreshToken(): null {
	return null;
}

export function clearAuthCookies() {
	document.cookie = `${TOKEN_KEY}=; path=/; max-age=0; SameSite=Strict; Secure`;
}

function parseCookie(key: string): string | null {
	const match = document.cookie.match(new RegExp(`(?:^|;\\s*)${key}=([^;]*)`));
	return match ? decodeURIComponent(match[1]) : null;
}
