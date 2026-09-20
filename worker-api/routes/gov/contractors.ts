import {Env} from "../../worker";
import {lookupCEIDG} from "./ceidg";
import {lookupVATLB} from "./vat-lb";
import {lookupKRS} from "./krs";
import {KsefContractor} from "../../types/gov";

/**
 * In which order to query the different gov.pl endpoints for contractor information
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
    const profile = url.searchParams.get("profile") as "user" | "customer" ;
    if (!nip || !/^\d{10}$/.test(nip))
        return Response.json({ success: false, error: "Invalid NIP" }, { status: 400 });
    try {
        // Profile argument is a hint to know whether to start looking at CEIDG or rather KRS,
        const govEndpoint: KsefContractor["source"][] = profile ? lookupOrder[profile] : Math.random() < 0.5
            // ...but in absence of any clue, pick randomly one, then the other, and the VAP payers whitelist as last resource
            ? ["CEIDG", "KRS", "VAT-LB"] : ["KRS", "CEIDG", "VAT-LB"];
        for (const source of govEndpoint) {
            try {
                const result = await lookupContractorSource(source, nip, env);
                return Response.json(result, { status: 200 });
            } catch {
                // Off to the next lookup method
            }
        }
        return Response.json({ success: false, error: "Contractor not found", nip: nip }, { status: 404 });
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

export function titleCase(value: string): string {
    return value.trim().toLowerCase().replace(/(^|[\s-])(\p{L})/gu, (_, separator, char) => separator + char.toUpperCase());
}