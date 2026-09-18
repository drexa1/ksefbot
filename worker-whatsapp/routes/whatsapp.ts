import {Env} from "../worker";

export async function verificationHandler(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const mode = url.searchParams.get("hub.mode");
    const token = url.searchParams.get("hub.verify_token");
    const challenge = url.searchParams.get("hub.challenge");
    if (mode === "subscribe" && token === env.WHATSAPP_VERIFY_TOKEN && challenge)
        return new Response(challenge);
    return new Response("Forbidden", { status: 403 });
}

export async function messageHandler(request: Request, env: Env): Promise<Response> {
    const body = await request.json();
    console.log("Webhook:", JSON.stringify(body, null, 2));
    // TODO: process incoming messages here
    return new Response("OK");
}

export async function test(request: Request, env: Env): Promise<Response> {
    const body = await request.json() as { to?: string, message?: string };
    if (!body.to || !body.message)
        return Response.json({ error: "'to' and 'message' are required" }, { status: 400 });
    const response = await fetch(`https://graph.facebook.com/${env.META_API_VERSION}/${env.WHATSAPP_PHONE_ID}/messages`, {
            method: "POST",
            headers: { "Authorization": `Bearer ${env.WHATSAPP_ACCESS_TOKEN}`, "Content-Type": "application/json" },
            body: JSON.stringify({
                messaging_product: "whatsapp",
                recipient_type: "individual",
                to: body.to,
                type: "text",
                text: { preview_url: false, body: body.message }
            })
        }
    );
    const result = await response.json();
    if (!response.ok)
        return Response.json({ error: "WhatsApp API error", details: result }, { status: response.status });
    return Response.json(result);
}