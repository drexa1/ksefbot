import {Env} from "../worker";
import {CloudflareKV} from "../repository/kv";
import {IncomingMessage} from "../types/whatsapp";
import {saveImage, sendFlow, sendTemplate} from "../clients/whatsapp";
import {initializeUser} from "../clients/ksefbot";
import {attendExistingUser, triggerOnboarding} from "../flows/onboarding";

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

export async function testMessage(request: Request, env: Env): Promise<Response> {
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

export async function testFlow(request: Request, env: Env): Promise<Response> {
    const body = await request.json() as {to?: string, flowId?: string};
    if (!body.to || !body.flowId)
        return Response.json({error: "'to' and 'flowId' are required"}, {status: 400});
    try {
        const result = await sendFlow(env, body.to, body.flowId);
        return Response.json(result);
    } catch (error) {
        console.error("Failed to send flow:", error);
        return Response.json({error: error instanceof Error ? error.message : String(error)}, {status: 500});
    }
}

export async function messageHandler(request: Request, env: Env): Promise<Response> {
    const incomingMessage = await request.json() as IncomingMessage;
    const message = incomingMessage.entry[0].changes[0].value.messages?.[0];
    if (!message)
        return Response.json("OK", { status: 200 });
    console.info(`Message received from ${message.from}`, incomingMessage);
    await kv.binding(env.KV).save(`in::${message.from}::${message.timestamp}`, incomingMessage.entry[0].changes[0].value.messages);
    if (message.image)
        await saveImage(env, message);
    const user = await env.KSEFBOT.fetch(`${env.KSEFBOT_BASE_URL}/app/users?phone=${message.from}`, {
        method: "GET",
        headers: { "Content-Type": "application/json", "X-API-Key": env.API_KEY }
    });
    if (user.ok) {
        const language: "en" | "pl" = ({ language_en: "en", language_pl: "pl" } as const)[message.button?.payload!] ?? "en";
        if (!language) {
            // Unseen user -> language choice
            await sendTemplate(env, message.from, "onboarding_language");
        } else {
            // 🐣 Initialize user with language preference
            await initializeUser(env, message.from, language);
            await triggerOnboarding(env, message.from, language);
        }
    } else {
        await attendExistingUser(env, message);
    }
    return Response.json("OK", { status: 200 });
}