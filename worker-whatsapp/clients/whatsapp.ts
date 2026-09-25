import {Env} from "../worker";
import {WhatsappMessage} from "../types/whatsapp";

export async function sendTemplate( env: Env, to: string, template: string, language: string = "en") {
    const response = await fetch(`https://graph.facebook.com/${env.META_API_VERSION}/${env.WHATSAPP_PHONE_ID}/messages`, {
        method: "POST",
        headers: { "Authorization": `Bearer ${env.WHATSAPP_ACCESS_TOKEN}`, "Content-Type": "application/json" },
        body: JSON.stringify({
            messaging_product: "whatsapp",
            to,
            type: "template",
            template: {
                name: template,
                language: { code: language }
            }
        })}
    );
    if (!response.ok)
        console.error("WhatsApp error:", await response.text());
    return response;
}

export async function sendFlow(env: Env, to: string, flowId: string, language = "en") {
    const response = await fetch(`https://graph.facebook.com/${env.META_API_VERSION}/${env.WHATSAPP_PHONE_ID}/messages`, {
        method: "POST",
        headers: {"Authorization": `Bearer ${env.WHATSAPP_ACCESS_TOKEN}`, "Content-Type": "application/json"},
        body: JSON.stringify({
            messaging_product: "whatsapp",
            recipient_type: "individual",
            to: to,
            type: "interactive",
            interactive: {
                type: "flow",
                body: { text: "..." },
                action: {
                    name: "flow",
                    parameters: {
                        flow_message_version: "3",
                        flow_id: flowId,
                        flow_cta: "Start flow: "
                    }
                }
            }
        })
    });
    const result = await response.json();
    console.log("Whatsapp flow response:", JSON.stringify({status: response.status, result}));
    if (!response.ok)
        throw new Error(`Failed to send flow: ${response.status} ${JSON.stringify(result)}`);
    return result;
}

export async function saveImage(env: Env, message: WhatsappMessage) {
    try {
        const image = await downloadImage(env, message);
        const fileExtension = message.image!.mime_type.split("/")[1];
        const key = `user/images/${message.from}/${message.image!.id}.${fileExtension}`;
        await env.R2.put(key, await image.arrayBuffer());
        console.info(`Saved image from ${message.from} message to R2 ${key}`);
    } catch (error) {
        console.error("Saving image to R2 failed", error);
    }
}

async function downloadImage(env: Env, message: WhatsappMessage): Promise<Response> {
    const response = await fetch(message.image!.url, { headers: { Authorization: `Bearer ${env.WHATSAPP_ACCESS_TOKEN}` }});
    if (!response.ok || !response.body)
        throw new Error(`Failed to download media: ${response.status} ${await response.text()}`);
    return response;
}