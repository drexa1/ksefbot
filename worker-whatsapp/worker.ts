interface Env {
    WHATSAPP_VERIFY_TOKEN: string
}

// noinspection JSUnusedGlobalSymbols
export default {
    async fetch(request: Request, env: Env): Promise<Response> {
        const url = new URL(request.url);
        if (url.pathname === "/whatsapp/webhooks" && request.method === "GET")
            return verificationHandler(request, env);
        if (url.pathname === "/whatsapp/webhooks" && request.method === "POST")
            return messageHandler(request, env);
        return new Response("Not found", { status: 404 });
    }
};

async function verificationHandler(request: Request, env: Env,): Promise<Response> {
    const url = new URL(request.url);
    const mode = url.searchParams.get("hub.mode");
    const token = url.searchParams.get("hub.verify_token");
    const challenge = url.searchParams.get("hub.challenge");
    if (mode === "subscribe" && token === env.WHATSAPP_VERIFY_TOKEN && challenge)
        return new Response(challenge);
    return new Response("Forbidden", { status: 403 });
}

async function messageHandler(request: Request, env: Env): Promise<Response> {
    const body = await request.json();
    // We'll process messages here next.
    return new Response("OK");
}