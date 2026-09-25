import {FlowRequest} from "../types/whatsapp";

export async function decryptFlowRequest(request: FlowRequest, privateKeyPem: string) {
    const privateKey = await importPrivateKey(privateKeyPem);
    const aesKeyRaw = await crypto.subtle.decrypt({ name: "RSA-OAEP" }, privateKey, base64ToBytes(request.encrypted_aes_key));
    const aesKey = await crypto.subtle.importKey("raw", aesKeyRaw, { name: "AES-GCM" }, false, ["decrypt", "encrypt"]);
    const iv = base64ToBytes(request.initial_vector);
    const encryptedData = base64ToBytes(request.encrypted_flow_data);
    const decrypted = await crypto.subtle.decrypt({ name: "AES-GCM", iv }, aesKey, encryptedData);
    const payload = JSON.parse(new TextDecoder().decode(decrypted));
    return { payload, aesKey, iv };
}

export async function encryptFlowResponse(payload: Record<string, unknown>, aesKey: CryptoKey, iv: Uint8Array): Promise<string> {
    const responseIv = new Uint8Array(iv.length);
    for (let i = 0; i < iv.length; i++) {
        responseIv[i] = (~iv[i]) & 0xff;
    }
    const plaintext = new TextEncoder().encode(JSON.stringify(payload));
    const encrypted = await crypto.subtle.encrypt({ name: "AES-GCM", iv: responseIv }, aesKey, plaintext);
    return bytesToBase64(new Uint8Array(encrypted));
}

async function importPrivateKey(pem: string): Promise<CryptoKey> {
    return crypto.subtle.importKey("pkcs8", pemToArrayBuffer(pem), { name: "RSA-OAEP", hash: "SHA-256" }, false, ["decrypt"]);
}

function pemToArrayBuffer(pem: string): ArrayBuffer {
    const base64 = pem
        .replace(/-----BEGIN PRIVATE KEY-----/g, "")
        .replace(/-----END PRIVATE KEY-----/g, "")
        .replace(/\s/g, "");
    return base64ToBytes(base64).slice().buffer as ArrayBuffer;
}

function base64ToBytes(value: string): Uint8Array<ArrayBuffer> {
    const binary = atob(value);
    const bytes = new Uint8Array(new ArrayBuffer(binary.length));
    for (let i = 0; i < binary.length; i++) {
        bytes[i] = binary.charCodeAt(i);
    }
    return bytes;
}

function bytesToBase64(bytes: Uint8Array): string {
    let binary = "";
    for (const byte of bytes) {
        binary += String.fromCharCode(byte);
    }
    return btoa(binary);
}