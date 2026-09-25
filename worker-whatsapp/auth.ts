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
        case "/whatsapp/webhooks":
        case "/whatsapp/flows/onboarding":
            return true;
        //🔒 Protected routes
        case "/whatsapp/test/message":
        case "/whatsapp/test/flow":
            return req.headers.get("X-API-Key") === env.API_KEY;
        // Default
        default:
            return false;
    }
}