import {KVNamespace, R2Bucket, Fetcher} from "@cloudflare/workers-types"
import {auth, withCors, corsHeaders} from "./auth";
import {Method, routes} from "./routes/routes";

export interface Env {
    KV: KVNamespace
    R2: R2Bucket
    KSEFBOT: Fetcher
    KSEFBOT_BASE_URL: string
    API_KEY: string
    WHATSAPP_VERIFY_TOKEN: string
    WHATSAPP_ACCESS_TOKEN: string
    META_API_VERSION: string
    WHATSAPP_PHONE_ID: string
}

// noinspection JSUnusedGlobalSymbols
export default {
    async fetch(req: Request, env: Env): Promise<Response> {
        if (req.method === "OPTIONS") return new Response(null, { status: 204, headers: corsHeaders });
        if (!await auth(req, env))
            return new Response("Unauthorized", { status: 401, headers: corsHeaders });
        const url = new URL(req.url);
        const routePath = routes[url.pathname];
        if (!routePath)
            return new Response("Not Found", { status: 404, headers: corsHeaders });
        const route = routePath[req.method as Method]!;
        const response = await route(req, env);
        return withCors(response);
    }
};