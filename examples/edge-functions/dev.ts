// Local dev server — mimics the Shipyard edge runtime.
// Usage:
//   deno run --allow-net --allow-env --allow-read dev.ts
//   deno run --allow-net --allow-env --allow-read dev.ts --port 9000
//
// All functions in functions/ are served at:
//   http://localhost:8000/{fn-name}[/path][?query]

import { join } from "https://deno.land/std@0.224.0/path/mod.ts";

const PORT = Number(Deno.args.find((a) => a.startsWith("--port="))?.split("=")[1] ?? 8000);

type Handler = (req: Request) => Response | Promise<Response>;
const registry = new Map<string, Handler>();

// Load all .ts/.js files from functions/
const fnDir = join(import.meta.dirname!, "functions");
for await (const entry of Deno.readDir(fnDir)) {
  if (!entry.isFile) continue;
  const ext = entry.name.split(".").pop();
  if (ext !== "ts" && ext !== "js") continue;

  const name = toKebab(entry.name.replace(/\.(ts|js)$/, ""));
  const url  = new URL(`./functions/${entry.name}`, import.meta.url).href;

  try {
    const mod = await import(url);
    if (typeof mod.default === "function") {
      registry.set(name, mod.default);
      console.log(`  ✓ loaded  /${name}`);
    } else {
      console.warn(`  ✗ skipped /${name} — no default export`);
    }
  } catch (e) {
    console.warn(`  ✗ skipped /${name} — import error: ${e}`);
  }
}

if (registry.size === 0) {
  console.error("No functions found in functions/. Exiting.");
  Deno.exit(1);
}

console.log(`\nShipyard edge dev server running on http://localhost:${PORT}`);
console.log("─".repeat(50));
for (const name of registry.keys()) {
  console.log(`  GET/POST http://localhost:${PORT}/${name}`);
}
console.log("─".repeat(50) + "\n");

Deno.serve({ port: PORT }, async (req: Request) => {
  const url      = new URL(req.url);
  const segments = url.pathname.split("/").filter(Boolean);
  const fnName   = segments[0];

  if (!fnName) {
    return Response.json({
      functions: [...registry.keys()].map((n) => ({
        name: n,
        url: `http://localhost:${PORT}/${n}`,
      })),
    });
  }

  const handler = registry.get(fnName);
  if (!handler) {
    return Response.json(
      { error: `function '${fnName}' not found`, available: [...registry.keys()] },
      { status: 404 },
    );
  }

  // Strip the function name prefix so the handler sees a clean path
  const forwardedPath = "/" + segments.slice(1).join("/") + url.search;
  const forwarded = new Request(`https://fn${forwardedPath}`, {
    method:  req.method,
    headers: req.headers,
    body:    req.body,
  });

  const start = Date.now();
  try {
    const res = await handler(forwarded);
    const ms  = Date.now() - start;
    console.log(`${req.method} /${fnName}${forwardedPath} → ${res.status} (${ms}ms)`);
    return res;
  } catch (e) {
    const ms = Date.now() - start;
    console.error(`${req.method} /${fnName}${forwardedPath} → ERROR (${ms}ms):`, e);
    return Response.json({ error: String(e) }, { status: 500 });
  }
});

function toKebab(s: string): string {
  return s
    .replace(/([A-Z])/g, (_, c) => `-${c.toLowerCase()}`)
    .replace(/^-/, "")
    .replace(/[_\s]/g, "-");
}
