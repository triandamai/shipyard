/**
 * Shipyard Edge Runtime
 * A Deno HTTP server that dynamically loads and serves user functions.
 *
 * Env vars (injected by Shipyard):
 *   SHIPYARD_RUNTIME_API_URL  — base URL of the Shipyard API, e.g. http://shipyard-api:3001
 *   SHIPYARD_RUNTIME_ORG_ID   — UUID of the organisation whose functions to serve
 *   SHIPYARD_RUNTIME_SECRET   — shared secret for authenticating API/reload calls
 *   PORT                      — listening port (default 8000)
 */

const API_URL = Deno.env.get("SHIPYARD_RUNTIME_API_URL") ?? "http://shipyard-api:3001";
const ORG_ID  = Deno.env.get("SHIPYARD_RUNTIME_ORG_ID")  ?? "";
const SECRET  = Deno.env.get("SHIPYARD_RUNTIME_SECRET")  ?? "";
const PORT    = Number(Deno.env.get("PORT") ?? "8000");

type Handler = (req: Request) => Response | Promise<Response>;

interface FunctionEntry {
  handler:      Handler;
  timeout_secs: number;
  env:          Record<string, string>;
}

// Monotonically increasing counter per function name — used to write unique
// temp files so Deno's module cache doesn't serve stale code on reload.
const reloadCounter = new Map<string, number>();

let registry = new Map<string, FunctionEntry>();

// ── Manifest types ─────────────────────────────────────────────────────────────

interface ManifestEntry {
  name:         string;
  code:         string;
  env:          Record<string, string>;
  timeout_secs: number;
}

// ── Load manifest from API ─────────────────────────────────────────────────────

async function fetchManifest(): Promise<ManifestEntry[]> {
  const url = `${API_URL}/api/internal/edge-runtime/manifest?org_id=${ORG_ID}`;
  const res = await fetch(url, {
    headers: { Authorization: `Bearer ${SECRET}` },
  });
  if (!res.ok) {
    throw new Error(`manifest fetch failed: ${res.status} ${await res.text()}`);
  }
  return res.json();
}

// ── Load a single function from code string via temp file ─────────────────────
//
// Deno caches modules by URL, so each reload needs a unique file path.
// We increment a per-function counter and remove the previous temp file
// after the new one is imported.

async function loadFunction(
  name: string,
  code: string,
): Promise<Handler | null> {
  const counter = (reloadCounter.get(name) ?? 0) + 1;
  reloadCounter.set(name, counter);

  const tmpPath = `/tmp/shipyard-fn-${name}-${counter}.ts`;
  const prevPath = `/tmp/shipyard-fn-${name}-${counter - 1}.ts`;

  await Deno.writeTextFile(tmpPath, code);

  try {
    const mod = await import(`file://${tmpPath}`);
    if (typeof mod.default !== "function") {
      console.warn(`  ✗ skipped /${name} — no default export`);
      return null;
    }
    return mod.default as Handler;
  } catch (e) {
    console.error(`  ✗ error   /${name}: ${e}`);
    return null;
  } finally {
    // Remove the previous temp file (the current one is still needed for serving).
    if (counter > 1) {
      try { await Deno.remove(prevPath); } catch { /* ignore */ }
    }
  }
}

// ── Reload all functions from the API ─────────────────────────────────────────

async function reload(): Promise<void> {
  console.log(`[reload] fetching manifest from ${API_URL}`);

  let manifest: ManifestEntry[];
  try {
    manifest = await fetchManifest();
  } catch (e) {
    console.error(`[reload] failed: ${e}`);
    return;
  }

  const newRegistry = new Map<string, FunctionEntry>();
  for (const entry of manifest) {
    const handler = await loadFunction(entry.name, entry.code);
    if (handler) {
      newRegistry.set(entry.name, {
        handler,
        timeout_secs: entry.timeout_secs,
        env: entry.env,
      });
      console.log(`  ✓ loaded  /${entry.name}`);
    }
  }

  registry = newRegistry;
  console.log(`[reload] done — ${registry.size} function(s) active`);
}

// ── Invoke handler ─────────────────────────────────────────────────────────────

async function invokeFunction(
  entry: FunctionEntry,
  req: Request,
  fnPath: string,
): Promise<Response> {
  // Build a clean Request for the handler with the fn-name prefix stripped.
  const forwarded = new Request(`https://fn${fnPath}`, {
    method:  req.method,
    headers: req.headers,
    body:    ["GET", "HEAD"].includes(req.method) ? undefined : req.body,
    // deno-lint-ignore no-explicit-any
    duplex: "half" as any,
  });

  // Apply timeout
  const timeoutMs = entry.timeout_secs * 1000;
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const res = await entry.handler(forwarded);
    return res;
  } catch (e) {
    if (controller.signal.aborted) {
      return Response.json(
        { error: "function timed out" },
        { status: 504 },
      );
    }
    console.error(`[invoke] error in handler: ${e}`);
    return Response.json({ error: String(e) }, { status: 500 });
  } finally {
    clearTimeout(timer);
  }
}

// ── HTTP server ────────────────────────────────────────────────────────────────

async function handleRequest(req: Request): Promise<Response> {
  const url      = new URL(req.url);
  const segments = url.pathname.split("/").filter(Boolean);
  const fnName   = segments[0] ?? "";

  // ── /health ──────────────────────────────────────────────────────────────────
  if (!fnName || fnName === "health") {
    return Response.json({
      status:    "ok",
      org_id:    ORG_ID,
      functions: registry.size,
      names:     [...registry.keys()],
    });
  }

  // ── /reload ──────────────────────────────────────────────────────────────────
  if (fnName === "reload") {
    if (req.method !== "POST") {
      return new Response("Method Not Allowed", { status: 405 });
    }
    const auth = req.headers.get("Authorization") ?? "";
    if (auth !== `Bearer ${SECRET}`) {
      return new Response("Unauthorized", { status: 401 });
    }
    await reload();
    return Response.json({ ok: true, functions: registry.size });
  }

  // ── User function ─────────────────────────────────────────────────────────────
  const entry = registry.get(fnName);
  if (!entry) {
    return Response.json(
      { error: `function '${fnName}' not found`, available: [...registry.keys()] },
      { status: 404 },
    );
  }

  const fnPath = "/" + segments.slice(1).join("/") + url.search;
  const start  = Date.now();

  try {
    const res = await invokeFunction(entry, req, fnPath);
    console.log(`${req.method} /${fnName}${fnPath} → ${res.status} (${Date.now() - start}ms)`);
    return res;
  } catch (e) {
    console.error(`${req.method} /${fnName}${fnPath} → ERROR: ${e}`);
    return Response.json({ error: String(e) }, { status: 500 });
  }
}

// ── Bootstrap ──────────────────────────────────────────────────────────────────

if (!ORG_ID) {
  console.error("SHIPYARD_RUNTIME_ORG_ID is not set — exiting");
  Deno.exit(1);
}

console.log(`Shipyard Edge Runtime starting — org: ${ORG_ID}`);
await reload();

Deno.serve({ port: PORT }, handleRequest);
console.log(`Listening on :${PORT}`);
