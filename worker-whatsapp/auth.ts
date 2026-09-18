import {Env} from "./worker";

export const corsHeaders = {
    "Access-Control-Allow-Origin": "*",
    "Access-Control-Allow-Headers": "*",
    "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS"
};

export const withCors = ( res: Response) => {
    const headers = new Headers(res.headers);
    for (const [k, v] of Object.entries(corsHeaders)) {
        headers.set(k, v);
    }
    return new Response(res.body, { status: res.status, statusText: res.statusText, headers });
};

/**
 * Potential client made it through Zero Trust.
 */
export async function auth(req: Request, env: Env): Promise<boolean> {
    const url = new URL(req.url);
    switch (url.pathname) {
        //🔓 Public routes
        case "/":
        case "/openapi.json":
        case "/swagger":
        case "/docs":
        case "/health":
            return true;
        //🔒 Protected routes
        case "/whoami":
        case "/whatsapp/test":
            return req.headers.get("X-API-Key") === env.API_KEY;
        // Default
        default:
            return false;
    }
}

/**
 * Get the Cloudflare trusted client user.
 */
export async function whoami(req: Request, env: Env): Promise<Response> {
    //🛡️ CF Zero Trust logged user?
    const jwt = req.headers.get("Cf-Access-Jwt-Assertion");
    //💻 Zero Trust specific access policy, or public access
    const userId = jwt ? decodeJWT(jwt).email : undefined;
    return Response.json({ userId, ...(jwt ? { origin: "Cf-Access-Jwt" } : {}) });
}

function decodeJWT(jwt: string): { name: string, email: string } {
    const payloadBase64 = jwt.split(".")[1];
    const payloadJson = atob(payloadBase64.replace(/-/g, "+").replace(/_/g, "/"));
    const payload = JSON.parse(payloadJson);
    return { name: payload.name, email: payload.email };
}