import {Env} from "../worker";
import {WhatsappMessage} from "../types/whatsapp";

export async function saveImage(message: WhatsappMessage, env: Env) {
    const imageResponse = await downloadImage(message, env);
    const fileExtension = message.image!.mime_type.split("/")[1];
    const key = `whatsapp/${message.from}/${message.image!.id}.${fileExtension}`;
    await env.R2.put(key, await imageResponse.arrayBuffer(), {
        httpMetadata: {contentType: message.image!.mime_type},
        customMetadata: {whatsappMediaId: message.image!.id, sha256: message.image!.sha256}
    });
}

async function downloadImage(message: WhatsappMessage, env: Env): Promise<Response> {
    const response = await fetch(message.image!.url, { headers: { Authorization: `Bearer ${env.WHATSAPP_ACCESS_TOKEN}` }});
    if (!response.ok || !response.body)
        throw new Error(`Failed to download media: ${response.status} ${await response.text()}`);
    return response;
}