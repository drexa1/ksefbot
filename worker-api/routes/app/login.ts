import {Env} from "../../worker";
import {AppUser} from "../../types/users";
import {AuthError, PhoneLoginChallenge} from "../../types/auth";
import {D1Driver, Repository} from "../../repository/d1";
import {hashCode, issueToken, verifyToken} from "../../service/phone";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

export async function get(req: Request, env: Env): Promise<Response> {
    const { userId, phone } = await verifyToken(req, env);
    const user = await getRepo(env).get<AppUser>("users", { id: userId, phone: phone });
    return Response.json({ phone: user.phone, email: user.email });
}

export async function post(req: Request, env: Env): Promise<Response> {
    const body = await req.json() as { phone: string, code?: string };
    const user = await getRepo(env).get<AppUser>("users", { phone: body.phone });
    if (body.code)
        return verifyPhoneCode(env, user, body.phone, body.code);
    return sendPhoneCode(env, user, body.phone);
}

async function verifyPhoneCode(env: Env, user: AppUser, phone: string, code: string): Promise<Response> {
    // Validate 6-digit code
    if (!/^\d{6}$/.test(code))
        throw new AuthError("Enter a six-digit code", 400);
    // Retrieve stored login challenge
    const hash = await hashCode(env, phone, code);
    const challenge = await getRepo(env).get<PhoneLoginChallenge>("login_challenges", { phone });
    const now = new Date().toISOString();
    // Check expiration, attempts, and previous use
    if (!challenge || challenge.expiresAt <= now || challenge.attempts >= 3)
        throw new AuthError("Invalid or expired verification code", 401);
    // Update attempts and mark valid code as consumed
    const { updatedAt, ...filters } = challenge;
    const result = await getRepo(env).update<Partial<PhoneLoginChallenge>>("login_challenges", {
        attempts: challenge.attempts + 1,
        expiresAt: challenge.codeHash === hash ? now : challenge.expiresAt,
        updatedAt: now
    }, filters);
    // Verify code hash and update result
    if (!result.changes || challenge.codeHash !== hash)
        throw new AuthError("Invalid or expired verification code", 401);
    // Issue authentication token
    const token = await issueToken(env, user.id, phone);
    return Response.json(token);
}

async function sendPhoneCode(env: Env, user: AppUser, phone: string): Promise<Response> {
    // Generate a new 6-digit code
    let random: number;
    do {
        random = crypto.getRandomValues(new Uint32Array(1))[0];
    } while (random >= 4294000000);
    const code = (random % 1000000).toString().padStart(6, "0");
    // Hash code and retrieve previous challenge
    const hash = await hashCode(env, phone, code);
    const previous = await getRepo(env).get<PhoneLoginChallenge>("login_challenges", { phone });
    const now = Date.now();
    // Check resend cooldown
    if (previous && previous.resendAfter > new Date(now).toISOString())
        throw new AuthError("Wait one minute before requesting another code", 429);
    // Create login challenge
    const challenge: PhoneLoginChallenge = {
        phone, codeHash: hash, attempts: 0,
        expiresAt: new Date(now + 5 * 60 * 1000).toISOString(),
        resendAfter: new Date(now + 60 * 1000).toISOString(),
        updatedAt: new Date(now).toISOString()
    };
    // Save or update login challenge
    const result = previous
        ? await getRepo(env).update("login_challenges", challenge, { phone, codeHash: previous.codeHash, resendAfter: previous.resendAfter })
        : await getRepo(env).save("login_challenges", challenge, true);
    if (!result.changes)
        throw new AuthError("Wait one minute before requesting another code", 429);
    // Send verification code via Whatsapp
    let response: Response | null;
    try {
        response = await env.WHATSAPP.fetch(`${env.WHATSAPP_API_URL}/whatsapp/login/phone`, {
            method: "POST",
            headers: { "Content-Type": "application/json", "X-API-Key": env.API_KEY },
            body: JSON.stringify({ phone, code, language: user.language })
        });
    } catch {
        response = null;
    }
    // Remove challenge if sending fails
    if (!response?.ok) {
        await getRepo(env).delete("login_challenges", { phone, codeHash: hash });
        throw new AuthError("Could not send the WhatsApp verification code", 502);
    }
    return Response.json({ expiresIn: 300 }, { status: 202 });
}
