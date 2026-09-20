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
        const result = await lookupCEIDG(nip, env);
        if (!result)
            return Response.json({ success: false, error: "Firm not found in CEIDG" }, { status: 404 });
        return Response.json(result, { status: 200 });
    } catch (error: any) {
        return Response.json({ success: false, error: String(error) }, { status: 404 });
    }
}

/**
 * Search by Tax Identification Number in the Central Register on Business Activity.
 */
export async function lookupCEIDG(nip: string, env: Env): Promise<KsefContractor> {
    const ceidgLookup = await fetch(`${env.CEIDG_URL}/firmy?nip=${nip}`, { headers: {
        Accept: "application/json", Authorization: `Bearer ${env.CEIDG_API_KEY}`
    }});
    if (!ceidgLookup.ok)
        throw new Error(`CEIDG NIP lookup failed: ${ceidgLookup.status}`);
    if (ceidgLookup.status === 204)
        throw new Error(`No CEIDG entry found for NIP ${nip}`);
    const lookupResult = await ceidgLookup.json() as { firmy?: { id: string }[] };
    const company = lookupResult["firmy"]?.[0];
    if (!company)
        throw new Error(`No CEIDG entry found for NIP ${nip}`);
    const ceidgDetails = await fetch(`${env.CEIDG_URL}/firma/${company.id}`, { headers: {
        Accept: "application/json", Authorization: `Bearer ${env.CEIDG_API_KEY}`
    }});
    if (!ceidgDetails.ok)
        throw new Error(`CEIDG details lookup failed: ${ceidgDetails.status}`);
    const detailsResult = await ceidgDetails.json();
    const ceidgLookupAvroSchema = await env.assets.fetch(new URL(env.CEIDG_LOOKUP_SCHEMA)).then(res => res.json());
    return mapCEIDG(detailsResult, ceidgLookupAvroSchema);
}

function mapCEIDG(result: any, schema: any): KsefContractor {
    const dto = dtoFromAliases(result, schema);
    const company = dto.companies?.[0];
    const address = company.correspondenceAddress;
    const building = [address?.buildingNumber, address?.apartmentNumber ? `/${address.apartmentNumber}` : undefined].filter(Boolean).join("");
    const addressLine = [address?.city, address?.postalCode, building].filter(Boolean).join(", ");
    return {
        source: "CEIDG",
        name: titleCase(company?.name),
        nip: company?.owner?.nip,
        regon: company?.owner?.regon ?? undefined,
        countryCode: address?.countryCode,
        addressLine: titleCase(addressLine),
        active: company?.status === "ACTIVE"
    };
}