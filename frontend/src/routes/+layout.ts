// This is a fully client-side dashboard app — disable SSR globally.
// All data is fetched after mount with user-specific auth tokens,
// so server rendering provides no benefit and causes `page.params`
// access errors on the server.
export const ssr = false;
