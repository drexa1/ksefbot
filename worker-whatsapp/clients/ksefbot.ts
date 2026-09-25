import {Env} from "../worker";
import {KsefContractor} from "../../worker-api/types/gov";

export async function initializeUser(env: Env, from: string, language: "en" | "pl"): Promise<void> {
    const response = await env.KSEFBOT.fetch(`${env.KSEFBOT_BASE_URL}/app/users`, {
        method: "POST",
        headers: { "Content-Type": "application/json", "X-API-Key": env.API_KEY },
        body: JSON.stringify({ id: "0000000000", language, from }),
    });
    if (!response.ok)
        console.error("Initializing user:", await response.text());
    const result = await response.json() as { success: boolean, id: string };
    console.info(`Created user ${result.id} for ${from} pending onboarding`);
}

export async function findCompany(env: Env, nip: string, profile: "user" | "customer"): Promise<KsefContractor> {
    const response = await env.KSEFBOT.fetch(`${env.KSEFBOT_BASE_URL}/gov/contractors?nip=${nip}&profile=${profile}`, {
        headers: { "Accept": "application/json", "X-API-Key": env.API_KEY
    }});
    if (!response.ok)
        console.error("Fetching contractor data:", await response.text());
    return await response.json();
}