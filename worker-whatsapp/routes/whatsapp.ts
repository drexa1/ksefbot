import {Env} from "../worker";
import {CloudflareKV} from "../repository/kv";
import {IncomingMessage} from "../types/whatsapp";
import {saveImage} from "../clients/whatsapp";

const kv = new CloudflareKV();

export async function verificationHandler(request: Request, env: Env): Promise<Response> {
    const url = new URL(request.url);
    const mode = url.searchParams.get("hub.mode");
    const token = url.searchParams.get("hub.verify_token");
    const challenge = url.searchParams.get("hub.challenge");
    if (mode === "subscribe" && token === env.WHATSAPP_VERIFY_TOKEN && challenge)
        return new Response(challenge);
    return new Response("Forbidden", { status: 403 });
}

export async function testSendout(request: Request, env: Env): Promise<Response> {
    const body = await request.json() as { to?: string, message?: string };
    if (!body.to || !body.message)
        return Response.json({ error: "'to' and 'message' are required" }, { status: 400 });
    const url = `https://graph.facebook.com/${env.META_API_VERSION}/${env.WHATSAPP_PHONE_ID}/messages`;
    const payload = {
        messaging_product: "whatsapp",
        recipient_type: "individual",
        to: body.to,
        type: "text",
        text: { preview_url: false, body: body.message }
    };
    console.log("Whatsapp request:", JSON.stringify({ url, phoneId: env.WHATSAPP_PHONE_ID, payload }));
    const response = await fetch(url, {
        method: "POST",
        headers: { "Authorization": `Bearer ${env.WHATSAPP_ACCESS_TOKEN}`, "Content-Type": "application/json" },
        body: JSON.stringify(payload)
    });
    const result = await response.json();
    console.log("Whatsapp response:", JSON.stringify({ status: response.status, result }));
    return Response.json(result, { status: response.status });
}

export async function messageHandler(request: Request, env: Env): Promise<Response> {
    const incomingMessage = await request.json() as IncomingMessage;
    const message = incomingMessage.entry[0].changes[0].value.messages?.[0];
    console.info(`Message received from ${message?.from}`, incomingMessage);
    await kv.binding(env.KV).save(`in::${message?.from}::${message?.timestamp}`, incomingMessage.entry[0].changes[0].value.messages);
    // If the message contains an image, save it in the user folder
    if (message?.image)
        await saveImage(message, env);
    // If new
        // 1. Language choice
        // 2. Onboarding flow
    // Existing user
    const text = message?.text?.body;
    return Response.json("OK", { status: 200 });
}