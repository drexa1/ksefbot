import {Env} from "../../worker";
import {getAuthUser} from "../../auth";
import {AppUser} from "../../types/users";
import {AppInvoice} from "../../types/invoices";
import {KsefClient} from "../../clients/ksef";
import {D1Driver, Repository} from "../../repository/d1";
import {invoiceFromXml} from "../app/invoices";
import {findUncoveredPeriods, saveSyncedPeriod} from "./ksef-sync-periods";

let repo: Repository;
const getRepo = (env: Env): Repository => repo ??= new Repository(new D1Driver(env.D1));

export async function getInvoices(req: Request, env: Env, type: "sales" | "purchase"): Promise<Response> {
    const appUser = await getAuthUser(req, env);
    const url = new URL(req.url);
    const fromDate = url.searchParams.has("from") ? new Date(url.searchParams.get("from")!) : undefined;
    const toDate = url.searchParams.has("to") ? new Date(url.searchParams.get("to")!) : undefined;
    if ((fromDate && isNaN(fromDate.getTime())) || (toDate && isNaN(toDate.getTime())) || (fromDate && toDate && fromDate > toDate))
        return Response.json({ success: false, error: "Invalid date parameters" }, { status: 400 });
    try {
        // Query by invoice number
        if (url.searchParams.has("invoiceNumber")) {
            const invoiceNumber = url.searchParams.get("invoiceNumber")!;
            const {invoice, fromDb, fromKsef} = await getByInvoiceNumber(env, appUser, type, invoiceNumber, fromDate, toDate);
            return invoice
                ? Response.json({ success: true, count: 1, counts: { fromDb, fromKsef }, result: [invoice] })
                : Response.json({ success: false, error: "Invoice not found at KSeF." }, { status: 404 });
        }
        // Query by dates range
        if (!fromDate || !toDate)
            return Response.json({ success: false, error: "From and To are required together" }, { status: 400 });
        if ((Date.parse(toDate.toISOString().slice(0, 10)) - Date.parse(fromDate.toISOString().slice(0, 10))) / 86_400_000 + 1 > 100)
            return Response.json({ success: false, error: "The maximum date range supported by KSeF is 100 days." }, { status: 400 });
        const {appInvoices, fromDb, fromKsef} = await getByDatesRange(env, appUser, type, fromDate, toDate);
        return Response.json({
            success: appInvoices.length > 0,
            count: appInvoices.length,
            counts: { fromDb, fromKsef },
            result: appInvoices,
            ...(appInvoices.length === 0 && { error: "No invoices found for the specified date range." })
        });
    } catch (error: any) {
        if (String(error).includes("Too Many Requests"))
            return Response.json({ success: false, error: "The limit of 20 requests per hour has been exceeded." }, { status: 429 });
        return Response.json({ success: false, error: error instanceof Error ? error.message : String(error) }, { status: 502 });
    }
}

async function getByInvoiceNumber(env: Env, appUser: AppUser, type: "sales" | "purchase", invoiceNumber: string, from?: Date, to?: Date) {
    // Try first to find app invoice in database
    const appInvoice = await getRepo(env).get<AppInvoice & { ownerId: string }>("invoices", {
        id: invoiceNumber,
        ownerId: appUser.id,
        type: type
    });
    if (appInvoice)
        return { invoice: appInvoice, fromDb: 1, fromKsef: 0 };
    // Try to find it in KSeF
    const client = new KsefClient(env);
    // Query KSeF invoices metadata
    const metadata = await client.queryInvoiceMetadata(appUser, type, invoiceNumber, from, to);
    if (!metadata.invoices[0])
        return { invoice: null, fromDb: 0, fromKsef: 0 };
    // Request invoice XML
    const ksefInvoiceXml = await client.downloadInvoice(metadata.invoices[0]?.ksefNumber);
    // Map KSeF invoice XML to app invoice
    const ksefInvoice = await invoiceFromXml(env, ksefInvoiceXml, appUser, type, "Downloaded from KSeF");
    // Save as app invoice
    await getRepo(env).save("invoices", ksefInvoice, true);
    return { invoice: ksefInvoice, fromDb: 0, fromKsef: 1 };
}

/// Also used from tax record computations
export async function getByDatesRange(env: Env, appUser: AppUser, type: "sales" | "purchase", from: Date, to: Date) {
    const fromDate = from.toISOString().slice(0, 10);
    const toDate = to.toISOString().slice(0, 10);
    const range = {field: "issueDate", start: fromDate, startInclusive: true, end: toDate, endInclusive: true};
    // Read matching invoices already in the database
    const existingAppInvoices = await getRepo(env).getAll<AppInvoice & { ownerId: string }>("invoices", {ownerId: appUser.id, type}, range);
    // This might download more invoices
    const ksefInvoices = await syncWithKsef(env, appUser, type, fromDate, toDate);
    // Retrieve the final list including those newly saved invoices
    const appInvoices = await getRepo(env).getAll<AppInvoice & { ownerId: string }>("invoices", {ownerId: appUser.id, type}, range);
    return { appInvoices, fromDb: existingAppInvoices.length, fromKsef: ksefInvoices.count };
}

async function syncWithKsef(env: Env, appUser: AppUser, type: "sales" | "purchase", from: string, to: string) {
    const uncoveredPeriods = await findUncoveredPeriods(env, appUser.id, type, from, to);
    let count = 0;
    let inserted = 0;
    for (const period of uncoveredPeriods) {
        const fromDate = new Date(period.from);
        const toDate = new Date(`${period.to}T23:59:59.999Z`);
        const result = await fetchFromKsefForPeriod(env, appUser, type, fromDate, toDate);
        count += result.count;
        inserted += result.inserted;
    }
    return { count, inserted };
}

async function fetchFromKsefForPeriod(env: Env, appUser: AppUser, type: "sales" | "purchase", from: Date, to: Date) {
    const client = new KsefClient(env);
    const invoiceMetadata = await client.queryInvoiceMetadata(appUser, type, undefined, from, to);
    let inserted = 0;
    for (let batchStart = 0; batchStart < invoiceMetadata.invoices.length; batchStart += 20) {
        const invoices = await Promise.all(invoiceMetadata.invoices.slice(batchStart, batchStart + 20).map(async invoiceMetadata => {
            const xml = await client.downloadInvoice(invoiceMetadata.ksefNumber);
            return invoiceFromXml(env, xml, appUser, type, "Downloaded from KSeF");
        }));
        for (const invoice of invoices)
            inserted += (await getRepo(env).save("invoices", invoice, true)).changes;
    }
    if (type === "sales")
        await saveSyncedPeriod(env, appUser.id, "sales", from.toISOString().slice(0, 10), to.toISOString().slice(0, 10));
    return { count: invoiceMetadata.invoices.length, inserted };
}
