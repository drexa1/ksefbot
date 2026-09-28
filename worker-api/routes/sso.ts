import {Env} from "../worker";

export async function loginWithGoogle(req: Request, env: Env): Promise<Response> {
    const callback = req.headers.get("X-OAuth-Callback")!;
    const state = crypto.randomUUID();
    const authorizeUrl = new URL(env.GOOGLE_AUTH_URL);
    authorizeUrl.search = new URLSearchParams({
        client_id: env.GOOGLE_CLIENT_ID,
        redirect_uri: env.GOOGLE_CALLBACK_URI,
        response_type: "code",
        scope: "openid email profile",
        state: `${state}:${callback}`
    }).toString();
    return Response.json({authorizationUrl: authorizeUrl.toString()});
}

export async function loginWithGoogleCallback(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    const error = url.searchParams.get("error");
    const state = url.searchParams.get("state")!;
    const callback = state.substring(state.indexOf(":") + 1);
    const callbackUrl = new URL(callback);
    if (error) {
        callbackUrl.searchParams.set("error", error);
        return Response.redirect(callbackUrl.toString(), 302);
    }
    const code = url.searchParams.get("code")!;
    const tokenResponse = await fetch(env.GOOGLE_AUTH_TOKEN_URL, {
        method: "POST",
        headers: {"Content-Type": "application/x-www-form-urlencoded"},
        body: new URLSearchParams({
            client_id: env.GOOGLE_CLIENT_ID,
            client_secret: env.GOOGLE_CLIENT_SECRET,
            code,
            grant_type: "authorization_code",
            redirect_uri: env.GOOGLE_CALLBACK_URI
        })
    });
    const tokenBody = await tokenResponse.text();
    const token = JSON.parse(tokenBody) as {access_token: string};
    const userResponse = await fetch(env.GOOGLE_AUTH_USERINFO_URL, {
        headers: {Authorization: `Bearer ${token.access_token}`}
    });
    const userBody = await userResponse.text();
    console.log("[Google UserInfo]", userResponse.status, userBody);
    const googleUser = JSON.parse(userBody) as { sub: string, email: string, name: string };
    callbackUrl.searchParams.set("email", googleUser.email);
    callbackUrl.searchParams.set("name", googleUser.name);
    return Response.redirect(callbackUrl.toString(), 302);
}