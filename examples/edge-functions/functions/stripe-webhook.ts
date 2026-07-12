// Handle Stripe webhook events.
// Required env vars: STRIPE_WEBHOOK_SECRET, BACKEND_URL
//
// Point your Stripe webhook at:
//   https://fn.your-shipyard.com/your-org/stripe-webhook
// Events to send: checkout.session.completed, customer.subscription.deleted
export default async function handler(req: Request): Promise<Response> {
  if (req.method !== "POST") {
    return Response.json({ error: "method not allowed" }, { status: 405 });
  }

  const webhookSecret = Deno.env.get("STRIPE_WEBHOOK_SECRET");
  const backendUrl    = Deno.env.get("BACKEND_URL");

  if (!webhookSecret) {
    return Response.json({ error: "STRIPE_WEBHOOK_SECRET not configured" }, { status: 500 });
  }

  const body      = await req.text();
  const signature = req.headers.get("stripe-signature") ?? "";

  // Verify Stripe signature (timestamp + HMAC-SHA256)
  const valid = await verifyStripeSignature(webhookSecret, body, signature);
  if (!valid) {
    return Response.json({ error: "invalid signature" }, { status: 401 });
  }

  const event = JSON.parse(body);

  // Forward to your backend or handle inline
  if (backendUrl) {
    await fetch(`${backendUrl}/internal/stripe-event`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(event),
    }).catch(() => {});
  } else {
    // Inline handling
    switch (event.type) {
      case "checkout.session.completed":
        console.log("Payment completed for session:", event.data.object.id);
        break;
      case "customer.subscription.deleted":
        console.log("Subscription cancelled:", event.data.object.id);
        break;
      default:
        console.log("Unhandled event:", event.type);
    }
  }

  return Response.json({ received: true });
}

async function verifyStripeSignature(
  secret: string,
  payload: string,
  header: string,
): Promise<boolean> {
  const parts     = header.split(",").reduce<Record<string, string>>((acc, part) => {
    const [k, v] = part.split("=");
    acc[k] = v;
    return acc;
  }, {});

  const timestamp = parts["t"];
  const signature = parts["v1"];
  if (!timestamp || !signature) return false;

  const signedPayload = `${timestamp}.${payload}`;
  const key = await crypto.subtle.importKey(
    "raw",
    new TextEncoder().encode(secret),
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );

  const mac     = await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(signedPayload));
  const computed = Array.from(new Uint8Array(mac))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");

  return computed === signature;
}
