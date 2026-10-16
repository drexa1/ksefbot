import {Env} from "../worker";
import {lookupCEIDG} from "./contractors/ceidg";
import {lookupVATLB} from "./contractors/vat-lb";
import {lookupKRS} from "./contractors/krs";
import {KsefContractor, ServiceUnavailableError} from "../types/gov";

/**
 * In which order to query the different gov.pl endpoints for contractor information.
 * For the current user: assume is a JDG and if not try as KRS company
 * For a customer contractor: assume is a KRS company and if not try with JDG's
 * If nothing works, try to find the look for the details as VAT payer.
 */
const lookupOrder = {
    user:     ["CEIDG", "KRS", "VAT-LB"],
    customer: ["KRS", "CEIDG", "VAT-LB"]
} satisfies Record<"user" | "customer", KsefContractor["source"][]>;

export async function contractors(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    const nip = url.searchParams.get("nip");
    const profile = url.searchParams.get("profile") as "user" | "customer";
    if (!nip || !/^\d{10}$/.test(nip))
        return Response.json({ success: false, error: "Invalid NIP" }, { status: 400 });
    try {
        // Profile argument is a hint to know whether to start looking at CEIDG or rather KRS,
        const govEndpoint: KsefContractor["source"][] = profile ? lookupOrder[profile] : Math.random() < 0.5
            // ...but in absence of any clue, pick randomly one, then the other, and the VAT payers whitelist as last resource
            ? ["CEIDG", "KRS", "VAT-LB"] : ["KRS", "CEIDG", "VAT-LB"];
        let unavailable: ServiceUnavailableError | undefined;
        for (const source of govEndpoint) {
            try {
                const result = await lookupContractorSource(source, nip, env);
                return Response.json(result, { status: 200 });
            } catch (error: any) {
                // Off to the next lookup method
                if (error.status === 503) unavailable = error;
            }
        }
        return Response.json({ success: false, error: unavailable?.message ?? "Contractor not found", details: unavailable?.details, nip: nip }, { status: unavailable ? 503 : 404 });
    } catch (error) {
        return Response.json({ success: false, error: String(error) }, { status: 404 });
    }
}

async function lookupContractorSource(source: KsefContractor["source"], nip: string, env: Env): Promise<KsefContractor> {
    switch (source) {
        case "CEIDG":
            return lookupCEIDG(nip, env);
        case "KRS":
            return lookupKRS(nip, env);
        case "VAT-LB":
            return lookupVATLB(nip, env);
    }
}

export async function assertAvailable(response: Response, registry: string): Promise<void> {
    if (response.status >= 500 || response.headers.get("Content-Type")?.toLowerCase().includes("text/html")) {
        const details = await new HTMLRewriter().on("title", { text(text) {
            let title;
            title += text.text
        }}).transform(response).text().trim() || "Unknown error";
        throw new ServiceUnavailableError(`${registry} temporarily unavailable`, details);
    }
}