import {Env} from "../../worker";
import {dtoFromAliases} from "../../dto/avro";
import {titleCase} from "./contractors";
import {KsefContractor} from "../../types/gov";

// noinspection JSUnusedGlobalSymbols
export async function contractors(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    const nip = url.searchParams.get("nip");
    if (!nip || !/^\d{10}$/.test(nip))
        return Response.json({ success: false, error: "Invalid NIP" }, { status: 400 });
    try {
        const result = await lookupVATLB(nip, env);
        return Response.json(result, { status: 200 });
    } catch (error: any) {
        return Response.json({ success: false, error: String(error) }, { status: 404 });
    }
}

/**
 * Search by Tax Identification Number in the VAT payers whitelist.
 */
export async function lookupVATLB(nip: string, env: Env): Promise<KsefContractor> {
    const date = new Date().toISOString().substring(0, 10);
    const lbLookup = await fetch(`${env.VAT_LB_URL}/${nip}?date=${date}`, {
        headers: { Accept: "application/json" }
    });
    if (!lbLookup.ok)
        throw new Error(`VAT whitelist lookup failed: ${lbLookup.status}`);
    const vatPayer = await lbLookup.json() as { result?: { subject?: { nip?: string }} };
    if (!vatPayer?.result?.subject?.nip)
        throw new Error(`No VAT payers whitelist entry found for NIP ${nip}`);
    const vatWhitelistLookupAvroSchema = await env.assets.fetch(new URL(env.VAT_LB_LOOKUP_SCHEMA)).then(res => res.json());
    return mapVATLB(vatPayer, vatWhitelistLookupAvroSchema);
}

function mapVATLB(result: any, schema: any): KsefContractor {
    const dto = dtoFromAliases(result, schema);
    const subject = dto.result?.subject;
    const addressLine = subject?.workingAddress ?? subject?.residenceAddress;
    return {
        source: "VAT-LB",
        name: titleCase(subject?.name),
        nip: subject?.nip,
        regon: subject?.regon ?? undefined,
        countryCode: "PL",
        addressLine: titleCase(addressLine),
        active: subject?.status === "ACTIVE"
    };
}