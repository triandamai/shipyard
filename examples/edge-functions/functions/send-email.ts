// Send a transactional email via Resend.
// Required env vars: RESEND_API_KEY, FROM_ADDRESS
//
// POST https://fn.your-shipyard.com/your-org/send-email
// Body: { "to": "user@example.com", "subject": "Hello", "html": "<p>Hi</p>" }
export default async function handler(req: Request): Promise<Response> {
  if (req.method !== "POST") {
    return Response.json({ error: "method not allowed" }, { status: 405 });
  }

  const apiKey = Deno.env.get("RESEND_API_KEY");
  const from   = Deno.env.get("FROM_ADDRESS") ?? "no-reply@example.com";

  if (!apiKey) {
    return Response.json({ error: "RESEND_API_KEY not configured" }, { status: 500 });
  }

  let body: { to: string; subject: string; html?: string; text?: string };
  try {
    body = await req.json();
  } catch {
    return Response.json({ error: "invalid JSON body" }, { status: 400 });
  }

  if (!body.to || !body.subject) {
    return Response.json({ error: "missing required fields: to, subject" }, { status: 400 });
  }

  const res = await fetch("https://api.resend.com/emails", {
    method: "POST",
    headers: {
      "Authorization": `Bearer ${apiKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      from,
      to: body.to,
      subject: body.subject,
      html: body.html ?? `<p>${body.text ?? body.subject}</p>`,
    }),
  });

  const data = await res.json();

  if (!res.ok) {
    return Response.json({ error: "resend error", detail: data }, { status: 502 });
  }

  return Response.json({ ok: true, id: data.id }, { status: 200 });
}
