import {Env} from "../../worker";
import {getInvoicesFor} from "./ksef";
import {getAuthUser} from "../../auth";
import {KsefClient} from "../../clients/ksef";
import { XMLParser } from "fast-xml-parser";

/**
 * Invoices where the user is the issuer.
 */
export async function get(req: Request, env: Env): Promise<Response> {
    return getInvoicesFor(req, env, "Subject1");
}

export async function post(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const form = await req.formData();
    const file = form.get("file");
    if (!(file instanceof File)) return Response.json({ error: "Missing XML file" }, { status: 400 });
    try {
        const invoiceBytes = new Uint8Array(await file.arrayBuffer());
        console.info("⚖️ Input file:", { size: invoiceBytes.length });
        const ksefClient = new KsefClient(env);
        const result = await ksefClient.postInvoice(appUser, invoiceBytes);
        return Response.json({ success: true, result }, { status: 200 });
    } catch (error) {
        if (String(error).includes("Too Many Requests"))
            return Response.json({ success: false, error: "The limit of 20 requests per hour has been exceeded." }, { status: 429 });
        throw error;
    }
}

export async function sessions(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const sessionReferenceNumber = url.searchParams.get("sessionReferenceNumber");
    const client = new KsefClient(env);
    await client.authenticate(appUser);
    const sessionStatus = await client.getSessionStatus(sessionReferenceNumber ?? undefined);
    return Response.json(sessionStatus);
}

export async function invoiceStatus(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const sessionReferenceNumber = url.searchParams.get("sessionReferenceNumber");
    const invoiceReferenceNumber = url.searchParams.get("invoiceReferenceNumber");
    if (!sessionReferenceNumber || !invoiceReferenceNumber)
        return Response.json({ error: "Missing reference numbers" }, { status: 400 });
    const client = new KsefClient(env);
    await client.authenticate(appUser);
    const result = await client.getInvoiceStatus(sessionReferenceNumber, invoiceReferenceNumber);
    return Response.json(result);
}

const parser = new XMLParser({
    ignoreAttributes: false,
    attributeNamePrefix: "@_",
});
export async function downloadReceipt(req: Request, env: Env): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const sessionReferenceNumber = url.searchParams.get("sessionReferenceNumber");
    const invoiceReferenceNumber = url.searchParams.get("invoiceReferenceNumber");
    const format = url.searchParams.get("format") ?? "xml";
    if (!sessionReferenceNumber || !invoiceReferenceNumber)
        return new Response("Missing parameters", { status: 400 });
    const ksefClient = new KsefClient(env);
    await ksefClient.authenticate(appUser);
    const result = await ksefClient.getInvoiceStatus(sessionReferenceNumber, invoiceReferenceNumber);
    const upoResponse = await fetch(result.upoDownloadUrl);
    if (!upoResponse.ok)
        return new Response(`KSeF UPO download failed: ${upoResponse.status}`, { status: 502 });
    const body = await upoResponse.arrayBuffer();
    if (format === "xml") {
        return new Response(body, { status: 200, headers: {
            "Content-Type": "application/xml; charset=utf-8",
            "Content-Disposition": 'attachment; filename="upo.xml"',
            "Access-Control-Allow-Origin": "*"
        }});
    }
    const text = new TextDecoder().decode(body);
    const json = parser.parse(text);
    return new Response(JSON.stringify(json, null, 2), { status: 200, headers: {
        "Content-Type": "application/json; charset=utf-8",
        "Content-Disposition": "inline",
        "Access-Control-Allow-Origin": "*"
    }});
}