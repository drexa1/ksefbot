import {Env} from "../../worker";
import {dtoFromAliases} from "../../dto/avro";
import {encodeToken} from "./krs-apikey";
import {titleCase} from "./contractors";
import {KsefContractor} from "../../types/gov";

// noinspection JSUnusedGlobalSymbols
export async function contractors(req: Request, env: Env): Promise<Response> {
    const url = new URL(req.url);
    const nip = url.searchParams.get("nip");
    if (!nip || !/^\d{10}$/.test(nip))
        return Response.json({ success: false, error: "Invalid NIP" }, { status: 400 });
    try {
        const result = await lookupKRS(nip, env);
        if (!result)
            return Response.json({ success: false, error: "Firm not found in KRS" }, { status: 404 });
        return Response.json(result, { status: 200 });
    } catch (error: any) {
        return Response.json({ success: false, error: String(error) }, { status: 404 });
    }
}

/**
 * Search by Tax Identification Number in the National Court Registry.
 */
export async function lookupKRS(nip: string, env: Env): Promise<KsefContractor> {
    const timestamp = new Date().toISOString().slice(0, 19);
    const apiKey = encodeToken("0".repeat(10), timestamp);
    const krsLookup = await fetch(`${env.KRS_SEARCH_URL}/api/wyszukiwarka/krs`, {
        method: "POST",
        headers: {
            Accept: "application/json",
            "Content-Type": "application/json",
            "User-Agent": "Mozilla/5.0",
            Apikey: apiKey
        },
        body: JSON.stringify({ podmiot: { nip }, rejestr: ["P", "S"], paginacja: {} })
    });
    if (!krsLookup.ok)
        throw new Error(`KRS NIP lookup failed: ${krsLookup.status}`);
    if (krsLookup.status === 204)
        throw new Error(`No KRS entry found for NIP ${nip}`);
    const lookupResult = await krsLookup.json() as { [key: string]: any };
    const krsNumber = lookupResult?.["listaPodmiotow"]?.[0]?.["numer"];
    if (!krsNumber)
        throw new Error(`No KRS entry found for NIP ${nip}`);
    const krsDetails = await fetch(`${env.KRS_API_URL}/api/krs/OdpisAktualny/${krsNumber}`, {
        headers: { Accept: "application/json" }
    });
    if (!krsDetails.ok)
        throw new Error(`KRS details lookup failed: ${krsDetails.status}`);
    const detailsResult = await krsDetails.json();
    const krsLookupAvroSchema = await env.assets.fetch(new URL(env.KRS_LOOKUP_SCHEMA)).then(res => res.json());
    return mapKRS(detailsResult, krsLookupAvroSchema);
}

function mapKRS(result: any, schema: any): KsefContractor {
    const dto = dtoFromAliases(result, schema);
    const header = dto.copy?.header;
    const company = dto.copy?.data?.section1?.entityData;
    const registeredOffice = dto.copy?.data?.section1?.registeredOfficeAndAddress;
    const address = registeredOffice?.address;
    const addressLine = [
        address?.city,
        address?.postalCode,
        [address?.street, address?.buildingNumber]
            .filter(Boolean)
            .join(" ")
    ].filter(Boolean).join(", ");
    return {
        source: "KRS",
        name: titleCase(company?.name),
        nip: company?.identifiers?.nip,
        regon: company?.identifiers?.regon?.slice(0, 9) ?? undefined,
        countryCode: address?.country,
        addressLine: titleCase(addressLine),
        active: header?.positionStatus === 1
    };
}