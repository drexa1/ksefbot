import {Env} from "../../worker";
import {getAuthUser} from "../../auth";
import {AppUser} from "../../types/users";
import {AppInvoice} from "../../types/invoices";
import {KsefClient} from "../../clients/ksef";
import {KsefInvoiceMetadata} from "../../types/ksef";
import {D1Driver, Repository} from "../../repository/d1";
import {invoiceFromXml} from "../app/invoices";
import {findUncoveredPeriods, saveSyncPeriod} from "./sync-periods";

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
        const client = new KsefClient(env);
        // Query by invoice number
        if (url.searchParams.has("invoiceNumber")) {
            const invoiceNumber = url.searchParams.get("invoiceNumber")!;
            const {invoice, fromDb, fromKsef} = await getByInvoiceNumber(env, client, type, appUser, invoiceNumber, fromDate, toDate);
            return invoice
                ? Response.json({ success: true, count: 1, counts: { fromDb, fromKsef }, result: [invoice] })
                : Response.json({ success: false, error: "Invoice not found at KSeF." }, { status: 404 });
        }
        // Query by dates range
        if (!fromDate || !toDate)
            return Response.json({ success: false, error: "From and To are required together" }, { status: 400 });
        if ((Date.parse(toDate.toISOString().slice(0, 10)) - Date.parse(fromDate.toISOString().slice(0, 10))) / 86_400_000 + 1 > 100)
            return Response.json({ success: false, error: "The maximum date range supported by KSeF is 100 days." }, { status: 400 });
        const {appInvoices, fromDb, fromKsef} = await getByDatesRange(env, client, type, appUser, fromDate, toDate);
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

async function getByInvoiceNumber(env: Env, client: KsefClient, type: "sales" | "purchase", appUser: AppUser, invoiceNumber: string, from?: Date, to?: Date) {
    const appInvoice = await getRepo(env).get<AppInvoice & { ownerId: string }>("invoices", {id: invoiceNumber, ownerId: appUser.id, type});
    if (appInvoice)
        return { invoice: appInvoice, fromDb: 1, fromKsef: 0 };
    const metadata = await client.queryInvoiceMetadata(appUser, type, invoiceNumber, from, to);
    const [ksefInvoice] = await downloadInvoices(env, appUser, type, client, metadata.invoices.slice(0, 1));
    if (!ksefInvoice)
        return { invoice: null, fromDb: 0, fromKsef: 0 };
    return { invoice: ksefInvoice, fromDb: 0, fromKsef: 1 };
}

/// Also used from tax record computations
export async function getByDatesRange(env: Env, client: KsefClient,  type: "sales" | "purchase", appUser: AppUser, from: Date, to: Date) {
    const range = {
        field: "issueDate",
        start: from.toISOString().slice(0, 10),
        end: to.toISOString().slice(0, 10),
        startInclusive: true,
        endInclusive: true
    };
    // Read matching invoices already in the database
    const existingAppInvoices = await getRepo(env).getAll<AppInvoice & { ownerId: string }>("invoices", { ownerId: appUser.id, type }, range);
    // This might download more invoices
    const ksefInvoices = await syncWithKsef(env, type, appUser, from, to, client);
    // Retrieve the final list including those newly saved invoices
    const appInvoices = await getRepo(env).getAll<AppInvoice & { ownerId: string }>("invoices", { ownerId: appUser.id, type }, range);
    return { appInvoices, fromDb: existingAppInvoices.length, fromKsef: ksefInvoices.downloaded };
}

async function syncWithKsef(env: Env, type: "sales" | "purchase", appUser: AppUser, from: Date, to: Date, client: KsefClient): Promise<{ downloaded: number }> {
    // Compute periods without sync coverage
    const uncoveredPeriods = await findUncoveredPeriods(env, appUser.id, type, from, to);
    let downloaded = 0;
    for (const period of uncoveredPeriods) {
        const fromDate = new Date(period.from);
        const toDate = new Date(`${period.to}T23:59:59.999Z`);
        const invoicesMetadata = await client.queryInvoiceMetadata(appUser, type, undefined, fromDate, toDate);
        const invoices = await downloadInvoices(env, appUser, type, client, invoicesMetadata.invoices);
        await saveSyncPeriod(env, appUser.id, type, period.from, period.to);
        downloaded += invoices.length;
    }
    return { downloaded };
}

async function downloadInvoices(env: Env, appUser: AppUser, type: "sales" | "purchase", client: KsefClient, metadata: KsefInvoiceMetadata[]) {
    return await Promise.all(metadata.map(async invoiceMetadata => {
        const invoiceXml = await client.downloadInvoice(invoiceMetadata.ksefNumber);
        const appInvoice = await invoiceFromXml(env, invoiceXml, appUser, type, "Downloaded from KSeF");
        await getRepo(env).save("invoices", appInvoice, true);
        return appInvoice;
    }));
}
