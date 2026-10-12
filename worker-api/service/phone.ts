import {Env} from "../worker";
import {AuthError} from "../types/auth";

const encode = (value: Uint8Array) => btoa(String.fromCharCode(...value)).replace(/=/g, "").replace(/\+/g, "-").replace(/\//g, "_");
const decode = (value: string) => Uint8Array.from(atob(value.replace(/-/g, "+").replace(/_/g, "/")), character => character.charCodeAt(0));
const bytes = (value: string) => new TextEncoder().encode(value);
const key = (env: Env) => crypto.subtle.importKey("raw", bytes(env.PHONE_TOKEN_SIGNER), { name: "HMAC", hash: "SHA-256" }, false, ["sign", "verify"]);

export async function hashCode(env: Env, phone: string, code: string) {
    return encode(new Uint8Array(await crypto.subtle.sign("HMAC", await key(env), bytes(`phone-code:${phone}:${code}`))));
}

export async function issueToken(env: Env, userId: string, phone: string) {
    const iat = Math.floor(Date.now() / 1000);
    const exp = iat + 30 * 24 * 60 * 60;
    const header = encode(bytes(JSON.stringify({ alg: "HS256", typ: "JWT" })));
    const payload = encode(bytes(JSON.stringify({ sub: userId, phone, iat, exp, iss: "ksefbot-api", aud: "ksefbot-cli" })));
    const signature = encode(new Uint8Array(await crypto.subtle.sign("HMAC", await key(env), bytes(`${header}.${payload}`))));
    return { token: `${header}.${payload}.${signature}`, expiresAt: new Date(exp * 1000).toISOString() };
}

export async function verifyToken(req: Request, env: Env) {
    try {
        const authorization = req.headers.get("Authorization") ?? "";
        const [header, payload, signature] = authorization.slice(7).split(".");
        const valid = await crypto.subtle.verify("HMAC", await key(env), decode(signature), bytes(`${header}.${payload}`));
        const metadata = JSON.parse(new TextDecoder().decode(decode(header)));
        const claims = JSON.parse(new TextDecoder().decode(decode(payload)));
        if (valid
            && metadata.typ === "JWT"
            && claims?.iss === "ksefbot-api"
            && claims.aud === "ksefbot-cli"
            && Number.isFinite(claims.exp)
            && claims.exp > Math.floor(Date.now() / 1000)
        ) return { userId: claims.sub, phone: claims.phone };
    } catch {
        throw new AuthError("Invalid or expired phone login", 401);
    }
    throw new AuthError("Invalid or expired phone login", 401);
}
