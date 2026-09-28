import {Env} from "./worker";
import {D1Driver, Repository} from "./repository/d1";
import {AuthError} from "./types/auth";
import {AppUser} from "./types/users";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

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
        case "/ksef/sales":
        case "/ksef/sales/sessions":
        case "/ksef/sales/status":
        case "/ksef/sales/receipt":
        case "/ksef/purchases":
        case "/gov/contractors":
        case "/gov/ceidg":
        case "/gov/krs":
        case "/gov/vat-lb":
        case "/app/users":
        case "/app/invoices":
        case "/app/invoices/pii":
        case "/app/contractors":
        case "/app/taxes/simulate":
        case "/app/taxes":
            return req.headers.get("X-API-Key") === env.API_KEY;
        // Default
        default:
            return false;
    }
}

/**
 * Returns the application user for the authenticated user.
 */
export async function getAuthUser(req: Request, env: Env): Promise<AppUser> {
    const whoamiResponse = await whoami(req, env);
    const { userId, origin } = await whoamiResponse.json() as { userId: string, origin?: "Cf-Access-Jwt" };
    const appUser = origin === "Cf-Access-Jwt"
        // If it is directly connected via specific CF Zero Trust policy use email (or the policy method),
        ? await getRepo(env).get<AppUser>("users", { email: userId })
        // ...otherwise find by PK (tax identifier)
        : await getRepo(env).get<AppUser>("users", { id: userId });
    //❌ This should never trigger, either have created a specific access policy in Zero Trust or either the client made it through
    if (!appUser) throw new AuthError("Authenticated user not found in app", 404, { userId });
    return appUser;
}

/**
 * Get the Cloudflare trusted client user.
 */
export async function whoami(req: Request, env: Env): Promise<Response> {
    //🐛 For local development without Zero Trust, return the admin user just to simplify testing...
    if (env.ENVIRONMENT === "dev") {
        const adminUser =  await getRepo(env).getAll<AppUser>("users", { tier: 0 });
        return Response.json({ userId: adminUser[0].email, origin: "Cf-Access-Jwt" });
    }
    //🛡️ CF Zero Trust logged user?
    const jwt = req.headers.get("Cf-Access-Jwt-Assertion");
    //💻 User identifier|email from client (that made it through CF Zero Trust), or Zero Trust specific access policy
    const userId = jwt ? decodeJWT(jwt).email : req.headers.get("X-User-Id");
    if (!userId) throw new AuthError("Unauthenticated user", 401);
    console.info("[Whoami] requester:", userId);
    return Response.json({ userId, ...(jwt ? { origin: "Cf-Access-Jwt" } : {}) });
}

function decodeJWT(jwt: string): { name: string, email: string } {
    const payloadBase64 = jwt.split(".")[1];
    const payloadJson = atob(payloadBase64.replace(/-/g, "+").replace(/_/g, "/"));
    const payload = JSON.parse(payloadJson);
    return { name: payload.name, email: payload.email };
}