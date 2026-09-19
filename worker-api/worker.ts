import {D1Database} from "@cloudflare/workers-types"
import {Method, routes} from "./routes";
import {auth, withCors, corsHeaders} from "./auth";

export interface Env {
    ENVIRONMENT: "dev" | "cloudflare"
    TEST_MODE: boolean
    // KSeF (National e-Invoicing System)
    KSEF_URL: string
    KSEF_INVOICE_SCHEMA: string
    KSEF_MIN_TIMEOUT: number
    KSEF_MAX_TIMEOUT: number
    // CEIDG (Central Register on Business Activity)
    CEIDG_URL: string
    CEIDG_API_KEY: string
    CEIDG_LOOKUP_SCHEMA: string
    // KRS (National Court Registry)
    KRS_SEARCH_URL: string
    KRS_API_URL: string
    KRS_LOOKUP_SCHEMA: string
    // VAT payers whitelist
    VAT_LB_URL: string
    VAT_LB_LOOKUP_SCHEMA: string
    // Taxes
    DEFAULT_VAT_PERCENTAGE: number
    DEFAULT_TAX_RATE: number
    DEFAULT_HEALTH_INSURANCE_BASE: number
    DEFAULT_HEALTH_INSURANCE_RATE: number
    // App bindings
    API_KEY: string
    D1: D1Database
    assets: Fetcher
}

// noinspection JSUnusedGlobalSymbols
export default {
    async fetch(req: Request, env: Env): Promise<Response> {
        if (req.method === "OPTIONS")
            return new Response(null, { status: 204, headers: corsHeaders });
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