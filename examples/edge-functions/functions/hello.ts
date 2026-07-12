// Basic example — returns JSON with request info.
// Deploy and call: GET https://fn.your-shipyard.com/your-org/hello
export default async function handler(req: Request): Promise<Response> {
  const url = new URL(req.url);

  return Response.json({
    message: "Hello from Shipyard Edge Functions!",
    method: req.method,
    path: url.pathname,
    query: Object.fromEntries(url.searchParams),
    timestamp: new Date().toISOString(),
  });
}
