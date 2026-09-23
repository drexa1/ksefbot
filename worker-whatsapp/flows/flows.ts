import {Env} from "../worker";
import {WhatsappMessage} from "../types/whatsapp";

export async function initializeUser(env: Env, from: string, language: "en" | "pl") {
    await env.KSEFBOT.fetch(`${env.KSEFBOT_BASE_URL}/app/users`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-API-Key": env.API_KEY },
        body: JSON.stringify({ id: "0000000000", language, from }),
    });
    console.info(`Create user for ${from} pending onboarding`);
}

export async function triggerOnboarding(env: Env, from: string, language: any) {
    console.info(`Starting onboarding for ${from}`);
    // TODO: start onboarding flow
}

export async function onboardingFlow(request: Request, env: Env): Promise<Response> {
    const payload = await request.json();
    console.info("Onboarding flow exchange", payload);
    // Determine which screen/action this exchange belongs to, then perform the appropriate backend operation
    return Response.json({
        // Flow response
    });
}

export async function attendExistingUser(env: Env, message: WhatsappMessage) {
    // TODO
}
