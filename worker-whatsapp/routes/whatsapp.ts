import {Env} from "../worker";
import {CloudflareKV} from "../repository/kv";
import {IncomingMessage} from "../types/whatsapp";
import {saveImage, sendFlow, sendTemplate, sendText} from "../clients/whatsapp";
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
    try {
        console.info(`Message request for ${body.to}`, body.message);
        const result = await sendText(env, body.to, body.message);
        console.info("Message response:", result);
        return Response.json(result);
    } catch (error) {
        console.error("Failed to send message:", error);
        return Response.json({error: error instanceof Error ? error.message : String(error)}, { status: 500 });
    }
}

export async function testFlow(request: Request, env: Env): Promise<Response> {
    const body = await request.json() as { to: string, message: string, buttonCaption: string, flowId: string };
    if (!body.to || !body.message || !body.buttonCaption || !body.flowId)
        return Response.json({ error: "'to' and 'flowId' are required" }, {status: 400});
    try {
        console.info(`Flow request for ${body.to}`, body.flowId);
        const result = await sendFlow(env, body.to, body.message, body.buttonCaption, body.flowId);
        console.info("`Flow response:", result);
        return Response.json(result);
    } catch (error) {
        console.error("Failed to send flow:", error);
        return Response.json({error: error instanceof Error ? error.message : String(error)}, {status: 500});
    }
}

export async function messageHandler(request: Request, env: Env): Promise<Response> {
    const rawBody = await request.text();
    console.info("Whatsapp webhook raw:", rawBody);
    let incomingMessage: IncomingMessage;
    try {
        incomingMessage = JSON.parse(rawBody) as IncomingMessage;
    } catch (error) {
        console.error("Invalid webhook JSON:", error);
        return Response.json({ error: "invalid JSON" }, { status: 400 });
    }
    const message = incomingMessage.entry?.[0]?.changes?.[0]?.value?.messages?.[0];
    if (!message) {
        console.info("Webhook contains no incoming message");
        return Response.json("OK", { status: 200 });
    }
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