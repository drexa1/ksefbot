import {Env} from "../worker";
import {FlowRequest, WhatsappMessage} from "../types/whatsapp";
import {decryptFlowRequest, encryptFlowResponse} from "./crypto";
import {findCompany} from "../clients/ksefbot";

export async function triggerOnboarding(env: Env, from: string, language: any) {
    console.info(`Starting onboarding for ${from}`);
    // TODO: start onboarding flow
}

export async function onboardingExchange(request: Request, env: Env): Promise<Response> {
    const encryptedRequest = await request.json() as FlowRequest;
    console.info("Onboarding flow exchange received");
    const {payload, aesKey, iv} = await decryptFlowRequest(encryptedRequest, env.WHATSAPP_FLOWS_PEM);
    // Handle the action/screen
    const responsePayload = await onboardingAction(payload, env);
    // Encrypt response using the Flow encryption protocol
    const encryptedResponse = await encryptFlowResponse(responsePayload, aesKey, iv);
    return new Response(encryptedResponse, { headers: {"Content-Type": "text/plain"}});
}

async function onboardingAction(payload: Record<string, string>, env: Env): Promise<Record<string, unknown>> {
    console.info(`Flow action: ${payload.action}`);
    console.info(`Flow screen: ${payload.screen}`);
    switch (payload.action) {
        case "start_onboarding": {
            return {
                screen: "ONBOARDING_USER",
                data: {
                    company_name: ""
                }
            };
        }
        case "find_user_company": {
            const company = await findCompany(env, payload.company_nip, "user");
            return {
                screen: "ONBOARDING_USER",
                data: {
                    company_name: company?.name ?? "not found"
                }
            };
        }
        case "find_customer_company": {
            if (!payload.customer_nip)
                return {
                    screen: "ONBOARDING_CUSTOMER",
                    data: {
                        customer_name: ""
                    }
                };
            const company = await findCompany(env, payload.customer_nip, "customer");
            return {
                screen: "ONBOARDING_CUSTOMER",
                data: {
                    customer_name: company?.name ?? "not found"
                }
            };
        }
        // TODO: where to validate ksef_token?
        case "finish_onboarding": {
            // Update user and mark onboarded
            const companyNip = payload.company_nip;
            const companyName = payload.company_name;
            const ksefToken = payload.ksef_token;
            const customerNip = payload.customer_nip;
            const customerName = payload.customer_name;
            const bankName = payload.bank_name;
            const accountNumber = payload.account_number;
            const notificationDay = payload.notification_day;
            const specificDay = payload.specific_day;
            console.info({ companyNip, companyName, ksefToken, customerNip, customerName, bankName, accountNumber, notificationDay, specificDay });
            // Finalize initialized user
            // Create contractor data for user and customer
            // Notify new customer
            return {
                screen: "ONBOARDING_SUCCESS"
            };
        }
        default:
            throw new Error(`Unknown flow action: ${payload.action}`);
    }
}

export async function attendExistingUser(env: Env, message: WhatsappMessage) {
    // TODO
}
