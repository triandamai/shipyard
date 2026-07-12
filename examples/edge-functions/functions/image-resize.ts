// Fetch a remote image and return it resized via Canvas API.
// Works in Deno — no native deps needed.
//
// GET https://fn.your-shipyard.com/your-org/image-resize?url=https://...&w=400&h=300
export default async function handler(req: Request): Promise<Response> {
  const { searchParams } = new URL(req.url);
  const imageUrl = searchParams.get("url");
  const width    = Number(searchParams.get("w") ?? 400);
  const height   = Number(searchParams.get("h") ?? 300);

  if (!imageUrl) {
    return Response.json({ error: "missing ?url= parameter" }, { status: 400 });
  }

  if (width > 2000 || height > 2000) {
    return Response.json({ error: "max dimensions are 2000x2000" }, { status: 400 });
  }

  // Fetch the source image
  const upstream = await fetch(imageUrl);
  if (!upstream.ok) {
    return Response.json({ error: "could not fetch image" }, { status: 502 });
  }

  const contentType = upstream.headers.get("content-type") ?? "image/jpeg";
  if (!contentType.startsWith("image/")) {
    return Response.json({ error: "URL does not point to an image" }, { status: 400 });
  }

  // Pass through for now — real resize requires a Wasm image lib (e.g. @cf/image-resize)
  // This demonstrates the pattern; swap the body for actual resize logic.
  const imageBytes = await upstream.arrayBuffer();

  return new Response(imageBytes, {
    headers: {
      "Content-Type": contentType,
      "X-Original-Width": String(width),
      "X-Original-Height": String(height),
      "Cache-Control": "public, max-age=86400",
    },
  });
}
