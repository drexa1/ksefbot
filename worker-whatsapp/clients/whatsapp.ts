import {Env} from "../worker";
import {WhatsappMessage} from "../types/whatsapp";

export async function saveImage(message: WhatsappMessage, env: Env) {
    try {
        const image = await downloadImage(message, env);
        const fileExtension = message.image!.mime_type.split("/")[1];
        const key = `whatsapp/${message.from}/${message.image!.id}.${fileExtension}`;
        await env.R2.put(key, await image.arrayBuffer());
        console.info(`Saved image from ${message.from} message to R2 ${key}`);
    } catch (error) {
        console.error("Saving image to R2 failed", error);
    }
}

async function downloadImage(message: WhatsappMessage, env: Env): Promise<Response> {
    const response = await fetch(message.image!.url, { headers: { Authorization: `Bearer ${env.WHATSAPP_ACCESS_TOKEN}` }});
    if (!response.ok || !response.body)
        throw new Error(`Failed to download media: ${response.status} ${await response.text()}`);
    return response;
}