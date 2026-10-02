import {Env} from "../../worker";
import {D1Driver, Repository} from "../../repository/d1";
import {getAuthUser} from "../../auth";
import {AppUser} from "../../types/users";
import {AppInvoice} from "../../types/invoices";
import {KsefClient} from "../../clients/ksef";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

export async function getInvoices(req: Request, env: Env, subjectType: "Subject1" | "Subject2"): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const from = new Date(url.searchParams.get("from")!);
    const to = new Date(url.searchParams.get("to")!);
    if ((from && isNaN(from.getTime())) || (to && isNaN(to.getTime())) || (from && to && from > to))
        return Response.json({ success: false, error: "Invalid date parameters" }, { status: 400 });
    try {
        // Download XML for specific invoice
        if (url.searchParams.has("invoiceNumber")) {
            const ksefClient = new KsefClient(env);
            await ksefClient.authenticate(appUser);
            const metadataResult = await ksefClient.queryInvoiceMetadata(subjectType, url.searchParams.get("invoiceNumber")!, from, to);
            const invoiceXml = await ksefClient.downloadInvoice(metadataResult.invoices[0]?.ksefNumber);
            return invoiceXml
                ? new Response(invoiceXml, { status: 200, headers: { "Content-Type": "application/xml; charset=utf-8" } })
                : Response.json({ success: false, error: "Invoice not found at KSeF." }, { status: 404 });
        }
        // Download app invoices for dates range
        if ((to.getTime() - from.getTime()) / 86_400_000 > 90)  // 3 months max span from KSeF
            return Response.json({ success: false, error: `The maximum date range supported by KSeF is 3 calendar months.` }, { status: 400 });
        const result = await fetchInvoices(env, appUser, subjectType, from, to);
        return Response.json({
            success: result.length > 0,
            ...(result.length > 0 && { count: result.length }),
            result,
            ...(result.length === 0 && { error: "No invoices found for the specified date range." })
        }, { status: 200 });
    } catch (error: any) {
        if (String(error).includes("Too Many Requests"))
            return Response.json({ success: false, error: "The limit of 20 requests per hour has been exceeded." }, { status: 429 });
        return Response.json({ success: false, error: error instanceof Error ? error.message : String(error) }, { status: 502 });
    }
}

/// Used also by for record computations
export async function fetchInvoices(env: Env, appUser: AppUser, subjectType: "Subject1" | "Subject2", from: Date, to: Date) {
    const ksefClient = new KsefClient(env);
    const invoices = await ksefClient.queryPurchaseInvoices(env, appUser, subjectType, from, to);
    await saveInvoices(env, invoices);
    // Return the JSON formatted
    return invoices.map(row => JSON.parse(row.jsonData));
}

async function saveInvoices(env: Env, invoices: Awaited<AppInvoice & { ownerId: string }>[]) {
    // Cache in app
    const saved = [];
    const existing = [];
    for (const invoice of invoices) {
        try {
            await getRepo(env).save<AppInvoice>("invoices", invoice);
            saved.push(invoice);
        } catch (error) {
            if (String(error).includes("UNIQUE constraint failed")) {
                console.warn("Invoice already exists:", invoice.id);
                existing.push(invoice.id);
            } else
                throw error;
        }
    }
}